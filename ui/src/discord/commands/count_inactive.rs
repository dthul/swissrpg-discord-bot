use command_macro::command;

#[command]
#[regex(r"count\s*inactive")]
#[level(admin)]
#[help("count inactive", "returns the number of members without any role")]
fn count_inactive<'a>(
    context: &'a mut super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    if let Some(guild) = lib::discord::sync::ids::GUILD_ID.to_guild_cached(&context.ctx) {
        let num_inactive_users = &guild
            .members
            .iter()
            .filter(|(_id, member)| member.roles.is_empty())
            .count();
        context
            .msg
            .channel_id
            .say(
                &context.ctx,
                format!("There are {} users without any role", num_inactive_users),
            )
            .await
            .ok();
    } else {
        context
            .msg
            .channel_id
            .say(&context.ctx, "Could not find the guild")
            .await
            .ok();
    }
    Ok(())
}

#[command]
#[regex(r"count\s*members")]
#[level(admin)]
#[help("count members", "returns the number of members")]
fn count_members<'a>(
    context: &'a mut super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    if let Some(guild) = lib::discord::sync::ids::GUILD_ID.to_guild_cached(&context.ctx) {
        let num_members = &guild.members.len();
        context
            .msg
            .channel_id
            .say(&context.ctx, format!("There are {} members", num_members))
            .await
            .ok();
    } else {
        context
            .msg
            .channel_id
            .say(&context.ctx, "Could not find the guild")
            .await
            .ok();
    }
    Ok(())
}
