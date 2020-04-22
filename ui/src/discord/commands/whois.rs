use command_macro::command;
use redis::Commands;
use serenity::model::id::UserId;

#[command]
#[regex(
    r"whois\s+{mention_pattern}|{username_tag_pattern}|{meetup_id_pattern}",
    mention_pattern,
    username_tag_pattern,
    meetup_id_pattern
)]
#[level(admin)]
fn whois(
    mut context: super::CommandContext,
    captures: regex::Captures,
) -> Result<(), lib::meetup::Error> {
    if let Some(capture) = captures.name("mention_id") {
        // Look up by Discord ID
        let discord_id = capture.as_str();
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
        whois_by_discord_id(&mut context, UserId(discord_id))?;
    } else if let Some(capture) = captures.name("discord_username_tag") {
        let username_tag = capture.as_str();
        // Look up by Discord username and tag
        whois_by_discord_username_tag(&mut context, &username_tag)?;
    } else if let Some(capture) = captures.name("meetup_user_id") {
        // Look up by Meetup ID
        let meetup_id = capture.as_str();
        // Try to convert the specified ID to an integer
        let meetup_id = match meetup_id.parse::<u64>() {
            Ok(id) => id,
            _ => {
                let _ = context
                    .msg
                    .channel_id
                    .say(context.ctx, lib::strings::CHANNEL_ADD_USER_INVALID_DISCORD);
                return Ok(());
            }
        };
        whois_by_meetup_id(&mut context, meetup_id)?
    }
    Ok(())
}

fn whois_by_discord_id(
    context: &mut super::CommandContext,
    user_id: UserId,
) -> Result<(), lib::meetup::Error> {
    let redis_discord_meetup_key = format!("discord_user:{}:meetup_user", user_id.0);
    let res: Option<String> = context.redis_connection()?.get(&redis_discord_meetup_key)?;
    match res {
        Some(meetup_id) => {
            let _ = context.msg.channel_id.say(
                context.ctx,
                format!(
                    "<@{}> is linked to https://www.meetup.com/members/{}/",
                    user_id.0, meetup_id
                ),
            );
        }
        None => {
            let _ = context.msg.channel_id.say(
                context.ctx,
                format!(
                    "<@{}> does not seem to be linked to a Meetup account",
                    user_id.0
                ),
            );
        }
    }
    Ok(())
}

pub fn whois_by_discord_username_tag(
    context: &mut super::CommandContext,
    username_tag: &str,
) -> Result<(), lib::meetup::Error> {
    if let Some(guild) = lib::discord::sync::ids::GUILD_ID.to_guild_cached(context.ctx) {
        let discord_id = guild
            .read()
            .member_named(username_tag)
            .map(|m| m.user.read().id);
        if let Some(discord_id) = discord_id {
            // Look up by Discord ID
            whois_by_discord_id(context, discord_id)?;
        } else {
            let _ = context.msg.channel_id.say(
                context.ctx,
                format!("{} is not a Discord user", username_tag),
            );
        }
    } else {
        let _ = context
            .msg
            .channel_id
            .say(context.ctx, "Something went wrong (guild not found)");
    }
    Ok(())
}

pub fn whois_by_meetup_id(
    context: &mut super::CommandContext,
    meetup_id: u64,
) -> Result<(), lib::meetup::Error> {
    let redis_meetup_discord_key = format!("meetup_user:{}:discord_user", meetup_id);
    let res: Option<String> = context.redis_connection()?.get(&redis_meetup_discord_key)?;
    match res {
        Some(discord_id) => {
            let _ = context.msg.channel_id.say(
                context.ctx,
                format!(
                    "https://www.meetup.com/members/{}/ is linked to <@{}>",
                    meetup_id, discord_id
                ),
            );
        }
        None => {
            let _ = context.msg.channel_id.say(
                context.ctx,
                format!(
                    "https://www.meetup.com/members/{}/ does not seem to be linked to a Discord \
                     user",
                    meetup_id
                ),
            );
        }
    }
    Ok(())
}
