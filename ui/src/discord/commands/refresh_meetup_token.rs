use command_macro::command;
use redis::Commands;

#[command]
#[regex(r"refresh\s*meetup(-|\s*)token\s+{mention_pattern}", mention_pattern)]
#[level(admin)]
fn refresh_meetup_token(
    mut context: super::CommandContext,
    captures: regex::Captures,
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
    let async_runtime_mutex = context.async_runtime()?.clone();
    let runtime_guard = futures::executor::block_on(async_runtime_mutex.read());
    let async_runtime = match *runtime_guard {
        Some(ref async_runtime) => async_runtime,
        None => {
            let _ = context.msg.channel_id.say(
                context.ctx,
                "Could not submit asynchronous Meetup token refresh task",
            );
            return Ok(());
        }
    };
    let redis_client = context.redis_client()?.clone();
    let oauth2_consumer = context.oauth2_consumer()?.clone();
    async_runtime.enter(|| {
        futures::executor::block_on(async {
            let mut redis_connection = redis_client.get_async_connection().await?;
            oauth2_consumer
                .refresh_oauth_tokens(
                    lib::meetup::oauth2::TokenType::User(meetup_id),
                    &mut redis_connection,
                )
                .await
        })
    })?;
    let _ = context.msg.react(context.ctx, "\u{2705}");
    Ok(())
}
