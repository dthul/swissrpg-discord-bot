use crate::meetup_api;
use crate::meetup_sync;
use futures::future;
use futures::future::Future;
use futures::stream::Stream;
use serenity::model::id::UserId;
use serenity::prelude::{Mutex, RwLock};
use std::sync::Arc;
use std::time::Duration;
use tokio::prelude::*;

pub fn create_recurring_syncing_task(
    redis_client: redis::Client,
    meetup_client: Arc<RwLock<Option<meetup_api::AsyncClient>>>,
    discord_api: crate::discord_bot::CacheAndHttp,
    bot_id: UserId,
    task_scheduler: Arc<Mutex<white_rabbit::Scheduler>>,
) -> impl Future<Item = (), Error = crate::BoxedError> {
    // Run forever
    tokio::timer::Interval::new_interval(Duration::from_secs(15 * 60))
        .map_err(|err| {
            eprintln!("Interval timer error: {}", err);
            err.into()
        })
        .for_each(move |_| {
            let redis_client = redis_client.clone();
            let discord_api = discord_api.clone();
            let task_scheduler = task_scheduler.clone();
            tokio::spawn(
                meetup_sync::sync_task(meetup_client.clone(), redis_client.clone())
                    .map_err(|err| {
                        eprintln!("Syncing task failed: {}", err);
                        err
                    })
                    .timeout(Duration::from_secs(360))
                    .map_err(|err| {
                        eprintln!("Syncing task timed out: {}", err);
                    })
                    .then(move |_res| {
                        // Send the Discord syncing task to the scheduler
                        task_scheduler.lock().add_task_datetime(
                            white_rabbit::Utc::now(),
                            crate::discord_sync::create_sync_discord_task(
                                redis_client,
                                discord_api,
                                bot_id.0,
                                /*recurring*/ false,
                            ),
                        );
                        future::ok(())
                    }),
            );
            future::ok(())
        })
}
