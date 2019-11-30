use futures_util::compat::Future01CompatExt;
use serenity::model::id::ChannelId;

pub async fn say_in_event_series_channel(
    series_id: &str,
    message: &str,
    redis_connection: redis::aio::Connection,
    discord_cache_http: &super::CacheAndHttp,
) -> Result<redis::aio::Connection, crate::BoxedError> {
    // First, find the channel for this series id
    let redis_series_channel_key = format!("event_series:{}:discord_channel", series_id);
    let (redis_connection, channel_id): (_, u64) = redis::cmd("GET")
        .arg(&redis_series_channel_key)
        .query_async(redis_connection)
        .compat()
        .await?;
    ChannelId(channel_id).say(&discord_cache_http.http, message)?;
    Ok(redis_connection)
}
