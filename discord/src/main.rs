use redis::Commands;
use regex::Regex;
use serenity::{
    model::{
        channel::Channel, channel::Message, channel::PermissionOverwrite,
        channel::PermissionOverwriteType, gateway::Ready, id::RoleId, id::UserId,
        permissions::Permissions,
    },
    prelude::*,
};
use std::env;
use std::sync::Arc;

const CHANNEL_NAME_PATTERN: &'static str = r"(?:[0-9a-zA-Z_]+\-?)+"; // TODO: this is too strict
const MENTION_PATTERN: &'static str = r"<@[0-9]+>";

struct Regexes {
    bot_mention: String,
    create_channel_dm: Regex,
    create_channel_mention: Regex,
    link_meetup_dm: Regex,
    link_meetup_mention: Regex,
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
    let link_meetup = r"link[ -]?meetup\s+(?P<meetupid>[0-9]+)";
    let link_meetup_dm = format!(r"^{link_meetup}\s*$", link_meetup = link_meetup);
    let link_meetup_mention = format!(
        r"^{bot_mention}\s+{link_meetup}\s*$",
        bot_mention = bot_mention,
        link_meetup = link_meetup
    );
    Regexes {
        bot_mention: bot_mention,
        create_channel_dm: Regex::new(create_channel_dm.as_str()).unwrap(),
        create_channel_mention: Regex::new(create_channel_mention.as_str()).unwrap(),
        link_meetup_dm: Regex::new(link_meetup_dm.as_str()).unwrap(),
        link_meetup_mention: Regex::new(link_meetup_mention.as_str()).unwrap(),
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

struct Handler;

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
        if let Some(captures) = regexes.link_meetup(is_dm).captures(&msg.content) {
            let user_id = msg.author.id.0;
            let meetup_id = captures.name("meetupid").unwrap().as_str();
            let redis_key = format!("user:{}:meetupid", user_id);
            let redis_connection_mutex = {
                ctx.data
                    .read()
                    .get::<RedisConnectionKey>()
                    .expect("Redis connection was not set")
                    .clone()
            };
            {
                let mut redis_connection = redis_connection_mutex.lock();
                if let Ok(()) = redis_connection.sadd("users", user_id) {
                    if let Ok(()) = redis_connection.set(&redis_key, meetup_id) {
                        let _ = msg.channel_id.say(&ctx.http, format!("Assigned meetup id"));
                        return;
                    }
                }
                let _ = msg
                    .channel_id
                    .say(&ctx.http, "Could not assign meetup id (internal error)");
            }
        } else if let Some(captures) = regexes.create_channel(is_dm).captures(&msg.content) {
            // TODO: check permission
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

fn main() {
    // Connect to the local Redis server
    let client =
        redis::Client::open("redis://127.0.0.1/").expect("Could not create a Redis client");
    let connection = client
        .get_connection()
        .expect("Could not create a Redis connection");

    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    let mut client = Client::new(&token, Handler).expect("Err creating client");

    // We will fetch the bot's id.
    let bot_id = match client.cache_and_http.http.get_current_application_info() {
        Ok(info) => info.id,
        Err(why) => panic!("Could not access application info: {:?}", why),
    };

    // pre-compile the regexes
    let regexes = compile_regexes(bot_id.0);

    // Store the bot's id in the client for easy access
    {
        let mut data = client.data.write();
        data.insert::<BotIdKey>(bot_id);
        data.insert::<RedisConnectionKey>(Arc::new(Mutex::new(connection)));
        data.insert::<RegexesKey>(Arc::new(regexes));
    }

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform
    // exponential backoff until it reconnects.
    if let Err(why) = client.start() {
        println!("Client error: {:?}", why);
    }
}
