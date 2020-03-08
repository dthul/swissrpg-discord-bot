use crate::meetup::oauth2::TokenType;
use chrono::Datelike;
use futures_util::lock::Mutex;
use redis::AsyncCommands;
use std::sync::Arc;

// Refreshes the authorization token
pub async fn organizer_token_refresh_task(
    oauth2_consumer: crate::meetup::oauth2::OAuth2Consumer,
    redis_client: redis::Client,
    async_meetup_client: Arc<Mutex<Option<Arc<crate::meetup::api::AsyncClient>>>>,
) -> ! {
    let mut next_refresh_time = chrono::Utc::now();
    if let Ok(mut redis_connection) = redis_client.get_async_connection().await {
        // Check Redis for a refresh time. If there is one, use that
        // if it is in the future. Otherwise schedule the task now
        let refresh_time: redis::RedisResult<Option<String>> = redis_connection
            .get("meetup_access_token_refresh_time")
            .await;
        // Try to get the next scheduled refresh time from Redis, otherwise
        // schedule a refresh immediately
        if let Ok(Some(refresh_time)) = refresh_time {
            if let Ok(refresh_time) = chrono::DateTime::parse_from_rfc3339(&refresh_time) {
                next_refresh_time = refresh_time.into();
            }
        }
    }
    loop {
        println!(
            "Next organizer token refresh @ {}",
            next_refresh_time.to_rfc3339()
        );
        let wait_duration_in_secs = (next_refresh_time - chrono::Utc::now()).num_seconds();
        if wait_duration_in_secs > 0 {
            tokio::time::delay_for(tokio::time::Duration::from_secs(
                wait_duration_in_secs as u64,
            ))
            .await;
        }
        println!("Starting organizer token refresh");
        // Try to refresh the organizer oauth tokens.
        // We spawn this onto a new task, such that when this long-lived refresh task
        // is aborted, the short-lived refresh task still has a chance to run to completion.
        let join_handle = {
            let oauth2_consumer = oauth2_consumer.clone();
            let redis_client = redis_client.clone();
            let async_meetup_client = async_meetup_client.clone();
            tokio::spawn(async move {
                organizer_token_refresh_task_impl(
                    oauth2_consumer,
                    redis_client,
                    async_meetup_client,
                )
                .await
            })
        };
        match join_handle.await {
            Err(err) => {
                eprintln!("Could not refresh the organizer's oauth2 token:\n{}\n", err);
                // Try to refresh again in an hour
                next_refresh_time = chrono::Utc::now() + chrono::Duration::hours(1);
            }
            Ok(Err(err)) => {
                eprintln!("Could not refresh the organizer's oauth2 token:\n{}\n", err);
                // Try to refresh again in an hour
                next_refresh_time = chrono::Utc::now() + chrono::Duration::hours(1);
            }
            Ok(Ok(())) => {
                // Refresh the access token in two days from now
                next_refresh_time = chrono::Utc::now() + chrono::Duration::days(2);
                println!(
                    "Refreshed the organizer's Meetup OAuth token. Next refresh @ {}",
                    next_refresh_time.to_rfc3339()
                );
                // Store refresh date in Redis, ignore failures
                if let Ok(mut redis_connection) = redis_client.get_async_connection().await {
                    let _: redis::RedisResult<()> = redis_connection
                        .set(
                            "meetup_access_token_refresh_time",
                            next_refresh_time.to_rfc3339(),
                        )
                        .await;
                }
            }
        }
    }
}

async fn organizer_token_refresh_task_impl(
    oauth2_consumer: crate::meetup::oauth2::OAuth2Consumer,
    redis_client: redis::Client,
    async_meetup_client: Arc<Mutex<Option<Arc<crate::meetup::api::AsyncClient>>>>,
) -> Result<(), crate::meetup::Error> {
    // Get an async Redis connection
    let mut redis_connection = redis_client.get_async_connection().await?;
    // Try to refresh the organizer oauth tokens
    let new_auth_token = crate::meetup::oauth2::refresh_oauth_tokens(
        TokenType::Organizer,
        &oauth2_consumer.authorization_client,
        &mut redis_connection,
    )
    .await?;
    let mut async_meetup_guard = async_meetup_client.lock().await;
    *async_meetup_guard = Some(Arc::new(crate::meetup::api::AsyncClient::new(
        new_auth_token.secret(),
    )));
    drop(async_meetup_guard);
    Ok(())
}

pub async fn users_token_refresh_task(
    oauth2_consumer: crate::meetup::oauth2::OAuth2Consumer,
    redis_client: redis::Client,
) -> ! {
    loop {
        // Poor man's try block
        let res: Result<_, crate::meetup::Error> = (|| async {
            let mut redis_connection = redis_client.get_async_connection().await?;
            let meetup_user_ids: Vec<u64> = redis_connection.smembers("meetup_users").await?;
            // For each user, check if there is a refresh token.
            // If so, check whether refresh is due.
            println!(
                "Users token refresh task: Checking {} users",
                meetup_user_ids.len()
            );
            for user_id in meetup_user_ids {
                println!("Starting user token refresh for Meetup user {}", user_id);
                // Try to refresh the user's oauth tokens.
                // We spawn this onto a new task, such that when this long-lived refresh task
                // is aborted, the short-lived refresh task still has a chance to run to completion.
                let join_handle = {
                    let oauth2_consumer = oauth2_consumer.clone();
                    let redis_client = redis_client.clone();
                    tokio::spawn(async move {
                        user_token_refresh_task_impl(user_id, oauth2_consumer, redis_client).await
                    })
                };
                match join_handle.await {
                    Err(err) => {
                        eprintln!("Could not refresh the user's oauth2 token:\n{:#?}\n", err);
                    }
                    Ok(Err(err)) => {
                        eprintln!("Could not refresh the user's oauth2 token:\n{:#?}\n", err);
                    }
                    Ok(Ok(())) => {
                        // Nothing to do
                    }
                }
            }
            Ok(())
        })()
        .await;
        if let Err(err) = res {
            eprintln!("Error in users refresh token task:\n{:#?}", err);
        }
        // Wait half a day for the next refresh
        tokio::time::delay_for(tokio::time::Duration::from_secs(12 * 60 * 60)).await;
    }
}

async fn user_token_refresh_task_impl(
    meetup_user_id: u64,
    oauth2_consumer: crate::meetup::oauth2::OAuth2Consumer,
    redis_client: redis::Client,
) -> Result<(), crate::meetup::Error> {
    // Get an async Redis connection
    let mut redis_connection = redis_client.get_async_connection().await?;
    let redis_user_token_key = format!("meetup_user:{}:oauth2_tokens", meetup_user_id);
    let has_oauth2_tokens: bool = redis_connection.exists(&redis_user_token_key).await?;
    if !has_oauth2_tokens {
        // Nothing to do
        return Ok(());
    }
    // Check whether the user's token is due for refresh
    let redis_user_token_refresh_key = format!(
        "meetup_user:{}:oauth2_tokens:last_refresh_time",
        meetup_user_id
    );
    let last_refresh_time: Option<String> =
        redis_connection.get(&redis_user_token_refresh_key).await?;
    let last_refresh_time = last_refresh_time
        .map(|time| chrono::DateTime::parse_from_rfc3339(&time))
        .transpose()
        .unwrap_or(None)
        .map(|time| time.with_timezone(&chrono::Utc));
    if let Some(last_refresh_time) = last_refresh_time {
        // If the last refresh has been more recently than a month, skip it
        if last_refresh_time + chrono::Duration::days(30) > chrono::Utc::now() {
            return Ok(());
        }
    } else {
        // If the token has not been refreshed yet, check whether today is a
        // good day to do so.
        let day_number = chrono::Utc::now().num_days_from_ce();
        if day_number as u64 % 15 != meetup_user_id % 15 {
            // Today is not a good day
            return Ok(());
        }
    }
    // Try to refresh the user's oauth tokens
    println!("Refreshing oauth2 token of Meetup user {}", meetup_user_id);
    let _new_auth_token = crate::meetup::oauth2::refresh_oauth_tokens(
        TokenType::User(meetup_user_id),
        &oauth2_consumer.authorization_client,
        &mut redis_connection,
    )
    .await?;
    println!(
        "OAuth2 tokens of Meetup user {} successfully refreshed",
        meetup_user_id
    );
    // Store the new refresh time in Redis
    let _: () = redis_connection
        .set(
            &redis_user_token_refresh_key,
            chrono::Utc::now().to_rfc3339(),
        )
        .await?;
    Ok(())
}
