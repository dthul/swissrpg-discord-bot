use crate::meetup_api;
use futures::future;
use futures::stream;
use futures::{Future, Stream};
use lazy_static::lazy_static;
use redis;
use serenity::prelude::RwLock;
use simple_error::SimpleError;
use std::sync::Arc;
use std::time::Duration;
use tokio;
use tokio::prelude::*;

const SESSION0_PATTERN: &'static str = r".+(?i)Session\s+0\s*$";
const INTRO_PATTERN: &'static str = r".+(?i)\[\s*Intro\s*games*series\s*\]\s*$";
const ONESHOT_PATTERN: &'static str = r".+(?i)\[.*One\s*\-?\s*Shot.*\]\s*$";

lazy_static! {
    static ref SESSION0_REGEX: regex::Regex = regex::Regex::new(SESSION0_PATTERN).unwrap();
    static ref INTRO_REGEX: regex::Regex = regex::Regex::new(INTRO_PATTERN).unwrap();
    static ref ONESHOT_REGEX: regex::Regex = regex::Regex::new(ONESHOT_PATTERN).unwrap();
    static ref FILTER_REGEX_SET: regex::RegexSet =
        regex::RegexSet::new(&[SESSION0_PATTERN, INTRO_PATTERN, ONESHOT_PATTERN]).unwrap();
    static ref EVENT_NAME_REGEX: regex::Regex =
        regex::Regex::new(r"^\s*(?P<name>.+?)\s*\[").unwrap();
}

type BoxedFuture<T> = Box<dyn Future<Item = T, Error = crate::BoxedError> + Send>;

pub fn create_recurring_syncing_task(
    meetup_client: Arc<RwLock<Option<meetup_api::AsyncClient>>>,
    redis_client: &redis::Client,
) -> impl Future<Item = (), Error = crate::BoxedError> {
    redis_client
        .get_async_connection()
        .and_then(|conn| redis::aio::SharedConnection::new(conn))
        .map_err(|err| {
            Box::new(SimpleError::new(format!(
                "Could not acquire async Redis connection: {}",
                err
            ))) as crate::BoxedError
        })
        .and_then(move |shared_conn| {
            // Run forever
            tokio::timer::Interval::new_interval(Duration::from_secs(15 * 60))
                .map_err(|err| {
                    eprintln!("Interval timer error: {}", err);
                    Box::new(err) as crate::BoxedError
                })
                .for_each(move |_| {
                    tokio::spawn(
                        sync_task(meetup_client.clone(), shared_conn.clone())
                            .timeout(Duration::from_secs(60))
                            .map_err(|err| {
                                eprintln!("Syncing task timed out: {}", err);
                            }),
                    );
                    future::ok(())
                })
        })
}

// TODO: Introduce a type like "Meetup connection" that contains
// an Arc<RwLock<Option<MeetupClient>>> internally and has the same
// methods as MeetupClient (so we don't need to match on the Option
// every time we want to use the client)
pub fn sync_task(
    meetup_client: Arc<RwLock<Option<meetup_api::AsyncClient>>>,
    redis_connection: redis::aio::SharedConnection,
) -> impl Future<Item = (), Error = crate::BoxedError> + Send + 'static {
    let upcoming_events = match *meetup_client.read() {
        Some(ref meetup_client) => meetup_client.get_upcoming_events(),
        None => {
            return Box::new(
                future::err(SimpleError::new("Meetup API unavailable"))
                    .from_err::<crate::BoxedError>(),
            ) as BoxedFuture<_>
        }
    };
    let sync_future = upcoming_events
        .filter(|event| FILTER_REGEX_SET.is_match(&event.name))
        .and_then(move |event| {
            let rsvps = match *meetup_client.read() {
                Some(ref meetup_client) => meetup_client.get_rsvps(event.id),
                None => {
                    return Box::new(
                        future::err(SimpleError::new("Meetup API unavailable"))
                            .from_err::<crate::BoxedError>(),
                    ) as BoxedFuture<_>
                }
            };
            Box::new(rsvps.map(|rsvps| (event, rsvps)))
        })
        .for_each(
            move |(event, rsvps): (meetup_api::Event, Vec<meetup_api::RSVP>)| {
                tokio::spawn(
                    sync_event(event, rsvps, redis_connection.clone())
                        .map_err(|err| eprintln!("Event sync failed: {}", err)),
                );
                future::ok(())
            },
        );
    return Box::new(sync_future);
}

fn sync_event(
    event: meetup_api::Event,
    rsvps: Vec<meetup_api::RSVP>,
    redis_connection: redis::aio::SharedConnection,
) -> impl Future<Item = (), Error = crate::BoxedError> {
    let event_name = match EVENT_NAME_REGEX.captures(&event.name) {
        Some(captures) => captures.name("name").unwrap().as_str(),
        None => &event.name,
    };
    future::ok(())
}
