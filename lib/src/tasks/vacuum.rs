use redis::Commands;
use regex::Regex;
use serenity::model::id::{ChannelId, RoleId};
use std::collections::HashSet;

// Vacuum task:
// - go through "discord_channels"
//   - if the underlying Discord channel doesn't exist anymore:
//     - first, remove everything that is indexed by the channel ID in Redis (like discord_channel:{id}:host_role)
//     - then, remove the channel id from the "discord_channels" set
// - go through "discord_roles" (and analogous "discord_host_roles")
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
    discord_api: crate::discord::CacheAndHttp,
) -> Result<(), crate::BoxedError> {
    let mut con = redis_client.get_connection()?;
    vacuum_discord_channels(&mut con, &discord_api)?;
    vacuum_discord_roles(&mut con, &discord_api)?;
    Ok(())
}

fn vacuum_discord_channels(
    con: &mut redis::Connection,
    discord_api: &crate::discord::CacheAndHttp,
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
        if !channel_exists(channel_id, discord_api)? {
            // Remove obsolete "discord_channel:{}:..." keys from Redis
            let _: () = redis::pipe()
                .atomic()
                .srem("discord_channels", channel_id.0)
                .del(format!("discord_channel:{}:event_series", channel_id.0))
                .del(format!("discord_channel:{}:discord_role", channel_id.0))
                .del(format!(
                    "discord_channel:{}:discord_host_role",
                    channel_id.0
                ))
                .del(format!("discord_channel:{}:removed_users", channel_id.0))
                .del(format!("discord_channel:{}:removed_hosts", channel_id.0))
                .del(format!("discord_channel:{}:expiration_time", channel_id.0))
                .del(format!(
                    "discord_channel:{}:last_expiration_reminder_time",
                    channel_id.0
                ))
                .del(format!("discord_channel:{}:snooze_until", channel_id.0))
                .del(format!("discord_channel:{}:deletion_time", channel_id.0))
                .query(con)?;
        }
    }
    Ok(())
}

fn vacuum_discord_roles(
    con: &mut redis::Connection,
    discord_api: &crate::discord::CacheAndHttp,
) -> Result<(), crate::BoxedError> {
    // We could just use the `discord_roles` set, but maybe the Redis state
    // is inconsistent, so we actually scan for all keys that might contain
    // a role ID.
    let orphaned_discord_role_ids: Vec<u64> = con.smembers("orphaned_discord_roles")?;
    // Step 1: Try to delete orphaned Discord roles
    for orphaned_role_id in orphaned_discord_role_ids {
        if role_exists(RoleId(orphaned_role_id), discord_api) {
            // Delete it from Discord
            discord_api
                .http
                .delete_role(crate::discord::sync::ids::GUILD_ID.0, orphaned_role_id)?;
        }
        // Remove it from orphaned_roles (only if Discord deletion was successful)
        let _: () = con.srem("orphaned_discord_roles", orphaned_role_id)?;
    }
    // Step 2: Check all role IDs we store in Redis.
    // If a role does not exist anymore on Discord, remove it from Redis
    let discord_role_ids = get_redis_discord_role_ids(con)?;
    for role in discord_role_ids {
        if !channel_exists(channel_id, discord_api)? {
            // Remove obsolete "discord_channel:{}:..." keys from Redis
            let _: () = redis::pipe()
                .atomic()
                .srem("discord_channels", channel_id.0)
                .del(format!("discord_channel:{}:event_series", channel_id.0))
                .del(format!("discord_channel:{}:discord_role", channel_id.0))
                .del(format!(
                    "discord_channel:{}:discord_host_role",
                    channel_id.0
                ))
                .del(format!("discord_channel:{}:removed_users", channel_id.0))
                .del(format!("discord_channel:{}:removed_hosts", channel_id.0))
                .del(format!("discord_channel:{}:expiration_time", channel_id.0))
                .del(format!(
                    "discord_channel:{}:last_expiration_reminder_time",
                    channel_id.0
                ))
                .del(format!("discord_channel:{}:snooze_until", channel_id.0))
                .del(format!("discord_channel:{}:deletion_time", channel_id.0))
                .query(con)?;
        }
    }
}

fn get_redis_discord_channel_ids(
    con: &mut redis::Connection,
) -> Result<HashSet<ChannelId>, crate::BoxedError> {
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
    // "discord_channel:{}:expiration_time"
    // "discord_channel:{}:last_expiration_reminder_time"
    // "discord_channel:{}:snooze_until"
    // "discord_channel:{}:deletion_time"
    {
        let discord_channels: Vec<u64> = con.smembers("discord_channels")?;
        all_channel_ids.extend(discord_channels.into_iter().map(ChannelId));
    }
    let redis_channel_key_patterns = [
        "event_series:*:discord_channel",
        "discord_role:*:discord_channel",
        "discord_host_role:*:discord_channel",
    ];
    for redis_key_pattern in &redis_channel_key_patterns {
        let channel_ids: Vec<u64> = get_ids_from_key_values(con, redis_key_pattern)?;
        all_channel_ids.extend(channel_ids.into_iter().map(ChannelId));
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
        (
            "discord_channel:*:expiration_time",
            "^discord_channel:(?P<channel_id>[0-9]+):expiration_time$",
        ),
        (
            "discord_channel:*:last_expiration_reminder_time",
            "^discord_channel:(?P<channel_id>[0-9]+):last_expiration_reminder_time$",
        ),
        (
            "discord_channel:*:snooze_until",
            "^discord_channel:(?P<channel_id>[0-9]+):snooze_until$",
        ),
        (
            "discord_channel:*:deletion_time",
            "^discord_channel:(?P<channel_id>[0-9]+):deletion_time$",
        ),
    ];
    for (redis_key_pattern, redis_key_regex) in &redis_key_pattern_regex_pairs {
        let redis_key_regex = Regex::new(redis_key_regex)?;
        let channel_ids: Vec<u64> =
            get_ids_from_key_names(con, redis_key_pattern, &redis_key_regex)?;
        all_channel_ids.extend(channel_ids.into_iter().map(ChannelId));
    }
    Ok(all_channel_ids)
}

fn get_redis_discord_role_ids(
    con: &mut redis::Connection,
) -> Result<HashSet<RoleId>, crate::BoxedError> {
    let mut all_role_ids = HashSet::new();
    // Relevant keys:
    // "discord_roles"
    // "discord_channel:{}:discord_role"
    // "discord_role:{}:discord_channel"
    {
        let discord_roles: Vec<u64> = con.smembers("discord_roles")?;
        all_role_ids.extend(discord_roles.into_iter().map(RoleId));
    }
    let redis_role_key_patterns = ["discord_channel:*:discord_role"];
    for redis_key_pattern in &redis_role_key_patterns {
        let role_ids: Vec<u64> = get_ids_from_key_values(con, redis_key_pattern)?;
        all_role_ids.extend(role_ids.into_iter().map(RoleId));
    }
    let redis_key_pattern_regex_pairs = [(
        "discord_role:*:discord_channel",
        "^discord_role:(?P<role_id>[0-9]+):discord_channel$",
    )];
    for (redis_key_pattern, redis_key_regex) in &redis_key_pattern_regex_pairs {
        let redis_key_regex = Regex::new(redis_key_regex)?;
        let role_ids: Vec<u64> = get_ids_from_key_names(con, redis_key_pattern, &redis_key_regex)?;
        all_role_ids.extend(role_ids.into_iter().map(RoleId));
    }
    Ok(all_role_ids)
}

fn get_redis_discord_host_role_ids(
    con: &mut redis::Connection,
) -> Result<HashSet<RoleId>, crate::BoxedError> {
    let mut all_host_role_ids = HashSet::new();
    // Relevant keys:
    // "discord_host_roles"
    // "discord_channel:{}:discord_host_role"
    // "discord_host_role:{}:discord_channel"
    {
        let discord_host_roles: Vec<u64> = con.smembers("discord_host_roles")?;
        all_host_role_ids.extend(discord_host_roles.into_iter().map(RoleId));
    }
    let redis_role_key_patterns = ["discord_channel:*:discord_host_role"];
    for redis_key_pattern in &redis_role_key_patterns {
        let host_role_ids: Vec<u64> = get_ids_from_key_values(con, redis_key_pattern)?;
        all_host_role_ids.extend(host_role_ids.into_iter().map(RoleId));
    }
    let redis_key_pattern_regex_pairs = [(
        "discord_host_role:*:discord_channel",
        "^discord_host_role:(?P<host_role_id>[0-9]+):discord_channel$",
    )];
    for (redis_key_pattern, redis_key_regex) in &redis_key_pattern_regex_pairs {
        let redis_key_regex = Regex::new(redis_key_regex)?;
        let host_role_ids: Vec<u64> =
            get_ids_from_key_names(con, redis_key_pattern, &redis_key_regex)?;
        all_host_role_ids.extend(host_role_ids.into_iter().map(RoleId));
    }
    Ok(all_host_role_ids)
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

// TODO: check that the below method of checking existence actually works
fn channel_exists(
    channel_id: ChannelId,
    discord_api: &crate::discord::CacheAndHttp,
) -> Result<bool, crate::BoxedError> {
    match channel_id.to_channel(discord_api) {
        Ok(_) => Ok(true),
        Err(err) => {
            if let serenity::Error::Http(http_err) = &err {
                if let serenity::http::HttpError::UnsuccessfulRequest(response) = http_err.as_ref()
                {
                    if response.status_code == reqwest::StatusCode::NOT_FOUND {
                        return Ok(false);
                    }
                }
            }
            return Err(err.into());
        }
    }
}

// TODO: check that the below method of checking existence actually works
fn role_exists(role_id: RoleId, discord_api: &crate::discord::CacheAndHttp) -> bool {
    role_id.to_role_cached(&discord_api.cache).is_some()
}
