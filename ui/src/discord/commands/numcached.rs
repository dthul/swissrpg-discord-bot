use command_macro::command;

#[command]
#[regex(r"numcached")]
#[level(admin)]
fn numcached(
    context: super::CommandContext<'_>,
    _: regex::Captures<'_>,
) -> Result<(), lib::meetup::Error> {
    if let Some(guild) = lib::discord::sync::ids::GUILD_ID.to_guild_cached(context.ctx) {
        let num_cached_members = guild.read().members.len();
        let _ = context.msg.channel_id.say(
            context.ctx,
            format!(
                "I have {} members cached for this guild",
                num_cached_members
            ),
        );
    } else {
        let _ = context.msg.channel_id.say(
            context.ctx,
            "No guild associated with this message (use the command from a guild channel instead \
             of a direct message).",
        );
    }
    Ok(())
}
