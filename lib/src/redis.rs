use futures_util::FutureExt;
use rand::Rng;
use redis::{AsyncCommands, Commands};
use std::{future::Future, pin::Pin};

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
pub async fn get_events_participants(
    event_ids: &[&str],
    hosts: bool,
    redis_connection: &mut redis::aio::Connection,
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
        .query_async(redis_connection)
        .await?;
    Ok(meetup_user_ids)
}

// Try to translate Meetup user IDs to Discord user IDs. Returns mappings from
// the Meetup ID to a Discord ID or None if the user is not linked. The order of
// the mapping is the same as the input order.
pub async fn meetup_to_discord_ids(
    meetup_user_ids: &[u64],
    redis_connection: &mut redis::aio::Connection,
) -> Result<Vec<(u64, Option<u64>)>, crate::meetup::Error> {
    // Try to associate the RSVP'd Meetup users with Discord users
    let mut discord_user_ids = Vec::with_capacity(meetup_user_ids.len());
    for meetup_id in meetup_user_ids {
        let redis_meetup_discord_key = format!("meetup_user:{}:discord_user", meetup_id);
        let discord_id: Option<u64> = redis_connection.get(&redis_meetup_discord_key).await?;
        discord_user_ids.push(discord_id);
    }
    Ok(meetup_user_ids
        .iter()
        .cloned()
        .zip(discord_user_ids.into_iter())
        .collect())
}

pub struct Lock<'a> {
    redis_connection: &'a mut redis::Connection,
    lockname: &'a str,
    identifier: u64,
}

// https://redislabs.com/ebook/part-2-core-concepts/chapter-6-application-components-in-redis/6-2-distributed-locking/6-2-5-locks-with-timeouts/
impl<'a> Lock<'a> {
    pub fn acquire_with_timeout(
        redis_connection: &'a mut redis::Connection,
        lockname: &'a str,
        acquire_timeout: std::time::Duration,
        lock_timeout: std::time::Duration,
    ) -> Result<Option<Self>, crate::meetup::Error> {
        let identifier: u64 = rand::thread_rng().gen();
        let start = std::time::Instant::now();
        while start.elapsed() < acquire_timeout {
            let was_set: u8 = redis_connection.set_nx(lockname, identifier)?;
            if was_set == 1 {
                redis_connection.expire(lockname, lock_timeout.as_secs() as usize)?;
                return Ok(Some(Lock {
                    redis_connection,
                    lockname,
                    identifier,
                }));
            } else {
                // We failed to get the lock.
                // Make sure that a timeout is set
                let current_timeout: isize = redis_connection.ttl(lockname)?;
                if current_timeout < 0 {
                    // No timeout set: set it now
                    redis_connection.expire(lockname, lock_timeout.as_secs() as usize)?;
                }
            }
            // Sleep for a moment and re-try
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        Ok(None)
    }

    fn release(&mut self) -> Result<(), crate::meetup::Error> {
        let lockname = self.lockname;
        let identifier = self.identifier;
        let _: () = redis::transaction(self.redis_connection, &[self.lockname], |con, pipe| {
            let current_identifier: u64 = con.get(lockname)?;
            if current_identifier == identifier {
                pipe.del(lockname);
            }
            pipe.query(con)
        })?;
        Ok(())
    }

    pub fn con(&mut self) -> &mut redis::Connection {
        self.redis_connection
    }
}

impl<'a> Drop for Lock<'a> {
    fn drop(&mut self) {
        if let Err(err) = self.release() {
            eprintln!("Error when trying to release Redis lock:\n{:#?}", err);
        }
    }
}

pub struct AsyncLock<'a> {
    redis_connection: &'a mut redis::aio::Connection,
    lockname: &'a str,
    identifier: u64,
}

impl<'a> AsyncLock<'a> {
    pub async fn acquire_with_timeout(
        redis_connection: &'a mut redis::aio::Connection,
        lockname: &'a str,
        acquire_timeout: std::time::Duration,
        lock_timeout: std::time::Duration,
    ) -> Result<Option<AsyncLock<'a>>, crate::meetup::Error> {
        let identifier: u64 = rand::thread_rng().gen();
        let start = std::time::Instant::now();
        while start.elapsed() < acquire_timeout {
            let was_set: u8 = redis_connection.set_nx(lockname, identifier).await?;
            if was_set == 1 {
                redis_connection
                    .expire(lockname, lock_timeout.as_secs() as usize)
                    .await?;
                return Ok(Some(AsyncLock {
                    redis_connection,
                    lockname,
                    identifier,
                }));
            } else {
                // We failed to get the lock.
                // Make sure that a timeout is set
                let current_timeout: isize = redis_connection.ttl(lockname).await?;
                if current_timeout < 0 {
                    // No timeout set: set it now
                    redis_connection
                        .expire(lockname, lock_timeout.as_secs() as usize)
                        .await?;
                }
            }
            // Sleep for a moment and re-try
            tokio::time::delay_for(tokio::time::Duration::from_millis(1)).await;
        }
        Ok(None)
    }

    // There is no AsyncDrop yet, so release is manual
    pub async fn release(self) -> Result<(), crate::meetup::Error> {
        let lockname = self.lockname;
        let identifier = self.identifier;
        let _: () = async_redis_transaction(self.redis_connection, &[lockname], |con, mut pipe| {
            let lockname = lockname.to_string();
            async move {
                let current_identifier: u64 = con.get(&lockname).await?;
                if current_identifier == identifier {
                    pipe.del(&lockname);
                }
                pipe.query_async(con).await
            }
            .boxed()
        })
        .await?;
        Ok(())
    }

    pub fn con(&mut self) -> &mut redis::aio::Connection {
        self.redis_connection
    }
}
