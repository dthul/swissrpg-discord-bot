use futures::future::TryFutureExt;
use futures_util::lock::Mutex;
use serenity::model::id::UserId;
use std::sync::Arc;
use tokio::time::{Duration, Instant};

pub async fn create_recurring_syncing_task(
    redis_client: redis::Client,
    meetup_client: Arc<Mutex<Option<Arc<crate::meetup::api::AsyncClient>>>>,
    discord_api: crate::discord::CacheAndHttp,
    bot_id: UserId,
    task_scheduler: Arc<Mutex<white_rabbit::Scheduler>>,
    static_file_prefix: &'static str,
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
        tokio::spawn(async move {
            let mut redis_connection = redis_client.get_async_connection().await?;
            let event_collector = tokio::time::timeout(
                Duration::from_secs(360),
                crate::meetup::sync::sync_task(meetup_client, &mut redis_connection).map_err(
                    |err| {
                        eprintln!("Syncing task failed: {}", err);
                        err
                    },
                ),
            )
            .map_err(|err| {
                eprintln!("Syncing task timed out: {}", err);
                err
            })
            .await??;
            // Send the Discord syncing task to the scheduler
            let mut guard = task_scheduler.lock().await;
            guard.add_task_datetime(
                white_rabbit::Utc::now(),
                crate::discord::sync::create_sync_discord_task(
                    redis_client,
                    discord_api.clone(),
                    bot_id.0,
                    /*recurring*/ false,
                ),
            );
            drop(guard);
            // Finally, update Discord with the information on open spots.
            if let Some(channel_id) = crate::discord::sync::ids::FREE_SPOTS_CHANNEL_ID {
                if let Err(err) =
                    event_collector.update_channel(&discord_api, channel_id, static_file_prefix)
                {
                    eprintln!("Error when posting open game spots:\n{:#?}", err);
                }
            } else {
                eprintln!("No channel configured for posting open game spots");
            }
            Ok::<_, crate::BoxedError>(())
        });
    }
}
