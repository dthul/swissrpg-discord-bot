use futures_util::compat::Future01CompatExt;
use std::{
    future::Future,
    io::{self, Write},
    sync::Arc,
};

async fn try_with_token_refresh<
    T,
    Ret: Future<Output = Result<T, crate::meetup_api::Error>>,
    F: Fn(crate::meetup_api::AsyncClient) -> Ret,
>(
    f: F,
    user_id: u64,
    redis_client: redis::Client,
    oauth2_consumer: Arc<crate::meetup_oauth2::OAuth2Consumer>,
) -> crate::Result<T> {
    let redis_connection = redis_client.get_async_connection().compat().await?;
    // Look up the Meetup access token for this user
    println!("Looking up the oauth access token");
    io::stdout().flush().unwrap();
    let redis_meetup_user_oauth_tokens_key = format!("meetup_user:{}:oauth2_tokens", user_id);
    let (_redis_connection, access_token): (_, Option<String>) = redis::cmd("HGET")
        .arg(&redis_meetup_user_oauth_tokens_key)
        .arg("access_token")
        .query_async(redis_connection)
        .compat()
        .await?;
    let access_token = match access_token {
        Some(access_token) => access_token,
        None => {
            // There is no access token: try to obtain a new one
            println!("No access token, calling oauth2_consumer.refresh_oauth_tokens");
            io::stdout().flush().unwrap();
            let (_, access_token) = oauth2_consumer
                .refresh_oauth_tokens(
                    crate::meetup_oauth2::TokenType::User(user_id),
                    redis_client.clone(),
                )
                .await?;
            println!("Got an access token!");
            io::stdout().flush().unwrap();
            access_token.secret().clone()
        }
    };
    // Run the provided function
    let meetup_api_user_client = crate::meetup_api::AsyncClient::new(&access_token);
    println!("Running the provided funtion");
    io::stdout().flush().unwrap();
    match f(meetup_api_user_client).await {
        Err(err) => {
            println!("Got an error");
            io::stdout().flush().unwrap();
            if let crate::meetup_api::Error::AuthenticationFailure = err {
                // The request seems to have failed due to invalid credentials.
                // Try to obtain a new access token and re-run the provided function.
                println!("Calling oauth2_consumer.refresh_oauth_tokens");
                io::stdout().flush().unwrap();
                let (_, access_token) = oauth2_consumer
                    .refresh_oauth_tokens(
                        crate::meetup_oauth2::TokenType::User(user_id),
                        redis_client,
                    )
                    .await?;
                println!("Got an access token!");
                io::stdout().flush().unwrap();
                // Re-run the provided function one more time
                let meetup_api_user_client =
                    crate::meetup_api::AsyncClient::new(access_token.secret());
                f(meetup_api_user_client).await.map_err(Into::into)
            } else {
                eprintln!("Meetup API error: {:#?}", err);
                io::stdout().flush().unwrap();
                return Err(err.into());
            }
        }
        Ok(t) => {
            println!("Everything is fine!");
            io::stdout().flush().unwrap();
            Ok(t)
        }
    }
}

pub async fn rsvp_user_to_event(
    user_id: u64,
    urlname: &str,
    event_id: &str,
    redis_client: redis::Client,
    oauth2_consumer: Arc<crate::meetup_oauth2::OAuth2Consumer>,
) -> crate::Result<crate::meetup_api::RSVP> {
    let rsvp_fun = |async_meetup_user_client: crate::meetup_api::AsyncClient| {
        async move { async_meetup_user_client.rsvp(urlname, event_id, true).await }
    };
    try_with_token_refresh(rsvp_fun, user_id, redis_client, oauth2_consumer).await
}

pub async fn clone_event(
    urlname: &str,
    event_id: &str,
    meetup_client: crate::meetup_api::AsyncClient,
) -> crate::Result<crate::meetup_api::Event> {
    let event = meetup_client.get_event(urlname, event_id).await?;
    let event = match event {
        Some(event) => event,
        None => {
            return Err(simple_error::SimpleError::new(format!(
                "Specified event ({}/{}) was not found",
                urlname, event_id
            ))
            .into())
        }
    };
    let new_event = crate::meetup_api::NewEvent {
        description: event.simple_html_description.unwrap_or(event.description),
        duration_ms: event.duration_ms,
        featured_photo_id: event.featured_photo.map(|p| p.id),
        hosts: event.event_hosts.iter().map(|host| host.id).collect(),
        how_to_find_us: event.how_to_find_us,
        name: event.name,
        rsvp_limit: event.rsvp_limit,
        guest_limit: event.rsvp_rules.map(|r| r.guest_limit),
        time: event.time,
        venue_id: event
            .venue
            .map(|v| v.id)
            .ok_or(simple_error::SimpleError::new(
                "Cannot clone an event that doesn't have a venue",
            ))?,
    };
    let new_event = meetup_client.create_event(urlname, new_event).await?;
    return Ok(new_event);
}

pub async fn clone_rsvps(
    urlname: &str,
    src_event_id: &str,
    dst_event_id: &str,
    redis_client: redis::Client,
    meetup_client: crate::meetup_api::AsyncClient,
    oauth2_consumer: Arc<crate::meetup_oauth2::OAuth2Consumer>,
) -> crate::Result<()> {
    // First, query the source event's RSVPs
    let rsvps = meetup_client.get_rsvps(urlname, src_event_id).await?;
    // Now, try to RSVP each user to the destination event
    let mut last_err = None;
    let mut num_success: u16 = 0;
    for rsvp in &rsvps {
        if let Err(err) = rsvp_user_to_event(
            rsvp.member.id,
            urlname,
            dst_event_id,
            redis_client.clone(),
            oauth2_consumer.clone(),
        )
        .await
        {
            eprintln!(
                "Could not RSVP user {} to event {}:\n{:#?}",
                rsvp.member.id, dst_event_id, err
            );
            last_err = Some(err);
        } else {
            num_success += 1;
        }
    }
    if let Some(err) = last_err {
        return Err(simple_error::SimpleError::new(format!(
            "Could not RSVP all users. {} out of {} were successfully RSVPd. Last error:\n{:#?}",
            num_success,
            rsvps.len(),
            err,
        ))
        .into());
    }
    Ok(())
}
