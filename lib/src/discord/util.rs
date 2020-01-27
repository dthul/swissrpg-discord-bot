use redis::AsyncCommands;
use serenity::model::id::ChannelId;

// TODO: introduce newtyoes for series id, event id, ...
pub async fn say_in_event_series_channel(
    series_id: &str,
    message: &str,
    redis_connection: &mut redis::aio::Connection,
    discord_cache_http: &super::CacheAndHttp,
) -> Result<(), crate::BoxedError> {
    // Find the channel for this series id
    let redis_series_channel_key = format!("event_series:{}:discord_channel", series_id);
    let channel_id: u64 = redis_connection.get(&redis_series_channel_key).await?;
    // TODO: blocking
    ChannelId(channel_id).say(&discord_cache_http.http, message)?;
    Ok(())
}

pub async fn say_in_event_channel(
    event_id: &str,
    message: &str,
    redis_connection: &mut redis::aio::Connection,
    discord_cache_http: &super::CacheAndHttp,
) -> Result<(), crate::BoxedError> {
    // First, find the series id for this event
    let redis_event_series_key = format!("meetup_event:{}:event_series", &event_id);
    let series_id: String = redis_connection.get(&redis_event_series_key).await?;
    // Then, find the channel for this series id
    let redis_series_channel_key = format!("event_series:{}:discord_channel", &series_id);
    let channel_id: u64 = redis_connection.get(&redis_series_channel_key).await?;
    // TODO: blocking
    ChannelId(channel_id).say(&discord_cache_http.http, message)?;
    Ok(())
}

pub fn say_in_bot_alerts_channel(
    message: &str,
    discord_cache_http: &super::CacheAndHttp,
) -> Result<(), crate::BoxedError> {
    if let Some(channel_id) = super::sync::ids::BOT_ALERTS_CHANNEL_ID {
        channel_id
            .say(&discord_cache_http.http, message)
            .map(|_| ())
            .map_err(|err| err.into())
    } else {
        Err(simple_error::SimpleError::new(
            "Could not sent a bot alert message, since no bot alerts channel ID is set",
        )
        .into())
    }
}
