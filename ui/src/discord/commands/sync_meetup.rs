use command_macro::command;
use futures_util::TryFutureExt;
use std::time::Duration;

#[command]
#[regex(r"sync\s*meetup")]
#[level(admin)]
fn sync_meetup(
    context: super::CommandContext,
    _: regex::Captures,
) -> Result<(), lib::meetup::Error> {
    // Send the syncing future to the executor
    let runtime_lock = context.async_runtime()?.clone();
    let runtime_guard = futures::executor::block_on(runtime_lock.read());
    if let Some(ref async_runtime) = *runtime_guard {
        let redis_client = context.redis_client()?.clone();
        let async_meetup_client = context.meetup_client()?.clone();
        async_runtime.enter(move || {
            let sync_task = {
                let task = async move {
                    let mut redis_connection = redis_client.get_async_connection().await?;
                    lib::meetup::sync::sync_task(async_meetup_client, &mut redis_connection).await
                };
                // Wrap the task in a timeout
                tokio::time::timeout(
                    Duration::from_secs(5 * 60),
                    task.unwrap_or_else(|err| {
                        eprintln!("Syncing task failed: {}", err);
                    }),
                )
                .unwrap_or_else(|err| {
                    eprintln!("Syncing task timed out: {}", err);
                })
            };
            tokio::spawn(sync_task)
        });
        let _ = context.msg.channel_id.say(
            context.ctx,
            "Started asynchronous Meetup synchronization task",
        );
    } else {
        let _ = context.msg.channel_id.say(
            context.ctx,
            "Could not submit asynchronous Meetup synchronization task",
        );
    }
    Ok(())
}
