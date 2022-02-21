use std::{ops::Deref, sync::Arc};

use axum::{
    async_trait,
    extract::{Extension, FromRequest, RequestParts, TypedHeader},
    headers::Header,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use lazy_static::lazy_static;

use super::{server::State, WebError};

pub fn create_routes() -> Router {
    Router::new().route(
        "/check_discord_username",
        get(check_discord_username_handler),
    )
}

struct ApiKeyHeader(String);

lazy_static! {
    static ref API_KEY_HEADER: axum::headers::HeaderName =
        axum::headers::HeaderName::from_lowercase(b"api-key").unwrap();
}

impl Deref for ApiKeyHeader {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Header for ApiKeyHeader {
    fn name() -> &'static axum::headers::HeaderName {
        &API_KEY_HEADER
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, axum::headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i axum::headers::HeaderValue>,
    {
        let value = values.next().ok_or_else(axum::headers::Error::invalid)?;
        let value = value
            .to_str()
            .map_err(|_| axum::headers::Error::invalid())?;
        Ok(ApiKeyHeader(value.into()))
    }

    fn encode<E: Extend<axum::headers::HeaderValue>>(&self, values: &mut E) {
        match axum::headers::HeaderValue::from_str(&self.0) {
            Ok(header_value) => values.extend(Some(header_value)),
            Err(err) => eprintln!("Failed to encode Api-Key HTTP header: {:#?}", err),
        }
    }
}

struct DiscordUsernameHeader(String);

lazy_static! {
    static ref DISCORD_USERNAME_HEADER: axum::headers::HeaderName =
        axum::headers::HeaderName::from_lowercase(b"discord-username").unwrap();
}

impl Deref for DiscordUsernameHeader {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Header for DiscordUsernameHeader {
    fn name() -> &'static axum::headers::HeaderName {
        &DISCORD_USERNAME_HEADER
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, axum::headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i axum::headers::HeaderValue>,
    {
        let value = values.next().ok_or_else(axum::headers::Error::invalid)?;
        let value = value
            .to_str()
            .map_err(|_| axum::headers::Error::invalid())?;
        Ok(DiscordUsernameHeader(value.into()))
    }

    fn encode<E: Extend<axum::headers::HeaderValue>>(&self, values: &mut E) {
        match axum::headers::HeaderValue::from_str(&self.0) {
            Ok(header_value) => values.extend(Some(header_value)),
            Err(err) => eprintln!("Failed to encode Discord-Username HTTP header: {:#?}", err),
        }
    }
}

struct ApiKeyIsValid;

#[async_trait]
impl<B> FromRequest<B> for ApiKeyIsValid
where
    B: Send,
{
    type Rejection = Response;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let TypedHeader(api_key) = TypedHeader::<ApiKeyHeader>::from_request(req)
            .await
            .map_err(|err| err.into_response())?;
        let Extension(state): Extension<Arc<State>> = Extension::from_request(req)
            .await
            .map_err(|err| err.into_response())?;
        match &state.api_key {
            None => Err(StatusCode::INTERNAL_SERVER_ERROR.into_response()),
            Some(key) if *key == api_key.0 => Ok(ApiKeyIsValid),
            Some(_) => Err(StatusCode::UNAUTHORIZED.into_response()),
        }
    }
}

async fn check_discord_username_handler(
    _: ApiKeyIsValid,
    TypedHeader(discord_username): TypedHeader<DiscordUsernameHeader>,
    Extension(state): Extension<Arc<State>>,
) -> Result<StatusCode, WebError> {
    let id = lib::tasks::subscription_roles::discord_username_to_id(
        &state.discord_cache_http,
        &discord_username.0,
    )
    .await?;
    if id.is_none() {
        // The username seems to be invalid, return a 204 HTTP code
        Ok(StatusCode::NO_CONTENT)
    } else {
        // The username could be matched to an ID, return a 200 HTTP code
        Ok(StatusCode::OK)
    }
}
