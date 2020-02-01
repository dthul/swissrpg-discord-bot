use std::sync::Arc;
use warp::Filter;

pub fn create_routes(
    discord_cache_http: lib::discord::CacheAndHttp,
    stripe_client: Arc<stripe::Client>,
    stripe_webhook_secret: String,
) -> impl warp::Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    let get_route = {
        warp::post()
            .and(warp::path!("webhooks" / "stripe"))
            .and(warp::header::<String>("Stripe-Signature"))
            .and(warp::body::content_length_limit(1024 * 32))
            .and(warp::body::bytes())
            .and_then(move |signature: String, payload: bytes::Bytes| {
                println!("Webhook!");
                let stripe_webhook_secret = stripe_webhook_secret.clone();
                let discord_cache_http = discord_cache_http.clone();
                let stripe_client = stripe_client.clone();
                let webhook_handler_future = async move {
                    if let Ok(payload) = std::str::from_utf8(&payload) {
                        if let Ok(event) = stripe::Webhook::construct_event(
                            &payload,
                            &signature,
                            &stripe_webhook_secret,
                        ) {
                            eprintln!("Webhook success! Event:\n{:#?}", event);
                            if event.event_type == stripe::EventType::CustomerSubscriptionCreated {
                                if let stripe::EventObject::Subscription(subscription) =
                                    event.data.object
                                {
                                    if let Err(err) = handle_new_subscription(
                                        &discord_cache_http,
                                        &stripe_client,
                                        &subscription,
                                    )
                                    .await
                                    {
                                        eprintln!(
                                            "Could not handle new subscription event:\n{:#?}",
                                            err
                                        );
                                    }
                                }
                            }
                        } else {
                            eprintln!("Event construction failed");
                        }
                    } else {
                        eprintln!("Payload to UTF8 conversion failed");
                    }
                };
                tokio::spawn(webhook_handler_future);
                async { Ok::<_, warp::Rejection>("Webhook!") }
            })
    };
    get_route
}

async fn handle_new_subscription(
    discord_api: &lib::discord::CacheAndHttp,
    stripe_client: &stripe::Client,
    subscription: &stripe::Subscription,
) -> Result<&'static str, lib::meetup::Error> {
    // let customer = match &subscription.customer {
    //     stripe::Expandable::Object(customer) => *customer.clone(),
    //     stripe::Expandable::Id(customer_id) => {
    //         let customer = stripe::Customer::retrieve(stripe_client, customer_id, &[]).await?;
    //         customer
    //     }
    // };
    let (customer, product) =
        lib::tasks::subscription_roles::get_customer_and_product(stripe_client, subscription)
            .await?;
    println!(
        "Received new subscription with product '{:#?}'",
        product.name
    );
    // Wait for a few seconds and re-query the customer object from Stripe,
    // since the webhook is sometimes faster than the customer metadata is updated
    tokio::time::delay_for(tokio::time::Duration::from_secs(20)).await;
    let customer = stripe::Customer::retrieve(stripe_client, &customer.id, &[]).await?;
    if let Some(username) = customer.metadata.get("Discord") {
        // Try to find the Discord user associated with this subscription
        let ids = lib::tasks::subscription_roles::discord_usernames_to_ids(
            discord_api,
            &[username.clone()],
        )?;
        if let Some(discord_id) = ids.first() {
            // TODO: might block
            let discord_user = discord_id.to_user(discord_api)?;
            let is_champion_product = product.name.as_ref().map_or(false, |name| {
                lib::tasks::subscription_roles::CHAMPION_PRODUCT_REGEX.is_match(name)
            });
            let is_insider_product = product.name.as_ref().map_or(false, |name| {
                lib::tasks::subscription_roles::INSIDER_PRODUCT_REGEX.is_match(name)
            });
            if is_champion_product {
                if let Ok(true) = discord_user.has_role(
                    discord_api,
                    lib::discord::sync::ids::GUILD_ID,
                    lib::discord::sync::ids::GAME_MASTER_ID,
                ) {
                    println!("Adding GM Champion role");
                    lib::tasks::subscription_roles::add_member_role(
                        discord_api.clone(),
                        *discord_id,
                        lib::tasks::subscription_roles::ids::GM_CHAMPION_ID,
                    )
                    .await?;
                } else {
                    println!("Adding Champion role");
                    lib::tasks::subscription_roles::add_member_role(
                        discord_api.clone(),
                        *discord_id,
                        lib::tasks::subscription_roles::ids::CHAMPION_ID,
                    )
                    .await?;
                }
            }
            if is_insider_product {
                println!("Adding Insider role");
                lib::tasks::subscription_roles::add_member_role(
                    discord_api.clone(),
                    *discord_id,
                    lib::tasks::subscription_roles::ids::INSIDER_ID,
                )
                .await?;
            }
        } else {
            eprintln!(
                "Could not match the Discord username '{}' to an actual Discord user",
                username
            );
        }
    } else {
        eprintln!("Found no 'Discord' field on the customer metadata object");
    }
    Ok("")
}
