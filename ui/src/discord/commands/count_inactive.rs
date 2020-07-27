use command_macro::command;

#[command]
#[regex(r"count\s*inactive")]
#[level(admin)]
#[help("count inactive", "returns the number of members without any role")]
fn count_inactive<'a>(
    context: &'a mut super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    if let Some(guild) = lib::discord::sync::ids::GUILD_ID
        .to_guild_cached(&context.ctx)
        .await
    {
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
