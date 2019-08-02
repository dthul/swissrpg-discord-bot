use lazy_static::lazy_static;
use redis;
use redis::{Commands, PipelineCommands};
use serenity::model::{
    channel::PermissionOverwrite, channel::PermissionOverwriteType, id::ChannelId, id::GuildId,
    id::RoleId, id::UserId, permissions::Permissions,
};
use simple_error::SimpleError;
use white_rabbit;

// Test server:
pub const GUILD_ID: GuildId = GuildId(601070848446824509);
pub const ORGANIZER_ID: RoleId = RoleId(606829075226689536);
// SwissRPG:
// pub const GUILD_ID: GuildId = GuildId(401856510709202945);
// pub const ORGANIZER_ID: RoleId = RoleId(539447673988841492);

lazy_static! {
    static ref EVENT_NAME_REGEX: regex::Regex =
        regex::Regex::new(r"^\s*(?P<name>.+?)\s*\[").unwrap();
}

// Syncs Discord with the state of the Redis database
pub fn create_sync_discord_task(
    redis_client: redis::Client,
    discord_api: impl serenity::http::CacheHttp + Send + Sync + 'static,
    bot_id: u64,
    recurring: bool,
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
        if recurring {
            white_rabbit::DateResult::Repeat(next_sync_time)
        } else {
            white_rabbit::DateResult::Done
        }
    }
}

pub fn sync_discord(
    redis_client: &redis::Client,
    discord_api: &impl serenity::http::CacheHttp,
    bot_id: u64,
) -> Result<(), crate::BoxedError> {
    let redis_series_key = "event_series";
    let mut con = redis_client.get_connection()?;
    let event_series: Vec<String> = con.smembers(redis_series_key)?;
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
  - create a channel if it doesn't exist yet
  - store it in Redis
  - create a player role if it doesn't exist yet
  - store it in Redis
  - create a host role if it doesn't exist yet
  - store it in Redis
  - adjust channel permission overwrites if necessary
  - find all enrolled Meetup users
  - map those Meetup users to Discord users if possible
  - assign the users (including hosts) the player role
  - assign the hosts the host role
*/
fn sync_event_series(
    series_id: &str,
    redis_connection: &mut redis::Connection,
    discord_api: &impl serenity::http::CacheHttp,
    bot_id: u64,
) -> Result<(), crate::BoxedError> {
    // Step 0: Figure out the title of this event series
    let redis_series_events_key = format!("event_series:{}:meetup_events", &series_id);
    let some_event_id: Option<String> = redis_connection
        .smembers(&redis_series_events_key)
        .map(|mut event_ids: Vec<String>| event_ids.pop())?;
    let some_event_id = match some_event_id {
        Some(id) => id,
        None => {
            println!("Event series \"{}\" seems to have no events associated with it, not syncing to Discord", series_id);
            return Ok(());
        }
    };
    let redis_event_key = format!("meetup_event:{}", some_event_id);
    let event_name: String = redis_connection.hget(&redis_event_key, "name")?;
    // Parse the series name from the event title
    let series_name = match EVENT_NAME_REGEX.captures(&event_name) {
        Some(captures) => captures.name("name").unwrap().as_str(),
        None => {
            return Err(Box::new(SimpleError::new(format!(
                "Could not extract a series name from the event \"{}\"",
                event_name
            ))))
        }
    };
    if series_name.len() < 2 || series_name.len() > 80 {
        return Err(Box::new(SimpleError::new(format!(
            "Channel name \"{}\" is too short or too long",
            series_name
        ))));
    }
    // Step 1: Sync the channel
    let channel_id = sync_channel(
        series_name,
        series_id,
        bot_id,
        redis_connection,
        discord_api,
    )?;
    // Step 2: Sync the channel's associated role
    let channel_role_id = sync_role(
        series_name,
        /*is_host_role*/ false,
        channel_id,
        redis_connection,
        discord_api,
    )?;
    // Step 3: Sync the channel's associated host role
    let host_role_name = format!("[Host] {}", series_name);
    let channel_host_role_id = sync_role(
        &host_role_name,
        /*is_host_role*/ true,
        channel_id,
        redis_connection,
        discord_api,
    )?;
    // Step 4: Sync the channel permissions
    sync_channel_permissions(
        channel_id,
        channel_role_id,
        channel_host_role_id,
        bot_id,
        discord_api,
    )?;
    // Step 5: Sync RSVP'd users
    sync_user_role_assignments(
        series_id,
        channel_role_id,
        channel_host_role_id,
        redis_connection,
        discord_api,
    )?;
    Ok(())
}

fn sync_role(
    role_name: &str,
    is_host_role: bool,
    channel_id: ChannelId,
    redis_connection: &mut redis::Connection,
    discord_api: &impl serenity::http::CacheHttp,
) -> Result<RoleId, crate::BoxedError> {
    let max_retries = 1;
    let mut current_num_try = 0;
    loop {
        if current_num_try > max_retries {
            return Err(Box::new(SimpleError::new(
                "Role sync failed, max retries reached",
            )));
        }
        current_num_try += 1;
        let role = sync_role_impl(
            role_name,
            is_host_role,
            channel_id,
            redis_connection,
            discord_api,
        )?;
        // Make sure that the role ID that was returned actually exists on Discord
        let guild_roles = discord_api.http().get_guild_roles(GUILD_ID.0)?;
        let guild_role = guild_roles
            .iter()
            .find(|guild_role| guild_role.id.0 == role.0);
        if guild_role.is_none() {
            // This role does not exist on Discord
            // Delete it from Redis and retry
            let redis_discord_roles_key = if is_host_role {
                "discord_host_roles"
            } else {
                "discord_roles"
            };
            let redis_role_channel_key = if is_host_role {
                format!("discord_host_role:{}:discord_channel", role.0)
            } else {
                format!("discord_role:{}:discord_channel", role.0)
            };
            let redis_channel_role_key = if is_host_role {
                format!("discord_channel:{}:discord_host_role", channel_id.0)
            } else {
                format!("discord_channel:{}:discord_role", channel_id.0)
            };
            redis::transaction(redis_connection, &[&redis_channel_role_key], |con, pipe| {
                let current_role: Option<u64> = con.get(&redis_channel_role_key)?;
                if current_role == Some(role.0) {
                    // Remove the broken role from Redis
                    pipe.del(&redis_channel_role_key)
                        .del(&redis_role_channel_key)
                        .srem(redis_discord_roles_key, role.0)
                        .query(con)
                } else {
                    // It seems like the role changed in the meantime
                    // Don't remove it and retry the loop instead
                    pipe.query(con)
                }
            })?;
            continue;
        } else {
            // The role exists on Discord, so everything is good
            return Ok(role);
        }
    }
}

fn sync_role_impl(
    role_name: &str,
    is_host_role: bool,
    channel_id: ChannelId,
    redis_connection: &mut redis::Connection,
    discord_api: &impl serenity::http::CacheHttp,
) -> Result<RoleId, crate::BoxedError> {
    let redis_channel_role_key = if is_host_role {
        format!("discord_channel:{}:discord_host_role", channel_id.0)
    } else {
        format!("discord_channel:{}:discord_role", channel_id.0)
    };
    // Check if the role already exists
    {
        let channel_role: Option<u64> = redis_connection.get(&redis_channel_role_key)?;
        if let Some(channel_role) = channel_role {
            // The role already exists
            return Ok(RoleId(channel_role));
        }
    }
    // The role doesn't exist yet -> try to create it
    let temp_channel_role = GUILD_ID.create_role(discord_api.http(), |role_builder| {
        role_builder
            .name(role_name)
            .permissions(Permissions::empty())
    })?;
    println!(
        "Discord event sync: created new temporary channel role {} \"{}\"",
        temp_channel_role.id.0, &temp_channel_role.name
    );
    let redis_discord_roles_key = if is_host_role {
        "discord_host_roles"
    } else {
        "discord_roles"
    };
    let redis_role_channel_key = if is_host_role {
        format!(
            "discord_host_role:{}:discord_channel",
            temp_channel_role.id.0
        )
    } else {
        format!("discord_role:{}:discord_channel", temp_channel_role.id.0)
    };
    let channel_role: redis::RedisResult<(u64,)> =
        redis::transaction(redis_connection, &[&redis_channel_role_key], |con, pipe| {
            let channel_role: Option<u64> = con.get(&redis_channel_role_key)?;
            if channel_role.is_some() {
                // Some role already exists in Redis -> return it
                pipe.get(&redis_channel_role_key).query(con)
            } else {
                // Persist the new role to Redis
                pipe.sadd(redis_discord_roles_key, temp_channel_role.id.0)
                    .ignore()
                    .set(&redis_channel_role_key, temp_channel_role.id.0)
                    .ignore()
                    .set(&redis_role_channel_key, channel_id.0)
                    .ignore()
                    .get(&redis_channel_role_key)
                    .query(con)
            }
        });
    // In case the Redis transaction failed or the role ID returned by Redis
    // doesn't match the newly created role, delete it
    let delete_temp_role = match channel_role {
        Ok((role,)) => role != temp_channel_role.id.0,
        Err(_) => true,
    };
    if delete_temp_role {
        println!("Trying to delete temporary channel role");
        match discord_api
            .http()
            .delete_role(GUILD_ID.0, temp_channel_role.id.0)
        {
            Ok(_) => println!("Successfully deleted temporary channel role"),
            Err(_) => {
                eprintln!(
                    "Could not delete temporary channel role {}",
                    temp_channel_role.id.0
                );
                // Try to persist the information to Redis that we have an orphaned role now
                match redis_connection.sadd("orphaned_discord_roles", temp_channel_role.id.0) {
                    Err(_) => eprintln!(
                        "Could not record orphaned channel role {}",
                        temp_channel_role.id.0
                    ),
                    Ok(()) => println!("Recorded orphaned channel role {}", temp_channel_role.id.0),
                }
            }
        }
    } else {
        println!("Persisted new channel role {}", temp_channel_role.id.0);
    }
    // Return the channel role we got from Redis, no matter
    // if it was newly created or already existing
    channel_role
        .map(|id| RoleId(id.0))
        .map_err(|err| Box::new(err) as crate::BoxedError)
}

fn sync_channel(
    channel_name: &str,
    event_series_id: &str,
    bot_id: u64,
    redis_connection: &mut redis::Connection,
    discord_api: &impl serenity::http::CacheHttp,
) -> Result<ChannelId, crate::BoxedError> {
    let max_retries = 1;
    let mut current_num_try = 0;
    loop {
        if current_num_try > max_retries {
            return Err(Box::new(SimpleError::new(
                "Channel sync failed, max retries reached",
            )));
        }
        current_num_try += 1;
        let channel = sync_channel_impl(
            channel_name,
            event_series_id,
            bot_id,
            redis_connection,
            discord_api,
        )?;
        // Make sure that the channel ID that was returned actually exists on Discord
        let channel_exists = match channel.to_channel(discord_api.http()) {
            Ok(_) => true,
            Err(err) => {
                if let serenity::Error::Http(http_err) = &err {
                    if let serenity::http::HttpError::UnsuccessfulRequest(response) =
                        http_err.as_ref()
                    {
                        if response.status_code == reqwest::StatusCode::NOT_FOUND {
                            false
                        } else {
                            return Err(Box::new(err));
                        }
                    } else {
                        return Err(Box::new(err));
                    }
                } else {
                    return Err(Box::new(err));
                }
            }
        };
        if !channel_exists {
            // This channel does not exist on Discord
            // Delete it from Redis and retry
            let redis_discord_channels_key = "discord_channels";
            let redis_channel_series_key = format!("discord_channel:{}:event_series", channel.0);
            let redis_series_channel_key =
                format!("event_series:{}:discord_channel", event_series_id);
            redis::transaction(
                redis_connection,
                &[&redis_series_channel_key],
                |con, pipe| {
                    let current_channel: Option<u64> = con.get(&redis_series_channel_key)?;
                    if current_channel == Some(channel.0) {
                        // Remove the broken channel from Redis
                        pipe.del(&redis_series_channel_key)
                            .del(&redis_channel_series_key)
                            .srem(redis_discord_channels_key, channel.0)
                            .query(con)
                    } else {
                        // It seems like the channel changed in the meantime
                        // Don't remove it and retry the loop instead
                        pipe.query(con)
                    }
                },
            )?;
            continue;
        } else {
            // The channel exists on Discord, so everything is good
            return Ok(channel);
        }
    }
}

fn sync_channel_impl(
    channel_name: &str,
    event_series_id: &str,
    bot_id: u64,
    redis_connection: &mut redis::Connection,
    discord_api: &impl serenity::http::CacheHttp,
) -> Result<ChannelId, crate::BoxedError> {
    let redis_series_channel_key = format!("event_series:{}:discord_channel", event_series_id);
    // Check if the channel already exists
    {
        let channel: Option<u64> = redis_connection.get(&redis_series_channel_key)?;
        if let Some(channel) = channel {
            // The channel already exists
            return Ok(ChannelId(channel));
        }
    }
    // The channel doesn't exist yet -> try to create it
    // The @everyone role has the same id as the guild
    let role_everyone_id = RoleId(GUILD_ID.0);
    let permission_overwrites = vec![
        PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::READ_MESSAGES,
            kind: PermissionOverwriteType::Role(role_everyone_id),
        },
        PermissionOverwrite {
            allow: Permissions::READ_MESSAGES,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Member(UserId(bot_id)),
        },
    ];
    let temp_channel = GUILD_ID.create_channel(discord_api.http(), |channel_builder| {
        channel_builder
            .name(channel_name)
            .permissions(permission_overwrites)
    })?;
    println!(
        "Discord event sync: created new temporary channel {} \"{}\"",
        temp_channel.id.0, &temp_channel.name
    );
    let redis_discord_channels_key = "discord_channels";
    let redis_channel_series_key = format!("discord_channel:{}:event_series", temp_channel.id.0);
    let channel: redis::RedisResult<(u64,)> = redis::transaction(
        redis_connection,
        &[&redis_series_channel_key],
        |con, pipe| {
            let channel: Option<u64> = con.get(&redis_series_channel_key)?;
            if channel.is_some() {
                // Some channel already exists in Redis -> return it
                pipe.get(&redis_series_channel_key).query(con)
            } else {
                // Persist the new channel to Redis
                pipe.sadd(redis_discord_channels_key, temp_channel.id.0)
                    .ignore()
                    .set(&redis_series_channel_key, temp_channel.id.0)
                    .ignore()
                    .set(&redis_channel_series_key, event_series_id)
                    .ignore()
                    .get(&redis_series_channel_key)
                    .query(con)
            }
        },
    );
    // In case the Redis transaction failed or the channel ID returned by Redis
    // doesn't match the newly created channel, delete it
    let delete_temp_channel = match channel {
        Ok((channel,)) => channel != temp_channel.id.0,
        Err(_) => true,
    };
    if delete_temp_channel {
        println!("Trying to delete temporary channel");
        match discord_api.http().delete_channel(temp_channel.id.0) {
            Ok(_) => println!("Successfully deleted temporary channel"),
            Err(_) => {
                eprintln!("Could not delete temporary channel {}", temp_channel.id.0);
                // Try to persist the information to Redis that we have an orphaned channel now
                match redis_connection.sadd("orphaned_discord_channels", temp_channel.id.0) {
                    Err(_) => eprintln!("Could not record orphaned channel {}", temp_channel.id.0),
                    Ok(()) => println!("Recorded orphaned channel {}", temp_channel.id.0),
                }
            }
        }
    } else {
        println!("Persisted new channel {}", temp_channel.id.0);
    }
    // Return the channel we got from Redis, no matter
    // if it was newly created or already existing
    channel
        .map(|id| ChannelId(id.0))
        .map_err(|err| Box::new(err) as crate::BoxedError)
}

// Makes sure that the Discord channel has the appropriate permission
// overwrites for the channel's role and host role.
// Specifically does not remove any additional permission overwrites
// that the channel might have.
fn sync_channel_permissions(
    channel_id: ChannelId,
    role_id: RoleId,
    host_role_id: RoleId,
    bot_id: u64,
    discord_api: &impl serenity::http::CacheHttp,
) -> Result<(), crate::BoxedError> {
    // The @everyone role has the same id as the guild
    let role_everyone_id = RoleId(GUILD_ID.0);
    // Make this channel private.
    // This is achieved by denying @everyone the READ_MESSAGES permission
    // but allowing the now role the READ_MESSAGES permission.
    // see: https://support.discordapp.com/hc/en-us/articles/206143877-How-do-I-set-up-a-Role-Exclusive-channel-
    let permission_overwrites = [
        PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::READ_MESSAGES,
            kind: PermissionOverwriteType::Role(role_everyone_id),
        },
        PermissionOverwrite {
            allow: Permissions::READ_MESSAGES,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Member(UserId(bot_id)),
        },
        PermissionOverwrite {
            allow: Permissions::READ_MESSAGES | Permissions::MENTION_EVERYONE,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Role(role_id),
        },
        PermissionOverwrite {
            allow: Permissions::READ_MESSAGES
                | Permissions::MENTION_EVERYONE
                | Permissions::MANAGE_MESSAGES,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Role(host_role_id),
        },
    ];
    for permission_overwrite in &permission_overwrites {
        channel_id.create_permission(discord_api.http(), permission_overwrite)?;
    }
    Ok(())
}

fn sync_user_role_assignments(
    event_series_id: &str,
    user_role: RoleId,
    host_role: RoleId,
    redis_connection: &mut redis::Connection,
    discord_api: &impl serenity::http::CacheHttp,
) -> Result<(), crate::BoxedError> {
    // First, find all events belonging to this event series
    let redis_series_events_key = format!("event_series:{}:meetup_events", &event_series_id);
    let event_ids: Vec<String> = redis_connection.smembers(&redis_series_events_key)?;
    if event_ids.is_empty() {
        println!("Event series \"{}\" seems to have no events associated with it, not syncing to Discord", event_series_id);
        return Ok(());
    }
    // Then, find all Meetup users RSVP'd to those events
    let redis_event_users_keys: Vec<_> = event_ids
        .iter()
        .map(|event_id| format!("meetup_event:{}:meetup_users", event_id))
        .collect();
    let redis_event_hosts_keys: Vec<_> = event_ids
        .iter()
        .map(|event_id| format!("meetup_event:{}:meetup_hosts", event_id))
        .collect();
    let (meetup_user_ids, meetup_host_ids): (Vec<u64>, Vec<u64>) = redis::pipe()
        .sinter(redis_event_users_keys)
        .sinter(redis_event_hosts_keys)
        .query(redis_connection)?;
    // Now, try to associate the RSVP'd Meetup users with Discord users
    let redis_meetup_user_discord_keys: Vec<_> = meetup_user_ids
        .into_iter()
        .map(|meetup_id| format!("meetup_user:{}:discord_user", meetup_id))
        .collect();
    let redis_meetup_host_discord_keys: Vec<_> = meetup_host_ids
        .into_iter()
        .map(|meetup_id| format!("meetup_user:{}:discord_user", meetup_id))
        .collect();
    let (discord_user_ids, discord_host_ids): (Vec<Option<u64>>, Vec<Option<u64>>) = redis::pipe()
        .get(redis_meetup_user_discord_keys)
        .get(redis_meetup_host_discord_keys)
        .query(redis_connection)?;
    // Filter the None values
    let discord_user_ids: Vec<_> = discord_user_ids.into_iter().filter_map(|id| id).collect();
    let discord_host_ids: Vec<_> = discord_host_ids.into_iter().filter_map(|id| id).collect();
    // Lastly, actually assign the roles to the Discord users
    for user_id in discord_user_ids {
        match discord_api
            .http()
            .add_member_role(GUILD_ID.0, user_id, user_role.0)
        {
            Ok(_) => println!("Assigned user {} to role {}", user_id, user_role.0),
            Err(err) => eprintln!(
                "Could not assign user {} to role {}: {}",
                user_id, user_role.0, err
            ),
        }
    }
    for host_id in discord_host_ids {
        match discord_api
            .http()
            .add_member_role(GUILD_ID.0, host_id, host_role.0)
        {
            Ok(_) => println!("Assigned user {} to host role {}", host_id, host_role.0),
            Err(err) => eprintln!(
                "Could not assign user {} to host role {}: {}",
                host_id, host_role.0, err
            ),
        }
    }
    Ok(())
}

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
