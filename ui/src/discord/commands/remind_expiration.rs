use command_macro::command;

#[command]
#[regex(r"remind\s*expiration")]
#[level(admin)]
fn remind_expiration(
    context: super::CommandContext<'_>,
    _: regex::Captures<'_>,
) -> Result<(), lib::meetup::Error> {
    let redis_client = context.redis_client()?.clone();
    let task_scheduler_mutex = context.task_scheduler()?;
    // Send the syncing task to the scheduler
    let mut task_scheduler_guard = futures::executor::block_on(task_scheduler_mutex.lock());
    task_scheduler_guard.add_task_datetime(
        white_rabbit::Utc::now(),
        lib::tasks::end_of_game::create_end_of_game_task(
            redis_client,
            context.ctx.into(),
            context.bot_id()?,
            /*recurring*/ false,
        ),
    );
    drop(task_scheduler_guard);
    let _ = context
        .msg
        .channel_id
        .say(context.ctx, "Started expiration reminder task");
    Ok(())
}
