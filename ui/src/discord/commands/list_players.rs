use command_macro::command;
use redis::AsyncCommands;
use serenity::model::id::UserId;
use std::collections::HashMap;

#[command]
#[regex(r"list\s*players")]
#[level(host)]
#[help(
    "list players",
    "shows information about people in this channel and people signed up for this channel's events on Meetup."
)]
fn list_players<'a>(
    context: &'a mut super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    // Check whether this is a bot controlled channel
    let channel_roles = lib::get_channel_roles(
        context.msg.channel_id.0,
        context.async_redis_connection().await?,
    )
    .await?;
    let channel_roles = match channel_roles {
        Some(roles) => roles,
        None => {
            context
                .msg
                .channel_id
                .say(&context.ctx, lib::strings::CHANNEL_NOT_BOT_CONTROLLED)
                .await
                .ok();
            return Ok(());
        }
    };
    // Get all Meetup users that RSVPd "yes" to this event
    // As a first step, get all upcoming events
    let redis_channel_series_key =
        format!("discord_channel:{}:event_series", &context.msg.channel_id.0);
    let series_id: String = context
        .async_redis_connection()
        .await?
        .get(&redis_channel_series_key)
        .await?;
    let redis_series_events_key = format!("event_series:{}:meetup_events", &series_id);
    let event_ids: Vec<String> = context
        .async_redis_connection()
        .await?
        .smembers(&redis_series_events_key)
        .await?;
    let mut event_ids_and_time = {
        let redis_connection = context.async_redis_connection().await?;
        let mut event_ids_and_time = vec![];
        for id in event_ids {
            let redis_event_key = format!("meetup_event:{}", id);
            let time: redis::RedisResult<String> =
                redis_connection.hget(&redis_event_key, "time").await;
            if let Ok(time) = time {
                if let Ok(time) = chrono::DateTime::parse_from_rfc3339(time.as_ref()) {
                    event_ids_and_time.push((id, time.with_timezone(&chrono::Utc)));
                }
            }
        }
        event_ids_and_time
    };
    // Sort by date
    event_ids_and_time.sort_unstable_by_key(|(_id, time)| time.clone());
    // Only look at future events (or the last one, if there is none in the future)
    let now = chrono::Utc::now();
    let future_event_idx = event_ids_and_time
        .iter()
        .position(|(_id, time)| time > &now);
    let latest_event_ids_and_time = match future_event_idx {
        Some(idx) => &event_ids_and_time[idx..],
        None => {
            if event_ids_and_time.is_empty() {
                context
                    .msg
                    .channel_id
                    .say(&context.ctx, "There are no events")
                    .await
                    .ok();
                return Ok(());
            } else {
                &event_ids_and_time[event_ids_and_time.len() - 1..]
            }
        }
    };
    let latest_event_id_refs: Vec<_> = latest_event_ids_and_time
        .iter()
        .map(|(id, _time)| id.as_ref())
        .collect();
    // Now, look up all participants of those events
    let meetup_player_ids = lib::redis::get_events_participants(
        &latest_event_id_refs,
        /*hosts*/ false,
        context.async_redis_connection().await?,
    )
    .await?;

    // Look up all Discord users that have the player role
    // TODO: check whether this returns offline members
    let discord_player_ids: Vec<_> =
        if let Some(serenity::model::channel::Channel::Guild(channel)) =
            context.msg.channel(&context.ctx).await
        {
            let mut discord_player_ids = vec![];
            let members = channel.members(&context.ctx).await?;
            for member in &members {
                match member
                    .user
                    .has_role(
                        &context.ctx,
                        lib::discord::sync::ids::GUILD_ID,
                        channel_roles.user,
                    )
                    .await
                {
                    Ok(has_role) => {
                        if has_role {
                            discord_player_ids.push(member.user.id);
                        }
                    }
                    Err(err) => {
                        eprintln!(
                            "Error when trying to check whether user has role:\n{:#?}",
                            err
                        );
                    }
                }
            }
            discord_player_ids
        } else {
            return Ok(());
        };

    // Four categories of users:
    // - Meetup ID (with Discord ID) [The following Discord users signed up for this event on Meetup (in channel? yes / no)]
    // - Only Meetup ID [The following people are signed up for this event on Meetup but are not linked to a Discord user]
    // - Discord ID (with Meetup ID) (except for the ones that already fall into the first category)
    //   [The following Discord users are in this channel but did not sign up for this event on Meetup]
    // - Only Discord ID
    //   If there is no Meetup ID without mapping, use text from third category, otherwise use:
    //   [The following Discord users are in this channel but are not linked to a Meetup account. I cannot
    //   tell whether they signed up for this event on Meetup or not]

    // Create lists for all four categories
    let mut meetup_id_with_discord_id = HashMap::with_capacity(meetup_player_ids.len());
    let mut meetup_id_only = Vec::with_capacity(meetup_player_ids.len());
    let mut discord_id_with_meetup_id = HashMap::with_capacity(discord_player_ids.len());
    let mut discord_id_only = Vec::with_capacity(discord_player_ids.len());

    for &meetup_id in &meetup_player_ids {
        let redis_meetup_discord_key = format!("meetup_user:{}:discord_user", meetup_id);
        let discord_id: Option<u64> = context
            .async_redis_connection()
            .await?
            .get(&redis_meetup_discord_key)
            .await?;
        match discord_id {
            Some(discord_id) => {
                // Falls into the first category
                meetup_id_with_discord_id.insert(meetup_id, UserId(discord_id));
            }
            None => {
                // Falls into the second category
                meetup_id_only.push(meetup_id);
            }
        }
    }
    for &discord_id in &discord_player_ids {
        let redis_discord_meetup_key = format!("discord_user:{}:meetup_user", discord_id);
        let meetup_id: Option<u64> = context
            .async_redis_connection()
            .await?
            .get(&redis_discord_meetup_key)
            .await?;
        match meetup_id {
            Some(meetup_id) => {
                // Check whether this meetup ID is already in the first category
                if meetup_id_with_discord_id.contains_key(&meetup_id) {
                    continue;
                }
                // Falls into the third category
                discord_id_with_meetup_id.insert(discord_id, meetup_id);
            }
            None => {
                // Falls into the fourth category
                discord_id_only.push(discord_id);
            }
        }
    }

    // Construct the answer
    let mut reply = "**Note:** *this data might be a few minutes out of date. It is refreshed \
                     several times per hour.*\n\n"
        .to_string();

    if !meetup_id_with_discord_id.is_empty() {
        reply += "The following Discord users signed up for an upcoming event on Meetup:\n";
        for (&meetup_id, &discord_id) in &meetup_id_with_discord_id {
            let is_in_channel = discord_player_ids.contains(&discord_id);
            reply += &format!(
                "• <@{discord_id}> (<https://www.meetup.com/members/{meetup_id}/>)\n",
                discord_id = discord_id.0,
                meetup_id = meetup_id
            );
            if is_in_channel {
                reply += " (in this channel ✅)\n";
            } else {
                reply += " (not in this channel ❌)\n";
            }
        }
        reply += "\n\n";
    }

    if !meetup_id_only.is_empty() {
        reply += "The following people are signed up for an upcoming event on Meetup but are not \
                  linked to a Discord user:\n";
        for &meetup_id in &meetup_id_only {
            reply += &format!(
                "• <https://www.meetup.com/members/{meetup_id}/>\n",
                meetup_id = meetup_id
            );
        }
        reply += "\n\n";
    }

    if !discord_id_with_meetup_id.is_empty()
        || (!discord_id_only.is_empty() && meetup_id_only.is_empty())
    {
        reply += "The following Discord users are in this channel but did not sign up for an \
                  upcoming event on Meetup:\n";
        for (&discord_id, &meetup_id) in &discord_id_with_meetup_id {
            reply += &format!(
                "• <@{discord_id}> (<https://www.meetup.com/members/{meetup_id}/>)\n",
                discord_id = discord_id.0,
                meetup_id = meetup_id
            );
        }
        if meetup_id_only.is_empty() {
            for &discord_id in &discord_id_only {
                reply += &format!("• <@{discord_id}>\n", discord_id = discord_id.0);
            }
        }
        reply += "\n\n";
    }

    if !discord_id_only.is_empty() && !meetup_id_only.is_empty() {
        reply += "The following Discord users are in this channel but are not linked to a Meetup \
                  account. I cannot tell whether they signed up for an upcoming event on Meetup \
                  or not:\n";
        for &discord_id in &discord_id_only {
            reply += &format!("• <@{discord_id}>\n", discord_id = discord_id.0);
        }
    }
    context.msg.channel_id.say(&context.ctx, &reply).await.ok();
    Ok(())
}
