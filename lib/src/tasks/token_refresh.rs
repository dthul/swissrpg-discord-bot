use crate::meetup::oauth2::TokenType;
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
    // Try to refresh the organizer oauth tokens
    // Get an async Redis connection
    let mut redis_connection = redis_client.get_async_connection().await?;
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
