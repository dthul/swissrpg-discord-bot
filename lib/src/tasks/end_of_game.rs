use crate::{discord::sync::ChannelType, strings};
use redis::AsyncCommands;
use serenity::model::id::{ChannelId, RoleId, UserId};
use simple_error::SimpleError;

// Sends channel deletion reminders to expired Discord channels
pub fn create_end_of_game_task(
    redis_client: redis::Client,
    mut discord_api: crate::discord::CacheAndHttp,
    bot_id: UserId,
    recurring: bool,
) -> impl FnMut(&mut white_rabbit::Context) -> white_rabbit::DateResult + Send + Sync + 'static {
    move |_ctx| {
        let next_sync_time = match end_of_game_task(&redis_client, &mut discord_api, bot_id.0) {
            Err(err) => {
                eprintln!("End of game task failed: {}", err);
                // Retry in an hour
                white_rabbit::Utc::now() + white_rabbit::Duration::hours(1)
            }
            _ => {
                // Run again tomorrow at 6:30pm
                (white_rabbit::Utc::now() + white_rabbit::Duration::days(1))
                    .date()
                    .and_hms(18, 30, 0)
            }
        };
        if recurring {
            white_rabbit::DateResult::Repeat(next_sync_time)
        } else {
            white_rabbit::DateResult::Done
        }
    }
}

fn end_of_game_task(
    redis_client: &redis::Client,
    discord_api: &mut crate::discord::CacheAndHttp,
    bot_id: u64,
) -> Result<(), crate::meetup::Error> {
    let redis_series_key = "event_series";
    let mut con = redis_client.get_connection()?;
    let event_series: Vec<String> = con.smembers(redis_series_key)?;
    let mut some_failed = false;
    // First, update each series's channel's expiration time
    for series in &event_series {
        if let Err(err) = update_series_channel_expiration(series, &mut con) {
            some_failed = true;
            eprintln!("Series channel expiration update failed: {}", err);
        }
    }
    let redis_channels_key = "discord_channels";
    let discord_channels: Vec<u64> = con.smembers(redis_channels_key)?;
    for channel in discord_channels {
        // Then, check if the channel is due for deletion
        match delete_marked_channel(ChannelType::Text, channel, &mut con, discord_api) {
            Ok(deletion_status) => {
                if deletion_status == DeletionStatus::NotDeleted {
                    // Lastly, send a reminder if necessary
                    if let Err(err) =
                        send_channel_expiration_reminder(channel, &mut con, discord_api, bot_id)
                    {
                        some_failed = true;
                        eprintln!("Channel expiration reminder failed: {}", err);
                    }
                }
            }
            Err(err) => {
                some_failed = true;
                eprintln!("Error during channel deletion: {}", err);
            }
        }
    }
    let redis_voice_channels_key = "discord_voice_channels";
    let discord_voice_channels: Vec<u64> = con.smembers(redis_voice_channels_key)?;
    for channel in discord_voice_channels {
        // Then, check if the channel is due for deletion
        if let Err(err) = delete_marked_channel(ChannelType::Voice, channel, &mut con, discord_api)
        {
            some_failed = true;
            eprintln!("Error during voice channel deletion: {}", err);
        }
    }
    if some_failed {
        Err(SimpleError::new("One or more end of game tasks failed").into())
    } else {
        Ok(())
    }
}

fn update_series_channel_expiration(
    series_id: &str,
    con: &mut redis::Connection,
) -> Result<(), crate::meetup::Error> {
    let redis_series_events_key = format!("event_series:{}:meetup_events", &series_id);
    let redis_series_channel_key = format!("event_series:{}:discord_channel", &series_id);
    // Check if this event series has a channel
    let channel_id: u64 = match con.get(&redis_series_channel_key)? {
        Some(id) => id,
        None => {
            println!(
                "Expiration update: Event series {} has no channel associated with it",
                series_id
            );
            return Ok(());
        }
    };
    // Get all events belonging to this event series
    let event_ids: Vec<String> = con.smembers(&redis_series_events_key)?;
    let mut events: Vec<_> = event_ids
        .into_iter()
        .filter_map(|event_id| {
            let redis_event_key = format!("meetup_event:{}", event_id);
            let tuple: redis::RedisResult<(String, String)> =
                con.hget(&redis_event_key, &["time", "name"]);
            match tuple {
                Ok((time, name)) => match chrono::DateTime::parse_from_rfc3339(&time) {
                    Ok(time) => Some((event_id, time.with_timezone(&chrono::Utc), name)),
                    Err(err) => {
                        eprintln!("Event {} has an invalid time: {}", event_id, err);
                        None
                    }
                },
                Err(err) => {
                    eprintln!("Redis error when querying event time: {}", err);
                    None
                }
            }
        })
        .collect();
    // Sort by date
    events.sort_unstable_by_key(|pair| pair.1);
    // Check if this is a one-shot or a campaign series
    let is_campaign = {
        let redis_series_type_key = format!("event_series:{}:type", series_id);
        let series_type: Option<String> = con.get(&redis_series_type_key)?;
        match series_type.as_ref().map(String::as_str) {
            Some("campaign") => true,
            _ => false,
        }
    };
    // Query the channel's current expiration time
    let redis_channel_expiration_key = format!("discord_channel:{}:expiration_time", channel_id);
    let current_expiration_time =
        match con.get::<_, Option<String>>(&redis_channel_expiration_key)? {
            Some(time) => match chrono::DateTime::parse_from_rfc3339(&time) {
                Ok(time) => Some(time.with_timezone(&chrono::Utc)),
                Err(err) => {
                    eprintln!(
                        "Discord channel {} had an invalid expiration time: {}",
                        channel_id, err
                    );
                    None
                }
            },
            None => None,
        };
    // The last element in this vector will be the last event in the series
    let (new_expiration_time, needs_update) = if let Some(last_event) = events.last() {
        let last_event_time = last_event.1;
        let new_expiration_time = if is_campaign {
            last_event_time + chrono::Duration::days(3)
        } else {
            last_event_time + chrono::Duration::days(1)
        };
        let needs_update = current_expiration_time
            .map(|current| current != new_expiration_time)
            .unwrap_or(true);
        (new_expiration_time, needs_update)
    } else {
        // No event in this series, expire the channel immediately
        let new_expiration_time = chrono::Utc::now();
        let needs_update = current_expiration_time
            .map(|current| current > new_expiration_time)
            .unwrap_or(true);
        (new_expiration_time, needs_update)
    };
    // Store the new expiration time in Redis
    if needs_update {
        // Also delete any possibly stored deletion times from the channel and
        // the possibly associated voice channel
        let redis_channel_deletion_key = format!("discord_channel:{}:deletion_time", channel_id);
        let redis_voice_channel_deletion_key =
            format!("discord_voice_channel:{}:deletion_time", channel_id);
        let mut pipe = redis::pipe();
        let _: () = pipe
            .set(
                &redis_channel_expiration_key,
                new_expiration_time.to_rfc3339(),
            )
            .del(&redis_channel_deletion_key)
            .del(&redis_voice_channel_deletion_key)
            .query(con)?;
        println!(
            "Set expiration time of channel {} to {}",
            channel_id, new_expiration_time
        );
    }
    Ok(())
}

async fn send_channel_expiration_reminder(
    channel_id: u64,
    con: &mut redis::aio::Connection,
    discord_api: &mut crate::discord::CacheAndHttp,
    bot_id: u64,
) -> Result<(), crate::meetup::Error> {
    let redis_channel_expiration_key = format!("discord_channel:{}:expiration_time", channel_id);
    let redis_channel_reminder_time = format!(
        "discord_channel:{}:last_expiration_reminder_time",
        channel_id
    );
    let redis_channel_snooze_key = format!("discord_channel:{}:snooze_until", channel_id);
    let (expiration_time, snooze_until, last_reminder_time): (
        Option<String>,
        Option<String>,
        Option<String>,
    ) = con
        .get(&[
            &redis_channel_expiration_key,
            &redis_channel_snooze_key,
            &redis_channel_reminder_time,
        ])
        .await?;
    let expiration_time = expiration_time
        .map(|t| chrono::DateTime::parse_from_rfc3339(&t))
        .transpose()?
        .map(|t| t.with_timezone(&chrono::Utc));
    let snooze_until = snooze_until
        .map(|t| chrono::DateTime::parse_from_rfc3339(&t))
        .transpose()?
        .map(|t| t.with_timezone(&chrono::Utc));
    let last_reminder_time = last_reminder_time
        .map(|t| chrono::DateTime::parse_from_rfc3339(&t))
        .transpose()?
        .map(|t| t.with_timezone(&chrono::Utc));
    if let Some(expiration_time) = expiration_time {
        let now = chrono::Utc::now();
        if expiration_time > now {
            // The expiration time hasn't come yet
            return Ok(());
        }
        if let Some(snooze_until) = snooze_until {
            if snooze_until > now {
                // Reminders are snoozed
                return Ok(());
            }
        }
        // Check if this is a one-shot or a campaign series
        let is_campaign = {
            let series_id: String = con
                .get(format!("discord_channel:{}:event_series", channel_id))
                .await?;
            let redis_series_type_key = format!("event_series:{}:type", &series_id);
            let series_type: Option<String> = con.get(&redis_series_type_key).await?;
            match series_type.as_ref().map(String::as_str) {
                Some("campaign") => true,
                _ => false,
            }
        };
        if let Some(last_reminder_time) = last_reminder_time {
            // Reminders will be sent with an interval of two days for
            // one-shots and four days for campaign channels
            let reminder_interval = if is_campaign {
                chrono::Duration::days(4) - chrono::Duration::hours(2)
            } else {
                chrono::Duration::days(2) - chrono::Duration::hours(2)
            };
            if last_reminder_time + reminder_interval > chrono::Utc::now() {
                // We already sent a reminder recently
                return Ok(());
            }
        }
        // Send a reminder and update the last reminder time
        println!("Reminding channel {} of its expiration", channel_id);
        let channel_roles = crate::get_channel_roles(channel_id, con)?;
        let user_role = channel_roles.map(|roles| roles.user);
        ChannelId(channel_id)
            .send_message(&discord_api.http, |message_builder| {
                if is_campaign {
                    message_builder.content(strings::END_OF_CAMPAIGN_MESSAGE(bot_id, user_role))
                } else {
                    message_builder.content(strings::END_OF_ADVENTURE_MESSAGE(bot_id, user_role))
                }
            })
            .await?;
        let last_reminder_time = chrono::Utc::now().to_rfc3339();
        con.set(&redis_channel_reminder_time, last_reminder_time)
            .await?;
        println!(
            "Updated channel's {} latest expiration reminder time",
            channel_id
        );
    }
    Ok(())
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum DeletionStatus {
    Deleted,
    NotDeleted,
    AlreadyDeleted,
}

fn delete_marked_channel(
    channel_type: ChannelType,
    channel_id: u64,
    con: &mut redis::Connection,
    discord_api: &crate::discord::CacheAndHttp,
) -> Result<DeletionStatus, crate::meetup::Error> {
    // Check if there is an expiration time in the future
    // -> don't delete channel and remove deletion marker
    let redis_channel_deletion_key = match channel_type {
        ChannelType::Text => format!("discord_channel:{}:deletion_time", channel_id),
        ChannelType::Voice => format!("discord_voice_channel:{}:deletion_time", channel_id),
    };
    // Check if the channel was marked for deletion
    let deletion_time: Option<String> = con.get(&redis_channel_deletion_key)?;
    let deletion_time = deletion_time
        .map(|t| chrono::DateTime::parse_from_rfc3339(&t))
        .transpose()?
        .map(|t| t.with_timezone(&chrono::Utc));
    let deletion_time = match deletion_time {
        Some(deletion_time) => deletion_time,
        None => return Ok(DeletionStatus::NotDeleted),
    };
    if deletion_time > chrono::Utc::now() {
        return Ok(DeletionStatus::NotDeleted);
    }
    // Check whether the Channel still exists on Discord
    let channel_exists = match ChannelId(channel_id).to_channel(discord_api) {
        Ok(_) => true,
        Err(err) => {
            if let serenity::Error::Http(http_err) = &err {
                if let serenity::http::HttpError::UnsuccessfulRequest(response) = http_err.as_ref()
                {
                    if response.status_code == serenity::http::StatusCode::NOT_FOUND {
                        false
                    } else {
                        return Err(err.into());
                    }
                } else {
                    return Err(err.into());
                }
            } else {
                return Err(err.into());
            }
        }
    };
    if channel_exists {
        // Delete the channel from Discord
        ChannelId(channel_id).delete(&discord_api.http)?;
        // Delete the channel deletion request from Redis
        let _: () = con.del(&redis_channel_deletion_key)?;
        // Delete the associated roles
        if channel_type == ChannelType::Text {
            let host_role: Option<u64> =
                con.get(format!("discord_channel:{}:discord_host_role", channel_id))?;
            if let Some(host_role) = host_role {
                delete_role(RoleId(host_role), con, discord_api)?;
            }
            let player_role: Option<u64> =
                con.get(format!("discord_channel:{}:discord_role", channel_id))?;
            if let Some(player_role) = player_role {
                delete_role(RoleId(player_role), con, discord_api)?;
            }
        }
        // Let the vacuum task handle all other stale Redis keys
        Ok(DeletionStatus::Deleted)
    } else {
        Ok(DeletionStatus::AlreadyDeleted)
    }
}

fn delete_role(
    role_id: RoleId,
    con: &mut redis::Connection,
    discord_api: &crate::discord::CacheAndHttp,
) -> Result<(), crate::meetup::Error> {
    // Try to delete the role
    if let Err(err) = crate::discord::sync::ids::GUILD_ID.delete_role(&discord_api.http, role_id) {
        // If something went wrong, check whether we should record this role as orphaned
        let role_is_orphaned = if let serenity::Error::Http(http_err) = &err {
            if let serenity::http::HttpError::UnsuccessfulRequest(response) = http_err.as_ref() {
                if response.status_code == serenity::http::StatusCode::NOT_FOUND {
                    false
                } else {
                    true
                }
            } else {
                true
            }
        } else {
            true
        };
        if role_is_orphaned {
            // Try to add this role to Redis
            let _: () = con.sadd("orphaned_roles", role_id.0)?;
        }
        // Return the error to the caller
        return Err(err.into());
    }
    Ok(())
}
