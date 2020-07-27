use command_macro::command;
use redis::AsyncCommands;

#[command]
#[regex(r"snooze\s+(?P<num_days>[0-9]+)\s*d(ay)?s?")]
#[level(admin)]
#[help(
    "snooze `X` days",
    "_(in game channel)_ snoozes reminders for _X_ days"
)]
fn snooze<'a>(
    context: &'a mut super::CommandContext,
    captures: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    let num_days: u32 = captures
        .name("num_days")
        .expect("Regex capture does not contain 'num_days'")
        .as_str()
        .parse()
        .map(|num_days: u32| num_days.min(180))
        .map_err(|_err| simple_error::SimpleError::new("Invalid number of days specified"))?;
    // Check whether this is a game channel
    // TODO: make this a macro
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
    let redis_channel_snooze_key =
        format!("discord_channel:{}:snooze_until", context.msg.channel_id.0);
    if num_days == 0 {
        // Remove the snooze
        let _: () = context
            .async_redis_connection()
            .await?
            .del(&redis_channel_snooze_key)
            .await?;
        context
            .msg
            .channel_id
            .say(&context.ctx, "Disabled snoozing.")
            .await
            .ok();
    } else {
        let snooze_until = chrono::Utc::now() + chrono::Duration::days(num_days as i64);
        // Set a new snooze date
        let _: () = context
            .async_redis_connection()
            .await?
            .set(&redis_channel_snooze_key, snooze_until.to_rfc3339())
            .await?;
        context
            .msg
            .channel_id
            .say(&context.ctx, format!("Snoozing for {} days.", num_days))
            .await
            .ok();
    }
    Ok(())
}
