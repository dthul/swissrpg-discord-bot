use command_macro::command;
use redis::Commands;

#[command]
#[regex(r"schedule\s*session")]
#[level(host)]
#[help(
    "schedule session",
    "allows you to schedule a new session for your adventure."
)]
fn schedule_session(
    mut context: super::CommandContext<'_>,
    _: regex::Captures<'_>,
) -> Result<(), lib::meetup::Error> {
    // TODO: make macro check
    // Check whether this is a game channel
    let is_game_channel: bool = context.is_game_channel()?;
    if !is_game_channel {
        let _ = context
            .msg
            .channel_id
            .say(context.ctx, lib::strings::CHANNEL_NOT_BOT_CONTROLLED);
        return Ok(());
    };
    // Find the series belonging to the channel
    let redis_channel_series_key =
        format!("discord_channel:{}:event_series", context.msg.channel_id.0);
    let event_series: String = context.redis_connection()?.get(&redis_channel_series_key)?;
    // Create a new Flow
    let flow = lib::flow::ScheduleSessionFlow::new(context.redis_connection()?, event_series)?;
    let link = format!("{}/schedule_session/{}", lib::urls::BASE_URL, flow.id);
    let _ = context
        .msg
        .author
        .direct_message(context.ctx, |message_builder| {
            message_builder.content(format!(
                "Use the following link to schedule your next session:\n{}",
                link
            ))
        });
    let _ = context.msg.react(context.ctx, "\u{2705}");
    Ok(())
}
