use command_macro::command;
use serenity::model::channel::Channel;

#[command]
#[regex(r"end\s*adventure")]
#[level(host)]
#[help(
    "end adventure",
    "sets the channel for closure at the end of an adventure. The channel won't be deleted \
     immediately but within 24 hours."
)]
fn end_adventure<'a>(
    context: &'a mut super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    // TODO: make this a macro check
    // Check whether this is a game channel
    let is_game_channel = context.is_game_channel(None).await?;
    if !is_game_channel {
        context
            .msg
            .channel_id
            .say(&context.ctx, lib::strings::CHANNEL_NOT_BOT_CONTROLLED)
            .await
            .ok();
        return Ok(());
    };
    let pool = context.pool().await?;
    let mut tx = pool.begin().await?;
    // Check if there is a channel expiration time in the future
    let expiration_time = sqlx::query_scalar!(
        r#"SELECT expiration_time FROM event_series_text_channel WHERE discord_id = $1"#,
        context.msg.channel_id.0 as i64
    )
    .fetch_one(&mut *tx)
    .await?;
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
    let current_deletion_time = sqlx::query_scalar!(
        r#"SELECT deletion_time FROM event_series_text_channel WHERE discord_id = $1"#,
        context.msg.channel_id.0 as i64
    )
    .fetch_one(&mut *tx)
    .await?;
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
    // Figure out whether there is an associated voice channel
    let voice_channel_id = lib::get_channel_voice_channel(context.msg.channel_id, &mut tx).await?;
    let channel_roles = lib::get_channel_roles(context.msg.channel_id, &mut tx).await?;
    sqlx::query!(
        r#"UPDATE event_series_text_channel SET deletion_time = $2 WHERE discord_id = $1"#,
        context.msg.channel_id.0 as i64,
        new_deletion_time
    )
    .execute(&mut *tx)
    .await?;
    // If there is an associated voice channel, mark it also for deletion
    if let Some(voice_channel_id) = voice_channel_id {
        sqlx::query!(
            r#"UPDATE event_series_voice_channel SET deletion_time = $2 WHERE discord_id = $1"#,
            voice_channel_id.0 as i64,
            new_deletion_time
        )
        .execute(&mut *tx)
        .await?;
    }
    if let Some(channel_roles) = channel_roles {
        sqlx::query!(
            r#"UPDATE event_series_role SET deletion_time = $2 WHERE discord_id = $1"#,
            channel_roles.user.0 as i64,
            new_deletion_time
        )
        .execute(&mut *tx)
        .await?;
        if let Some(host_role_id) = channel_roles.host {
            sqlx::query!(
                r#"UPDATE event_series_host_role SET deletion_time = $2 WHERE discord_id = $1"#,
                host_role_id.0 as i64,
                new_deletion_time
            )
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    context
        .msg
        .channel_id
        .say(&context.ctx, lib::strings::CHANNEL_MARKED_FOR_CLOSING)
        .await
        .ok();
    let channel = context.channel().await;
    let channel_name = match &channel {
        Ok(Channel::Guild(channel)) => &channel.name,
        _ => "'unknown'",
    };
    if let Some(bot_alerts_channel_id) = lib::discord::sync::ids::BOT_ALERTS_CHANNEL_ID {
        bot_alerts_channel_id
            .say(
                &context.ctx,
                lib::strings::CHANNEL_MARKED_FOR_CLOSING_ALERT(
                    context.msg.channel_id,
                    channel_name,
                    context.msg.author.id,
                ),
            )
            .await
            .ok();
    }
    Ok(())
}
