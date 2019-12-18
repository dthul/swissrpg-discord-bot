use futures::future::TryFutureExt;
use futures_util::lock::Mutex;
use serenity::model::id::UserId;
use std::sync::Arc;
use tokio::time::{Duration, Instant};

pub async fn create_recurring_syncing_task(
    redis_client: redis::Client,
    meetup_client: Arc<Mutex<Option<Arc<lib::meetup::api::AsyncClient>>>>,
    discord_api: lib::discord::CacheAndHttp,
    bot_id: UserId,
    task_scheduler: Arc<Mutex<white_rabbit::Scheduler>>,
) {
    let mut interval_timer = tokio::time::interval_at(
        Instant::now() + Duration::from_secs(15 * 60),
        Duration::from_secs(15 * 60),
    );
    // Run forever
    loop {
        // Wait for the next interval tick
        interval_timer.tick().await;
        let redis_client = redis_client.clone();
        let discord_api = discord_api.clone();
        let meetup_client = meetup_client.clone();
        let task_scheduler = task_scheduler.clone();
        lib::ASYNC_RUNTIME.spawn(async move {
            let sync_result = tokio::time::timeout(
                Duration::from_secs(360),
                lib::meetup::sync::sync_task(meetup_client, redis_client.clone()).map_err(|err| {
                    eprintln!("Syncing task failed: {}", err);
                    err
                }),
            )
            .map_err(|err| {
                eprintln!("Syncing task timed out: {}", err);
            })
            .await;
            if let Ok(Ok(_)) = sync_result {
                // Send the Discord syncing task to the scheduler
                let mut guard = task_scheduler.lock().await;
                guard.add_task_datetime(
                    white_rabbit::Utc::now(),
                    lib::discord::sync::create_sync_discord_task(
                        redis_client,
                        discord_api,
                        bot_id.0,
                        /*recurring*/ false,
                    ),
                );
            }
        });
    }
}
