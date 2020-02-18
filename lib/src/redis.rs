use futures_util::FutureExt;
use redis::{AsyncCommands, Commands};
use std::future::Future;
use std::pin::Pin;

pub fn closure_type_helper<
    T: redis::FromRedisValue + Send + 'static,
    F: for<'c> FnMut(
        &'c mut redis::aio::Connection,
        redis::Pipeline,
    ) -> Pin<Box<dyn Future<Output = redis::RedisResult<Option<T>>> + Send + 'c>>,
>(
    func: F,
) -> F {
    func
}

// A direct translation of redis::transaction for the async case
pub async fn async_redis_transaction<
    K: redis::ToRedisArgs,
    T: redis::FromRedisValue + Send + 'static,
    F: for<'c> FnMut(
        &'c mut redis::aio::Connection,
        redis::Pipeline,
    ) -> Pin<Box<dyn Future<Output = redis::RedisResult<Option<T>>> + Send + 'c>>,
>(
    con: &mut redis::aio::Connection,
    keys: &[K],
    mut func: F,
) -> redis::RedisResult<T> {
    loop {
        let () = redis::cmd("WATCH").arg(keys).query_async(con).await?;
        let mut p = redis::pipe();
        p.atomic();
        let response: Option<T> = func(con, p).await?;
        match response {
            None => continue,
            Some(response) => {
                // make sure no watch is left in the connection, even if
                // someone forgot to use the pipeline.
                let () = redis::cmd("UNWATCH").query_async(con).await?;
                return Ok(response);
            }
        }
    }
}

#[tracing::instrument(skip(con))]
pub async fn delete_event(
    con: &mut redis::aio::Connection,
    event_id: &str,
) -> Result<(), redis::RedisError> {
    // Figure out which series this event belongs to
    let redis_event_series_key = format!("meetup_event:{}:event_series", &event_id);
    let series_id: Option<String> = con.get(&redis_event_series_key).await?;
    let redis_events_key = "meetup_events";
    let redis_event_users_key = format!("meetup_event:{}:meetup_users", &event_id);
    let redis_event_hosts_key = format!("meetup_event:{}:meetup_hosts", &event_id);
    let redis_event_key = format!("meetup_event:{}", event_id);
    let redis_series_events_key =
        series_id.map(|series_id| format!("event_series:{}:meetup_events", series_id));
    let mut keys_to_watch: Vec<&str> = vec![
        &redis_event_series_key,
        redis_events_key,
        &redis_event_users_key,
        &redis_event_hosts_key,
        &redis_event_key,
    ];
    if let Some(key) = &redis_series_events_key {
        keys_to_watch.push(&key);
    }
    let () = async_redis_transaction(con, &keys_to_watch, |con, mut pipe: redis::Pipeline| {
        pipe.del(&redis_event_series_key)
            .del(&redis_event_users_key)
            .del(&redis_event_hosts_key)
            .del(&redis_event_key)
            .srem(redis_events_key, event_id);
        if let Some(redis_series_events_key) = &redis_series_events_key {
            pipe.srem(redis_series_events_key, event_id);
        }
        async move { pipe.query_async(con).await }.boxed()
    })
    .await?;
    Ok(())
}

// Return a list of Meetup IDs of all participants of the specified events.
// If hosts is `false` returns all guests, if `hosts` is true, returns all hosts.
pub fn get_events_participants(
    event_ids: &[&str],
    hosts: bool,
    redis_connection: &mut redis::Connection,
) -> Result<Vec<u64>, crate::meetup::Error> {
    // Find all Meetup users RSVP'd to the specified events
    let redis_event_users_keys: Vec<_> = event_ids
        .iter()
        .map(|event_id| {
            if hosts {
                format!("meetup_event:{}:meetup_hosts", event_id)
            } else {
                format!("meetup_event:{}:meetup_users", event_id)
            }
        })
        .collect();
    let (meetup_user_ids,): (Vec<u64>,) = redis::pipe()
        .sunion(redis_event_users_keys)
        .query(redis_connection)?;
    Ok(meetup_user_ids)
}

// Try to translate Meetup user IDs to Discord user IDs. Returns mappings from
// the Meetup ID to a Discord ID or None if the user is not linked. The order of
// the mapping is the same as the input order.
pub fn meetup_to_discord_ids(
    meetup_user_ids: &[u64],
    redis_connection: &mut redis::Connection,
) -> Result<Vec<(u64, Option<u64>)>, crate::meetup::Error> {
    // Try to associate the RSVP'd Meetup users with Discord users
    let discord_user_ids: Result<Vec<Option<u64>>, _> = meetup_user_ids
        .iter()
        .map(|meetup_id| {
            let redis_meetup_discord_key = format!("meetup_user:{}:discord_user", meetup_id);
            redis_connection.get(&redis_meetup_discord_key)
        })
        .collect();
    Ok(meetup_user_ids
        .iter()
        .cloned()
        .zip(discord_user_ids?.into_iter())
        .collect())
}
