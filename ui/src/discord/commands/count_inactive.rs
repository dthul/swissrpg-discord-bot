use command_macro::command;

#[command]
#[regex(r"count\s*inactive")]
#[level(admin)]
fn count_inactive(
    context: super::CommandContext<'_>,
    _: regex::Captures<'_>,
) -> Result<(), lib::meetup::Error> {
    if let Some(guild) = lib::discord::sync::ids::GUILD_ID.to_guild_cached(context.ctx) {
        let num_inactive_users = &guild
            .read()
            .members
            .iter()
            .filter(|(_id, member)| member.roles.is_empty())
            .count();
        let _ = context.msg.channel_id.say(
            context.ctx,
            format!("There are {} users without any role", num_inactive_users),
        );
    } else {
        let _ = context
            .msg
            .channel_id
            .say(context.ctx, "Could not find the guild");
    }
    Ok(())
}
