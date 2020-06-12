use command_macro::command;
use futures_util::TryFutureExt;
use std::time::Duration;

#[command]
#[regex(r"sync\s*meetup")]
#[level(admin)]
fn sync_meetup<'a>(
    context: super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    // Send the syncing future to the executor
    let redis_client = context.redis_client().await?.clone();
    let async_meetup_client = context.meetup_client()?.clone();
    let sync_task = {
        let task = async move {
            let mut redis_connection = redis_client.get_async_connection().await?;
            lib::meetup::sync::sync_task(async_meetup_client, &mut redis_connection)
                .await
                .map(|_| ())
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
    tokio::spawn(sync_task);
    context
        .msg
        .channel_id
        .say(
            context.ctx,
            "Started asynchronous Meetup synchronization task",
        )
        .await
        .ok();
    Ok(())
}
