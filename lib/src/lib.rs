#![forbid(unsafe_code)]
#![warn(rust_2018_idioms)]
pub mod db;
pub mod discord;
pub mod error;
pub mod flow;
mod free_spots;
pub mod meetup;
pub mod redis;
pub mod role_shortcode;
pub mod strings;
pub mod stripe;
pub mod tasks;
pub mod urls;

pub use error::BoxedError;
use rand::Rng;
use serenity::model::id::ChannelId;

pub type BoxedFuture<T> = Box<dyn std::future::Future<Output = T> + Send>;

pub fn new_random_id(num_bytes: u32) -> String {
    let random_bytes: Vec<u8> = (0..num_bytes)
        .map(|_| rand::thread_rng().gen::<u8>())
        .collect();
    base64::encode_config(&random_bytes, base64::URL_SAFE_NO_PAD)
}

pub struct ChannelRoles {
    pub user: u64,
    pub host: Option<u64>,
}

pub async fn get_event_series_roles(
    event_series_id: db::EventSeriesId,
    db_connection: &sqlx::PgPool,
) -> Result<Option<ChannelRoles>, crate::meetup::Error> {
    let row = sqlx::query!(
        "SELECT discord_role_id, discord_host_role_id FROM event_series WHERE id = $1",
        event_series_id.0
    )
    .fetch_one(db_connection)
    .await?;
    match (row.discord_role_id, row.discord_host_role_id) {
        (Some(role), host_role) => Ok(Some(ChannelRoles {
            user: role as u64,
            host: host_role.map(|role| role as u64),
        })),
        (None, None) => Ok(None),
        _ => {
            return Err(simple_error::SimpleError::new("Channel has only host role").into());
        }
    }
}

pub async fn get_channel_roles(
    channel_id: ChannelId,
    db_connection: &sqlx::PgPool,
) -> Result<Option<ChannelRoles>, crate::meetup::Error> {
    // Figure out whether this is a game channel
    let event_series_id = sqlx::query_scalar!(
        "SELECT id FROM event_series WHERE discord_text_channel_id = $1",
        channel_id.0 as i64
    )
    .fetch_optional(db_connection)
    .await?
    .map(|id| db::EventSeriesId(id));
    match event_series_id {
        None => Ok(None),
        Some(event_series_id) => get_event_series_roles(event_series_id, db_connection).await,
    }
}

pub async fn get_series_voice_channel(
    event_series_id: db::EventSeriesId,
    db_connection: &sqlx::PgPool,
) -> Result<Option<ChannelId>, crate::meetup::Error> {
    let discord_voice_channel_id = sqlx::query_scalar!(
        "SELECT discord_voice_channel_id FROM event_series WHERE id = $1",
        event_series_id.0
    )
    .fetch_optional(db_connection)
    .await?;
    Ok(discord_voice_channel_id
        .flatten()
        .map(|id| ChannelId(id as u64)))
}

pub async fn get_channel_voice_channel(
    channel_id: ChannelId,
    db_connection: &sqlx::PgPool,
) -> Result<Option<ChannelId>, crate::meetup::Error> {
    let discord_voice_channel_id = sqlx::query_scalar!(
        "SELECT discord_voice_channel_id FROM event_series WHERE discord_text_channel_id = $1",
        channel_id.0 as i64
    )
    .fetch_optional(db_connection)
    .await?;
    Ok(discord_voice_channel_id
        .flatten()
        .map(|id| ChannelId(id as u64)))
}

pub trait DefaultStr {
    fn unwrap_or_str<'s>(&'s self, default: &'s str) -> &'s str;
}

impl DefaultStr for Option<String> {
    fn unwrap_or_str<'s>(&'s self, default: &'s str) -> &'s str {
        self.as_ref().map(String::as_str).unwrap_or(default)
    }
}
