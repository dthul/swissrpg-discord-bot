use crate::DefaultStr;

use super::free_spots::EventCollector;
use futures_util::lock::Mutex;
use redis::AsyncCommands;
use serenity::model::id::{RoleId, UserId};
use simple_error::SimpleError;
use std::sync::Arc;

impl EventCollector {
    pub async fn assign_roles(
        &self,
        meetup_client: Arc<Mutex<Option<Arc<super::meetup::newapi::AsyncClient>>>>,
        redis_connection: &mut redis::aio::Connection,
        discord_api: &crate::discord::CacheAndHttp,
    ) -> Result<(), crate::meetup::Error> {
        let meetup_client = {
            let guard = meetup_client.lock().await;
            match *guard {
                Some(ref meetup_client) => meetup_client.clone(),
                None => return Err(SimpleError::new("Meetup API unavailable").into()),
            }
            // The Mutex guard will be dropped here
        };
        println!("Role shortcode: Checking {} events", self.events.len());
        for event in &self.events {
            // Check whether this event uses the role shortcode
            let role_captures =
                crate::meetup::sync::ROLE_REGEX.captures_iter(event.description.unwrap_or_str(""));
            let roles = {
                let mut roles = vec![];
                for captures in role_captures {
                    if let Some(role_id) = captures.name("role_id") {
                        match role_id.as_str().parse::<u64>() {
                            Ok(id) => roles.push(RoleId(id)),
                            _ => eprintln!(
                                "Meetup event {} specifies invalid role id {}",
                                event.id,
                                role_id.as_str()
                            ),
                        }
                    }
                }
                roles
            };
            let title = event.title.unwrap_or_str("No title");
            if roles.is_empty() {
                println!("Role shortcode: skipping {}", title);
                continue;
            }
            println!("Role shortcode: event {} has role(s)", title);
            // Some events (games) might already have their RSVPs stored in Redis.
            // For the others we query Meetup.
            let redis_event_key = format!("meetup_event:{}", event.id);
            let event_is_in_redis: bool = redis_connection.exists(&redis_event_key).await?;
            // Meetup user IDs
            let meetup_rsvps: Vec<u64> = if event_is_in_redis {
                // Get the RSVPs from Redis
                println!("Role shortcode: event {} has RSVPs in Redis", title);
                let hosts =
                    crate::redis::get_events_participants(&[&event.id.0], true, redis_connection)
                        .await?;
                let participants =
                    crate::redis::get_events_participants(&[&event.id.0], false, redis_connection)
                        .await?;
                hosts.into_iter().chain(participants.into_iter()).collect()
            } else {
                // Get the RSVPs from Meetup
                println!(
                    "Role shortcode: querying RSVPs for event {} from Meetup",
                    title
                );
                match meetup_client.get_tickets_vec(event.id.0.clone()).await {
                    Err(err) => {
                        eprintln!("Error in assign_roles::get_rsvps:\n{:#?}", err);
                        continue;
                    }
                    Ok(tickets) => tickets.iter().map(|ticket| ticket.user.id.0).collect(),
                }
            };
            // Assign each role to each user
            let discord_user_ids =
                crate::redis::meetup_to_discord_ids(&meetup_rsvps, redis_connection).await?;
            let discord_user_ids: Vec<_> = discord_user_ids
                .into_iter()
                .filter_map(|(_meetup_id, discord_id)| discord_id)
                .map(|id| UserId(id))
                .collect();
            for &user_id in &discord_user_ids {
                let member = match crate::discord::sync::ids::GUILD_ID
                    .member(discord_api, user_id)
                    .await
                {
                    Err(err) => {
                        eprintln!("Could not find Discord member:\n{:#?}", err);
                        continue;
                    }
                    Ok(member) => member,
                };
                for &role_id in &roles {
                    if !member.roles.contains(&role_id) {
                        // Assign the role
                        if let Ok(_) = crate::tasks::subscription_roles::add_member_role(
                            discord_api,
                            user_id,
                            role_id,
                        )
                        .await
                        {
                            let role_text = role_id
                                .to_role_cached(&discord_api.cache)
                                .await
                                .map(|role| format!("**{}**", role.name))
                                .unwrap_or_else(|| format!("<@&{}>", role_id.0));
                            // Let the user know about the new role
                            if let Ok(user) = user_id.to_user(discord_api).await {
                                user.direct_message(discord_api, |m| {
                                    m.content(crate::strings::NEW_ROLE_ASSIGNED_DM(&role_text))
                                })
                                .await
                                .ok();
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
