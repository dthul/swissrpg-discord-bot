use command_macro::command;

#[command]
#[regex(r"numcached")]
#[level(admin)]
#[help("numcached", "Shows the number of known server members")]
pub fn numcached<'a>(
    context: &'a mut super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    if let Some(guild) = lib::discord::sync::ids::GUILD_ID
        .to_guild_cached(&context.ctx)
        .await
    {
        let num_cached_members = guild.members.len();
        context
            .msg
            .channel_id
            .say(
                &context.ctx,
                format!(
                    "I have {} members cached for this guild",
                    num_cached_members
                ),
            )
            .await
            .ok();
    } else {
        context.msg.channel_id.say(
            &context.ctx,
            "No guild associated with this message (use the command from a guild channel instead \
             of a direct message).",
        ).await.ok();
    }
    Ok(())
}
