use futures_util::{compat::Future01CompatExt, lock::Mutex};
use oauth2::{
    basic::BasicClient, AsyncRefreshTokenRequest, AuthUrl, ClientId, ClientSecret, RedirectUrl,
    TokenResponse, TokenUrl,
};
use redis::PipelineCommands;
use simple_error::SimpleError;
use std::sync::Arc;

// TODO: move into flow?
pub async fn generate_meetup_linking_link(
    redis_connection: redis::aio::Connection,
    discord_id: u64,
) -> Result<(redis::aio::Connection, String), super::Error> {
    let linking_id = crate::new_random_id(16);
    let redis_key = format!("meetup_linking:{}:discord_user", &linking_id);
    let redis_connection = match redis::cmd("SET")
        .arg(&redis_key)
        .arg(discord_id)
        .query_async(redis_connection)
        .compat()
        .await
    {
        Ok((redis_connection, ())) => redis_connection,
        Err(err) => {
            return Err(SimpleError::new(format!(
                "Redis error when trying to generate Meetup linking link: {}",
                err
            ))
            .into())
        }
    };
    return Ok((
        redis_connection,
        format!("{}/link/{}", crate::urls::BASE_URL, &linking_id),
    ));
}

#[derive(Clone)]
pub struct OAuth2Consumer {
    pub authorization_client: Arc<BasicClient>,
    pub link_client: Arc<BasicClient>,
}

impl OAuth2Consumer {
    pub fn new(
        meetup_client_id: String,
        meetup_client_secret: String,
    ) -> Result<Self, crate::BoxedError> {
        let meetup_client_id = ClientId::new(meetup_client_id);
        let meetup_client_secret = ClientSecret::new(meetup_client_secret);
        let auth_url = AuthUrl::new(crate::urls::MEETUP_OAUTH2_AUTH_URL.to_string())?;
        let token_url = TokenUrl::new(crate::urls::MEETUP_OAUTH2_TOKEN_URL.to_string())?;

        // Set up the config for the Github OAuth2 process.
        let authorization_client = BasicClient::new(
            meetup_client_id,
            Some(meetup_client_secret),
            auth_url,
            Some(token_url),
        )
        .set_auth_type(oauth2::AuthType::RequestBody)
        .set_redirect_url(RedirectUrl::new(format!(
            "{}/authorize/redirect",
            crate::urls::BASE_URL
        ))?);
        let link_client = authorization_client
            .clone()
            .set_redirect_url(RedirectUrl::new(format!(
                "{}/link/redirect",
                crate::urls::BASE_URL
            ))?);
        let authorization_client = Arc::new(authorization_client);
        let link_client = Arc::new(link_client);

        Ok(OAuth2Consumer {
            authorization_client: authorization_client,
            link_client: link_client,
        })
    }

    // TODO: move to tasks
    // Refreshes the authorization token
    pub fn organizer_token_refresh_task(
        &self,
        redis_client: redis::Client,
        async_meetup_client: Arc<Mutex<Option<Arc<super::api::AsyncClient>>>>,
    ) -> impl FnMut(&mut white_rabbit::Context) -> white_rabbit::DateResult + Send + Sync + 'static
    {
        let oauth2_client = self.authorization_client.clone();
        let refresh_meetup_access_token_task =
            move |_context: &mut white_rabbit::Context| -> white_rabbit::DateResult {
                // Try to refresh the organizer oauth tokens
                let refresh_result = crate::ASYNC_RUNTIME.enter(|| {
                    futures::executor::block_on(async {
                        let redis_connection = redis_client.get_async_connection().compat().await?;
                        refresh_oauth_tokens(
                            TokenType::Organizer,
                            oauth2_client.clone(),
                            redis_connection,
                        )
                        .await
                    })
                });
                let (redis_connection, new_access_token) = match refresh_result {
                    Err(err) => {
                        eprintln!("Could not refresh the organizer's oauth2 token:\n{}\n", err);
                        // Try to refresh again in an hour
                        return white_rabbit::DateResult::Repeat(
                            white_rabbit::Utc::now() + white_rabbit::Duration::hours(1),
                        );
                    }
                    Ok(res) => res,
                };
                let mut async_meetup_guard = crate::ASYNC_RUNTIME.enter(|| {
                    futures::executor::block_on(async { async_meetup_client.lock().await })
                });
                *async_meetup_guard = Some(Arc::new(super::api::AsyncClient::new(
                    new_access_token.secret(),
                )));
                drop(async_meetup_guard);
                // Refresh the access token in two days from now
                let next_refresh = white_rabbit::Utc::now() + white_rabbit::Duration::days(2);
                println!(
                    "Refreshed the organizer's Meetup OAuth token. Next refresh @ {}",
                    next_refresh.to_rfc3339()
                );
                // Store refresh date in Redis, ignore failures
                let _: redis::RedisResult<(_, ())> = crate::ASYNC_RUNTIME.enter(|| {
                    futures::executor::block_on(
                        redis::cmd("SET")
                            .arg("meetup_access_token_refresh_time")
                            .arg(next_refresh.to_rfc3339())
                            .query_async(redis_connection)
                            .compat(),
                    )
                });
                // Re-schedule this task
                white_rabbit::DateResult::Repeat(next_refresh)
            };
        refresh_meetup_access_token_task
    }

    pub async fn refresh_oauth_tokens(
        &self,
        token_type: TokenType,
        redis_client: &mut redis::Client,
    ) -> Result<(redis::aio::Connection, oauth2::AccessToken), super::Error> {
        let redis_connection = redis_client.get_async_connection().compat().await?;
        refresh_oauth_tokens(
            token_type,
            self.authorization_client.clone(),
            redis_connection,
        )
        .await
    }
}

pub enum TokenType {
    User(u64),
    Organizer,
}

pub async fn refresh_oauth_tokens(
    token_type: TokenType,
    oauth2_client: Arc<BasicClient>,
    redis_connection: redis::aio::Connection,
) -> Result<(redis::aio::Connection, oauth2::AccessToken), super::Error> {
    // Try to get the refresh token from Redis
    let (redis_connection, refresh_token): (_, Option<String>) = match token_type {
        TokenType::Organizer => {
            redis::cmd("GET")
                .arg("meetup_refresh_token")
                .query_async(redis_connection)
                .compat()
                .await?
        }
        TokenType::User(meetup_user_id) => {
            let redis_user_token_key = format!("meetup_user:{}:oauth2_tokens", meetup_user_id);
            redis::cmd("HGET")
                .arg(&redis_user_token_key)
                .arg("refresh_token")
                .query_async(redis_connection)
                .compat()
                .await?
        }
    };
    let refresh_token: String = match refresh_token {
        Some(refresh_token) => refresh_token,
        None => {
            return Err(SimpleError::new(
                "Could not refresh the Meetup access token since there is no refresh token \
                 available",
            )
            .into())
        }
    };
    // Try to exchange the refresh token for fresh access and refresh tokens
    let refresh_token = oauth2::RefreshToken::new(refresh_token);
    let refresh_token_response = oauth2_client
        .exchange_refresh_token(&refresh_token)
        .request_async(super::oauth2_async_http_client::async_http_client)
        .await?;
    // Store the new tokens in Redis
    let mut pipe = redis::pipe();
    match token_type {
        TokenType::Organizer => {
            pipe.set(
                "meetup_access_token",
                refresh_token_response.access_token().secret(),
            );
            if let Some(new_refresh_token) = refresh_token_response.refresh_token() {
                pipe.set("meetup_refresh_token", new_refresh_token.secret());
            }
        }
        TokenType::User(meetup_user_id) => {
            let redis_user_token_key = format!("meetup_user:{}:oauth2_tokens", meetup_user_id);
            pipe.hset(
                &redis_user_token_key,
                "access_token",
                refresh_token_response.access_token().secret(),
            );
            if let Some(new_refresh_token) = refresh_token_response.refresh_token() {
                pipe.hset(
                    &redis_user_token_key,
                    "refresh_token",
                    new_refresh_token.secret(),
                );
            }
        }
    };
    let (redis_connection, _): (_, ()) = pipe.query_async(redis_connection).compat().await?;
    Ok((
        redis_connection,
        refresh_token_response.access_token().clone(),
    ))
}
