use command_macro::command;
use redis::AsyncCommands;

#[command]
#[regex(r"refresh\s*meetup(-|\s*)token\s+{mention_pattern}", mention_pattern)]
#[level(admin)]
fn refresh_meetup_token<'a>(
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
    // let redis_client = context.redis_client().await?;
    let oauth2_consumer = context.oauth2_consumer()?.clone();
    let redis_connection = context.async_redis_connection().await?;
    oauth2_consumer
        .refresh_oauth_tokens(
            lib::meetup::oauth2::TokenType::User(meetup_id),
            redis_connection,
        )
        .await?;
    context.msg.react(context.ctx, '\u{2705}').await.ok();
    Ok(())
}
