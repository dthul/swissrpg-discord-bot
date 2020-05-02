use command_macro::command;
use redis::Commands;

#[command]
#[regex(
    r"rsvp\s+{mention_pattern}\s+(?P<meetup_event_id>[^\s]+)",
    mention_pattern
)]
#[level(admin)]
fn rsvp_user(
    mut context: super::CommandContext<'_>,
    captures: regex::Captures<'_>,
) -> Result<(), lib::meetup::Error> {
    // Get the mentioned Discord ID
    let discord_id = captures.name("mention_id").unwrap().as_str();
    // Try to convert the specified ID to an integer
    let discord_id = match discord_id.parse::<u64>() {
        Ok(id) => id,
        _ => {
            let _ = context
                .msg
                .channel_id
                .say(context.ctx, lib::strings::CHANNEL_ADD_USER_INVALID_DISCORD);
            return Ok(());
        }
    };
    // Get the mentioned Meetup event
    let meetup_event_id = captures.name("meetup_event_id").unwrap().as_str();
    let urlname = "SwissRPG-Zurich";
    // Look up the Meetup ID for this user
    let redis_discord_user_meetup_user_key = format!("discord_user:{}:meetup_user", discord_id);
    let res: Option<u64> = context
        .redis_connection()?
        .get(&redis_discord_user_meetup_user_key)?;
    let meetup_id = match res {
        Some(meetup_id) => meetup_id,
        None => {
            let _ = context
                .msg
                .channel_id
                .say(context.ctx, lib::strings::CHANNEL_ADD_USER_INVALID_DISCORD);
            return Ok(());
        }
    };
    // Try to RSVP the user
    let runtime_mutex = context.async_runtime()?.clone();
    let runtime_guard = futures::executor::block_on(runtime_mutex.read());
    let async_runtime = match *runtime_guard {
        Some(ref async_runtime) => async_runtime,
        None => {
            let _ = context
                .msg
                .channel_id
                .say(context.ctx, "Could not submit asynchronous user RSVP task");
            return Ok(());
        }
    };
    let rsvp = async_runtime.enter(|| {
        futures::executor::block_on(async {
            let mut redis_connection = context.redis_client()?.get_async_connection().await?;
            lib::meetup::util::rsvp_user_to_event(
                meetup_id,
                urlname,
                meetup_event_id,
                &mut redis_connection,
                context.oauth2_consumer()?,
            )
            .await
        })
    })?;
    let _ = context.msg.react(context.ctx, "\u{2705}");
    println!(
        "RSVP'd Meetup user {} to Meetup event {}. RSVP:\n{:#?}",
        meetup_id, meetup_event_id, rsvp
    );
    Ok(())
}
