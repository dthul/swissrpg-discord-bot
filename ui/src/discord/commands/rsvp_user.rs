use command_macro::command;
use redis::AsyncCommands;

#[command]
#[regex(
    r"rsvp\s+{mention_pattern}\s+(?P<meetup_event_id>[^\s]+)",
    mention_pattern
)]
#[level(admin)]
fn rsvp_user<'a>(
    mut context: super::CommandContext,
    captures: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    // Get the mentioned Discord ID
    let discord_id = captures.name("mention_id").unwrap().as_str();
    // Try to convert the specified ID to an integer
    let discord_id = match discord_id.parse::<u64>() {
        Ok(id) => id,
        _ => {
            context
                .msg
                .channel_id
                .say(context.ctx, lib::strings::CHANNEL_ADD_USER_INVALID_DISCORD)
                .await
                .ok();
            return Ok(());
        }
    };
    // Get the mentioned Meetup event
    let meetup_event_id = captures.name("meetup_event_id").unwrap().as_str();
    let urlname = "SwissRPG-Zurich";
    // Look up the Meetup ID for this user
    let redis_discord_user_meetup_user_key = format!("discord_user:{}:meetup_user", discord_id);
    let res: Option<u64> = context
        .async_redis_connection()
        .await?
        .get(&redis_discord_user_meetup_user_key)
        .await?;
    let meetup_id = match res {
        Some(meetup_id) => meetup_id,
        None => {
            context
                .msg
                .channel_id
                .say(context.ctx, lib::strings::CHANNEL_ADD_USER_INVALID_DISCORD)
                .await
                .ok();
            return Ok(());
        }
    };
    // Try to RSVP the user
    let mut redis_connection = context.redis_client().await?.get_async_connection().await?;
    let rsvp = lib::meetup::util::rsvp_user_to_event(
        meetup_id,
        urlname,
        meetup_event_id,
        &mut redis_connection,
        context.oauth2_consumer()?,
    )
    .await?;
    context.msg.react(context.ctx, '\u{2705}').await.ok();
    println!(
        "RSVP'd Meetup user {} to Meetup event {}. RSVP:\n{:#?}",
        meetup_id, meetup_event_id, rsvp
    );
    Ok(())
}
