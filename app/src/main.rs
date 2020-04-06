#![forbid(unsafe_code)]
mod sync_task;

use futures::future;
use redis::Commands;
use std::{
    env,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

fn main() {
    let environment = env::var("BOT_ENV").expect("Found no BOT_ENV in environment");
    let is_test_environment = match environment.as_str() {
        "prod" => false,
        "test" => true,
        _ => panic!("BOT_ENV needs to be one of \"prod\" or \"test\""),
    };
    if cfg!(feature = "bottest") != is_test_environment {
        panic!(
            "The bot was compiled with bottest = {} but started with BOT_ENV = {}",
            cfg!(feature = "bottest"),
            environment
        );
    }
    let meetup_client_id =
        env::var("MEETUP_CLIENT_ID").expect("Found no MEETUP_CLIENT_ID in environment");
    let meetup_client_secret =
        env::var("MEETUP_CLIENT_SECRET").expect("Found no MEETUP_CLIENT_SECRET in environment");
    let discord_token = env::var("DISCORD_TOKEN").expect("Found no DISCORD_TOKEN in environment");
    let stripe_client_secret =
        env::var("STRIPE_CLIENT_SECRET").expect("Found no STRIPE_CLIENT_SECRET in environment");
    let stripe_webhook_signing_secret = env::var("STRIPE_WEBHOOK_SIGNING_SECRET").ok();
    if stripe_webhook_signing_secret.is_none() {
        eprintln!("No Stripe webhook signing secret set. Will not listen to Stripe webhooks.");
    }
    let api_key = env::var("API_KEY").ok();
    if api_key.is_none() {
        eprintln!("No API key set. Will not listen to API requests.");
    }

    // Connect to the local Redis server
    let redis_url = if cfg!(feature = "bottest") {
        "redis://127.0.0.1/1"
    } else {
        "redis://127.0.0.1/0"
    };
    let redis_client = redis::Client::open(redis_url).expect("Could not create a Redis client");
    let mut redis_connection = redis_client
        .get_connection()
        .expect("Could not connect to Redis");

    // Create a Meetup API client (might not be possible if there is no access token yet)
    let meetup_access_token: Option<String> = redis_connection
        .get("meetup_access_token")
        .expect("Meetup access token could not be loaded from Redis");
    let async_meetup_client = match meetup_access_token {
        Some(meetup_access_token) => Arc::new(futures_util::lock::Mutex::new(Some(Arc::new(
            lib::meetup::api::AsyncClient::new(&meetup_access_token),
        )))),
        None => Arc::new(futures_util::lock::Mutex::new(None)),
    };

    // Create a Meetup OAuth2 consumer
    let meetup_oauth2_consumer = Arc::new(
        lib::meetup::oauth2::OAuth2Consumer::new(meetup_client_id, meetup_client_secret)
            .expect("Could not create a Meetup OAuth2 consumer"),
    );

    // Create a Stripe client
    let stripe_client = Arc::new(stripe::Client::new(&stripe_client_secret).with_headers(
        stripe::Headers {
            // stripe_version: Some(stripe::ApiVersion::V2019_03_14),
            ..Default::default()
        },
    ));

    // Create a tokio runtime
    let async_runtime = Arc::new(tokio::sync::RwLock::new(Some(
        tokio::runtime::Builder::new()
            .enable_io()
            .enable_time()
            .threaded_scheduler()
            .build()
            .expect("Could not create tokio runtime"),
    )));

    // Create a task scheduler and schedule the refresh token task
    let task_scheduler = Arc::new(futures_util::lock::Mutex::new(
        white_rabbit::Scheduler::new(/*thread_count*/ 1),
    ));

    let bot_shutdown_signal = Arc::new(AtomicBool::new(false));
    let mut bot = ui::discord::bot::create_discord_client(
        &discord_token,
        redis_client.clone(),
        async_meetup_client.clone(),
        task_scheduler.clone(),
        meetup_oauth2_consumer.clone(),
        stripe_client.clone(),
        async_runtime.clone(),
        bot_shutdown_signal.clone(),
    )
    .expect("Could not create the Discord bot");
    let discord_api = lib::discord::CacheAndHttp {
        cache: bot.cache_and_http.cache.clone().into(),
        http: bot.cache_and_http.http.clone(),
    };

    // Start a server to handle Meetup OAuth2 logins
    let port = if cfg!(feature = "bottest") {
        3001
    } else {
        3000
    };
    let (abort_web_server_tx, abort_web_server_rx) = tokio::sync::oneshot::channel::<()>();
    let abort_web_server_signal = async {
        abort_web_server_rx.await.ok();
    };
    let web_server = ui::web::server::create_server(
        meetup_oauth2_consumer.clone(),
        ([127, 0, 0, 1], port).into(),
        redis_client.clone(),
        async_meetup_client.clone(),
        discord_api.clone(),
        bot.data
            .read()
            .get::<ui::discord::bot::BotNameKey>()
            .expect("Bot name was not set")
            .clone(),
        stripe_webhook_signing_secret,
        stripe_client.clone(),
        api_key,
        abort_web_server_signal,
    );

    // Organizer OAuth2 token refresh task
    let organizer_token_refresh_task = lib::tasks::token_refresh::organizer_token_refresh_task(
        (*meetup_oauth2_consumer).clone(),
        redis_client.clone(),
        async_meetup_client.clone(),
    );

    // Users OAuth2 token refresh task
    let users_token_refresh_task = lib::tasks::token_refresh::users_token_refresh_task(
        (*meetup_oauth2_consumer).clone(),
        redis_client.clone(),
    );

    // Schedule the end of game task
    let end_of_game_task = lib::tasks::end_of_game::create_end_of_game_task(
        redis_client.clone(),
        discord_api.clone(),
        bot.data
            .read()
            .get::<ui::discord::bot::BotIdKey>()
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
    let mut task_scheduler_guard = futures::executor::block_on(task_scheduler.lock());
    task_scheduler_guard.add_task_datetime(next_end_of_game_task_time, end_of_game_task);
    drop(task_scheduler_guard);

    let syncing_task = crate::sync_task::create_recurring_syncing_task(
        redis_client.clone(),
        async_meetup_client.clone(),
        discord_api.clone(),
        bot.data
            .read()
            .get::<ui::discord::bot::BotIdKey>()
            .expect("Bot ID was not set")
            .clone(),
        task_scheduler.clone(),
    );

    let stripe_subscription_refresh_task =
        lib::tasks::subscription_roles::stripe_subscriptions_refresh_task(
            discord_api.clone(),
            stripe_client.clone(),
        );

    // Wrap the long-running tasks in abortable Futures
    let (organizer_token_refresh_task, abort_handle_organizer_token_refresh_task) =
        future::abortable(organizer_token_refresh_task);
    let (users_token_refresh_task, abort_handle_users_token_refresh_task) =
        future::abortable(users_token_refresh_task);
    let (syncing_task, abort_handle_syncing_task) = future::abortable(syncing_task);
    let (stripe_subscription_refresh_task, abort_handle_stripe_subscription_refresh_task) =
        future::abortable(stripe_subscription_refresh_task);

    // Create a synchronization barrier that keeps the main thread from exiting
    // until the signal handler tells it to
    let barrier = Arc::new(std::sync::Barrier::new(2));

    // Create the signal handling task
    {
        let barrier = barrier.clone();
        let signals =
            signal_hook::iterator::Signals::new(&[signal_hook::SIGINT, signal_hook::SIGTERM])
                .expect("Could not register the signal handler");
        std::thread::spawn(move || {
            // Wait for SIGINT or SIGTERM to arrive
            for signal in signals.forever() {
                match signal {
                    signal_hook::SIGINT => println!("Received SIGINT. Shutting down."),
                    signal_hook::SIGTERM => println!("Received SIGTERM. Shutting down."),
                    _ => unreachable!(),
                }
                // Finally, tell the main thread to exit
                barrier.wait();
                break;
            }
        });
    }

    // Spawn all tasks onto the async runtime
    {
        let runtime_guard = futures::executor::block_on(async_runtime.read());
        let async_runtime = match *runtime_guard {
            Some(ref async_runtime) => async_runtime,
            None => panic!("Async runtime not available"),
        };
        async_runtime.enter(move || {
            tokio::spawn(async {
                let _ = organizer_token_refresh_task.await;
                println!("Organizer token refresh task shut down.");
            });
            tokio::spawn(async {
                let _ = users_token_refresh_task.await;
                println!("User token refresh task shut down.");
            });
            tokio::spawn(async {
                let _ = syncing_task.await;
                println!("Syncing task shut down.");
            });
            tokio::spawn(async {
                let _ = stripe_subscription_refresh_task.await;
                println!("Stripe subscription refresh task shut down.");
            });
            tokio::spawn(async {
                web_server.await;
                println!("Web server shut down.");
            });
        });
        // runtime guard dropped here
    }

    // Start the Discord bot in another thread
    std::thread::spawn(move || {
        if let Err(why) = bot.start() {
            println!("Client error: {:?}", why);
        }
    });

    // Wait for a signal to exit the main thread
    barrier.wait();

    // We received the signal to shut down.
    // Abort all long running futures
    bot_shutdown_signal.store(true, Ordering::Release);
    abort_handle_organizer_token_refresh_task.abort();
    abort_handle_users_token_refresh_task.abort();
    abort_handle_syncing_task.abort();
    abort_handle_stripe_subscription_refresh_task.abort();
    let _ = abort_web_server_tx.send(());
    let mut runtime_guard = futures::executor::block_on(async_runtime.write());
    // Give any currently running tasks a chance to finish
    println!("Waiting for tasks to finish.");
    std::thread::sleep(std::time::Duration::from_secs(10));
    if let Some(async_runtime) = runtime_guard.take() {
        println!("About to shut down the tokio runtime.");
        async_runtime.shutdown_timeout(tokio::time::Duration::from_secs(10));
        println!("Tokio runtime shut down.\nHyperion out.");
    }
}
