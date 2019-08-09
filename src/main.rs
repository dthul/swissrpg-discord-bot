#![recursion_limit = "256"]
pub mod discord_bot;
pub mod discord_bot_commands;
pub mod discord_end_of_game;
pub mod discord_sync;
pub mod error;
pub mod meetup_api;
pub mod meetup_oauth2;
pub mod meetup_sync;
pub mod strings;

use error::BoxedError;
use futures::{Future, Stream};
use redis::Commands;
use serenity::prelude::{Mutex, RwLock};
use std::env;
use std::sync::Arc;
use tokio;

type Result<T> = std::result::Result<T, BoxedError>;

fn main() {
    let meetup_client_id =
        env::var("MEETUP_CLIENT_ID").expect("Found no MEETUP_CLIENT_ID in environment");
    let meetup_client_secret =
        env::var("MEETUP_CLIENT_SECRET").expect("Found no MEETUP_CLIENT_SECRET in environment");
    let discord_token = env::var("DISCORD_TOKEN").expect("Found no DISCORD_TOKEN in environment");

    // Connect to the local Redis server
    let redis_client =
        redis::Client::open("redis://127.0.0.1/").expect("Could not create a Redis client");
    let mut redis_connection = redis_client
        .get_connection()
        .expect("Could not connect to Redis");

    // Create a Meetup API client (might not be possible if there is no access token yet)
    let meetup_access_token: Option<String> = redis_connection
        .get("meetup_access_token")
        .expect("Meetup access token could not be loaded from Redis");
    let (meetup_client, async_meetup_client) = match meetup_access_token {
        Some(meetup_access_token) => (
            Arc::new(RwLock::new(Some(meetup_api::Client::new(
                &meetup_access_token,
            )))),
            Arc::new(RwLock::new(Some(meetup_api::AsyncClient::new(
                &meetup_access_token,
            )))),
        ),
        None => (Arc::new(RwLock::new(None)), Arc::new(RwLock::new(None))),
    };

    // Create a Meetup OAuth2 consumer
    let meetup_oauth2_consumer =
        meetup_oauth2::OAuth2Consumer::new(meetup_client_id, meetup_client_secret);

    // Create a task scheduler and schedule the refresh token task
    let task_scheduler = Arc::new(Mutex::new(white_rabbit::Scheduler::new(
        /*thread_count*/ 1,
    )));
    // Check Redis for a refresh time. If there is one, use that
    // if it is in the future. Otherwise schedule the task now
    let next_refresh_time: Option<String> = redis_connection
        .get("meetup_access_token_refresh_time")
        .expect("Could not query Redis for the next refresh time");
    // Try to get the next scheduled refresh time from Redis, otherwise
    // schedule a refresh immediately
    let next_refresh_time = match next_refresh_time.and_then(|time_string| {
        white_rabbit::DateTime::parse_from_rfc3339(&time_string)
            .ok()
            .map(|date_time| date_time.with_timezone(&white_rabbit::Utc))
    }) {
        Some(time) => time,
        None => white_rabbit::Utc::now(),
    };
    task_scheduler.lock().add_task_datetime(
        next_refresh_time,
        meetup_oauth2_consumer.token_refresh_task(
            redis_client
                .get_connection()
                .expect("Could not connect to Redis"),
            meetup_client.clone(),
        ),
    );

    let (tx, rx) = futures::sync::mpsc::channel::<crate::meetup_sync::BoxedFuture<(), ()>>(1);
    let spawn_other_futures_future = rx.for_each(|fut| tokio::spawn(fut));

    let mut bot = discord_bot::create_discord_client(
        &discord_token,
        redis_client.clone(),
        meetup_client.clone(),
        async_meetup_client.clone(),
        task_scheduler,
        tx,
    )
    .expect("Could not create the Discord bot");

    // Start a server to handle Meetup OAuth2 logins
    let meetup_oauth2_server = meetup_oauth2_consumer.create_auth_server(
        ([127, 0, 0, 1], 3000).into(),
        redis_client
            .get_connection()
            .expect("Could not connect to Redis"),
        bot.cache_and_http.clone(),
        meetup_client.clone(),
        async_meetup_client.clone(),
        bot.data
            .read()
            .get::<discord_bot::BotNameKey>()
            .expect("Bot name was not set")
            .clone(),
    );

    // let meetup_syncing_task = meetup_sync::create_recurring_syncing_task(
    //     async_meetup_client.clone(),
    //     redis_client.clone(),
    // )
    // .map_err(|err| eprintln!("Meetup syncing task failed: {}", err));

    std::thread::spawn(move || {
        tokio::run(
            meetup_oauth2_server
                .join(spawn_other_futures_future)
                .map(|_| ()),
        )
    });

    // Finally, start the Discord bot
    if let Err(why) = bot.start() {
        println!("Client error: {:?}", why);
    }
}
