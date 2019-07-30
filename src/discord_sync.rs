use lazy_static::lazy_static;
use redis;
use redis::{Commands, PipelineCommands};
use serenity::model::{
    channel::PermissionOverwrite, channel::PermissionOverwriteType, id::GuildId, id::RoleId,
    id::UserId, permissions::Permissions,
};
use simple_error::SimpleError;
use std::sync::Arc;
use white_rabbit;

pub const GUILD_ID: GuildId = GuildId(601070848446824509); // Test server
                                                           // pub const GUILD_ID: u64 = 401856510709202945; // SwissRPG
lazy_static! {
    static ref EVENT_NAME_REGEX: regex::Regex =
        regex::Regex::new(r"^\s*(?P<name>.+?)\s*\[").unwrap();
    static ref WHITESPACE_REGEX: regex::Regex = regex::Regex::new(r"\s+").unwrap();
}

// Syncs Discord with the state of the Redis database
pub fn create_sync_discord_task(
    redis_client: redis::Client,
    discord_api: Arc<serenity::CacheAndHttp>,
    bot_id: u64,
) -> impl FnMut(&mut white_rabbit::Context) -> white_rabbit::DateResult + Send + Sync + 'static {
    move |_ctx| {
        let next_sync_time = match sync_discord(&redis_client, &discord_api, bot_id) {
            Err(err) => {
                eprintln!("Discord syncing task failed: {}", err);
                // Retry in a minute
                white_rabbit::Utc::now() + white_rabbit::Duration::minutes(1)
            }
            _ => {
                // Do another sync in 15 minutes
                white_rabbit::Utc::now() + white_rabbit::Duration::minutes(15)
            }
        };
        white_rabbit::DateResult::Repeat(next_sync_time)
    }
}

fn sync_discord(
    redis_client: &redis::Client,
    discord_api: &serenity::CacheAndHttp,
    bot_id: u64,
) -> Result<(), crate::BoxedError> {
    let redis_series_key = "event_series";
    let mut con = redis_client.get_connection()?;
    let event_series: Vec<String> = con.get(redis_series_key)?;
    let mut some_failed = false;
    for series in &event_series {
        if let Err(err) = sync_event_series(series, &mut con, discord_api, bot_id) {
            some_failed = true;
            eprintln!("Discord event series syncing task failed: {}", err);
        }
    }
    if some_failed {
        Err(Box::new(SimpleError::new(
            "One or more discord event series syncs failed",
        )))
    } else {
        Ok(())
    }
}

/*
For each event series:
  - "lock" this event series in Redis
  - create a channel if it doesn't exist yet
  - store it in Redis
  - create a player role if it doesn't exist yet
  - store it in Redis
  - create a host role if it doesn't exist yet
  - store it in Redis
  - find all enrolled Meetup users
  - map those Meetup users to Discord users if possible
  - assign the users (including hosts) the player role
  - assign the hosts the host role
*/
fn sync_event_series(
    series_id: &str,
    redis_connection: &mut redis::Connection,
    discord_api: &serenity::CacheAndHttp,
    bot_id: u64,
) -> Result<(), crate::BoxedError> {
    let redis_series_lock_key = format!("event_series:{}:lock", series_id);
    // Try to acquire the "lock" for this event series
    let mut acquired_lock = false;
    let _: () = redis::transaction(redis_connection, &[&redis_series_lock_key], |con, pipe| {
        let is_already_locked = {
            let locking_value: Option<String> = con.get(&redis_series_lock_key)?;
            locking_value.is_some()
        };
        if is_already_locked {
            acquired_lock = false;
            pipe.query(con)
        } else {
            acquired_lock = true;
            // Lock the event series for at most 60 seconds
            // in case we don't manage to unlock it for some reason
            pipe.set(&redis_series_lock_key, "locked")
                .expire(&redis_series_lock_key, 60)
                .ignore()
                .query(con)
        }
    })?;
    if !acquired_lock {
        return Err(Box::new(SimpleError::new(
            "Could not acquire the lock for the event series",
        )));
    }
    // Now that we hold the lock, do the actual sync
    let sync_result = sync_locked_event_series(series_id, redis_connection, discord_api, bot_id);
    // Unlock the event series
    let _: redis::RedisResult<()> = redis_connection.del(&redis_series_lock_key);
    sync_result
}

fn sync_locked_event_series(
    series_id: &str,
    redis_connection: &mut redis::Connection,
    discord_api: &serenity::CacheAndHttp,
    bot_id: u64,
) -> Result<(), crate::BoxedError> {
    let redis_series_channel_key = format!("event_series:{}:discord_channel", series_id);
    // Check whether we need to create a channel
    let channel_info = {
        let channel_id: Option<u64> = redis_connection.get(&redis_series_channel_key)?;
        if let Some(channel_id) = channel_id {
            let redis_channel_role_key = format!("discord_channel:{}:discord_role", channel_id);
            let redis_channel_host_role_key =
                format!("discord_channel:{}:discord_host_role", channel_id);
            let ids: (Option<u64>, Option<u64>) = redis::pipe()
                .get(&redis_channel_role_key)
                .get(&redis_channel_host_role_key)
                .query(redis_connection)?;
            match ids {
                (Some(role_id), Some(host_role_id)) => Some(ChannelInfo {
                    channel_id: channel_id,
                    role_id: role_id,
                    host_role_id: host_role_id,
                }),
                _ => {
                    return Err(Box::new(SimpleError::new(format!(
                        "Event series \"{}\" has an inconsistent channel / role id state",
                        series_id
                    ))))
                }
            }
        } else {
            None
        }
    };
    let channel_info = match channel_info {
        Some(channel_info) => channel_info,
        None => {
            // Create a new channel
            let redis_series_events_key = format!("event_series:{}:meetup_events", &series_id);
            let event_name: String = redis_connection
                .get::<_, Vec<String>>(&redis_series_events_key)
                .and_then(|event_ids: Vec<String>| {
                    // Redis doesn't store empty sets, so this Vec should never be empty
                    let redis_event_key = format!("meetup_event:{}", event_ids[0]);
                    redis_connection.hget::<_, _, String>(&redis_event_key, "name")
                })?;
            let channel_name = match EVENT_NAME_REGEX.captures(&event_name) {
                Some(captures) => captures.name("name").unwrap().as_str(),
                None => {
                    return Err(Box::new(SimpleError::new(format!(
                        "Could not extract a channel name from the event \"{}\"",
                        event_name
                    ))))
                }
            };
            if channel_name.len() < 2 || channel_name.len() > 80 {
                return Err(Box::new(SimpleError::new(format!(
                    "Channel name \"{}\" is too long",
                    channel_name
                ))));
            }
            println!(
                "Discord event sync: Syncing event \"{}\" to channel \"{}\"",
                event_name, channel_name
            );
            let channel_info = create_channel(channel_name, discord_api, bot_id)?;
            // Add the info to Redis
            let redis_channels_key = "discord_channels";
            let redis_discord_roles_key = "discord_roles";
            let redis_discord_host_roles_key = "discord_host_roles";
            let redis_channel_series_key =
                format!("discord_channel:{}:event_series", channel_info.channel_id);
            let redis_channel_role_key =
                format!("discord_channel:{}:discord_role", channel_info.channel_id);
            let redis_channel_host_role_key = format!(
                "discord_channel:{}:discord_host_role",
                channel_info.channel_id
            );
            let redis_role_channel_key =
                format!("discord_role:{}:discord_channel", channel_info.role_id);
            let redis_host_role_channel_key = format!(
                "discord_host_role:{}:discord_channel",
                channel_info.host_role_id
            );
            let _: () = redis::pipe()
                .atomic()
                .set(&redis_series_channel_key, channel_info.channel_id)
                .sadd(redis_channels_key, channel_info.channel_id)
                .sadd(redis_discord_roles_key, channel_info.role_id)
                .sadd(redis_discord_host_roles_key, channel_info.host_role_id)
                .set(&redis_channel_series_key, series_id)
                .set(&redis_channel_role_key, channel_info.role_id)
                .set(&redis_channel_host_role_key, channel_info.host_role_id)
                .set(&redis_role_channel_key, channel_info.channel_id)
                .set(&redis_host_role_channel_key, channel_info.host_role_id)
                .ignore()
                .query(redis_connection)?;
            // TODO: delete newly created Discord roles and channel if this Redis query fails
            channel_info
        }
    };
    // TODO: add RSVP'd users to the channel
    Ok(())
}

struct ChannelInfo {
    channel_id: u64,
    role_id: u64,
    host_role_id: u64,
}

fn create_channel(
    channel_name: &str,
    discord_api: &serenity::CacheAndHttp,
    bot_id: u64,
) -> Result<ChannelInfo, crate::BoxedError> {
    let channel_role = GUILD_ID.create_role(&discord_api.http, |role_builder| {
        role_builder
            .name(channel_name)
            .permissions(Permissions::empty())
    })?;
    println!(
        "Discord event sync: created new channel role \"{}\"",
        &channel_role.name
    );
    let delete_channel_role = || {
        eprint!("Trying to delete channel role... ");
        match discord_api.http.delete_role(GUILD_ID.0, channel_role.id.0) {
            Ok(_) => eprintln!("Successfully deleted channel role"),
            Err(_) => eprintln!("Could not dekete channel role"),
        }
    };
    let channel_host_role = match GUILD_ID.create_role(&discord_api.http, |role_builder| {
        role_builder
            .name(format!("Host {}", channel_name))
            .permissions(Permissions::empty())
    }) {
        Ok(role) => role,
        Err(err) => {
            delete_channel_role();
            return Err(Box::new(err));
        }
    };
    println!(
        "Discord event sync: created new channel host role \"{}\"",
        &channel_host_role.name
    );
    let delete_channel_host_role = || {
        eprint!("Trying to delete channel host role... ");
        match discord_api
            .http
            .delete_role(GUILD_ID.0, channel_host_role.id.0)
        {
            Ok(_) => eprintln!("Successfully deleted channel host role"),
            Err(_) => eprintln!("Could not dekete channel host role"),
        }
    };
    // The @everyone role has the same id as the guild
    let role_everyone_id = RoleId(GUILD_ID.0);
    // Make this channel private.
    // This is achieved by denying @everyone the READ_MESSAGES permission
    // but allowing the now role the READ_MESSAGES permission.
    // see: https://support.discordapp.com/hc/en-us/articles/206143877-How-do-I-set-up-a-Role-Exclusive-channel-
    let permission_overwrites = vec![
        PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::READ_MESSAGES,
            kind: PermissionOverwriteType::Role(role_everyone_id),
        },
        PermissionOverwrite {
            allow: Permissions::READ_MESSAGES | Permissions::MENTION_EVERYONE,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Role(channel_role.id),
        },
        PermissionOverwrite {
            allow: Permissions::READ_MESSAGES | Permissions::MENTION_EVERYONE,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Role(channel_host_role.id),
        },
        PermissionOverwrite {
            allow: Permissions::READ_MESSAGES,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Member(UserId(bot_id)),
        },
    ];
    let channel = match GUILD_ID.create_channel(&discord_api.http, |channel_builder| {
        channel_builder
            .name(channel_name)
            .permissions(permission_overwrites)
    }) {
        Ok(channel) => channel,
        Err(err) => {
            delete_channel_role();
            delete_channel_host_role();
            return Err(Box::new(err));
        }
    };
    Ok(ChannelInfo {
        channel_id: channel.id.0,
        role_id: channel_role.id.0,
        host_role_id: channel_host_role.id.0,
    })
}
