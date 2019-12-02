use futures_util::compat::Future01CompatExt;
use redis::PipelineCommands;
use std::future::Future;

// A direct translation of redis::transaction for the async case
pub async fn async_redis_transaction<
    K: redis::ToRedisArgs,
    T: redis::FromRedisValue + Send + 'static,
    R: Future<Output = Result<(redis::aio::Connection, Option<T>), redis::RedisError>>,
    F: FnMut(redis::aio::Connection, redis::Pipeline) -> R,
>(
    mut con: redis::aio::Connection,
    keys: &[K],
    mut func: F,
) -> Result<(redis::aio::Connection, T), redis::RedisError> {
    loop {
        let (newcon, ()) = redis::cmd("WATCH")
            .arg(keys)
            .query_async(con)
            .compat()
            .await?;
        con = newcon;
        let mut p = redis::pipe();
        p.atomic();
        let (newcon, response): (_, Option<T>) = func(con, p).await?;
        con = newcon;
        match response {
            None => continue,
            Some(response) => {
                // make sure no watch is left in the connection, even if
                // someone forgot to use the pipeline.
                let (con, ()) = redis::cmd("UNWATCH").query_async(con).compat().await?;
                return Ok((con, response));
            }
        }
    }
}

pub async fn delete_event(
    con: redis::aio::Connection,
    event_id: &str,
) -> Result<(), redis::RedisError> {
    // Figure out which series this event belongs to
    let redis_event_series_key = format!("meetup_event:{}:event_series", &event_id);
    let (con, series_id): (_, Option<String>) = redis::cmd("GET")
        .arg(&redis_event_series_key)
        .query_async(con)
        .compat()
        .await?;
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
    let transaction_fn = |con, mut pipe: redis::Pipeline| {
        pipe.del(&redis_event_series_key)
            .del(&redis_event_users_key)
            .del(&redis_event_hosts_key)
            .del(&redis_event_key)
            .srem(redis_events_key, event_id);
        if let Some(redis_series_events_key) = &redis_series_events_key {
            pipe.srem(redis_series_events_key, event_id);
        }
        async { pipe.query_async(con).compat().await }
    };
    let (_, ()) = async_redis_transaction(con, &keys_to_watch, transaction_fn).await?;
    Ok(())
}
