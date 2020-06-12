use command_macro::command;

#[command]
#[regex(r"mention\s*channel")]
#[level(host)]
fn mention_channel<'a>(
    mut context: super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    let channel_roles = lib::get_channel_roles(
        context.msg.channel_id.0,
        context.async_redis_connection().await?,
    )
    .await?;
    if let Some(channel_roles) = channel_roles {
        context
            .msg
            .channel_id
            .say(context.ctx, format!("<@&{}>", channel_roles.user))
            .await
            .ok();
    } else {
        context
            .msg
            .channel_id
            .say(context.ctx, format!("This channel has no role"))
            .await
            .ok();
    }
    Ok(())
}
