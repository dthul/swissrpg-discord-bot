use redis::Commands;
use serenity::model::id::ChannelId;
use simple_error::SimpleError;

// Sends channel deletion reminders to expired Discord channels
pub fn create_end_of_game_task(
    redis_client: redis::Client,
    mut discord_api: crate::discord_bot::CacheAndHttp,
    bot_id: u64,
    recurring: bool,
) -> impl FnMut(&mut white_rabbit::Context) -> white_rabbit::DateResult + Send + Sync + 'static {
    move |_ctx| {
        let next_sync_time = match end_of_game_task(&redis_client, &mut discord_api, bot_id) {
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
    discord_api: &mut crate::discord_bot::CacheAndHttp,
    bot_id: u64,
) -> Result<(), crate::BoxedError> {
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
        match delete_marked_channel(channel, &mut con, discord_api) {
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
    if some_failed {
        Err(SimpleError::new("One or more end of game tasks failed").into())
    } else {
        Ok(())
    }
}

fn update_series_channel_expiration(
    series_id: &str,
    con: &mut redis::Connection,
) -> Result<(), crate::BoxedError> {
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
    // The last element in this vector will be the last event in the series
    if let Some(last_event) = events.last() {
        let last_event_id = last_event.0.clone();
        let last_event_time = last_event.1;
        let last_event_name = &last_event.2;
        println!(
            "Expiration update: Event \"{}\" {} is the last event in series {} with datetime {}",
            last_event_name, last_event_id, series_id, last_event_time
        );
        let redis_channel_expiration_key =
            format!("discord_channel:{}:expiration_time", channel_id);
        // Query the current expiration time
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
        let new_expiration_time = last_event_time + chrono::Duration::days(1);
        let (new_expiration_time, needs_update) = match current_expiration_time {
            Some(current_expiration_time) => {
                if current_expiration_time > new_expiration_time {
                    (current_expiration_time, false)
                } else {
                    (new_expiration_time, true)
                }
            }
            None => (new_expiration_time, true),
        };
        // Store the new expiration time in Redis
        if needs_update {
            let _: () = con.set(
                &redis_channel_expiration_key,
                new_expiration_time.to_rfc3339(),
            )?;
            println!(
                "Set expiration time of channel {} to {}",
                channel_id, new_expiration_time
            );
        }
    }
    Ok(())
}

fn send_channel_expiration_reminder(
    channel_id: u64,
    con: &mut redis::Connection,
    discord_api: &mut crate::discord_bot::CacheAndHttp,
    bot_id: u64,
) -> Result<(), crate::BoxedError> {
    let redis_channel_expiration_key = format!("discord_channel:{}:expiration_time", channel_id);
    let redis_channel_reminder_time = format!(
        "discord_channel:{}:last_expiration_reminder_time",
        channel_id
    );
    let (expiration_time, last_reminder_time): (Option<String>, Option<String>) =
        con.get(&[&redis_channel_expiration_key, &redis_channel_reminder_time])?;
    let expiration_time = expiration_time
        .map(|t| chrono::DateTime::parse_from_rfc3339(&t))
        .transpose()?
        .map(|t| t.with_timezone(&chrono::Utc));
    let last_reminder_time = last_reminder_time
        .map(|t| chrono::DateTime::parse_from_rfc3339(&t))
        .transpose()?
        .map(|t| t.with_timezone(&chrono::Utc));
    if let Some(expiration_time) = expiration_time {
        if expiration_time > chrono::Utc::now() {
            // The expiration time hasn't come yet
            return Ok(());
        }
        if let Some(last_reminder_time) = last_reminder_time {
            if last_reminder_time + chrono::Duration::hours(46) > chrono::Utc::now() {
                // We already sent a reminder in the last two days
                return Ok(());
            }
        }
        // Send a reminder and update the last reminder time
        println!("Reminding channel {} of its expiration", channel_id);
        ChannelId(channel_id).send_message(&discord_api.http, |message_builder| {
            message_builder.content(format!(
                "I hope everyone @here had fun rolling dice!\n\
                 It looks like your adventure is coming to an end and so will this channel.\n\
                 As soon as you are ready, any of the hosts can close this channel by writing:\n\
                 ***<@{}> close channel***\n\
                 In case you want to continue your adventure instead, please schedule the next session(s) \
                 on Meetup and I will extend the lifetime of this channel.",
                bot_id
            ))
        })?;
        let last_reminder_time = chrono::Utc::now().to_rfc3339();
        con.set(&redis_channel_reminder_time, last_reminder_time)?;
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
    channel_id: u64,
    con: &mut redis::Connection,
    discord_api: &crate::discord_bot::CacheAndHttp,
) -> Result<DeletionStatus, crate::BoxedError> {
    // Check if there is an expiration time in the future
    // -> don't delete channel and remove deletion marker
    let redis_channel_deletion_key = format!("discord_channel:{}:deletion_time", channel_id);
    let redis_channel_expiration_key = format!("discord_channel:{}:expiration_time", channel_id);
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
    // Check if there is an expiration date that might be in the future
    let expiration_time: Option<String> = con.get(&redis_channel_expiration_key)?;
    let expiration_time = expiration_time
        .map(|t| chrono::DateTime::parse_from_rfc3339(&t))
        .transpose()?
        .map(|t| t.with_timezone(&chrono::Utc));
    if let Some(expiration_time) = expiration_time {
        if expiration_time > deletion_time {
            eprintln!(
                "Channel {} has an expiration time that is later than the \
                 scheduled deletion time. Not deleting...",
                channel_id
            );
            return Ok(DeletionStatus::NotDeleted);
        }
    }
    // Check whether the Channel still exists on Discord
    let channel_exists = match ChannelId(channel_id).to_channel(discord_api) {
        Ok(_) => true,
        Err(err) => {
            if let serenity::Error::Http(http_err) = &err {
                if let serenity::http::HttpError::UnsuccessfulRequest(response) = http_err.as_ref()
                {
                    if response.status_code == reqwest::StatusCode::NOT_FOUND {
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
        // Let the vacuum task handle all other stale Redis keys
        // and things like the associated roles
        Ok(DeletionStatus::Deleted)
    } else {
        Ok(DeletionStatus::AlreadyDeleted)
    }
}
