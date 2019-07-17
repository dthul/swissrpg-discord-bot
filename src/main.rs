use lazy_static::lazy_static;

use regex::Regex;

use std::env;

use serenity::{
    model::{channel::Message, gateway::Ready},
    prelude::*,
};

lazy_static! {
    static ref CREATE_CHANNEL_REGEX: Regex =
        Regex::new(r"^!createchannel\s+(?P<channelname>(?:[0-9a-zA-Z_]+\-?)+)\s*$").unwrap();
}

struct Handler;

impl EventHandler for Handler {
    // Set a handler for the `message` event - so that whenever a new message
    // is received - the closure (or function) passed will be called.
    //
    // Event handlers are dispatched through a threadpool, and so multiple
    // events can be dispatched simultaneously.
    fn message(&self, ctx: Context, msg: Message) {
        if !msg.content.starts_with("!") {
            // Quickly bail before any more expensive tests
            return;
        }
        if msg.content == "!help" {
            msg.channel_id
                .say(
                    &ctx.http,
                    "Available commands:\n!help\n!ping\n!createchannel",
                )
                .ok();
        } else if msg.content == "!ping" {
            // Sending a message can fail, due to a network error, an
            // authentication error, or lack of permissions to post in the
            // channel, so log to stdout when some error happens, with a
            // description of it.
            if let Err(why) = msg.channel_id.say(&ctx.http, "Pong!") {
                println!("Error sending message: {:?}", why);
            }
        } else if msg.content.starts_with("!createchannel") {
            // TODO: check permission
            if let Some(captures) = CREATE_CHANNEL_REGEX.captures(&msg.content) {
                let channel_name = captures.name("channelname").unwrap().as_str();
                if let Some(guild) = msg.guild(&ctx.cache) {
                    let guild = guild.read();
                    guild
                        .create_channel(&ctx, |channel_builder| channel_builder.name(channel_name))
                        .ok();
                }
            } else {
                msg.channel_id.say(&ctx.http, "Did not create channel. This is how to use the command:\n!createchannel channel_name").ok();
            }
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
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    let mut client = Client::new(&token, Handler).expect("Err creating client");

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform
    // exponential backoff until it reconnects.
    if let Err(why) = client.start() {
        println!("Client error: {:?}", why);
    }
}
