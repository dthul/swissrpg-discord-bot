use lazy_static::lazy_static;
use serenity::http::CacheHttp;
use serenity::model::id::{RoleId, UserId};
use std::collections::HashSet;

pub const CHAMPION_PRODUCT_PATTERN: &'static str =
    r"(?i).*(Novice|Apprentice|Adept|Master|Legendary).*";
pub const INSIDER_PRODUCT_PATTERN: &'static str = r"(?i).*(Apprentice|Adept|Master|Legendary).*";

lazy_static! {
    pub static ref CHAMPION_PRODUCT_REGEX: regex::Regex =
        regex::Regex::new(CHAMPION_PRODUCT_PATTERN).unwrap();
    pub static ref INSIDER_PRODUCT_REGEX: regex::Regex =
        regex::Regex::new(INSIDER_PRODUCT_PATTERN).unwrap();
}

#[cfg(feature = "bottest")]
pub mod ids {
    use super::*;
    // Test server:
    pub const CHAMPION_ID: RoleId = RoleId(670250507436294144);
    pub const INSIDER_ID: RoleId = RoleId(670250754422079488);
}

#[cfg(not(feature = "bottest"))]
pub mod ids {
    use super::*;
    // SwissRPG server:
    pub const CHAMPION_ID: RoleId = RoleId(670197555166052362);
    pub const INSIDER_ID: RoleId = RoleId(670201953883783169);
}

pub async fn update_roles(
    discord_api: &crate::discord::CacheAndHttp,
    stripe_client: &stripe::Client,
) -> Result<(), crate::meetup::Error> {
    // Get all active subscriptions from Stripe
    let subscriptions = crate::stripe::list_active_subscriptions(stripe_client).await?;
    // For each subscription, find which product and customer are associated with it
    // and add the Discord name stored in Stripe to the appropriate list
    let mut new_champions = vec![];
    let mut new_insiders = vec![];
    for subscription in &subscriptions {
        match get_customer_and_product(stripe_client, subscription).await {
            Err(err) => eprintln!("Error in update_roles:\n{:#?}", err),
            Ok((customer, product)) => {
                let discord_username = match customer.metadata.get("Discord") {
                    None => continue,
                    Some(username) => username,
                };
                let is_champion_product = product
                    .name
                    .as_ref()
                    .map_or(false, |name| CHAMPION_PRODUCT_REGEX.is_match(name));
                let is_insider_product = product
                    .name
                    .as_ref()
                    .map_or(false, |name| INSIDER_PRODUCT_REGEX.is_match(name));
                if is_champion_product {
                    new_champions.push(discord_username.clone());
                }
                if is_insider_product {
                    new_insiders.push(discord_username.clone());
                }
            }
        }
    }
    // Look up the Discord IDs for the new champions and insiders
    let new_champions = discord_usernames_to_ids(discord_api, &new_champions)?;
    let new_insiders = discord_usernames_to_ids(discord_api, &new_insiders)?;
    // Now, check which Discord users already have the Champion and Insider roles
    let mut current_champions = vec![];
    let mut current_insiders = vec![];
    for member_result in crate::discord::sync::ids::GUILD_ID.members_iter(&discord_api.http) {
        let member = match member_result {
            Err(err) => {
                eprintln!("Error when querying Discord member:\n{:#?}", err);
                continue;
            }
            Ok(member) => member,
        };
        let user = member.user.read();
        let is_champion = match user.has_role(
            discord_api,
            crate::discord::sync::ids::GUILD_ID,
            ids::CHAMPION_ID,
        ) {
            Ok(has_role) => has_role,
            Err(err) => {
                eprintln!(
                    "Error when querying whether Discord user {} has the Champion role:\n{:#?}",
                    user.id, err
                );
                false
            }
        };
        let is_insider = match user.has_role(
            discord_api,
            crate::discord::sync::ids::GUILD_ID,
            ids::INSIDER_ID,
        ) {
            Ok(has_role) => has_role,
            Err(err) => {
                eprintln!(
                    "Error when querying whether Discord user {} has the Insider role:\n{:#?}",
                    user.id, err
                );
                false
            }
        };
        if is_champion {
            current_champions.push(user.id);
        }
        if is_insider {
            current_insiders.push(user.id);
        }
    }
    // Assign the role(s) to users which earned it but don't have it yet and
    // remove it from those who currently have it, but are not subscribed
    // anymore
    let new_champions: HashSet<_> = new_champions.into_iter().collect();
    let new_insiders: HashSet<_> = new_insiders.into_iter().collect();
    let current_champions: HashSet<_> = current_champions.into_iter().collect();
    let current_insiders: HashSet<_> = current_insiders.into_iter().collect();
    for new_champion in &new_champions {
        if !current_champions.contains(new_champion) {
            // Assign champion role
            add_member_role(discord_api.clone(), *new_champion, ids::CHAMPION_ID).await?;
        }
    }
    for new_insider in &new_insiders {
        if !current_insiders.contains(new_insider) {
            // Assign insider role
            add_member_role(discord_api.clone(), *new_insider, ids::INSIDER_ID).await?;
        }
    }
    for current_champion in &current_champions {
        if !new_champions.contains(current_champion) {
            // Remove champion role
            remove_member_role(discord_api.clone(), *current_champion, ids::CHAMPION_ID).await?;
        }
    }
    for current_insider in &current_insiders {
        if !new_insiders.contains(current_insider) {
            // Remove insider role
            remove_member_role(discord_api.clone(), *current_insider, ids::INSIDER_ID).await?;
        }
    }
    Ok(())
}

async fn get_customer_and_product(
    client: &stripe::Client,
    subscription: &stripe::Subscription,
) -> Result<(stripe::Customer, stripe::Product), crate::meetup::Error> {
    // First, figure out which product was bought
    // Subscription -> Plan -> Product
    let product = match &subscription.plan {
        Some(plan) => match &plan.product {
            Some(product) => match product {
                stripe::Expandable::Object(product) => Some(*product.clone()),
                stripe::Expandable::Id(product_id) => {
                    let product = stripe::Product::retrieve(client, product_id, &[]).await?;
                    Some(product)
                }
            },
            _ => None,
        },
        _ => None,
    };
    if let Some(product) = product {
        // Now, figure out who the customer is
        let customer = match &subscription.customer {
            stripe::Expandable::Object(customer) => *customer.clone(),
            stripe::Expandable::Id(customer_id) => {
                let customer = stripe::Customer::retrieve(client, customer_id, &[]).await?;
                customer
            }
        };
        Ok((customer, product))
    } else {
        Err(simple_error::SimpleError::new(format!(
            "Could not find a product associated with Stripe subscription {}",
            subscription.id
        ))
        .into())
    }
}

fn discord_usernames_to_ids(
    discord_api: &crate::discord::CacheAndHttp,
    usernames: &[String],
) -> Result<Vec<UserId>, crate::meetup::Error> {
    let guild = match crate::discord::sync::ids::GUILD_ID.to_guild_cached(&discord_api.cache) {
        Some(guild) => guild,
        None => {
            eprintln!(
                "discord_usernames_to_ids: Could not find a guild with ID {}",
                crate::discord::sync::ids::GUILD_ID
            );
            return Err(simple_error::SimpleError::new("Guild not found").into());
        }
    };
    let discord_ids = usernames
        .iter()
        .filter_map(|username| match guild.read().member_named(username) {
            Some(member) => Some(member.user.read().id),
            None => {
                eprintln!(
                    "Subscription roles: Could not find a Discord ID for username {}",
                    username
                );
                None
            }
        })
        .collect();
    Ok(discord_ids)
}

async fn add_member_role(
    discord_api: crate::discord::CacheAndHttp,
    user_id: UserId,
    role_id: RoleId,
) -> Result<(), crate::meetup::Error> {
    match tokio::task::spawn_blocking(move || {
        discord_api.http().add_member_role(
            crate::discord::sync::ids::GUILD_ID.0,
            user_id.0,
            role_id.0,
        )
    })
    .await?
    {
        Ok(_) => {
            println!("Assigned user {} to role {}", user_id.0, role_id.0);
        }
        Err(err) => eprintln!(
            "Could not assign user {} to role {}:\n{:#?}",
            user_id.0, role_id.0, err
        ),
    }
    Ok(())
}

async fn remove_member_role(
    discord_api: crate::discord::CacheAndHttp,
    user_id: UserId,
    role_id: RoleId,
) -> Result<(), crate::meetup::Error> {
    match tokio::task::spawn_blocking(move || {
        discord_api.http().remove_member_role(
            crate::discord::sync::ids::GUILD_ID.0,
            user_id.0,
            role_id.0,
        )
    })
    .await?
    {
        Ok(_) => {
            println!("Removed role {} from user {}", role_id.0, user_id.0);
        }
        Err(err) => eprintln!(
            "Could not remove role {} from user {}:\n{:#?}",
            role_id.0, user_id.0, err
        ),
    }
    Ok(())
}
