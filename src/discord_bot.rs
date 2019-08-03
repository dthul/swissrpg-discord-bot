use futures::Future;
use redis::{Commands, PipelineCommands};
use regex::Regex;
use serenity::{
    model::{channel::Channel, channel::Message, gateway::Ready, id::UserId},
    prelude::*,
};
use simple_error::SimpleError;
use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;
use tokio::prelude::*;

const MENTION_PATTERN: &'static str = r"<@(?P<mention_id>[0-9]+)>";

pub fn create_discord_client(
    discord_token: &str,
    redis_client: redis::Client,
    meetup_client: Arc<RwLock<Option<crate::meetup_api::Client>>>,
    async_meetup_client: Arc<RwLock<Option<crate::meetup_api::AsyncClient>>>,
    task_scheduler: Arc<Mutex<white_rabbit::Scheduler>>,
    futures_spawner: futures::sync::mpsc::Sender<crate::meetup_sync::BoxedFuture<(), ()>>,
) -> crate::Result<Client> {
    let redis_connection = redis_client.get_connection()?;

    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    let client = Client::new(&discord_token, Handler)?;

    // We will fetch the bot's id.
    let bot_id = client
        .cache_and_http
        .http
        .get_current_application_info()
        .map(|info| info.id)?;

    // pre-compile the regexes
    let regexes = compile_regexes(bot_id.0);

    // Store the bot's id in the client for easy access
    {
        let mut data = client.data.write();
        data.insert::<BotIdKey>(bot_id);
        data.insert::<RedisConnectionKey>(Arc::new(Mutex::new(redis_connection)));
        data.insert::<RegexesKey>(Arc::new(regexes));
        data.insert::<MeetupClientKey>(meetup_client);
        data.insert::<AsyncMeetupClientKey>(async_meetup_client);
        data.insert::<RedisClientKey>(redis_client);
        data.insert::<TaskSchedulerKey>(task_scheduler);
        data.insert::<FuturesSpawnerKey>(futures_spawner);
    }

    Ok(client)
}

struct Regexes {
    bot_mention: String,
    link_meetup_dm: Regex,
    link_meetup_mention: Regex,
    link_meetup_organizer_dm: Regex,
    link_meetup_organizer_mention: Regex,
    unlink_meetup_dm: Regex,
    unlink_meetup_mention: Regex,
    unlink_meetup_organizer_dm: Regex,
    unlink_meetup_organizer_mention: Regex,
    sync_meetup_mention: Regex,
    sync_discord_mention: Regex,
}

impl Regexes {
    fn link_meetup(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.link_meetup_dm
        } else {
            &self.link_meetup_mention
        }
    }

    fn link_meetup_organizer(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.link_meetup_organizer_dm
        } else {
            &self.link_meetup_organizer_mention
        }
    }

    fn unlink_meetup(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.unlink_meetup_dm
        } else {
            &self.unlink_meetup_mention
        }
    }

    fn unlink_meetup_organizer(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.unlink_meetup_organizer_dm
        } else {
            &self.unlink_meetup_organizer_mention
        }
    }
}

fn compile_regexes(bot_id: u64) -> Regexes {
    let bot_mention = format!(r"<@{}>", bot_id);
    let link_meetup_dm = r"^link[ -]?meetup\s*$";
    let link_meetup_mention = format!(
        r"^{bot_mention}\s+link[ -]?meetup\s*$",
        bot_mention = bot_mention
    );
    let link_meetup_organizer = format!(
        r"link[ -]?meetup\s+{mention_pattern}\s+(?P<meetupid>[0-9]+)",
        mention_pattern = MENTION_PATTERN
    );
    let link_meetup_organizer_dm = format!(
        r"^{link_meetup_organizer}\s*$",
        link_meetup_organizer = link_meetup_organizer
    );
    let link_meetup_organizer_mention = format!(
        r"^{bot_mention}\s+{link_meetup_organizer}\s*$",
        bot_mention = bot_mention,
        link_meetup_organizer = link_meetup_organizer
    );
    let unlink_meetup = r"unlink[ -]?meetup";
    let unlink_meetup_dm = format!(r"^{unlink_meetup}\s*$", unlink_meetup = unlink_meetup);
    let unlink_meetup_mention = format!(
        r"^{bot_mention}\s+{unlink_meetup}\s*$",
        bot_mention = bot_mention,
        unlink_meetup = unlink_meetup
    );
    let unlink_meetup_organizer = format!(
        r"unlink[ -]?meetup\s+{mention_pattern}",
        mention_pattern = MENTION_PATTERN
    );
    let unlink_meetup_organizer_dm = format!(
        r"^{unlink_meetup_organizer}\s*$",
        unlink_meetup_organizer = unlink_meetup_organizer
    );
    let unlink_meetup_organizer_mention = format!(
        r"^{bot_mention}\s+{unlink_meetup_organizer}\s*$",
        bot_mention = bot_mention,
        unlink_meetup_organizer = unlink_meetup_organizer
    );
    let sync_meetup_mention = format!(
        r"^{bot_mention}\s+sync\s+meetup\s*$",
        bot_mention = bot_mention
    );
    let sync_discord_mention = format!(
        r"^{bot_mention}\s+sync\s+discord\s*$",
        bot_mention = bot_mention
    );
    Regexes {
        bot_mention: bot_mention,
        link_meetup_dm: Regex::new(link_meetup_dm).unwrap(),
        link_meetup_mention: Regex::new(link_meetup_mention.as_str()).unwrap(),
        link_meetup_organizer_dm: Regex::new(link_meetup_organizer_dm.as_str()).unwrap(),
        link_meetup_organizer_mention: Regex::new(link_meetup_organizer_mention.as_str()).unwrap(),
        unlink_meetup_dm: Regex::new(unlink_meetup_dm.as_str()).unwrap(),
        unlink_meetup_mention: Regex::new(unlink_meetup_mention.as_str()).unwrap(),
        unlink_meetup_organizer_dm: Regex::new(unlink_meetup_organizer_dm.as_str()).unwrap(),
        unlink_meetup_organizer_mention: Regex::new(unlink_meetup_organizer_mention.as_str())
            .unwrap(),
        sync_meetup_mention: Regex::new(sync_meetup_mention.as_str()).unwrap(),
        sync_discord_mention: Regex::new(sync_discord_mention.as_str()).unwrap(),
    }
}

struct BotIdKey;
impl TypeMapKey for BotIdKey {
    type Value = UserId;
}

struct RedisConnectionKey;
impl TypeMapKey for RedisConnectionKey {
    type Value = Arc<Mutex<redis::Connection>>;
}

struct RegexesKey;
impl TypeMapKey for RegexesKey {
    type Value = Arc<Regexes>;
}

struct MeetupClientKey;
impl TypeMapKey for MeetupClientKey {
    type Value = Arc<RwLock<Option<crate::meetup_api::Client>>>;
}

struct AsyncMeetupClientKey;
impl TypeMapKey for AsyncMeetupClientKey {
    type Value = Arc<RwLock<Option<crate::meetup_api::AsyncClient>>>;
}

struct RedisClientKey;
impl TypeMapKey for RedisClientKey {
    type Value = redis::Client;
}

struct TaskSchedulerKey;
impl TypeMapKey for TaskSchedulerKey {
    type Value = Arc<Mutex<white_rabbit::Scheduler>>;
}

struct FuturesSpawnerKey;
impl TypeMapKey for FuturesSpawnerKey {
    type Value = futures::sync::mpsc::Sender<crate::meetup_sync::BoxedFuture<(), ()>>;
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

struct Handler;

impl Handler {
    fn link_meetup(
        ctx: &Context,
        msg: &Message,
        regexes: &Regexes,
        user_id: u64,
    ) -> crate::Result<()> {
        let redis_key_d2m = format!("discord_user:{}:meetup_user", user_id);
        let (redis_connection_mutex, meetup_client_mutex) = {
            let data = ctx.data.read();
            (
                data.get::<RedisConnectionKey>()
                    .ok_or_else(|| Box::new(SimpleError::new("Redis connection was not set")))?
                    .clone(),
                data.get::<MeetupClientKey>()
                    .ok_or_else(|| Box::new(SimpleError::new("Meetup client was not set")))?
                    .clone(),
            )
        };
        // Check if there is already a meetup id linked to this user
        // and issue a warning
        let linked_meetup_id: Option<u64> = {
            let mut redis_connection = redis_connection_mutex.lock();
            redis_connection.get(&redis_key_d2m)?
        };
        if let Some(linked_meetup_id) = linked_meetup_id {
            match *meetup_client_mutex.read() {
                Some(ref meetup_client) => {
                    match meetup_client.get_member_profile(Some(linked_meetup_id))? {
                        Some(user) => {
                            let _ = msg.channel_id.say(
                                &ctx.http,
                                format!(
                                    "You are already linked to {}'s Meetup account. \
                                     If you really want to change this, unlink your currently \
                                     linked meetup account first by writing:\n\
                                     {} unlink meetup",
                                    user.name, regexes.bot_mention
                                ),
                            );
                        }
                        _ => {
                            let _ = msg.channel_id.say(
                                &ctx.http,
                                format!(
                                    "You are linked to a seemingly non-existent Meetup account. \
                                     If you want to change this, unlink the currently \
                                     linked meetup account by writing:\n\
                                     {} unlink meetup",
                                    regexes.bot_mention
                                ),
                            );
                        }
                    }
                }
                _ => {
                    let _ = msg.channel_id.say(
                        &ctx.http,
                        format!(
                            "You are already linked to a Meetup account. \
                             If you really want to change this, unlink your currently \
                             linked meetup account first by writing:\n\
                             {} unlink meetup",
                            regexes.bot_mention
                        ),
                    );
                }
            }
            return Ok(());
        }
        let url =
            crate::meetup_oauth2::generate_meetup_linking_link(&redis_connection_mutex, user_id)?;
        let dm = msg.author.direct_message(ctx, |message| {
            message.content(format!(
                "Visit the following website to link your Meetup profile: {}\n\
                 ***This is a private, ephemeral, one-time use link and meant just for you.***\n\
                 Don't share it with others or they might link your Discord account to their Meetup profile.",
                url
            ))
        });
        match dm {
            Ok(_) => {
                let _ = msg.react(ctx, "\u{2705}");
            }
            Err(why) => {
                eprintln!("Error sending Meetup linking DM: {:?}", why);
                let _ = msg.reply(ctx, "There was an error trying to send you instructions.");
            }
        }
        Ok(())
    }

    fn link_meetup_organizer(
        ctx: &Context,
        msg: &Message,
        regexes: &Regexes,
        user_id: u64,
        meetup_id: u64,
    ) -> crate::Result<()> {
        let redis_key_d2m = format!("discord_user:{}:meetup_user", user_id);
        let redis_key_m2d = format!("meetup_user:{}:discord_user", meetup_id);
        let (redis_connection_mutex, meetup_client_mutex) = {
            let data = ctx.data.read();
            (
                data.get::<RedisConnectionKey>()
                    .ok_or_else(|| Box::new(SimpleError::new("Redis connection was not set")))?
                    .clone(),
                data.get::<MeetupClientKey>()
                    .ok_or_else(|| Box::new(SimpleError::new("Meetup client was not set")))?
                    .clone(),
            )
        };
        // Check if there is already a meetup id linked to this user
        // and issue a warning
        let linked_meetup_id: Option<u64> = {
            let mut redis_connection = redis_connection_mutex.lock();
            redis_connection.get(&redis_key_d2m)?
        };
        if let Some(linked_meetup_id) = linked_meetup_id {
            if linked_meetup_id == meetup_id {
                let _ = msg.channel_id.say(
                    &ctx.http,
                    format!(
                        "All good, this Meetup account was already linked to <@{}>",
                        user_id
                    ),
                );
                return Ok(());
            } else {
                // TODO: answer in DM?
                let _ = msg.channel_id.say(
                    &ctx.http,
                    format!(
                        "<@{discord_id}> is already linked to a different Meetup account. \
                         If you want to change this, unlink the currently \
                         linked Meetup account first by writing:\n\
                         {bot_mention} unlink meetup <@{discord_id}>",
                        discord_id = user_id,
                        bot_mention = regexes.bot_mention
                    ),
                );
                return Ok(());
            }
        }
        // Check if this meetup id is already linked
        // and issue a warning
        let linked_discord_id: Option<u64> = {
            let mut redis_connection = redis_connection_mutex.lock();
            redis_connection.get(&redis_key_m2d)?
        };
        if let Some(linked_discord_id) = linked_discord_id {
            let _ = msg.author.direct_message(ctx, |message_builder| {
                message_builder.content(format!(
                    "This Meetup account is alread linked to <@{linked_discord_id}>. \
                     If you want to change this, unlink the Meetup account first \
                     by writing\n\
                     {bot_mention} unlink meetup <@{linked_discord_id}>",
                    linked_discord_id = linked_discord_id,
                    bot_mention = regexes.bot_mention
                ))
            });
            return Ok(());
        }
        // The user has not yet linked their meetup account.
        // Test whether the specified Meetup user actually exists.
        let meetup_user = meetup_client_mutex
            .read()
            .as_ref()
            .ok_or_else(|| SimpleError::new("Meetup API unavailable"))?
            .get_member_profile(Some(meetup_id))?;
        match meetup_user {
            None => {
                let _ = msg.channel_id.say(
                    &ctx.http,
                    "It looks like this Meetup profile does not exist",
                );
                return Ok(());
            }
            Some(meetup_user) => {
                let mut successful = false;
                {
                    let mut redis_connection = redis_connection_mutex.lock();
                    // Try to atomically set the meetup id
                    redis::transaction(
                        &mut *redis_connection,
                        &[&redis_key_d2m, &redis_key_m2d],
                        |con, pipe| {
                            let linked_meetup_id: Option<u64> = con.get(&redis_key_d2m)?;
                            let linked_discord_id: Option<u64> = con.get(&redis_key_m2d)?;
                            if linked_meetup_id.is_some() || linked_discord_id.is_some() {
                                // The meetup id was linked in the meantime, abort
                                successful = false;
                                // Execute empty transaction just to get out of the closure
                                pipe.query(con)
                            } else {
                                pipe.sadd("meetup_users", meetup_id)
                                    .sadd("discord_users", user_id)
                                    .set(&redis_key_d2m, meetup_id)
                                    .set(&redis_key_m2d, user_id);
                                successful = true;
                                pipe.query(con)
                            }
                        },
                    )?;
                }
                if successful {
                    let photo_url = meetup_user.photo.as_ref().map(|p| p.thumb_link.as_str());
                    let _ = msg.channel_id.send_message(&ctx.http, |message| {
                        message.embed(|embed| {
                            embed.title("Linked Meetup account");
                            embed.description(format!(
                                "Successfully linked <@{}> to {}'s Meetup account",
                                user_id, meetup_user.name
                            ));
                            if let Some(photo_url) = photo_url {
                                embed.image(photo_url)
                            } else {
                                embed
                            }
                        })
                    });
                    return Ok(());
                } else {
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, "Could not assign meetup id (timing error)");
                    return Ok(());
                }
            }
        }
    }

    fn unlink_meetup(
        ctx: &Context,
        msg: &Message,
        is_organizer_command: bool,
        user_id: u64,
    ) -> crate::Result<()> {
        let redis_key_d2m = format!("discord_user:{}:meetup_user", user_id);
        let redis_connection_mutex = {
            ctx.data
                .read()
                .get::<RedisConnectionKey>()
                .ok_or_else(|| SimpleError::new("Redis connection was not set"))?
                .clone()
        };
        let mut redis_connection = redis_connection_mutex.lock();
        // Check if there is actually a meetup id linked to this user
        let linked_meetup_id: Option<u64> = redis_connection.get(&redis_key_d2m)?;
        match linked_meetup_id {
            Some(meetup_id) => {
                let redis_key_m2d = format!("meetup_user:{}:discord_user", meetup_id);
                redis_connection.del(&[&redis_key_d2m, &redis_key_m2d])?;
                let message = if is_organizer_command {
                    Cow::Owned(format!("Unlinked <@{}>'s Meetup account", user_id))
                } else {
                    Cow::Borrowed("Unlinked your Meetup account")
                };
                let _ = msg.channel_id.say(&ctx.http, message);
            }
            None => {
                let message = if is_organizer_command {
                    Cow::Owned(format!(
                        "There was seemingly no meetup account linked to <@{}>",
                        user_id
                    ))
                } else {
                    Cow::Borrowed("There was seemingly no meetup account linked to you")
                };
                let _ = msg.channel_id.say(&ctx.http, message);
            }
        }
        Ok(())
    }
}

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
            if guild_id != crate::discord_sync::GUILD_ID {
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
        if regexes.link_meetup(is_dm).is_match(&msg.content) {
            let user_id = msg.author.id.0;
            match Self::link_meetup(&ctx, &msg, &regexes, user_id) {
                Err(err) => {
                    eprintln!("Error: {}", err);
                    let _ = msg.channel_id.say(&ctx.http, "Something went wrong");
                    return;
                }
                _ => return,
            }
        } else if let Some(captures) = regexes.link_meetup_organizer(is_dm).captures(&msg.content) {
            // This is only for organizers
            if !msg
                .author
                .has_role(
                    &ctx,
                    crate::discord_sync::GUILD_ID,
                    crate::discord_sync::ORGANIZER_ID,
                )
                .unwrap_or(false)
            {
                let _ = msg.channel_id.say(&ctx.http, "Only organizers can do this");
                return;
            }
            // TODO
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
            match Self::link_meetup_organizer(&ctx, &msg, &regexes, discord_id, meetup_id) {
                Err(err) => {
                    eprintln!("Error: {}", err);
                    let _ = msg.channel_id.say(&ctx.http, "Something went wrong");
                    return;
                }
                _ => return,
            }
        } else if regexes.unlink_meetup(is_dm).is_match(&msg.content) {
            let user_id = msg.author.id.0;
            match Self::unlink_meetup(&ctx, &msg, /*is_organizer_command*/ false, user_id) {
                Err(err) => {
                    eprintln!("Error: {}", err);
                    let _ = msg.channel_id.say(&ctx.http, "Something went wrong");
                    return;
                }
                _ => return,
            }
        } else if let Some(captures) = regexes
            .unlink_meetup_organizer(is_dm)
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
            match Self::unlink_meetup(&ctx, &msg, /*is_organizer_command*/ true, discord_id) {
                Err(err) => {
                    eprintln!("Error: {}", err);
                    let _ = msg.channel_id.say(&ctx.http, "Something went wrong");
                    return;
                }
                _ => return,
            }
        } else if regexes.sync_meetup_mention.is_match(&msg.content) {
            // This is only for organizers
            if !msg
                .author
                .has_role(
                    &ctx,
                    crate::discord_sync::GUILD_ID,
                    crate::discord_sync::ORGANIZER_ID,
                )
                .unwrap_or(false)
            {
                let _ = msg.channel_id.say(&ctx.http, "Only organizers can do this");
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
                crate::meetup_sync::sync_task(async_meetup_client, redis_client)
                    .map_err(|err| {
                        eprintln!("Syncing task failed: {}", err);
                        err
                    })
                    .timeout(Duration::from_secs(60))
                    .map_err(|err| {
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
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, format!("Could not submit asynchronous Meetup synchronization task to the queue (full={}, disconnected={})", err.is_full(), err.is_disconnected()));
                }
            }
        } else if regexes.sync_discord_mention.is_match(&msg.content) {
            // This is only for organizers
            if !msg
                .author
                .has_role(
                    &ctx,
                    crate::discord_sync::GUILD_ID,
                    crate::discord_sync::ORGANIZER_ID,
                )
                .unwrap_or(false)
            {
                let _ = msg.channel_id.say(&ctx.http, "Only organizers can do this");
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
            task_scheduler.lock().add_task_datetime(
                white_rabbit::Utc::now(),
                crate::discord_sync::create_sync_discord_task(
                    redis_client,
                    CacheAndHttp {
                        cache: ctx.cache.clone(),
                        http: ctx.http.clone(),
                    },
                    bot_id.0,
                    /*recurring*/ false,
                ),
            );
            let _ = msg
                .channel_id
                .say(&ctx.http, "Started Discord synchronization task");
        } else {
            let _ = msg
                .channel_id
                .say(&ctx.http, "Sorry, I do not understand that command");
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
}
