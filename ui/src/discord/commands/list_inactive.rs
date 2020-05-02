use command_macro::command;

#[command]
#[regex(r"list\s*inactive")]
#[level(admin)]
fn list_inactive(
    context: super::CommandContext,
    _: regex::Captures,
) -> Result<(), lib::meetup::Error> {
    if let Some(guild) = lib::discord::sync::ids::GUILD_ID.to_guild_cached(context.ctx) {
        let mut inactive_users = vec![];
        for (&id, member) in &guild.read().members {
            if member.roles.is_empty() {
                inactive_users.push(id);
            }
        }
        let inactive_users_strs: Vec<String> = inactive_users
            .into_iter()
            .map(|id| format!("* <@{}>", id))
            .collect();
        let inactive_users_str = inactive_users_strs.join("\n");
        let _ = context.msg.channel_id.say(
            context.ctx,
            "List of users with no roles assigned:\n".to_string() + &inactive_users_str,
        );
    } else {
        let _ = context
            .msg
            .channel_id
            .say(context.ctx, "Could not find the guild");
    }
    Ok(())
}
