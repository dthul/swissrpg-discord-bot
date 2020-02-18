use futures::future::TryFutureExt;
use futures_util::lock::Mutex;
use serenity::model::id::UserId;
use std::sync::Arc;
use tokio::time::{Duration, Instant};
use tracing_futures::Instrument;

#[tracing::instrument(
    target = "Syncing Task",
    skip(redis_client, meetup_client, discord_api, bot_id, task_scheduler)
)]
pub async fn create_recurring_syncing_task(
    redis_client: redis::Client,
    meetup_client: Arc<Mutex<Option<Arc<lib::meetup::api::AsyncClient>>>>,
    discord_api: lib::discord::CacheAndHttp,
    bot_id: UserId,
    task_scheduler: Arc<Mutex<white_rabbit::Scheduler>>,
) -> ! {
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
        lib::ASYNC_RUNTIME.spawn(
            async move {
                tracing::debug!("Syncing task started");
                let mut redis_connection = redis_client.get_async_connection().await?;
                tokio::time::timeout(
                    Duration::from_secs(360),
                    lib::meetup::sync::sync_task(meetup_client, &mut redis_connection).map_err(
                        |err| {
                            tracing::warn!("Syncing task failed: {:#?}", err);
                            err
                        },
                    ),
                )
                .map_err(|err| {
                    tracing::warn!("Syncing task timed out: {:#?}", err);
                    err
                })
                .await??;
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
                Ok::<_, lib::BoxedError>(())
            }
            .instrument(tracing::info_span!("Syncing Task")),
        );
    }
}
