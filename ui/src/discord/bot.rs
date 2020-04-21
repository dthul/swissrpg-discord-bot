use futures::future::TryFutureExt;
use futures_util::lock::Mutex as AsyncMutex;
use lib::strings;
use redis::Commands;
use serenity::{
    model::{
        channel::{Channel, Message},
        gateway::Ready,
        guild::Member,
        id::{GuildId, UserId},
    },
    prelude::*,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub fn create_discord_client(
    discord_token: &str,
    redis_client: redis::Client,
    async_meetup_client: Arc<AsyncMutex<Option<Arc<lib::meetup::api::AsyncClient>>>>,
    task_scheduler: Arc<AsyncMutex<white_rabbit::Scheduler>>,
    oauth2_consumer: Arc<lib::meetup::oauth2::OAuth2Consumer>,
    stripe_client: Arc<stripe::Client>,
    async_runtime: Arc<tokio::sync::RwLock<Option<tokio::runtime::Runtime>>>,
    shutdown_signal: Arc<AtomicBool>,
) -> Result<Client, lib::meetup::Error> {
    let redis_connection = redis_client.get_connection()?;

    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    let client = Client::new(&discord_token, Handler)?;

    // We will fetch the bot's id.
    let (bot_id, bot_name) = client
        .cache_and_http
        .http
        .get_current_user()
        .map(|info| (info.id, info.name))?;
    println!("Bot ID: {}", bot_id.0);
    println!("Bot name: {}", &bot_name);

    // Prepare the commands
    let prepared_commands = Arc::new(super::commands::prepare_commands(bot_id, &bot_name)?);

    // pre-compile the regexes
    // let regexes = super::bot_commands::compile_regexes(bot_id.0, &bot_name);

    // Store the bot's id in the client for easy access
    {
        let mut data = client.data.write();
        data.insert::<BotIdKey>(bot_id);
        data.insert::<BotNameKey>(bot_name);
        // data.insert::<RedisConnectionKey>(Arc::new(Mutex::new(redis_connection)));
        // data.insert::<RegexesKey>(Arc::new(regexes));
        data.insert::<AsyncMeetupClientKey>(async_meetup_client);
        data.insert::<RedisClientKey>(redis_client);
        data.insert::<TaskSchedulerKey>(task_scheduler);
        data.insert::<OAuth2ConsumerKey>(oauth2_consumer);
        data.insert::<StripeClientKey>(stripe_client);
        data.insert::<AsyncRuntimeKey>(async_runtime);
        data.insert::<ShutdownSignalKey>(shutdown_signal);
        data.insert::<PreparedCommandsKey>(prepared_commands);
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

// pub struct RedisConnectionKey;
// impl TypeMapKey for RedisConnectionKey {
//     type Value = Arc<Mutex<redis::Connection>>;
// }

// pub struct RegexesKey;
// impl TypeMapKey for RegexesKey {
//     type Value = Arc<super::bot_commands::Regexes>;
// }

pub struct AsyncMeetupClientKey;
impl TypeMapKey for AsyncMeetupClientKey {
    type Value = Arc<AsyncMutex<Option<Arc<lib::meetup::api::AsyncClient>>>>;
}

pub struct RedisClientKey;
impl TypeMapKey for RedisClientKey {
    type Value = redis::Client;
}

pub struct TaskSchedulerKey;
impl TypeMapKey for TaskSchedulerKey {
    type Value = Arc<AsyncMutex<white_rabbit::Scheduler>>;
}

pub struct OAuth2ConsumerKey;
impl TypeMapKey for OAuth2ConsumerKey {
    type Value = Arc<lib::meetup::oauth2::OAuth2Consumer>;
}

pub struct StripeClientKey;
impl TypeMapKey for StripeClientKey {
    type Value = Arc<stripe::Client>;
}

pub struct ShutdownSignalKey;
impl TypeMapKey for ShutdownSignalKey {
    type Value = Arc<AtomicBool>;
}

pub struct AsyncRuntimeKey;
impl TypeMapKey for AsyncRuntimeKey {
    type Value = Arc<tokio::sync::RwLock<Option<tokio::runtime::Runtime>>>;
}

pub(crate) struct PreparedCommandsKey;
impl TypeMapKey for PreparedCommandsKey {
    type Value = Arc<super::commands::PreparedCommands>;
}

pub struct Handler;

impl EventHandler for Handler {
    // Set a handler for the `message` event - so that whenever a new message
    // is received - the closure (or function) passed will be called.
    //
    // Event handlers are dispatched through a threadpool, and so multiple
    // events can be dispatched simultaneously.
    fn message(&self, ctx: Context, msg: Message) {
        let (bot_id, shutdown_signal) = {
            let data = ctx.data.read();
            let bot_id = data.get::<BotIdKey>().expect("Bot ID was not set").clone();
            let shutdown_signal = data
                .get::<ShutdownSignalKey>()
                .expect("Shutdown signal was not set")
                .load(Ordering::Acquire);
            (bot_id, shutdown_signal)
        };
        // Ignore all messages written by the bot itself
        if msg.author.id == bot_id {
            return;
        }
        // Ignore all messages that might have come from another guild
        // (shouldn't happen, but who knows)
        if let Some(guild_id) = msg.guild_id {
            if guild_id != lib::discord::sync::ids::GUILD_ID {
                return;
            }
        }
        // Get the prepared commands
        let commands = ctx
            .data
            .read()
            .get::<PreparedCommandsKey>()
            .cloned()
            .expect("Prepared commands have not been set");
        // let regexes = ctx
        //     .data
        //     .read()
        //     .get::<RegexesKey>()
        //     .cloned()
        //     .expect("Prepared commands have not been set");
        // Wrap Serenity's context and message objects into a CommandContext
        // for access to convenience functions.
        let mut cmdctx = super::commands::CommandContext::new(&ctx, &msg);
        // let channel = match msg.channel_id.to_channel(&ctx) {
        //     Ok(channel) => channel,
        //     _ => return,
        // };
        // Poor man's try block
        let res: Result<(), lib::meetup::Error> = (|| {
            // Is this a direct message to the bot?
            let is_dm = cmdctx.is_dm()?;
            // Does the message start with a mention of the bot?
            let is_mention = commands.bot_mention.is_match(&msg.content);
            // If the message is not a direct message and does not start with a
            // mention of the bot, ignore it
            if !is_dm && !is_mention {
                return Ok(());
            }
            if shutdown_signal {
                let _ = msg.channel_id.say(
                    &ctx,
                    "Sorry, I can not help you right now. I am about to shut down!",
                );
                return Ok(());
            }
            // Figure out which command matches
            let matches: Vec<_> = commands
                .regex_set
                .matches(&msg.content)
                .into_iter()
                .collect();
            let i = match matches.as_slice() {
                [] => {
                    // unknown command
                    eprintln!("Unrecognized command: {}", &msg.content);
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, strings::INVALID_COMMAND(bot_id.0));
                    return Ok(());
                }
                [i] => *i, // unique command found
                _ => {
                    // multiple commands found
                    eprintln!("Ambiguous command: {}", &msg.content);
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, "I can't figure out what to do. This is a bug. Could you please let a bot admin know about this?");
                    return Ok(());
                }
            };
            let captures = match commands.regexes[i].captures(&msg.content) {
                Some(captures) => captures,
                None => {
                    // This should not happen
                    eprintln!("Unmatcheable command: {}", &msg.content);
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, "I can't parse your command. This is a bug. Could you please let a bot admin know about this?");
                    return Ok(());
                }
            };
            // Check whether the user has the required permissions
            let command = commands.commands[i];
            match command.level {
                super::commands::CommandLevel::Everybody => (),
                super::commands::CommandLevel::AdminOnly => {
                    if !cmdctx.is_admin()? {
                        let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_BOT_ADMIN);
                        return Ok(());
                    }
                }
                super::commands::CommandLevel::HostAndAdminOnly => {
                    let is_admin = cmdctx.is_admin()?;
                    let is_host = cmdctx.is_host()?;
                    if !is_admin && !is_host {
                        let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_CHANNEL_ADMIN);
                        return Ok(());
                    }
                }
            }
            // Call the command
            (command.fun)(cmdctx, captures)?;
            Ok(())
        })();
        if let Err(err) = res {
            eprintln!("Error in message handler:\n{:#?}", err);
            let _ = msg.channel_id.say(&ctx, lib::strings::UNSPECIFIED_ERROR);
        }
        /*
        if let Some(captures) = regexes
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
                    lib::discord::sync::ids::GUILD_ID,
                    lib::discord::sync::ids::BOT_ADMIN_ID,
                )
                .unwrap_or(false)
            {
                let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_BOT_ADMIN);
                return;
            }
            let (async_meetup_client, redis_client, async_runtime) = {
                let data = ctx.data.read();
                let async_meetup_client = data
                    .get::<AsyncMeetupClientKey>()
                    .expect("Async Meetup client was not set")
                    .clone();
                let redis_client = data
                    .get::<RedisClientKey>()
                    .expect("Redis client was not set")
                    .clone();
                let async_runtime = data
                    .get::<AsyncRuntimeKey>()
                    .expect("Async runtime was not set")
                    .clone();
                (async_meetup_client, redis_client, async_runtime)
            };
            // Send the syncing future to the executor
            let runtime_guard = futures::executor::block_on(async_runtime.read());
            if let Some(ref async_runtime) = *runtime_guard {
                async_runtime.enter(move || {
                    let sync_task = {
                        let task = async move {
                            let mut redis_connection = redis_client.get_async_connection().await?;
                            lib::meetup::sync::sync_task(async_meetup_client, &mut redis_connection)
                                .await
                        };
                        // Wrap the task in a timeout
                        tokio::time::timeout(
                            Duration::from_secs(5 * 60),
                            task.unwrap_or_else(|err| {
                                eprintln!("Syncing task failed: {}", err);
                            }),
                        )
                        .unwrap_or_else(|err| {
                            eprintln!("Syncing task timed out: {}", err);
                        })
                    };
                    tokio::spawn(sync_task)
                });
                let _ = msg.channel_id.say(
                    &ctx.http,
                    "Started asynchronous Meetup synchronization task",
                );
            } else {
                let _ = msg.channel_id.say(
                    &ctx.http,
                    "Could not submit asynchronous Meetup synchronization task",
                );
            }
        } else if regexes.sync_discord_mention.is_match(&msg.content) {
            // This is only for bot_admins
            if !msg
                .author
                .has_role(
                    &ctx,
                    lib::discord::sync::ids::GUILD_ID,
                    lib::discord::sync::ids::BOT_ADMIN_ID,
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
            let mut task_scheduler_guard = futures::executor::block_on(task_scheduler.lock());
            task_scheduler_guard.add_task_datetime(
                white_rabbit::Utc::now(),
                lib::discord::sync::create_sync_discord_task(
                    redis_client,
                    lib::discord::CacheAndHttp {
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
                    lib::discord::sync::ids::GUILD_ID,
                    lib::discord::sync::ids::BOT_ADMIN_ID,
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
            let mut task_scheduler_guard = futures::executor::block_on(task_scheduler.lock());
            task_scheduler_guard.add_task_datetime(
                white_rabbit::Utc::now(),
                lib::tasks::end_of_game::create_end_of_game_task(
                    redis_client,
                    lib::discord::CacheAndHttp {
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
                    lib::discord::sync::ids::GUILD_ID,
                    lib::discord::sync::ids::BOT_ADMIN_ID,
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
                    lib::discord::sync::ids::GUILD_ID,
                    lib::discord::sync::ids::BOT_ADMIN_ID,
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
                    lib::discord::sync::ids::GUILD_ID,
                    lib::discord::sync::ids::BOT_ADMIN_ID,
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
        } else if regexes.schedule_session_mention.is_match(&msg.content) {
            let mut redis_client = {
                let data = ctx.data.read();
                data.get::<RedisClientKey>()
                    .expect("Redis client was not set")
                    .clone()
            };
            if let Err(err) = Self::schedule_session(&ctx, &msg, &mut redis_client) {
                eprintln!("Error when trying to schedule session:\n{:#?}", err);
                let _ = msg.channel_id.say(&ctx.http, "Something went wrong");
            }
        } else if let Some(captures) = regexes.whois_bot_admin(is_dm).captures(&msg.content) {
            // This is only for bot_admins
            if !msg
                .author
                .has_role(
                    &ctx,
                    lib::discord::sync::ids::GUILD_ID,
                    lib::discord::sync::ids::BOT_ADMIN_ID,
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
        } else if regexes.list_players_mention.is_match(&msg.content) {
            let redis_client = {
                let data = ctx.data.read();
                data.get::<RedisClientKey>()
                    .expect("Redis client was not set")
                    .clone()
            };
            if let Err(err) = Self::list_players(&ctx, &msg, redis_client) {
                eprintln!("Error in `list players` command:\n{:#?}", err);
                let _ = msg.channel_id.say(&ctx.http, "Something went wrong");
            }
        } else if regexes.list_stripe_subscriptions.is_match(&msg.content) {
            let stripe_client = {
                let data = ctx.data.read();
                data.get::<StripeClientKey>()
                    .expect("Stripe client was not set")
                    .clone()
            };
            if let Err(err) = Self::list_subscriptions(&ctx, &msg, &stripe_client) {
                eprintln!("Error in `list subscriptions` command:\n{:#?}", err);
                let _ = msg.channel_id.say(&ctx.http, "Something went wrong");
            }
        } else if regexes.sync_stripe_subscriptions.is_match(&msg.content) {
            // This is only for bot_admins
            if !msg
                .author
                .has_role(
                    &ctx,
                    lib::discord::sync::ids::GUILD_ID,
                    lib::discord::sync::ids::BOT_ADMIN_ID,
                )
                .unwrap_or(false)
            {
                let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_BOT_ADMIN);
                return;
            }
            let data = ctx.data.read();
            let (stripe_client, async_runtime) = {
                let stripe_client = data
                    .get::<StripeClientKey>()
                    .expect("Stripe client was not set")
                    .clone();
                let async_runtime = data
                    .get::<AsyncRuntimeKey>()
                    .expect("Async runtime was not set")
                    .clone();
                (stripe_client, async_runtime)
            };
            let discord_api = lib::discord::CacheAndHttp {
                cache: ctx.cache.clone(),
                http: ctx.http.clone(),
            };
            let runtime_guard = futures::executor::block_on(async_runtime.read());
            if let Some(ref async_runtime) = *runtime_guard {
                async_runtime.enter(|| {
                    async_runtime.spawn(async move {
                        lib::tasks::subscription_roles::update_roles(&discord_api, &stripe_client)
                            .await
                    })
                });
                let _ = msg.channel_id.say(&ctx.http, "Copy that");
            }
        } else if regexes.num_cached_members.is_match(&msg.content) {
            if let Some(guild) = msg.guild(&ctx) {
                let num_cached_members = guild.read().members.len();
                let _ = msg.channel_id.say(
                    &ctx.http,
                    format!(
                        "I have {} members cached for this guild",
                        num_cached_members
                    ),
                );
            } else {
                let _ = msg.channel_id.say(
                    &ctx.http,
                    "No guild associated with this message (use the command from a guild channel \
                     instead of a direct message).",
                );
            }
        } else if regexes.manage_channel_mention.is_match(&msg.content) {
            // This is only for bot_admins
            if !msg
                .author
                .has_role(
                    &ctx,
                    lib::discord::sync::ids::GUILD_ID,
                    lib::discord::sync::ids::BOT_ADMIN_ID,
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
            if let Err(err) = Self::manage_channel(&ctx, &msg, &redis_client, bot_id) {
                eprintln!("Something went wrong in manage_channel:\n{:#?}", err);
                let _ = msg.channel_id.say(&ctx, "Something went wrong");
            }
        } else if regexes.mention_channel_role_mention.is_match(&msg.content) {
            let redis_client = {
                let data = ctx.data.read();
                data.get::<RedisClientKey>()
                    .expect("Redis client was not set")
                    .clone()
            };
            // Poor man's try block
            let ctx = &ctx;
            if let Err(err) = (|| {
                let mut redis_connection = redis_client.get_connection()?;
                let channel_roles =
                    lib::get_channel_roles(msg.channel_id.0, &mut redis_connection)?;
                if let Some(channel_roles) = channel_roles {
                    msg.channel_id
                        .say(ctx, format!("<@&{}>", channel_roles.user))?;
                } else {
                    msg.channel_id
                        .say(ctx, format!("This channel has no role"))?;
                }
                Ok::<_, lib::meetup::Error>(())
            })() {
                eprintln!("Error in mention_channel_role command:\n{:#?}", err);
                let _ = msg.channel_id.say(ctx, "Something went wrong.");
            }
        } else if let Some(captures) = regexes.snooze_reminders.captures(&msg.content) {
            // This is only for bot_admins
            if !msg
                .author
                .has_role(
                    &ctx,
                    lib::discord::sync::ids::GUILD_ID,
                    lib::discord::sync::ids::BOT_ADMIN_ID,
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
            // Poor man's try block
            let ctx = &ctx;
            if let Err(err) = (|| {
                let num_days: u32 = captures
                    .name("num_days")
                    .expect("Regex capture does not contain 'num_days'")
                    .as_str()
                    .parse()
                    .map(|num_days: u32| num_days.min(180))
                    .map_err(|_err| {
                        simple_error::SimpleError::new("Invalid number of days specified")
                    })?;
                let mut redis_connection = redis_client.get_connection()?;
                // Check whether this is a game channel
                let is_game_channel: bool =
                    redis_connection.sismember("discord_channels", msg.channel_id.0)?;
                if !is_game_channel {
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, strings::CHANNEL_NOT_BOT_CONTROLLED);
                    return Ok(());
                };
                let redis_channel_snooze_key =
                    format!("discord_channel:{}:snooze_until", msg.channel_id.0);
                if num_days == 0 {
                    // Remove the snooze
                    let _: () = redis_connection.del(&redis_channel_snooze_key)?;
                    let _ = msg.channel_id.say(ctx, "Disabled snoozing.");
                } else {
                    let snooze_until = chrono::Utc::now() + chrono::Duration::days(num_days as i64);
                    // Set a new snooze date
                    let _: () = redis_connection
                        .set(&redis_channel_snooze_key, snooze_until.to_rfc3339())?;
                    let _ = msg
                        .channel_id
                        .say(ctx, format!("Snoozing for {} days.", num_days));
                }
                Ok::<_, lib::meetup::Error>(())
            })() {
                eprintln!("Error in mention_channel_role command:\n{:#?}", err);
                let _ = msg.channel_id.say(ctx, "Something went wrong.");
            }
        } else if regexes.list_inactive_users.is_match(&msg.content) {
            // This is only for bot_admins
            if !msg
                .author
                .has_role(
                    &ctx,
                    lib::discord::sync::ids::GUILD_ID,
                    lib::discord::sync::ids::BOT_ADMIN_ID,
                )
                .unwrap_or(false)
            {
                let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_BOT_ADMIN);
                return;
            }
            if let Some(guild) = lib::discord::sync::ids::GUILD_ID.to_guild_cached(&ctx) {
                let mut inactive_users = vec![];
                for (&id, member) in &guild.read().members {
                    if member.roles.is_empty() {
                        inactive_users.push(id);
                    }
                }
                let inactive_users_strs: Vec<String> = inactive_users
                    .into_iter()
                    .map(|id| format!("* <@{}>", id))
                    .collect();
                let inactive_users_str = inactive_users_strs.join("\n");
                let _ = msg.channel_id.say(
                    &ctx.http,
                    "List of users with no roles assigned:\n".to_string() + &inactive_users_str,
                );
            } else {
                let _ = msg.channel_id.say(&ctx.http, "Could not find the guild");
            }
        } else {
            eprintln!("Unrecognized command: {}", &msg.content);
            let _ = msg
                .channel_id
                .say(&ctx.http, strings::INVALID_COMMAND(bot_id.0));
        }
        */
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
        if guild_id != lib::discord::sync::ids::GUILD_ID {
            return;
        }
        Self::send_welcome_message(&ctx, &new_member.user.read());
    }
}
