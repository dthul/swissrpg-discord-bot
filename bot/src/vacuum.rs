use redis::Commands;
use regex::Regex;
use serenity::model::id::ChannelId;
use std::collections::HashSet;

// Vacuum task:
// - go through "discord_channels"
//   - if the underlying Discord channel doesn't exist anymore:
//     - first, remove everything that is indexed by the channel ID in Redis (like discord_channel:{id}:host_role)
//     - then, remove the channel id from the "discord_channels" set
// - go through "discord_roles"
//   - if the discord_channel doesn't exist anymore in the discord_channels set:
//     - delete the channel from Discord(?) probably not
//     - delete the role from Discord
//     - delete everything that is indexed by the role ID in Redis (like discord_user_role:{id}:discord_channel)
//     - delete the role id from the "discord_roles" set
// - go through "event_series"
//   - if there is no discord_channel for this series _and_ there are no upcoming events in this event series
//     - delete everything that is indexed by the event series id
//     - delete the event series id from "event_series"
// - go through "meetup_events"
//   - if the event series of this event does not exist anymore
//     - delete everything that is indexed by the event id
//     - delete event id from the event series
//     - delete the event id from "meetup_events"
// - go through "meetup_users" and "discord_users"
//   - if the Meetup <-> Discord link only exists in one direction, make it bi-directional again
//   - if the link does not exist at all anymore:
//     - delete everything that is indexed by the discord_user
//     - delete the discord_user from the "discord_users" set
//     - delete everything that is indexed by the meetup_user (including access token)
//     - delete the meetup_user from the "meetup_users" set
// - check "orphaned_roles" and "orphaned_channels"

pub fn vacuum(
    redis_client: redis::Client,
    discord_api: crate::discord_bot::CacheAndHttp,
) -> Result<(), crate::BoxedError> {
    let mut con = redis_client.get_connection()?;
    vacuum_discord_channels(&mut con, &discord_api)?;
    Ok(())
}

fn vacuum_discord_channels(
    con: &mut redis::Connection,
    discord_api: &crate::discord_bot::CacheAndHttp,
) -> Result<(), crate::BoxedError> {
    // Step 0: Figure out all the Discord channel IDs that Redis knows about.
    // We could just use the `discord_channels` set, but maybe the Redis state
    // is inconsistent, so we actually scan for all keys that might contain
    // a channel ID.
    let orphaned_discord_channel_ids: Vec<u64> = con.smembers("orphaned_discord_channels")?;
    // Step 1: Try to delete orphaned Discord channels
    for orphaned_channel_id in orphaned_discord_channel_ids {
        if channel_exists(ChannelId(orphaned_channel_id), discord_api)? {
            // Delete it from Discord
            discord_api.http.delete_channel(orphaned_channel_id)?;
        }
        // Remove it from orphaned_channels (only if Discord deletion was successful)
        let _: () = con.srem("orphaned_discord_channels", orphaned_channel_id)?;
    }
    // Step 2: Check all channel IDs we store in Redis.
    // If a channel does not exist anymore on Discord, remove it from Redis
    let discord_channel_ids = get_redis_discord_channel_ids(con)?;
    for channel_id in discord_channel_ids {
        if !channel_exists(ChannelId(channel_id), discord_api)? {
            // TODO: remove from Redis
            // redis::pipe()
            // .atomic()
            // .srem("discord_channels", channel_id)
            // .del(format!("event_series:{}:discord_channel", )
        }
    }
    Ok(())
}

fn get_redis_discord_channel_ids(
    con: &mut redis::Connection,
) -> Result<HashSet<u64>, crate::BoxedError> {
    let mut all_channel_ids = HashSet::new();
    // Relevant keys:
    // "discord_channels"
    // "event_series:{}:discord_channel"
    // "discord_role:{}:discord_channel"
    // "discord_host_role:{}:discord_channel"
    // "discord_channel:{}:event_series"
    // "discord_channel:{}:discord_role"
    // "discord_channel:{}:discord_host_role"
    // "discord_channel:{}:removed_users"
    // "discord_channel:{}:removed_hosts"
    {
        let discord_channels: Vec<u64> = con.smembers("discord_channels")?;
        all_channel_ids.extend(discord_channels);
    }
    let redis_channel_key_patterns = [
        "event_series:*:discord_channel",
        "discord_role:*:discord_channel",
        "discord_host_role:*:discord_channel",
    ];
    for redis_key_pattern in &redis_channel_key_patterns {
        let channel_ids: Vec<u64> = get_ids_from_key_values(con, redis_key_pattern)?;
        all_channel_ids.extend(channel_ids);
    }
    let redis_key_pattern_regex_pairs = [
        (
            "discord_channel:*:event_series",
            "^discord_channel:(?P<channel_id>[0-9]+):event_series$",
        ),
        (
            "discord_channel:*:discord_role",
            "^discord_channel:(?P<channel_id>[0-9]+):discord_role$",
        ),
        (
            "discord_channel:*:discord_host_role",
            "^discord_channel:(?P<channel_id>[0-9]+):discord_host_role$",
        ),
        (
            "discord_channel:*:removed_users",
            "^discord_channel:(?P<channel_id>[0-9]+):removed_users$",
        ),
        (
            "discord_channel:*:removed_hosts",
            "^discord_channel:(?P<channel_id>[0-9]+):removed_hosts$",
        ),
    ];
    for (redis_key_pattern, redis_key_regex) in &redis_key_pattern_regex_pairs {
        let redis_key_regex = Regex::new(redis_key_regex)?;
        let channel_ids: Vec<u64> =
            get_ids_from_key_names(con, redis_key_pattern, &redis_key_regex)?;
        all_channel_ids.extend(channel_ids);
    }
    Ok(all_channel_ids)
}

fn get_ids_from_key_values<T>(
    con: &mut redis::Connection,
    key_name_pattern: &str,
) -> Result<Vec<T>, crate::BoxedError>
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::error::Error,
{
    let redis_keys: Vec<String> = con.keys(key_name_pattern)?;
    let values: redis::RedisResult<Vec<Option<String>>> =
        redis_keys.into_iter().map(|key| con.get(key)).collect();
    let values: Vec<T> = values?
        .into_iter()
        .filter_map(|value| {
            if let Some(value) = value {
                match value.parse::<T>() {
                    Ok(value) => Some(value),
                    Err(err) => {
                        eprintln!("Vacuum: unparseable ID \"{}\": {}", value, err);
                        None
                    }
                }
            } else {
                None
            }
        })
        .collect();
    Ok(values)
}

fn get_ids_from_key_names<T>(
    con: &mut redis::Connection,
    key_name_pattern: &str,
    key_name_regex: &Regex,
) -> Result<Vec<T>, crate::BoxedError>
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::error::Error,
{
    let redis_keys: Vec<String> = con.keys(key_name_pattern)?;
    let ids: Vec<T> = redis_keys
        .iter()
        .filter_map(|key| match key_name_regex.captures(&key) {
            Some(captures) => captures.get(0),
            None => {
                eprintln!("Vacuum: regex didn't capture key name");
                None
            }
        })
        .filter_map(|id_match| match id_match.as_str().parse::<T>() {
            Ok(id) => Some(id),
            Err(err) => {
                eprintln!("Vacuum: unparseable ID \"{}\": {}", id_match.as_str(), err);
                None
            }
        })
        .collect();
    Ok(ids)
}

// TODO: check that the below method of checking existence actually works\
fn channel_exists(
    channel_id: ChannelId,
    discord_api: &crate::discord_bot::CacheAndHttp,
) -> Result<bool, crate::BoxedError> {
    match channel_id.to_channel(discord_api) {
        Ok(_) => Ok(true),
        Err(err) => {
            if let serenity::Error::Http(http_err) = &err {
                if let serenity::http::HttpError::UnsuccessfulRequest(response) = http_err.as_ref()
                {
                    if response.status_code == reqwest::StatusCode::NOT_FOUND {
                        Ok(false)
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
    }
}
