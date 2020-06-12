use command_macro::command;

#[command]
#[regex(r"sync\s*discord")]
#[level(admin)]
fn sync_discord<'a>(
    context: super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    let redis_connection = context.redis_client().await?.get_async_connection().await?;
    let discord_api = (&context.ctx).into();
    let bot_id = context.bot_id()?;
    // Spawn the syncing task
    tokio::spawn(async move {
        lib::discord::sync::sync_discord(&mut redis_connection, &mut discord_api, bot_id.0).await
    });
    context
        .msg
        .channel_id
        .say(context.ctx, "Started Discord synchronization task")
        .await
        .ok();
    Ok(())
}
