use command_macro::command;

#[command]
#[regex(r"remind\s*expiration")]
#[level(admin)]
fn remind_expiration<'a>(
    context: super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    let redis_client = context.redis_client().await?.clone();
    let bot_id = context.bot_id()?;
    let discord_api: lib::discord::CacheAndHttp = (&context.ctx).into();
    // Spawn the syncing task
    tokio::spawn(async move {
        let redis_connection = redis_client.get_async_connection().await?;
        lib::tasks::end_of_game::end_of_game_task(&mut redis_connection, &mut discord_api, bot_id.0)
            .await
    });
    context
        .msg
        .channel_id
        .say(context.ctx, "Started expiration reminder task")
        .await
        .ok();
    Ok(())
}
