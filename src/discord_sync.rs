use redis;
use redis::Commands;
use std::sync::Arc;
use white_rabbit;

// Syncs Discord with the state of the Redis database
pub fn create_sync_discord_task(
    redis_client: redis::Client,
    discord_api: Arc<serenity::CacheAndHttp>,
) -> impl FnMut(&mut white_rabbit::Context) -> white_rabbit::DateResult + Send + Sync + 'static {
    move |ctx| {
        let next_sync_time = match sync_discord_task(&redis_client, &discord_api) {
            Err(err) => {
                eprintln!("Discord syncing task failed: {}", err);
                // Retry in a minute
                white_rabbit::Utc::now() + white_rabbit::Duration::minutes(1)
            }
            _ => {
                // Do another sync in 15 minutes
                white_rabbit::Utc::now() + white_rabbit::Duration::minutes(15)
            }
        };
        white_rabbit::DateResult::Repeat(next_sync_time)
    }
}

fn sync_discord_task(
    redis_client: &redis::Client,
    discord_api: &serenity::CacheAndHttp,
) -> Result<(), crate::BoxedError> {
    let redis_series_key = "event_series";
    let mut con = redis_client.get_connection()?;
    let event_series: Vec<String> = con.get(redis_series_key)?;
    for series in &event_series {}
    Ok(())
}

/*
For each event series:
  - find all enrolled Meetup users
  - map those Meetup users to Discord users if possible
*/
