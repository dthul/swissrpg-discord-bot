use futures_util::compat::Future01CompatExt;
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
