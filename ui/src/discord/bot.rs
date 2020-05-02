use futures_util::lock::Mutex as AsyncMutex;
use lib::strings;
use serenity::{
    model::{
        channel::Message,
        gateway::Ready,
        guild::Member,
        id::{GuildId, UserId},
        user::User,
    },
    prelude::*,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
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
        // Wrap Serenity's context and message objects into a CommandContext
        // for access to convenience functions.
        let mut cmdctx = super::commands::CommandContext::new(&ctx, &msg);
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
                    let _ = msg.channel_id.say(
                        &ctx.http,
                        "I can't figure out what to do. This is a bug. Could you please let a bot \
                         admin know about this?",
                    );
                    return Ok(());
                }
            };
            let captures = match commands.regexes[i].captures(&msg.content) {
                Some(captures) => captures,
                None => {
                    // This should not happen
                    eprintln!("Unmatcheable command: {}", &msg.content);
                    let _ = msg.channel_id.say(
                        &ctx.http,
                        "I can't parse your command. This is a bug. Could you please let a bot \
                         admin know about this?",
                    );
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

impl Handler {
    fn send_welcome_message(ctx: &Context, user: &User) {
        let _ = user.direct_message(ctx, |message_builder| {
            message_builder.content(strings::WELCOME_MESSAGE)
        });
    }
}
