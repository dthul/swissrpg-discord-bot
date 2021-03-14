use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenResponse, TokenUrl,
};
use redis::AsyncCommands;
use simple_error::SimpleError;
use std::sync::Arc;

// TODO: move into flow?
pub async fn generate_meetup_linking_link(
    redis_connection: &mut redis::aio::Connection,
    discord_id: u64,
) -> Result<String, super::Error> {
    let linking_id = crate::new_random_id(16);
    let redis_key = format!("meetup_linking:{}:discord_user", &linking_id);
    if let Err::<(), _>(err) = redis_connection.set(&redis_key, discord_id).await {
        return Err(SimpleError::new(format!(
            "Redis error when trying to generate Meetup linking link:\n{:#?}",
            err
        ))
        .into());
    }
    return Ok(format!("{}/link/{}", crate::urls::BASE_URL, &linking_id));
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
        .set_redirect_uri(RedirectUrl::new(format!(
            "{}/authorize/redirect",
            crate::urls::BASE_URL
        ))?);
        let link_client = authorization_client
            .clone()
            .set_redirect_uri(RedirectUrl::new(format!(
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

    pub async fn refresh_oauth_tokens(
        &self,
        token_type: TokenType,
        redis_connection: &mut redis::aio::Connection,
    ) -> Result<oauth2::AccessToken, super::Error> {
        refresh_oauth_tokens(token_type, &self.authorization_client, redis_connection).await
    }
}

pub enum TokenType {
    User(u64),
    Organizer,
}

// Wrapper around the actual implementation that takes care of locking.
// This is necessary since there is no AsyncDrop yet that could release
// the lock RAII style
pub async fn refresh_oauth_tokens(
    token_type: TokenType,
    oauth2_client: &BasicClient,
    redis_connection: &mut redis::aio::Connection,
) -> Result<oauth2::AccessToken, super::Error> {
    // Lock the oauth2 tokens
    let lockname = match token_type {
        TokenType::Organizer => "lock:meetup_refresh_token".to_string(),
        TokenType::User(meetup_user_id) => {
            format!("lock:meetup_user:{}:oauth2_tokens", meetup_user_id)
        }
    };
    let token_lock = crate::redis::AsyncLock::acquire_with_timeout(
        redis_connection,
        &lockname,
        std::time::Duration::from_secs(5),
        std::time::Duration::from_secs(20),
    )
    .await?;
    match token_lock {
        None => Err(SimpleError::new("Could not acquire token lock").into()),
        Some(mut token_lock) => {
            let res = refresh_oauth_tokens_impl(token_type, oauth2_client, token_lock.con()).await;
            // Release the lock in any case
            let _ = token_lock.release().await;
            res
        }
    }
}

async fn refresh_oauth_tokens_impl(
    token_type: TokenType,
    oauth2_client: &BasicClient,
    redis_connection: &mut redis::aio::Connection,
) -> Result<oauth2::AccessToken, super::Error> {
    // Try to get the refresh token from Redis
    let refresh_token: Option<String> = match token_type {
        TokenType::Organizer => redis_connection.get("meetup_refresh_token").await?,
        TokenType::User(meetup_user_id) => {
            let redis_user_token_key = format!("meetup_user:{}:oauth2_tokens", meetup_user_id);
            redis_connection
                .hget(&redis_user_token_key, "refresh_token")
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
        .request_async(oauth2::reqwest::async_http_client)
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
    let _: () = pipe.query_async(redis_connection).await?;
    Ok(refresh_token_response.access_token().clone())
}
