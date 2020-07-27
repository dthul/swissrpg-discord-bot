use command_macro::command;
use redis::AsyncCommands;

#[command]
#[regex(r"schedule\s*session")]
#[level(host)]
#[help(
    "schedule session",
    "allows you to schedule a new session for your adventure."
)]
fn schedule_session<'a>(
    context: &'a mut super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    // TODO: make macro check
    // Check whether this is a game channel
    let is_game_channel: bool = context.is_game_channel().await?;
    if !is_game_channel {
        context
            .msg
            .channel_id
            .say(&context.ctx, lib::strings::CHANNEL_NOT_BOT_CONTROLLED)
            .await
            .ok();
        return Ok(());
    };
    // Find the series belonging to the channel
    let redis_channel_series_key =
        format!("discord_channel:{}:event_series", context.msg.channel_id.0);
    let event_series: String = context
        .async_redis_connection()
        .await?
        .get(&redis_channel_series_key)
        .await?;
    // Create a new Flow
    let flow =
        lib::flow::ScheduleSessionFlow::new(context.async_redis_connection().await?, event_series)
            .await?;
    let link = format!("{}/schedule_session/{}", lib::urls::BASE_URL, flow.id);
    context
        .msg
        .author
        .direct_message(&context.ctx, |message_builder| {
            message_builder.content(format!(
                "Use the following link to schedule your next session:\n{}",
                link
            ))
        })
        .await
        .ok();
    context.msg.react(&context.ctx, '\u{2705}').await.ok();
    Ok(())
}
