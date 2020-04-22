use command_macro::command;

#[command]
#[regex(r"mention\s*channel")]
#[level(host)]
fn mention_channel(
    mut context: super::CommandContext,
    _: regex::Captures,
) -> Result<(), lib::meetup::Error> {
    let channel_roles =
        lib::get_channel_roles(context.msg.channel_id.0, context.redis_connection()?)?;
    if let Some(channel_roles) = channel_roles {
        let _ = context
            .msg
            .channel_id
            .say(context.ctx, format!("<@&{}>", channel_roles.user));
    } else {
        let _ = context
            .msg
            .channel_id
            .say(context.ctx, format!("This channel has no role"));
    }
    Ok(())
}
