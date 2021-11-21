use super::commands::CommandContext;
use super::commands::PreparedCommands;
use futures_util::lock::Mutex as AsyncMutex;
use lib::strings;
use serenity::{
    async_trait,
    client::bridge::gateway::GatewayIntents,
    model::{
        channel::Message,
        gateway::Ready,
        guild::Member,
        id::{GuildId, UserId},
        interactions::Interaction,
        user::User,
    },
    prelude::*,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub async fn create_discord_client(
    discord_token: &str,
    application_id: u64,
    redis_client: redis::Client,
    pool: sqlx::PgPool,
    async_meetup_client: Arc<AsyncMutex<Option<Arc<lib::meetup::api::AsyncClient>>>>,
    oauth2_consumer: Arc<lib::meetup::oauth2::OAuth2Consumer>,
    stripe_client: Arc<stripe::Client>,
    shutdown_signal: Arc<AtomicBool>,
) -> Result<Client, lib::meetup::Error> {
    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    let client = Client::builder(&discord_token)
        .application_id(application_id)
        .intents(
            GatewayIntents::GUILDS
                | GatewayIntents::GUILD_MEMBERS
                | GatewayIntents::GUILD_MESSAGES
                | GatewayIntents::GUILD_MESSAGE_REACTIONS
                | GatewayIntents::DIRECT_MESSAGES
                | GatewayIntents::DIRECT_MESSAGE_REACTIONS
                | GatewayIntents::GUILD_PRESENCES
                | GatewayIntents::GUILD_VOICE_STATES,
        )
        .event_handler(Handler)
        .await?;

    // We will fetch the bot's id.
    let (bot_id, bot_name) = client
        .cache_and_http
        .http
        .get_current_user()
        .await
        .map(|info| (info.id, info.name))?;
    println!("Bot ID: {}", bot_id.0);
    println!("Bot name: {}", bot_name);

    // Prepare the commands
    let prepared_commands = Arc::new(super::commands::prepare_commands(bot_id, &bot_name)?);

    // Store the data to be shared by command invocations
    {
        let mut data = client.data.write().await;
        data.insert::<BotIdKey>(bot_id);
        data.insert::<BotNameKey>(bot_name);
        data.insert::<AsyncMeetupClientKey>(async_meetup_client);
        data.insert::<RedisClientKey>(redis_client);
        data.insert::<PoolKey>(pool);
        data.insert::<OAuth2ConsumerKey>(oauth2_consumer);
        data.insert::<StripeClientKey>(stripe_client);
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

pub(crate) struct PreparedCommandsKey;
impl TypeMapKey for PreparedCommandsKey {
    type Value = Arc<PreparedCommands>;
}

pub(crate) struct PoolKey;
impl TypeMapKey for PoolKey {
    type Value = sqlx::PgPool;
}

pub struct Handler;

impl Handler {
    async fn handle_message(
        cmdctx: &mut CommandContext,
        commands: Arc<PreparedCommands>,
    ) -> Result<(), lib::meetup::Error> {
        // Figure out which command matches
        let matches: Vec<_> = commands
            .regex_set
            .matches(&cmdctx.msg.content)
            .into_iter()
            .collect();
        let bot_id = cmdctx.bot_id().await?;
        let i = match matches.as_slice() {
            [] => {
                // unknown command
                eprintln!("Unrecognized command: {}", &cmdctx.msg.content);
                cmdctx
                    .msg
                    .channel_id
                    .say(&cmdctx.ctx, strings::INVALID_COMMAND(bot_id.0))
                    .await
                    .ok();
                return Ok(());
            }
            [i] => *i, // unique command found
            l @ _ => {
                // multiple commands found
                eprintln!(
                    "Ambiguous command: {}. Matching regexes: {:#?}",
                    &cmdctx.msg.content, l
                );
                let _ = cmdctx.msg.channel_id.say(
                    &cmdctx.ctx,
                    "I can't figure out what to do. This is a bug. Could you please let a bot \
             admin know about this?",
                );
                return Ok(());
            }
        };
        // We clone the message's content here, such that we don't keep a
        // reference to the message object
        let message_content = cmdctx.msg.content.clone();
        let captures = match commands.regexes[i].captures(&message_content) {
            Some(captures) => captures,
            None => {
                // This should not happen
                eprintln!("Unmatcheable command: {}", &message_content);
                let _ = cmdctx.msg.channel_id.say(
                    &cmdctx.ctx,
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
                if !cmdctx.is_admin().await? {
                    cmdctx
                        .msg
                        .channel_id
                        .say(&cmdctx.ctx, strings::NOT_A_BOT_ADMIN)
                        .await
                        .ok();
                    return Ok(());
                }
            }
            super::commands::CommandLevel::HostAndAdminOnly => {
                let is_admin = cmdctx.is_admin().await?;
                let is_host = cmdctx.is_host().await?;
                if !is_admin && !is_host {
                    cmdctx
                        .msg
                        .channel_id
                        .say(&cmdctx.ctx, strings::NOT_A_CHANNEL_ADMIN)
                        .await
                        .ok();
                    return Ok(());
                }
            }
        }
        // Call the command
        (command.fun)(cmdctx, captures).await?;
        Ok(())
    }
}

#[async_trait]
impl EventHandler for Handler {
    // Set a handler for the `message` event - so that whenever a new message
    // is received - the closure (or function) passed will be called.
    //
    // Event handlers are dispatched through a threadpool, and so multiple
    // events can be dispatched simultaneously.
    async fn message(&self, ctx: Context, msg: Message) {
        let (bot_id, shutdown_signal) = {
            let data = ctx.data.read().await;
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
            .await
            .get::<PreparedCommandsKey>()
            .cloned()
            .expect("Prepared commands have not been set");
        // Wrap Serenity's context and message objects into a CommandContext
        // for access to convenience functions.
        let mut cmdctx = CommandContext::new(ctx, msg);
        // Is this a direct message to the bot?
        let is_dm = match cmdctx.is_dm().await {
            Ok(is_dm) => is_dm,
            Err(err) => {
                eprintln!(
                    "Could not figure out whether this message is a DM:\n{:#?}",
                    err
                );
                cmdctx
                    .msg
                    .channel_id
                    .say(&cmdctx.ctx, lib::strings::UNSPECIFIED_ERROR)
                    .await
                    .ok();
                return;
            }
        };
        // Does the message start with a mention of the bot?
        let is_mention = commands.bot_mention.is_match(&cmdctx.msg.content);
        if !is_dm {
            // Forward this message to the spam hook
            super::spam::message_hook(&mut cmdctx).await.ok();
        }
        // If the message is not a direct message and does not start with a
        // mention of the bot, ignore it
        if !is_dm && !is_mention {
            return;
        }
        if shutdown_signal {
            let _ = cmdctx.msg.channel_id.say(
                &cmdctx.ctx,
                "Sorry, I can not help you right now. I am about to shut down!",
            );
            return;
        }
        // Poor man's try block
        let res: Result<(), lib::meetup::Error> = Self::handle_message(&mut cmdctx, commands).await;
        if let Err(err) = res {
            eprintln!("Error in message handler:\n{:#?}", err);
            let _ = cmdctx
                .msg
                .channel_id
                .say(&cmdctx.ctx, lib::strings::UNSPECIFIED_ERROR);
        }
    }

    // Set a handler to be called on the `ready` event. This is called when a
    // shard is booted, and a READY payload is sent by Discord. This payload
    // contains data like the current user's guild Ids, current user data,
    // private channels, and more.
    //
    // In this case, just print what the current user's username is.
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }

    async fn guild_member_addition(&self, ctx: Context, guild_id: GuildId, new_member: Member) {
        if guild_id != lib::discord::sync::ids::GUILD_ID {
            return;
        }
        Self::send_welcome_message(&ctx, &new_member.user);
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        // let (bot_id, shutdown_signal) = {
        //     let data = ctx.data.read().await;
        //     let bot_id = data.get::<BotIdKey>().expect("Bot ID was not set").clone();
        //     let shutdown_signal = data
        //         .get::<ShutdownSignalKey>()
        //         .expect("Shutdown signal was not set")
        //         .load(Ordering::Acquire);
        //     (bot_id, shutdown_signal)
        // };
        // In contrast to the message handler we don't need to check that this
        // is indeed a command.

        // Ignore all messages that might have come from another guild
        // (shouldn't happen, but who knows)
        if interaction.guild_id != lib::discord::sync::ids::GUILD_ID {
            return;
        }
        if let Some(data) = interaction.data {
            match data.name.as_str() {
                "link-meetup" => {
                    interaction
                        .channel_id
                        .say(&ctx, "Yessir! Linking Meetup")
                        .await
                        .ok();
                }
                "unlink-meetup" => {
                    interaction
                        .channel_id
                        .say(&ctx, "Un-linking Meetup")
                        .await
                        .ok();
                }
                _ => {
                    interaction
                        .channel_id
                        .say(&ctx, "Unknown command")
                        .await
                        .ok();
                }
            };
        }
    }
}

impl Handler {
    fn send_welcome_message(ctx: &Context, user: &User) {
        let _ = user.direct_message(ctx, |message_builder| {
            message_builder.content(strings::WELCOME_MESSAGE)
        });
    }
}
