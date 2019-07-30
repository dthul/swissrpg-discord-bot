use crate::meetup_api;
use futures::future;
use futures::{Future, Stream};
use lazy_static::lazy_static;
use redis;
use redis::PipelineCommands;
use serenity::prelude::RwLock;
use simple_error::SimpleError;
use std::sync::Arc;
use std::time::Duration;
use tokio;
use tokio::prelude::*;

const SESSION0_PATTERN: &'static str = r".+(?i)Session\s+0\s*$";
const INTRO_PATTERN: &'static str = r".+(?i)\[\s*Intro\s*game\s*series\s*\]\s*$";
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

pub type BoxedFuture<T, E = crate::BoxedError> = Box<dyn Future<Item = T, Error = E> + Send>;

pub fn create_recurring_syncing_task(
    meetup_client: Arc<RwLock<Option<meetup_api::AsyncClient>>>,
    redis_client: redis::Client,
) -> impl Future<Item = (), Error = crate::BoxedError> {
    // Run forever
    tokio::timer::Interval::new_interval(Duration::from_secs(15 * 60))
        .map_err(|err| {
            eprintln!("Interval timer error: {}", err);
            Box::new(err) as crate::BoxedError
        })
        .for_each(move |_| {
            tokio::spawn(
                sync_task(meetup_client.clone(), redis_client.clone())
                    .map_err(|err| {
                        eprintln!("Syncing task failed: {}", err);
                        err
                    })
                    .timeout(Duration::from_secs(60))
                    .map_err(|err| {
                        eprintln!("Syncing task timed out: {}", err);
                    }),
            );
            future::ok(())
        })
}

// TODO: Introduce a type like "Meetup connection" that contains
// an Arc<RwLock<Option<MeetupClient>>> internally and has the same
// methods as MeetupClient (so we don't need to match on the Option
// every time we want to use the client)
pub fn sync_task(
    meetup_client: Arc<RwLock<Option<meetup_api::AsyncClient>>>,
    redis_client: redis::Client,
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
        .filter(|event| if FILTER_REGEX_SET.is_match(&event.name) {
            println!("Syncing task: Found event \"{}\"", event.name);
            true
        } else {
            println!("Syncing task: Ignoring event \"{}\" (event name does not match any filter pattern)", event.name);
            false
        })
        .and_then(move |event| {
            println!("Syncing task: Querying RSVPs for event \"{}\"", event.name);
            let rsvps = match *meetup_client.read() {
                Some(ref meetup_client) => meetup_client.get_rsvps(&event.id),
                None => {
                    return Box::new(
                        future::err(SimpleError::new("Meetup API unavailable"))
                            .from_err::<crate::BoxedError>(),
                    ) as BoxedFuture<_>
                }
            };
            Box::new(rsvps.map(|rsvps| {
                println!("Syncing task: Found {} RSVPs for event \"{}\"", rsvps.len(), event.name);
                (event, rsvps)
            }))
        })
        .for_each(
            move |(event, rsvps): (meetup_api::Event, Vec<meetup_api::RSVP>)| {
                tokio::spawn(
                    sync_event(event, rsvps, redis_client.clone())
                        .map_err(|err| eprintln!("Event sync failed: {}", err)),
                );
                future::ok(())
            },
        );
    return Box::new(sync_future);
}

// This function is supposed to be idempotent, so calling it with the same
// event is fine.
fn sync_event(
    event: meetup_api::Event,
    rsvps: Vec<meetup_api::RSVP>,
    redis_client: redis::Client,
) -> impl Future<Item = (), Error = crate::BoxedError> {
    // let event_name = match EVENT_NAME_REGEX.captures(&event.name) {
    //     Some(captures) => captures.name("name").unwrap().as_str(),
    //     None => &event.name,
    // };
    let rsvp_yes_user_ids: Vec<_> = rsvps
        .iter()
        .filter_map(|rsvp| {
            if rsvp.response == meetup_api::RSVPResponse::Yes {
                Some(rsvp.member.id)
            } else {
                None
            }
        })
        .collect();
    // TODO: figure out whether this event belongs to a series
    // For now, we assume that an event that reaches this method does not yet
    // belong to a series and create a new one
    let redis_events_key = "meetup_events";
    let redis_series_key = "event_series";
    let redis_event_users_key = format!("meetup_event:{}:meetup_users", event.id);
    let redis_event_hosts_key = format!("meetup_event:{}:meetup_hosts", event.id);
    let redis_event_series_key = format!("meetup_event:{}:event_series", event.id);
    let redis_event_key = format!("meetup_event:{}", event.id);
    let event_name = event.name.clone();
    // technically: check that series id doesn't exist yet and generate a new one until it does not
    // practically: we will never generate a colliding id
    let fut = redis_client
        .get_async_connection()
        .from_err::<crate::BoxedError>()
        .and_then(move |con| {
            let transaction_fn = {
                let redis_event_users_key = redis_event_users_key.clone();
                let redis_event_hosts_key = redis_event_hosts_key.clone();
                let redis_event_series_key = redis_event_series_key.clone();
                let redis_event_key = redis_event_key.clone();
                move |con, mut pipe: redis::Pipeline| {
                    let event = event.clone();
                    let rsvp_yes_user_ids = rsvp_yes_user_ids.clone();
                    let redis_events_key = redis_events_key.clone();
                    let redis_series_key = redis_series_key.clone();
                    let redis_event_users_key = redis_event_users_key.clone();
                    let redis_event_hosts_key = redis_event_hosts_key.clone();
                    let redis_event_series_key = redis_event_series_key.clone();
                    let redis_event_key = redis_event_key.clone();
                    let transaction_future = redis::cmd("GET")
                        .arg(&redis_event_series_key)
                        .query_async(con)
                        .and_then(move |(con, series_id): (_, Option<String>)| {
                            let series_id = match series_id {
                                Some(id) => id, // This event was already synced before and as such already has an event series ID
                                None => crate::meetup_oauth2::new_random_id(16), // This event has never been synced before and we crate a new event series ID
                            };
                            let redis_series_events_key =
                                format!("event_series:{}:meetup_events", &series_id);
                            let host_user_ids: Vec<_> =
                                event.event_hosts.iter().map(|user| user.id).collect();
                            let event_hash = &[
                                ("name", event.name),
                                ("time", event.time.to_rfc3339()),
                                ("link", event.link),
                            ];
                            pipe.sadd(redis_events_key, &event.id)
                                .sadd(redis_series_key, &series_id)
                                .sadd(&redis_event_users_key, rsvp_yes_user_ids.as_slice())
                                .sadd(&redis_event_hosts_key, host_user_ids.as_slice())
                                .set(&redis_event_series_key, &series_id)
                                .sadd(&redis_series_events_key, &event.id)
                                .hset_multiple(&redis_event_key, event_hash)
                                .ignore();
                            pipe.query_async(con)
                        });
                    Box::new(transaction_future) as redis::RedisFuture<_>
                }
            };
            async_redis_transaction::<_, (), _>(
                con,
                &[
                    redis_event_users_key,
                    redis_event_hosts_key,
                    redis_event_series_key,
                    redis_event_key,
                ],
                transaction_fn,
            )
        })
        .map(move |_| {
            println!("Event syncing task: Synced event \"{}\"", event_name);
            ()
        })
        .from_err::<crate::BoxedError>();
    Box::new(fut)
}

// A direct translation of redis::transaction for the async case
// (except for the fact that it doesn't retry)
fn async_redis_transaction<
    K: redis::ToRedisArgs,
    T: redis::FromRedisValue + Send + 'static,
    F: FnMut(
        redis::aio::Connection,
        redis::Pipeline,
    ) -> redis::RedisFuture<(redis::aio::Connection, Option<T>)>,
>(
    con: redis::aio::Connection,
    keys: &[K],
    mut func: F,
) -> impl Future<Item = (redis::aio::Connection, T), Error = crate::BoxedError> {
    redis::cmd("WATCH")
        .arg(keys)
        .query_async(con)
        .from_err::<crate::BoxedError>()
        .and_then(move |(con, _): (_, ())| {
            let mut p = redis::pipe();
            p.atomic();
            func(con, p).from_err::<crate::BoxedError>().and_then(
                |(con, response): (_, Option<T>)| {
                    match response {
                        None => Box::new(future::err(Box::new(SimpleError::new(
                            "Redis transaction failed",
                        ))
                            as crate::BoxedError))
                            as BoxedFuture<_>,
                        Some(response) => {
                            // make sure no watch is left in the connection, even if
                            // someone forgot to use the pipeline.
                            let future = redis::cmd("UNWATCH")
                                .query_async(con)
                                .from_err::<crate::BoxedError>()
                                .map(|(con, _): (_, ())| (con, response));
                            Box::new(future)
                        }
                    }
                },
            )
        })
}
