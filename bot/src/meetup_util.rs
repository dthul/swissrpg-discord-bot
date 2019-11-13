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
