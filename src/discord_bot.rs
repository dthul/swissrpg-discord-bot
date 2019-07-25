use redis::{Commands, PipelineCommands};
use regex::Regex;
use serenity::{
    model::{
        channel::Channel, channel::Message, channel::PermissionOverwrite,
        channel::PermissionOverwriteType, gateway::Ready, id::RoleId, id::UserId,
        permissions::Permissions,
    },
    prelude::*,
};
use simple_error::SimpleError;
use std::sync::Arc;

const CHANNEL_NAME_PATTERN: &'static str = r"(?:[0-9a-zA-Z_]+\-?)+"; // TODO: this is too strict
const MENTION_PATTERN: &'static str = r"<@[0-9]+>";

pub fn create_discord_client(
    discord_token: &str,
    redis_client: &redis::Client,
    meetup_client: Arc<Mutex<Option<crate::meetup_api::Client>>>,
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
        data.insert::<MeetupClientKey>(meetup_client.clone());
    }

    Ok(client)
}

struct Regexes {
    bot_mention: String,
    create_channel_dm: Regex,
    create_channel_mention: Regex,
    link_meetup_dm: Regex,
    link_meetup_mention: Regex,
    link_meetup_direct_dm: Regex,
    link_meetup_direct_mention: Regex,
    unlink_meetup_dm: Regex,
    unlink_meetup_mention: Regex,
}

impl Regexes {
    fn create_channel(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.create_channel_dm
        } else {
            &self.create_channel_mention
        }
    }

    fn link_meetup(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.link_meetup_dm
        } else {
            &self.link_meetup_mention
        }
    }

    fn link_meetup_direct(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.link_meetup_direct_dm
        } else {
            &self.link_meetup_direct_mention
        }
    }

    fn unlink_meetup(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.unlink_meetup_dm
        } else {
            &self.unlink_meetup_mention
        }
    }
}

fn compile_regexes(bot_id: u64) -> Regexes {
    let bot_mention = format!(r"<@{}>", bot_id);
    let create_channel = format!(
        r"create[ -]?channel\s+(?P<channelname>{cnp})",
        cnp = CHANNEL_NAME_PATTERN
    );
    let create_channel_dm = format!(r"^{create_channel}\s*$", create_channel = create_channel);
    let create_channel_mention = format!(
        r"^{bot_mention}\s+{create_channel}\s*$",
        bot_mention = bot_mention,
        create_channel = create_channel
    );
    let link_meetup_dm = r"^link[ -]?meetup\s*$";
    let link_meetup_mention = format!(
        r"^{bot_mention}\s+link[ -]?meetup\s*$",
        bot_mention = bot_mention
    );
    let link_meetup_direct = r"link[ -]?meetup\s+(?P<meetupid>[0-9]+)";
    let link_meetup_direct_dm = format!(
        r"^{link_meetup_direct}\s*$",
        link_meetup_direct = link_meetup_direct
    );
    let link_meetup_direct_mention = format!(
        r"^{bot_mention}\s+{link_meetup_direct}\s*$",
        bot_mention = bot_mention,
        link_meetup_direct = link_meetup_direct
    );
    let unlink_meetup = r"unlink[ -]?meetup";
    let unlink_meetup_dm = format!(r"^{unlink_meetup}\s*$", unlink_meetup = unlink_meetup);
    let unlink_meetup_mention = format!(
        r"^{bot_mention}\s+{unlink_meetup}\s*$",
        bot_mention = bot_mention,
        unlink_meetup = unlink_meetup
    );
    Regexes {
        bot_mention: bot_mention,
        create_channel_dm: Regex::new(create_channel_dm.as_str()).unwrap(),
        create_channel_mention: Regex::new(create_channel_mention.as_str()).unwrap(),
        link_meetup_dm: Regex::new(link_meetup_dm).unwrap(),
        link_meetup_mention: Regex::new(link_meetup_mention.as_str()).unwrap(),
        link_meetup_direct_dm: Regex::new(link_meetup_direct_dm.as_str()).unwrap(),
        link_meetup_direct_mention: Regex::new(link_meetup_direct_mention.as_str()).unwrap(),
        unlink_meetup_dm: Regex::new(unlink_meetup_dm.as_str()).unwrap(),
        unlink_meetup_mention: Regex::new(unlink_meetup_mention.as_str()).unwrap(),
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
    type Value = Arc<Mutex<Option<crate::meetup_api::Client>>>;
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
            match *meetup_client_mutex.lock() {
                Some(ref meetup_client) => {
                    match meetup_client.get_member_profile(Some(linked_meetup_id))? {
                        Some(user) => {
                            let _ = msg.channel_id.say(
                                &ctx.http,
                                format!(
                                    "You are already linked to {}'s Meetup account. \
                                     If you really want to change this, unlink your currently \
                                     linked meetup account first by writing:\n\
                                     `{} unlink meetup`",
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
                                     `{} unlink meetup`",
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
                             `{} unlink meetup`",
                            regexes.bot_mention
                        ),
                    );
                }
            }
            return Ok(());
        }
        let url =
            crate::meetup_oauth2::generate_meetup_linking_link(&redis_connection_mutex, user_id)?;
        let _ = msg.channel_id.say(
            &ctx.http,
            format!(
                "Visit the following website to link your Meetup profile: {}\n\
                    ***This is a private one-time use link and meant just for you.***\n
                    Don't share it or others might link your Discord account to their Meetup profile.",
                url
            ),
        );
        Ok(())
    }

    fn link_meetup_direct(
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
                    "All good, your Meetup account was already linked",
                );
                return Ok(());
            } else {
                let _ = msg.channel_id.say(
                    &ctx.http,
                    format!(
                        "You are already linked to a different Meetup account. \
                         If you really want to change this, unlink your currently \
                         linked meetup account first by writing:\n\
                         {} unlink meetup",
                        regexes.bot_mention
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
        if let Some(_linked_discord_id) = linked_discord_id {
            let _ = msg.channel_id.say(
                &ctx.http,
                "This Meetup account is alread linked to someone else. \
                 If you are sure that you specified the correct Meetup id, \
                 please contact an @Organiser",
            );
            return Ok(());
        }
        // The user has not yet linked their meetup account.
        // Test whether the specified Meetup user actually exists.
        let meetup_user = meetup_client_mutex
            .lock()
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
                                    .set(&redis_key_m2d, user_id)
                                    .ignore();
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
                                "Successfully linked to {}'s Meetup account",
                                meetup_user.name
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
        _regexes: &Regexes,
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
                let _ = msg
                    .channel_id
                    .say(&ctx.http, "Unlinked your Meetup account");
            }
            None => {
                let _ = msg.channel_id.say(
                    &ctx.http,
                    "There was seemingly no meetup account linked to you",
                );
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
        if let Some(_) = regexes.link_meetup(is_dm).captures(&msg.content) {
            let user_id = msg.author.id.0;
            match Self::link_meetup(&ctx, &msg, &regexes, user_id) {
                Err(err) => {
                    eprintln!("Error: {}", err);
                    let _ = msg.channel_id.say(&ctx.http, "Something went wrong");
                    return;
                }
                _ => return,
            }
        } else if let Some(captures) = regexes.link_meetup_direct(is_dm).captures(&msg.content) {
            // TODO: this is only for organizers
            let user_id = msg.author.id.0;
            let meetup_id = captures.name("meetupid").unwrap().as_str();
            // Try to convert the specified ID to an integer
            let meetup_id = match meetup_id.parse::<u64>() {
                Ok(id) => id,
                _ => {
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, "Seems like the specified Meetup ID is invalid");
                    return;
                }
            };
            match Self::link_meetup_direct(&ctx, &msg, &regexes, user_id, meetup_id) {
                Err(err) => {
                    eprintln!("Error: {}", err);
                    let _ = msg.channel_id.say(&ctx.http, "Something went wrong");
                    return;
                }
                _ => return,
            }
        } else if let Some(_) = regexes.unlink_meetup(is_dm).captures(&msg.content) {
            let user_id = msg.author.id.0;
            match Self::unlink_meetup(&ctx, &msg, &regexes, user_id) {
                Err(err) => {
                    eprintln!("Error: {}", err);
                    let _ = msg.channel_id.say(&ctx.http, "Something went wrong");
                    return;
                }
                _ => return,
            }
        } else if let Some(captures) = regexes.create_channel(is_dm).captures(&msg.content) {
            // TODO: this is only for organizers
            // TODO: needs host
            // TODO: redis schema
            let channel_name = captures.name("channelname").unwrap().as_str();
            if let Some(guild_id) = msg.guild_id {
                match guild_id.create_role(&ctx, |role_builder| {
                    role_builder
                        .name(channel_name)
                        .permissions(Permissions::empty())
                }) {
                    Ok(role_channel) => {
                        // Make sure that the user that issued this command is assigned the new role
                        let _ = ctx.http.add_member_role(
                            guild_id.0,
                            msg.author.id.0,
                            role_channel.id.0,
                        );
                        // The @everyone role has the same id as the guild
                        let role_everyone_id = RoleId(guild_id.0);
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
                                kind: PermissionOverwriteType::Role(role_channel.id),
                            },
                            PermissionOverwrite {
                                allow: Permissions::READ_MESSAGES,
                                deny: Permissions::empty(),
                                kind: PermissionOverwriteType::Member(bot_id),
                            },
                        ];
                        let _ = guild_id.create_channel(&ctx, |channel_builder| {
                            channel_builder
                                .name(channel_name)
                                .permissions(permission_overwrites)
                        });
                    }
                    _ => {}
                };
            }
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
