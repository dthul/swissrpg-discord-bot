use command_macro::command;
use redis::AsyncCommands;

#[command]
#[regex(r"end\s*adventure")]
#[level(host)]
#[help(
    "end adventure",
    "sets the channel for closure at the end of an adventure. The channel won't be deleted immediately but within 24 hours."
)]
fn end_adventure<'a>(
    context: &'a mut super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    // TODO: make this a macro check
    // Check whether this is a game channel
    let is_game_channel = context.is_game_channel().await?;
    if !is_game_channel {
        context
            .msg
            .channel_id
            .say(&context.ctx, lib::strings::CHANNEL_NOT_BOT_CONTROLLED)
            .await
            .ok();
        return Ok(());
    };
    // Figure out whether there is an associated voice channel
    let voice_channel_id = lib::get_channel_voice_channel(
        context.msg.channel_id,
        context.async_redis_connection().await?,
    )
    .await?;
    // Check if there is a channel expiration time in the future
    let redis_channel_expiration_key = format!(
        "discord_channel:{}:expiration_time",
        context.msg.channel_id.0
    );
    let expiration_time: Option<String> = context
        .async_redis_connection()
        .await?
        .get(&redis_channel_expiration_key)
        .await?;
    let expiration_time = expiration_time
        .map(|t| chrono::DateTime::parse_from_rfc3339(&t))
        .transpose()?
        .map(|t| t.with_timezone(&chrono::Utc));
    let expiration_time = if let Some(expiration_time) = expiration_time {
        expiration_time
    } else {
        context
            .msg
            .channel_id
            .say(&context.ctx, lib::strings::CHANNEL_NO_EXPIRATION)
            .await
            .ok();
        return Ok(());
    };
    if expiration_time > chrono::Utc::now() {
        context
            .msg
            .channel_id
            .say(&context.ctx, lib::strings::CHANNEL_NOT_YET_CLOSEABLE)
            .await
            .ok();
        return Ok(());
    }
    // Schedule this channel for deletion
    let new_deletion_time = chrono::Utc::now() + chrono::Duration::hours(8);
    let redis_channel_deletion_key =
        format!("discord_channel:{}:deletion_time", context.msg.channel_id.0);
    let current_deletion_time: Option<String> = context
        .async_redis_connection()
        .await?
        .get(&redis_channel_deletion_key)
        .await?;
    let current_deletion_time = current_deletion_time
        .map(|t| chrono::DateTime::parse_from_rfc3339(&t))
        .transpose()?
        .map(|t| t.with_timezone(&chrono::Utc));
    if let Some(current_deletion_time) = current_deletion_time {
        if new_deletion_time > current_deletion_time && current_deletion_time > expiration_time {
            context
                .msg
                .channel_id
                .say(
                    &context.ctx,
                    lib::strings::CHANNEL_ALREADY_MARKED_FOR_CLOSING,
                )
                .await
                .ok();
            return Ok(());
        }
    }
    let mut pipe = redis::pipe();
    pipe.set(&redis_channel_deletion_key, new_deletion_time.to_rfc3339());
    // If there is an associated voice channel, mark it also for deletion
    if let Some(voice_channel_id) = voice_channel_id {
        let redis_voice_channel_deletion_key =
            format!("discord_voice_channel:{}:deletion_time", voice_channel_id.0);
        pipe.set(
            &redis_voice_channel_deletion_key,
            new_deletion_time.to_rfc3339(),
        );
    }
    let _: () = pipe
        .query_async(context.async_redis_connection().await?)
        .await?;
    context
        .msg
        .channel_id
        .say(&context.ctx, lib::strings::CHANNEL_MARKED_FOR_CLOSING)
        .await
        .ok();
    if let Some(bot_alerts_channel_id) = lib::discord::sync::ids::BOT_ALERTS_CHANNEL_ID {
        bot_alerts_channel_id
            .say(
                &context.ctx,
                lib::strings::CHANNEL_MARKED_FOR_CLOSING_ALERT(context.msg.channel_id),
            )
            .await
            .ok();
    }
    Ok(())
}
