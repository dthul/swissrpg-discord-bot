use command_macro::command;

#[command]
#[regex(r"archive")]
#[level(admin)]
#[help(
    "archive",
    "Adds information about games that happened in the past to the archive database"
)]
fn archive<'a>(
    context: &'a mut super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    let pool = context.pool().await?;
    let mut discord_api = (&context.ctx).into();
    let con = context.async_redis_connection().await?;
    let res = lib::tasks::archive::sync_redis_to_postgres(con, &pool, &mut discord_api).await?;
    context
        .msg
        .channel_id
        .say(
            &context.ctx,
            format!(
                "Archived {} new games, {} GMs and {} players",
                res.num_events_added, res.num_hosts_added, res.num_participants_added
            ),
        )
        .await?;
    if res.num_errors > 0 {
        context
            .msg
            .channel_id
            .say(
                &context.ctx,
                format!("Encountered {} errors", res.num_errors),
            )
            .await?;
    }
    Ok(())
}
