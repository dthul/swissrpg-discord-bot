use command_macro::command;

#[command]
#[regex(r"sync\s*discord")]
#[level(admin)]
fn sync_discord(
    context: super::CommandContext<'_>,
    _: regex::Captures<'_>,
) -> Result<(), lib::meetup::Error> {
    let redis_client = context.redis_client()?.clone();
    let task_scheduler_mutex = context.task_scheduler()?;
    // Send the syncing task to the scheduler
    let mut task_scheduler_guard = futures::executor::block_on(task_scheduler_mutex.lock());
    task_scheduler_guard.add_task_datetime(
        white_rabbit::Utc::now(),
        lib::discord::sync::create_sync_discord_task(
            redis_client,
            context.ctx.into(),
            context.bot_id()?.0,
            /*recurring*/ false,
        ),
    );
    drop(task_scheduler_guard);
    let _ = context
        .msg
        .channel_id
        .say(context.ctx, "Started Discord synchronization task");
    Ok(())
}
