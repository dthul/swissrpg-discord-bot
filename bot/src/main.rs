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
pub mod sync;
pub mod vacuum;

use error::BoxedError;
use futures::future;
use futures_util::stream::StreamExt;
use lazy_static::lazy_static;
use redis::Commands;
use std::{env, pin::Pin, sync::Arc};
use tokio;

type Result<T> = std::result::Result<T, BoxedError>;

lazy_static! {
    pub(crate) static ref ASYNC_RUNTIME: tokio::runtime::Runtime =
        tokio::runtime::Runtime::new().expect("Could not create tokio runtime");
}

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
            Arc::new(futures_util::lock::Mutex::new(Some(
                meetup_api::Client::new(&meetup_access_token),
            ))),
            Arc::new(futures_util::lock::Mutex::new(Some(
                meetup_api::AsyncClient::new(&meetup_access_token),
            ))),
        ),
        None => (
            Arc::new(futures_util::lock::Mutex::new(None)),
            Arc::new(futures_util::lock::Mutex::new(None)),
        ),
    };

    // Create a Meetup OAuth2 consumer
    let meetup_oauth2_consumer =
        meetup_oauth2::OAuth2Consumer::new(meetup_client_id, meetup_client_secret);

    // Create a task scheduler and schedule the refresh token task
    let task_scheduler = Arc::new(futures_util::lock::Mutex::new(
        white_rabbit::Scheduler::new(/*thread_count*/ 1),
    ));

    let (tx, rx) = futures_channel::mpsc::channel::<crate::meetup_sync::BoxedFuture<()>>(1);
    let spawn_other_futures_future = rx.for_each(|fut| {
        let pinned_fut: Pin<Box<_>> = fut.into();
        crate::ASYNC_RUNTIME.spawn(pinned_fut);
        future::ready(())
    });

    let mut bot = discord_bot::create_discord_client(
        &discord_token,
        redis_client.clone(),
        meetup_client.clone(),
        async_meetup_client.clone(),
        task_scheduler.clone(),
        tx,
    )
    .expect("Could not create the Discord bot");

    // Start a server to handle Meetup OAuth2 logins
    let meetup_oauth2_server = meetup_oauth2_consumer.create_auth_server(
        ([127, 0, 0, 1], 3000).into(),
        redis_client.clone(),
        bot.cache_and_http.clone(),
        meetup_client.clone(),
        async_meetup_client.clone(),
        bot.data
            .read()
            .get::<discord_bot::BotNameKey>()
            .expect("Bot name was not set")
            .clone(),
    );

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
    let mut task_scheduler_guard = ASYNC_RUNTIME.block_on(task_scheduler.lock());
    task_scheduler_guard.add_task_datetime(
        next_refresh_time,
        meetup_oauth2_consumer.organizer_token_refresh_task(
            redis_client.clone(),
            meetup_client.clone(),
            async_meetup_client.clone(),
        ),
    );
    drop(task_scheduler_guard);
    let discord_api = discord_bot::CacheAndHttp {
        cache: bot.cache_and_http.cache.clone().into(),
        http: bot.cache_and_http.http.clone(),
    };
    // Schedule the end of game task
    let end_of_game_task = discord_end_of_game::create_end_of_game_task(
        redis_client.clone(),
        discord_api.clone(),
        bot.data
            .read()
            .get::<discord_bot::BotIdKey>()
            .expect("Bot ID was not set")
            .clone(),
        /*recurring*/ true,
    );
    let next_end_of_game_task_time = {
        let mut task_time = white_rabbit::Utc::now().date().and_hms(18, 30, 0);
        // Check if it is later than 6:30pm
        // In that case, run the task tomorrow
        if white_rabbit::Utc::now() > task_time {
            task_time = task_time + white_rabbit::Duration::days(1);
        }
        task_time
    };
    let mut task_scheduler_guard = ASYNC_RUNTIME.block_on(task_scheduler.lock());
    task_scheduler_guard.add_task_datetime(next_end_of_game_task_time, end_of_game_task);
    drop(task_scheduler_guard);

    let syncing_task = sync::create_recurring_syncing_task(
        redis_client.clone(),
        async_meetup_client.clone(),
        discord_api,
        bot.data
            .read()
            .get::<discord_bot::BotIdKey>()
            .expect("Bot ID was not set")
            .clone(),
        task_scheduler.clone(),
    );

    ASYNC_RUNTIME.block_on(async {
        future::join3(
            meetup_oauth2_server,
            spawn_other_futures_future,
            syncing_task,
        )
        .await
    });

    // Finally, start the Discord bot
    if let Err(why) = bot.start() {
        println!("Client error: {:?}", why);
    }
}