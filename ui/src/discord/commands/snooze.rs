use command_macro::command;
use redis::Commands;

#[command]
#[regex(r"snooze\s+(?P<num_days>[0-9]+)\s*d(ay)?s?")]
#[level(admin)]
fn snooze(
    mut context: super::CommandContext,
    captures: regex::Captures,
) -> Result<(), lib::meetup::Error> {
    let num_days: u32 = captures
        .name("num_days")
        .expect("Regex capture does not contain 'num_days'")
        .as_str()
        .parse()
        .map(|num_days: u32| num_days.min(180))
        .map_err(|_err| simple_error::SimpleError::new("Invalid number of days specified"))?;
    // Check whether this is a game channel
    // TODO: make this a macro
    let is_game_channel: bool = context.is_game_channel()?;
    if !is_game_channel {
        let _ = context
            .msg
            .channel_id
            .say(context.ctx, lib::strings::CHANNEL_NOT_BOT_CONTROLLED);
        return Ok(());
    };
    let redis_channel_snooze_key =
        format!("discord_channel:{}:snooze_until", context.msg.channel_id.0);
    if num_days == 0 {
        // Remove the snooze
        let _: () = context.redis_connection()?.del(&redis_channel_snooze_key)?;
        let _ = context
            .msg
            .channel_id
            .say(context.ctx, "Disabled snoozing.");
    } else {
        let snooze_until = chrono::Utc::now() + chrono::Duration::days(num_days as i64);
        // Set a new snooze date
        let _: () = context
            .redis_connection()?
            .set(&redis_channel_snooze_key, snooze_until.to_rfc3339())?;
        let _ = context
            .msg
            .channel_id
            .say(context.ctx, format!("Snoozing for {} days.", num_days));
    }
    Ok(())
}
