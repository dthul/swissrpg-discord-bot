use common::strings;
use futures::future::TryFutureExt;
use futures_util::lock::Mutex as AsyncMutex;
use serenity::{
    model::{
        channel::{Channel, Message},
        gateway::Ready,
        guild::Member,
        id::{GuildId, UserId},
    },
    prelude::*,
};
use std::{sync::Arc, time::Duration};
use tokio::prelude::*;

pub fn create_discord_client(
    discord_token: &str,
    redis_client: redis::Client,
    async_meetup_client: Arc<AsyncMutex<Option<Arc<meetup::api::AsyncClient>>>>,
    task_scheduler: Arc<AsyncMutex<white_rabbit::Scheduler>>,
    futures_spawner: futures_channel::mpsc::Sender<common::BoxedFuture<()>>,
    oauth2_consumer: Arc<meetup::oauth2::OAuth2Consumer>,
) -> Result<Client, crate::Error> {
    let redis_connection = redis_client.get_connection()?;

    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    let client = Client::new(&discord_token, Handler)?;

    // We will fetch the bot's id.
    let (bot_id, bot_name) = client
        .cache_and_http
        .http
        .get_current_application_info()
        .map(|info| (info.id, info.name))?;

    // pre-compile the regexes
    let regexes = crate::bot_commands::compile_regexes(bot_id.0);

    // Store the bot's id in the client for easy access
    {
        let mut data = client.data.write();
        data.insert::<BotIdKey>(bot_id);
        data.insert::<BotNameKey>(bot_name);
        data.insert::<RedisConnectionKey>(Arc::new(Mutex::new(redis_connection)));
        data.insert::<RegexesKey>(Arc::new(regexes));
        data.insert::<AsyncMeetupClientKey>(async_meetup_client);
        data.insert::<RedisClientKey>(redis_client);
        data.insert::<TaskSchedulerKey>(task_scheduler);
        data.insert::<FuturesSpawnerKey>(futures_spawner);
        data.insert::<OAuth2ConsumerKey>(oauth2_consumer);
    }

    Ok(client)
}

pub struct BotIdKey;
impl TypeMapKey for BotIdKey {
    type Value = UserId;
}

pub struct BotNameKey;
impl TypeMapKey for BotNameKey {
    type Value = String;
}

pub struct RedisConnectionKey;
impl TypeMapKey for RedisConnectionKey {
    type Value = Arc<Mutex<redis::Connection>>;
}

pub struct RegexesKey;
impl TypeMapKey for RegexesKey {
    type Value = Arc<crate::bot_commands::Regexes>;
}

pub struct AsyncMeetupClientKey;
impl TypeMapKey for AsyncMeetupClientKey {
    type Value = Arc<AsyncMutex<Option<Arc<meetup::api::AsyncClient>>>>;
}

pub struct RedisClientKey;
impl TypeMapKey for RedisClientKey {
    type Value = redis::Client;
}

pub struct TaskSchedulerKey;
impl TypeMapKey for TaskSchedulerKey {
    type Value = Arc<AsyncMutex<white_rabbit::Scheduler>>;
}

pub struct FuturesSpawnerKey;
impl TypeMapKey for FuturesSpawnerKey {
    type Value = futures_channel::mpsc::Sender<common::BoxedFuture<()>>;
}

pub struct OAuth2ConsumerKey;
impl TypeMapKey for OAuth2ConsumerKey {
    type Value = Arc<meetup::oauth2::OAuth2Consumer>;
}

#[derive(Clone)]
pub struct CacheAndHttp {
    pub cache: serenity::cache::CacheRwLock,
    pub http: Arc<serenity::http::raw::Http>,
}

impl serenity::http::CacheHttp for CacheAndHttp {
    fn cache(&self) -> Option<&serenity::cache::CacheRwLock> {
        Some(&self.cache)
    }
    fn http(&self) -> &serenity::http::raw::Http {
        &self.http
    }
}

impl serenity::http::CacheHttp for &CacheAndHttp {
    fn cache(&self) -> Option<&serenity::cache::CacheRwLock> {
        Some(&self.cache)
    }
    fn http(&self) -> &serenity::http::raw::Http {
        &self.http
    }
}

pub struct Handler;

impl EventHandler for Handler {
    // Set a handler for the `message` event - so that whenever a new message
    // is received - the closure (or function) passed will be called.
    //
    // Event handlers are dispatched through a threadpool, and so multiple
    // events can be dispatched simultaneously.
    fn message(&self, ctx: Context, msg: Message) {
        let (bot_id, regexes) = {
            let data = ctx.data.read();
            let regexes = data
                .get::<RegexesKey>()
                .expect("Regexes were not compiled")
                .clone();
            let bot_id = data.get::<BotIdKey>().expect("Bot ID was not set").clone();
            (bot_id, regexes)
        };
        // Ignore all messages written by the bot itself
        if msg.author.id == bot_id {
            return;
        }
        // Ignore all messages that might have come from another guild
        // (shouldn't happen) but who knows
        if let Some(guild_id) = msg.guild_id {
            if guild_id != crate::sync::ids::GUILD_ID {
                return;
            }
        }
        let channel = match msg.channel_id.to_channel(&ctx) {
            Ok(channel) => channel,
            _ => return,
        };
        // Is this a direct message to the bot?
        let mut is_dm = match channel {
            Channel::Private(_) => true,
            _ => false,
        };
        // If the message is not a direct message and does not start with a
        // mention of the bot, ignore it
        if !is_dm && !msg.content.starts_with(&regexes.bot_mention) {
            return;
        }
        // If the message is a direct message but starts with a mention of the bot,
        // switch to the non-DM parsing
        if is_dm && msg.content.starts_with(&regexes.bot_mention) {
            is_dm = false;
        }
        // TODO: might want to use a RegexSet here to speed up matching
        if regexes.stop_bot_admin(is_dm).is_match(&msg.content) {
            // This is only for bot_admins
            if !msg
                .author
                .has_role(
                    &ctx,
                    crate::sync::ids::GUILD_ID,
                    crate::sync::ids::BOT_ADMIN_ID,
                )
                .unwrap_or(false)
            {
                let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_BOT_ADMIN);
                return;
            }
            std::process::Command::new("sudo")
                .args(&["systemctl", "stop", "bot"])
                .output()
                .expect("Could not stop the bot");
        } else if regexes.link_meetup(is_dm).is_match(&msg.content) {
            let user_id = msg.author.id.0;
            match Self::link_meetup(&ctx, &msg, user_id) {
                Err(err) => {
                    eprintln!("Error: {}", err);
                    let _ = msg.channel_id.say(&ctx.http, strings::UNSPECIFIED_ERROR);
                    return;
                }
                _ => return,
            }
        } else if let Some(captures) = regexes.link_meetup_bot_admin(is_dm).captures(&msg.content) {
            // This is only for bot_admins
            if !msg
                .author
                .has_role(
                    &ctx,
                    crate::sync::ids::GUILD_ID,
                    crate::sync::ids::BOT_ADMIN_ID,
                )
                .unwrap_or(false)
            {
                let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_BOT_ADMIN);
                return;
            }
            let discord_id = captures.name("mention_id").unwrap().as_str();
            let meetup_id = captures.name("meetupid").unwrap().as_str();
            // Try to convert the specified ID to an integer
            let (discord_id, meetup_id) =
                match (discord_id.parse::<u64>(), meetup_id.parse::<u64>()) {
                    (Ok(id1), Ok(id2)) => (id1, id2),
                    _ => {
                        let _ = msg.channel_id.say(
                            &ctx.http,
                            "Seems like the specified Discord or Meetup ID is invalid",
                        );
                        return;
                    }
                };
            match Self::link_meetup_bot_admin(&ctx, &msg, &regexes, discord_id, meetup_id) {
                Err(err) => {
                    eprintln!("Error: {}", err);
                    let _ = msg.channel_id.say(&ctx.http, strings::UNSPECIFIED_ERROR);
                    return;
                }
                _ => return,
            }
        } else if regexes.unlink_meetup(is_dm).is_match(&msg.content) {
            let user_id = msg.author.id.0;
            match Self::unlink_meetup(
                &ctx, &msg, /*is_bot_admin_command*/ false, user_id, bot_id.0,
            ) {
                Err(err) => {
                    eprintln!("Error: {}", err);
                    let _ = msg.channel_id.say(&ctx.http, strings::UNSPECIFIED_ERROR);
                    return;
                }
                _ => return,
            }
        } else if let Some(captures) = regexes
            .unlink_meetup_bot_admin(is_dm)
            .captures(&msg.content)
        {
            let discord_id = captures.name("mention_id").unwrap().as_str();
            // Try to convert the specified ID to an integer
            let discord_id = match discord_id.parse::<u64>() {
                Ok(id) => id,
                _ => {
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, "Seems like the specified Discord ID is invalid");
                    return;
                }
            };
            match Self::unlink_meetup(
                &ctx, &msg, /*is_bot_admin_command*/ true, discord_id, bot_id.0,
            ) {
                Err(err) => {
                    eprintln!("Error: {}", err);
                    let _ = msg.channel_id.say(&ctx.http, strings::UNSPECIFIED_ERROR);
                    return;
                }
                _ => return,
            }
        } else if regexes.sync_meetup_mention.is_match(&msg.content) {
            // This is only for bot_admins
            if !msg
                .author
                .has_role(
                    &ctx,
                    crate::sync::ids::GUILD_ID,
                    crate::sync::ids::BOT_ADMIN_ID,
                )
                .unwrap_or(false)
            {
                let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_BOT_ADMIN);
                return;
            }
            let (async_meetup_client, redis_client, mut future_spawner) = {
                let data = ctx.data.read();
                let async_meetup_client = data
                    .get::<AsyncMeetupClientKey>()
                    .expect("Async Meetup client was not set")
                    .clone();
                let redis_client = data
                    .get::<RedisClientKey>()
                    .expect("Redis client was not set")
                    .clone();
                let future_spawner = data
                    .get::<FuturesSpawnerKey>()
                    .expect("Future spawner was not set")
                    .clone();
                (async_meetup_client, redis_client, future_spawner)
            };
            let sync_task = Box::new(
                meetup::sync::sync_task(async_meetup_client, redis_client)
                    .unwrap_or_else(|err| {
                        eprintln!("Syncing task failed: {}", err);
                    })
                    .timeout(Duration::from_secs(60))
                    .unwrap_or_else(|err| {
                        eprintln!("Syncing task timed out: {}", err);
                    }),
            );
            // Send the syncing future to the executor
            match future_spawner.try_send(sync_task) {
                Ok(()) => {
                    let _ = msg.channel_id.say(
                        &ctx.http,
                        "Started asynchronous Meetup synchronization task",
                    );
                }
                Err(err) => {
                    let _ = msg.channel_id.say(
                        &ctx.http,
                        format!(
                            "Could not submit asynchronous Meetup synchronization task to the \
                             queue (full={}, disconnected={})",
                            err.is_full(),
                            err.is_disconnected()
                        ),
                    );
                }
            }
        } else if regexes.sync_discord_mention.is_match(&msg.content) {
            // This is only for bot_admins
            if !msg
                .author
                .has_role(
                    &ctx,
                    crate::sync::ids::GUILD_ID,
                    crate::sync::ids::BOT_ADMIN_ID,
                )
                .unwrap_or(false)
            {
                let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_BOT_ADMIN);
                return;
            }
            let (redis_client, bot_id, task_scheduler) = {
                let data = ctx.data.read();
                let redis_client = data
                    .get::<RedisClientKey>()
                    .expect("Redis client was not set")
                    .clone();
                let bot_id = *data.get::<BotIdKey>().expect("Bot ID was not set");
                let task_scheduler = data
                    .get::<TaskSchedulerKey>()
                    .expect("Task scheduler was not set")
                    .clone();
                (redis_client, bot_id, task_scheduler)
            };
            // Send the syncing task to the scheduler
            let mut task_scheduler_guard = common::ASYNC_RUNTIME.block_on(task_scheduler.lock());
            task_scheduler_guard.add_task_datetime(
                white_rabbit::Utc::now(),
                crate::sync::create_sync_discord_task(
                    redis_client,
                    CacheAndHttp {
                        cache: ctx.cache.clone(),
                        http: ctx.http.clone(),
                    },
                    bot_id.0,
                    /*recurring*/ false,
                ),
            );
            drop(task_scheduler_guard);
            let _ = msg
                .channel_id
                .say(&ctx.http, "Started Discord synchronization task");
        } else if regexes
            .send_expiration_reminder_bot_admin_mention
            .is_match(&msg.content)
        {
            // This is only for bot_admins
            if !msg
                .author
                .has_role(
                    &ctx,
                    crate::sync::ids::GUILD_ID,
                    crate::sync::ids::BOT_ADMIN_ID,
                )
                .unwrap_or(false)
            {
                let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_BOT_ADMIN);
                return;
            }
            let (redis_client, bot_id, task_scheduler) = {
                let data = ctx.data.read();
                let redis_client = data
                    .get::<RedisClientKey>()
                    .expect("Redis client was not set")
                    .clone();
                let bot_id = *data.get::<BotIdKey>().expect("Bot ID was not set");
                let task_scheduler = data
                    .get::<TaskSchedulerKey>()
                    .expect("Task scheduler was not set")
                    .clone();
                (redis_client, bot_id, task_scheduler)
            };
            // Send the syncing task to the scheduler
            let mut task_scheduler_guard = common::ASYNC_RUNTIME.block_on(task_scheduler.lock());
            task_scheduler_guard.add_task_datetime(
                white_rabbit::Utc::now(),
                crate::end_of_game::create_end_of_game_task(
                    redis_client,
                    CacheAndHttp {
                        cache: ctx.cache.clone(),
                        http: ctx.http.clone(),
                    },
                    bot_id,
                    /*recurring*/ false,
                ),
            );
            drop(task_scheduler_guard);
            let _ = msg
                .channel_id
                .say(&ctx.http, "Started expiration reminder task");
        } else if let Some(captures) = regexes.add_user_bot_admin_mention.captures(&msg.content) {
            // Get the Discord ID of the user that is supposed to
            // be added to the channel
            let discord_id = captures.name("mention_id").unwrap().as_str();
            // Try to convert the specified ID to an integer
            let discord_id = match discord_id.parse::<u64>() {
                Ok(id) => id,
                _ => {
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, strings::CHANNEL_ADD_USER_INVALID_DISCORD);
                    return;
                }
            };
            let redis_client = {
                let data = ctx.data.read();
                data.get::<RedisClientKey>()
                    .expect("Redis client was not set")
                    .clone()
            };
            if let Err(err) = Self::channel_add_or_remove_user(
                &ctx,
                &msg,
                discord_id,
                /*add*/ true,
                /*as_host*/ false,
                redis_client,
            ) {
                eprintln!("Error in add user: {}", err);
                let _ = msg.channel_id.say(&ctx.http, strings::UNSPECIFIED_ERROR);
            }
        } else if let Some(captures) = regexes.add_host_bot_admin_mention.captures(&msg.content) {
            // Get the Discord ID of the user that is supposed to
            // be added to the channel
            let discord_id = captures.name("mention_id").unwrap().as_str();
            // Try to convert the specified ID to an integer
            let discord_id = match discord_id.parse::<u64>() {
                Ok(id) => id,
                _ => {
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, strings::CHANNEL_ADD_USER_INVALID_DISCORD);
                    return;
                }
            };
            let redis_client = {
                let data = ctx.data.read();
                data.get::<RedisClientKey>()
                    .expect("Redis client was not set")
                    .clone()
            };
            if let Err(err) = Self::channel_add_or_remove_user(
                &ctx,
                &msg,
                discord_id,
                /*add*/ true,
                /*as_host*/ true,
                redis_client,
            ) {
                eprintln!("Error in add host: {}", err);
                let _ = msg.channel_id.say(&ctx.http, "Something went wrong");
            }
        } else if let Some(captures) = regexes.remove_user_mention.captures(&msg.content) {
            // Get the Discord ID of the user that is supposed to
            // be removed from this channel
            let discord_id = captures.name("mention_id").unwrap().as_str();
            // Try to convert the specified ID to an integer
            let discord_id = match discord_id.parse::<u64>() {
                Ok(id) => id,
                _ => {
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, strings::CHANNEL_ADD_USER_INVALID_DISCORD);
                    return;
                }
            };
            let redis_client = {
                let data = ctx.data.read();
                data.get::<RedisClientKey>()
                    .expect("Redis client was not set")
                    .clone()
            };
            if let Err(err) = Self::channel_add_or_remove_user(
                &ctx,
                &msg,
                discord_id,
                /*add*/ false,
                /*as_host*/ false,
                redis_client,
            ) {
                eprintln!("Error in remove user: {}", err);
                let _ = msg.channel_id.say(&ctx.http, "Something went wrong");
            }
        } else if let Some(captures) = regexes.remove_host_bot_admin_mention.captures(&msg.content)
        {
            // Get the Discord ID of the host that is supposed to
            // be removed from this channel
            let discord_id = captures.name("mention_id").unwrap().as_str();
            // Try to convert the specified ID to an integer
            let discord_id = match discord_id.parse::<u64>() {
                Ok(id) => id,
                _ => {
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, strings::CHANNEL_ADD_USER_INVALID_DISCORD);
                    return;
                }
            };
            let redis_client = {
                let data = ctx.data.read();
                data.get::<RedisClientKey>()
                    .expect("Redis client was not set")
                    .clone()
            };
            if let Err(err) = Self::channel_add_or_remove_user(
                &ctx,
                &msg,
                discord_id,
                /*add*/ false,
                /*as_host*/ true,
                redis_client,
            ) {
                eprintln!("Error in remove host: {}", err);
                let _ = msg.channel_id.say(&ctx.http, strings::UNSPECIFIED_ERROR);
            }
        } else if regexes.end_adventure_host_mention.is_match(&msg.content) {
            let redis_client = {
                let data = ctx.data.read();
                data.get::<RedisClientKey>()
                    .expect("Redis client was not set")
                    .clone()
            };
            if let Err(err) = Self::end_adventure(&ctx, &msg, redis_client) {
                eprintln!("Error in end_adventure: {}", err);
                let _ = msg.channel_id.say(&ctx.http, strings::UNSPECIFIED_ERROR);
            }
        } else if regexes.help(is_dm).is_match(&msg.content) {
            Self::send_help_message(&ctx, &msg, bot_id);
        } else if msg.content == "test" {
            if let Some(user) = UserId(456545153923022849).to_user_cached(&ctx) {
                Self::send_welcome_message(&ctx, &user.read());
                println!("Sent welcome message!");
            }
        } else if let Some(captures) = regexes.refresh_user_token_admin_dm.captures(&msg.content) {
            // This is only for bot_admins
            if !msg
                .author
                .has_role(
                    &ctx,
                    crate::sync::ids::GUILD_ID,
                    crate::sync::ids::BOT_ADMIN_ID,
                )
                .unwrap_or(false)
            {
                let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_BOT_ADMIN);
                return;
            }
            // Get the mentioned Discord ID
            let discord_id = captures.name("mention_id").unwrap().as_str();
            // Try to convert the specified ID to an integer
            let discord_id = match discord_id.parse::<u64>() {
                Ok(id) => id,
                _ => {
                    // let _ = msg
                    //     .channel_id
                    //     .say(&ctx.http, strings::CHANNEL_ADD_USER_INVALID_DISCORD);
                    // TODO
                    return;
                }
            };
            let (redis_client, oauth2_consumer) = {
                let data = ctx.data.read();
                (
                    data.get::<RedisClientKey>()
                        .expect("Redis client was not set")
                        .clone(),
                    data.get::<OAuth2ConsumerKey>()
                        .expect("OAuth2 consumer was not set")
                        .clone(),
                )
            };
            Self::refresh_meetup_token(
                &ctx,
                &msg,
                UserId(discord_id),
                redis_client,
                oauth2_consumer,
            );
        } else if let Some(captures) = regexes.rsvp_user_admin_mention.captures(&msg.content) {
            // This is only for bot_admins
            if !msg
                .author
                .has_role(
                    &ctx,
                    crate::sync::ids::GUILD_ID,
                    crate::sync::ids::BOT_ADMIN_ID,
                )
                .unwrap_or(false)
            {
                let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_BOT_ADMIN);
                return;
            }
            let (redis_client, oauth2_consumer) = {
                let data = ctx.data.read();
                (
                    data.get::<RedisClientKey>()
                        .expect("Redis client was not set")
                        .clone(),
                    data.get::<OAuth2ConsumerKey>()
                        .expect("OAuth2 consumer was not set")
                        .clone(),
                )
            };
            // Get the mentioned Discord ID
            let discord_id = captures.name("mention_id").unwrap().as_str();
            // Try to convert the specified ID to an integer
            let discord_id = match discord_id.parse::<u64>() {
                Ok(id) => id,
                _ => {
                    // let _ = msg
                    //     .channel_id
                    //     .say(&ctx.http, strings::CHANNEL_ADD_USER_INVALID_DISCORD);
                    // TODO
                    return;
                }
            };
            // Get the mentioned Meetup event
            let meetup_event_id = captures.name("meetup_event_id").unwrap().as_str();
            // Try to RSVP the user
            Self::rsvp_user_to_event(
                &ctx,
                &msg,
                UserId(discord_id),
                "SwissRPG-Zurich",
                meetup_event_id,
                redis_client,
                oauth2_consumer,
            )
        } else if let Some(captures) = regexes.clone_event_admin_mention.captures(&msg.content) {
            // This is only for bot_admins
            if !msg
                .author
                .has_role(
                    &ctx,
                    crate::sync::ids::GUILD_ID,
                    crate::sync::ids::BOT_ADMIN_ID,
                )
                .unwrap_or(false)
            {
                let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_BOT_ADMIN);
                return;
            }
            let (redis_client, async_meetup_client, oauth2_consumer) = {
                let data = ctx.data.read();
                (
                    data.get::<RedisClientKey>()
                        .expect("Redis client was not set")
                        .clone(),
                    data.get::<AsyncMeetupClientKey>()
                        .expect("Async Meetup client was not set")
                        .clone(),
                    data.get::<OAuth2ConsumerKey>()
                        .expect("OAuth2 consumer was not set")
                        .clone(),
                )
            };
            // Get the mentioned Meetup event
            let meetup_event_id = captures.name("meetup_event_id").unwrap().as_str();
            // Try to RSVP the user
            Self::clone_event(
                &ctx,
                &msg,
                "SwissRPG-Zurich",
                meetup_event_id,
                redis_client,
                async_meetup_client,
                oauth2_consumer,
            )
        } else if let Some(captures) = regexes.whois_bot_admin(is_dm).captures(&msg.content) {
            // This is only for bot_admins
            if !msg
                .author
                .has_role(
                    &ctx,
                    crate::sync::ids::GUILD_ID,
                    crate::sync::ids::BOT_ADMIN_ID,
                )
                .unwrap_or(false)
            {
                let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_BOT_ADMIN);
                return;
            }
            let redis_client = {
                let data = ctx.data.read();
                data.get::<RedisClientKey>()
                    .expect("Redis client was not set")
                    .clone()
            };
            if let Some(capture) = captures.name("mention_id") {
                // Look up by Discord ID
                let discord_id = capture.as_str();
                // Try to convert the specified ID to an integer
                let discord_id = match discord_id.parse::<u64>() {
                    Ok(id) => id,
                    _ => {
                        // let _ = msg
                        //     .channel_id
                        //     .say(&ctx.http, strings::CHANNEL_ADD_USER_INVALID_DISCORD);
                        // TODO
                        return;
                    }
                };
                Self::whois_by_discord_id(&ctx, &msg, UserId(discord_id), redis_client);
            } else if let Some(capture) = captures.name("discord_username_tag") {
                let username_tag = capture.as_str();
                // Look up by Discord username and tag
                Self::whois_by_discord_username_tag(&ctx, &msg, &username_tag, redis_client);
            } else if let Some(capture) = captures.name("meetup_user_id") {
                // Look up by Meetup ID
                let meetup_id = capture.as_str();
                // Try to convert the specified ID to an integer
                let meetup_id = match meetup_id.parse::<u64>() {
                    Ok(id) => id,
                    _ => {
                        // let _ = msg
                        //     .channel_id
                        //     .say(&ctx.http, strings::CHANNEL_ADD_USER_INVALID_DISCORD);
                        // TODO
                        return;
                    }
                };
                Self::whois_by_meetup_id(&ctx, &msg, meetup_id, redis_client)
            }
        } else {
            let _ = msg
                .channel_id
                .say(&ctx.http, strings::INVALID_COMMAND(bot_id.0));
        }
    }

    // Set a handler to be called on the `ready` event. This is called when a
    // shard is booted, and a READY payload is sent by Discord. This payload
    // contains data like the current user's guild Ids, current user data,
    // private channels, and more.
    //
    // In this case, just print what the current user's username is.
    fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }

    fn guild_member_addition(&self, ctx: Context, guild_id: GuildId, new_member: Member) {
        if guild_id != crate::sync::ids::GUILD_ID {
            return;
        }
        Self::send_welcome_message(&ctx, &new_member.user.read());
    }
}
