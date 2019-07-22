pub mod discord_bot;
pub mod meetup_api;
pub mod meetup_oauth2;

use redis::Commands;
use serenity::prelude::Mutex;
use std::env;
use std::sync::Arc;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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
    let meetup_client = match meetup_access_token {
        Some(meetup_access_token) => Arc::new(Mutex::new(Some(meetup_api::Client::new(
            &meetup_access_token,
        )))),
        None => Arc::new(Mutex::new(None)),
    };

    // Create a Meetup OAuth2 consumer
    let meetup_oauth2_consumer =
        meetup_oauth2::OAuth2Consumer::new(meetup_client_id, meetup_client_secret);

    // Create a task scheduler and schedule the refresh token task
    let mut task_scheduler = white_rabbit::Scheduler::new(/*thread_count*/ 1);
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
    task_scheduler.add_task_datetime(
        next_refresh_time,
        meetup_oauth2_consumer.token_refresh_task(
            redis_client
                .get_connection()
                .expect("Could not connect to Redis"),
            meetup_client.clone(),
        ),
    );

    // Finally, start the Discord bot
    let mut bot =
        discord_bot::create_discord_client(&discord_token, &redis_client, meetup_client.clone())
            .expect("Could not create the Discord bot");

    // Start a server to handle Meetup OAuth2 logins
    let meetup_oauth2_server = meetup_oauth2_consumer.create_auth_server(
        ([127, 0, 0, 1], 3000).into(),
        redis_client
            .get_connection()
            .expect("Could not connect to Redis"),
            bot.cache_and_http.clone()
    );
    std::thread::spawn(move || hyper::rt::run(meetup_oauth2_server));
    if let Err(why) = bot.start() {
        println!("Client error: {:?}", why);
    }
}
