use crate::meetup_api;
use futures::future;
use futures::{Future, Stream};
use lazy_static::lazy_static;
use redis;
use redis::Commands;
use redis::PipelineCommands;
use serenity::prelude::RwLock;
use simple_error::SimpleError;
use std::sync::Arc;
use tokio;
use tokio::prelude::*;

const NEW_ADVENTURE_PATTERN: &'static str = r"(?i)[\[\(]\s*new\s*adventure\s*[\]\)]";
const NEW_CAMPAIGN_PATTERN: &'static str = r"(?i)[\[\(]\s*new\s*campaign\s*[\]\)]";
const EVENT_SERIES_PATTERN: &'static str =
    r"(?i)[\[\(]\s*campaign\s*(?P<event_id>[a-zA-Z0-9]+)\s*[\]\)]";
const CHANNEL_PATTERN: &'static str = r"(?i)[\[\(]\s*channel\s*(?P<channel_id>[0-9]+)\s*[\]\)]";

lazy_static! {
    static ref NEW_ADVENTURE_REGEX: regex::Regex =
        regex::Regex::new(NEW_ADVENTURE_PATTERN).unwrap();
    static ref NEW_CAMPAIGN_REGEX: regex::Regex = regex::Regex::new(NEW_CAMPAIGN_PATTERN).unwrap();
    static ref EVENT_SERIES_REGEX: regex::Regex = regex::Regex::new(EVENT_SERIES_PATTERN).unwrap();
    static ref CHANNEL_REGEX: regex::Regex = regex::Regex::new(CHANNEL_PATTERN).unwrap();
}

pub type BoxedFuture<T, E = crate::BoxedError> = Box<dyn Future<Item = T, Error = E> + Send>;

// TODO: Introduce a type like "Meetup connection" that contains
// an Arc<RwLock<Option<MeetupClient>>> internally and has the same
// methods as MeetupClient (so we don't need to match on the Option
// every time we want to use the client)
pub fn sync_task(
    meetup_client: Arc<RwLock<Option<meetup_api::AsyncClient>>>,
    mut redis_client: redis::Client,
) -> impl Future<Item = (), Error = crate::BoxedError> + Send + 'static {
    let upcoming_events = match *meetup_client.read() {
        Some(ref meetup_client) => meetup_client
            .get_upcoming_events_all_groups()
            .from_err::<crate::BoxedError>(),
        None => {
            return Box::new(
                future::err(SimpleError::new("Meetup API unavailable"))
                    .from_err::<crate::BoxedError>(),
            ) as BoxedFuture<_>
        }
    };
    let event_sync_future = {
        let redis_client = redis_client.clone();
        upcoming_events.for_each(move |event| {
            sync_event(event, redis_client.clone()).then(|res| {
                // "Catch" any errors and don't abort the stream
                if let Err(err) = res {
                    eprintln!("Event sync failed: {}", err);
                }
                future::ok(())
            })
        })
    };
    let series_sync_future_fun = move || {
        // This code is in a closure such that the Redis query in the next line
        // is only run once the events have been synced
        let event_series_result = redis_client.smembers("event_series");
        future::result(event_series_result)
            .from_err::<crate::BoxedError>()
            .and_then(move |event_series: Vec<String>| {
                stream::iter_ok(event_series).for_each(move |series_id| {
                    let redis_client = redis_client.clone();
                    let meetup_client = meetup_client.clone();
                    sync_event_series(series_id, meetup_client, redis_client).and_then(|_| {
                        // Add a 1s delay between each item as a naive rate limit for the Meetup API
                        tokio::timer::Delay::new(
                            std::time::Instant::now() + std::time::Duration::from_secs(1),
                        )
                        .from_err::<crate::BoxedError>()
                    })
                })
            })
    };
    return Box::new(event_sync_future.and_then(|_| series_sync_future_fun()));
}

// This function is supposed to be idempotent, so calling it with the same
// event is fine.
fn sync_event(
    event: meetup_api::Event,
    mut redis_client: redis::Client,
) -> impl Future<Item = (), Error = crate::BoxedError> {
    let is_new_adventure = NEW_ADVENTURE_REGEX.is_match(&event.description);
    let is_new_campaign = NEW_CAMPAIGN_REGEX.is_match(&event.description);
    let event_series_captures = EVENT_SERIES_REGEX.captures(&event.description);
    let channel_captures = CHANNEL_REGEX.captures(&event.description);
    let indicated_channel_id = match channel_captures {
        Some(captures) => match captures.name("channel_id") {
            Some(id) => match id.as_str().parse::<u64>() {
                Ok(id) => Some(id),
                _ => {
                    return Box::new(future::err(SimpleError::new("Invalid channel id").into()))
                        as BoxedFuture<_>
                }
            },
            _ => {
                return Box::new(future::err(
                    SimpleError::new("Internal error parsing channel id").into(),
                )) as BoxedFuture<_>
            }
        },
        _ => None,
    };
    if indicated_channel_id.is_some() && !(is_new_adventure || is_new_campaign) {
        return Box::new(future::err(SimpleError::new(format!(
            "Skipping event \"{}\" since it indicates a channel to be connected with but is not the start of a new series",
            event.name
        )).into())) as BoxedFuture<_>;
    }
    // Either: new adventure or campaign, Or: event series
    if !(is_new_adventure || is_new_campaign || event_series_captures.is_some()) {
        // TODO: implement event series
        println!("Syncing task: Ignoring event \"{}\"", event.name);
        return Box::new(future::ok(()));
    } else {
        println!("Syncing task: found event \"{}\"", event.name);
    }
    if event_series_captures.is_some()
        && (is_new_adventure || is_new_campaign || indicated_channel_id.is_some())
    {
        eprintln!(
            "Syncing task: Event \"{}\" specifies a series as well as a new adventure/campaign tag, ignoring",
            event.name
        );
        return Box::new(future::ok(()));
    }
    // If this is part of an event series, figure out which
    let indicated_event_series_id = if let Some(event_series_captures) = event_series_captures {
        // This is the event ID of an event that belongs to this series
        let series_event_id = match event_series_captures.name("event_id") {
            Some(id) => id.as_str(),
            None => {
                eprintln!("Syncing task: error capturing event_id");
                return Box::new(future::ok(()));
            }
        };
        // Query Redis for that event's series ID
        let redis_series_event_series_key =
            format!("meetup_event:{}:event_series", series_event_id);
        let event_series_id: redis::RedisResult<Option<String>> =
            redis_client.get(&redis_series_event_series_key);
        let event_series_id = match event_series_id {
            Ok(id) => match id {
                Some(id) => Some(id),
                None => {
                    eprintln!(
                        "Event \"{}\" indicates that it wants to be in the same series \
                         as event {} but the latter does not belong to an event series yet",
                        event.name, series_event_id
                    );
                    return Box::new(future::ok(()));
                }
            },
            Err(err) => {
                eprintln!(
                    "Syncing task: error querying Redis for event series: {}",
                    err
                );
                return Box::new(future::ok(()));
            }
        };
        event_series_id
    } else {
        None
    };
    // TODO: figure out whether this event belongs to a series
    // For now, we assume that an event that reaches this method does not yet
    // belong to a series and create a new one
    let redis_events_key = "meetup_events";
    let redis_series_key = "event_series";
    let redis_event_hosts_key = format!("meetup_event:{}:meetup_hosts", event.id);
    let redis_event_series_key = format!("meetup_event:{}:event_series", event.id);
    let redis_event_key = format!("meetup_event:{}", event.id);
    let redis_channel_series_key = format!(
        "discord_channel:{}:event_series",
        indicated_channel_id.unwrap_or(0)
    );
    let event_name = event.name.clone();
    // technically: check that series id doesn't exist yet and generate a new one until it does not
    // practically: we will never generate a colliding id
    let fut = redis_client
        .get_async_connection()
        .from_err::<crate::BoxedError>()
        .and_then(move |con| {
            let transaction_fn = {
                let redis_event_series_key = redis_event_series_key.clone();
                let redis_event_key = redis_event_key.clone();
                let redis_channel_series_key = redis_channel_series_key.clone();
                move |con, mut pipe: redis::Pipeline| {
                    let event = event.clone();
                    let redis_events_key = redis_events_key.clone();
                    let redis_series_key = redis_series_key.clone();
                    let redis_event_hosts_key = redis_event_hosts_key.clone();
                    let redis_event_series_key = redis_event_series_key.clone();
                    let redis_event_key = redis_event_key.clone();
                    let redis_channel_series_key = redis_channel_series_key.clone();
                    let indicated_event_series_id = indicated_event_series_id.clone();
                    let mut query = redis::pipe();
                    query.get(&redis_event_series_key).get(&redis_channel_series_key);
                    let transaction_future = query.query_async(con).and_then(
                        move |(con, (existing_series_id, indicated_channel_series)): (_, (Option<String>, Option<String>))| {
                            if existing_series_id.is_none() {
                                // If this event has no series ID yet but also
                                // doesn't indicate that it is the start of a
                                // new series or belongs to an existing series, do nothing
                                if !(is_new_adventure || is_new_campaign || indicated_event_series_id.is_some()) {
                                    println!("Syncing task: Ignoring event \"{}\"", event.name);
                                    return pipe.query_async(con);
                                }
                                // If this event has no series ID yet, but the channel
                                // it wants to be associated with does, then something is fishy
                                if indicated_channel_series.is_some() {
                                    println!(
                                        "Event \"{}\" wants to be associated with a certain channel \
                                         but that channel already belongs to an event series",
                                        event.name
                                    );
                                    return pipe.query_async(con);
                                }
                            }
                            // Use the existing series ID or create a new one
                            let series_id = match existing_series_id {
                                Some(existing_series_id) => {
                                    // This event was already synced before and as such already has an event series ID.

                                    // If this event's series ID does not match the channel's series ID, something is fishy
                                    if let Some(channel_series) = indicated_channel_series {
                                        if channel_series != existing_series_id {
                                            eprintln!(
                                                "Event \"{}\" wants to be associated with a certain channel but \
                                                 that channel already belongs to a different event series",
                                                event.name
                                            );
                                            return pipe.query_async(con);
                                        }
                                    }
                                    // If this event's series ID does not match the indicated event series ID, issue a warning
                                    if let Some(indicated_event_series_id) = indicated_event_series_id {
                                        if existing_series_id != indicated_event_series_id {
                                            eprintln!(
                                                "Warning: Event \"{}\" indicates event series {} \
                                                 but is already associated with event series {}.",
                                                event.name, indicated_event_series_id, existing_series_id
                                            );
                                            return pipe.query_async(con);
                                        }
                                    }
                                    existing_series_id
                                }
                                None => {
                                    // This event has not been synced before and we either create a new event series ID
                                    // for new campaigns/adventures or we connect it to an existing event series
                                    if (is_new_adventure || is_new_campaign) && indicated_event_series_id.is_none() {
                                        let new_series_id = crate::meetup_oauth2::new_random_id(16);
                                        if let Some(channel_id) = indicated_channel_id {
                                            // If this event wants to be associated with a channel but that channel already
                                            // has an event series ID, something is fishy
                                            if indicated_channel_series.is_some() {
                                                eprintln!(
                                                    "Event \"{}\" wants to be associated with a certain channel \
                                                     but that channel already belongs to a different event series",
                                                    event.name
                                                );
                                                return pipe.query_async(con);
                                            } else {
                                                // The event wants to be associated with a channel and that channel is not
                                                // associated to anything else yet, looking good!
                                                println!("Associating event \"{}\" with Discord channel {}", event.name, channel_id);
                                                let redis_series_channel_key = format!("event_series:{}:discord_channel", &new_series_id);
                                                pipe.sadd("discord_channels", channel_id);
                                                pipe.set(&redis_channel_series_key, &new_series_id);
                                                pipe.set(&redis_series_channel_key, channel_id);
                                            }
                                        }
                                        new_series_id
                                    } else if let Some(indicated_event_series_id) = indicated_event_series_id {
                                        indicated_event_series_id
                                    } else {
                                        // Something went wrong
                                        eprintln!(
                                            "Syncing task: internal error (event has no series id yet, \
                                             but is neither a new adventure/campaign nor does it belong to a session"
                                        );
                                        return pipe.query_async(con);
                                    }
                                }
                            };
                            let redis_series_events_key = format!("event_series:{}:meetup_events", &series_id);
                            let host_user_ids: Vec<_> = event.event_hosts.iter().map(|user| user.id).collect();
                            let event_hash = &[
                                ("name", event.name),
                                ("time", event.time.to_rfc3339()),
                                ("link", event.link),
                                ("urlname", event.group.urlname),
                            ];
                            if is_new_adventure || is_new_campaign {
                                let redis_series_type_key = format!("event_series:{}:type", &series_id);
                                let series_type = 
                                    if is_new_campaign { "campaign" } else { "adventure" };
                                pipe.set(&redis_series_type_key, series_type);
                            }
                            pipe.sadd(redis_events_key, &event.id)
                                .sadd(redis_series_key, &series_id)
                                .sadd(&redis_event_hosts_key, host_user_ids)
                                .set(&redis_event_series_key, &series_id)
                                .sadd(&redis_series_events_key, &event.id)
                                .hset_multiple(&redis_event_key, event_hash);
                            pipe.query_async(con)
                        },
                    );
                    Box::new(transaction_future) as redis::RedisFuture<_>
                }
            };
            async_redis_transaction::<_, (), _>(con, &[redis_event_series_key, redis_event_key, redis_channel_series_key], transaction_fn)
        })
        .map(move |_| {
            println!("Event syncing task: Synced event \"{}\"", event_name);
            ()
        })
        .from_err::<crate::BoxedError>();
    Box::new(fut)
}

fn sync_event_series(
    series_id: String,
    meetup_client: Arc<RwLock<Option<meetup_api::AsyncClient>>>,
    mut redis_client: redis::Client,
) -> impl Future<Item = (), Error = crate::BoxedError> {
    let redis_series_events_key = format!("event_series:{}:meetup_events", &series_id);
    // Get all events belonging to this event series
    let event_ids: Vec<String> = match redis_client.smembers(&redis_series_events_key) {
        Ok(ids) => ids,
        Err(err) => return Box::new(future::err(err.into())) as BoxedFuture<_>,
    };
    let events: Vec<_> = event_ids
        .into_iter()
        .filter_map(|event_id| {
            let redis_event_key = format!("meetup_event:{}", event_id);
            let tuple: redis::RedisResult<(String, String, String)> =
                redis_client.hget(&redis_event_key, &["time", "name", "urlname"]);
            match tuple {
                Ok((time, name, urlname)) => Some((event_id, time, name, urlname)),
                Err(err) => {
                    eprintln!("Redis error when querying event time: {}", err);
                    None
                }
            }
        })
        .collect();
    // Filter past events
    let now = chrono::Utc::now();
    let mut upcoming: Vec<_> = events
        .into_iter()
        .filter_map(|(id, time, name, urlname)| {
            if let Ok(time) = chrono::DateTime::parse_from_rfc3339(&time) {
                let time = time.with_timezone(&chrono::Utc);
                if time > now {
                    return Some((id, time, name, urlname));
                }
            }
            None
        })
        .collect();
    // Sort by date
    upcoming.sort_unstable_by_key(|pair| pair.1);
    // The first element in this vector will be the next upcoming event
    if let Some(next_event) = upcoming.first() {
        let next_event_id = next_event.0.clone();
        let next_event_name = &next_event.2;
        let group_urlname = &next_event.3;
        println!(
            "Syncing task: Querying RSVPs for event \"{}\"",
            next_event_name
        );
        let rsvp_future = {
            // Query the RSVPs for that event
            let rsvps = match *meetup_client.read() {
                Some(ref meetup_client) => meetup_client
                    .get_rsvps(group_urlname, &next_event_id)
                    .from_err::<crate::BoxedError>(),
                None => {
                    return Box::new(
                        future::err(SimpleError::new("Meetup API unavailable"))
                            .from_err::<crate::BoxedError>(),
                    ) as BoxedFuture<_>
                }
            };
            // Sync the RSVPs
            rsvps.and_then(move |rsvps| {
                println!("Syncing task: Found {} RSVPs", rsvps.len());
                sync_rsvps(&next_event_id, rsvps, redis_client)
            })
        };
        Box::new(rsvp_future)
    } else {
        Box::new(future::ok(()))
    }
}

fn sync_rsvps(
    event_id: &str,
    rsvps: Vec<meetup_api::RSVP>,
    redis_client: redis::Client,
) -> impl Future<Item = (), Error = crate::BoxedError> {
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
    let redis_event_users_key = format!("meetup_event:{}:meetup_users", event_id);
    let fut = redis_client
        .get_async_connection()
        .and_then(move |con| {
            let mut pipe = redis::pipe();
            pipe.sadd(redis_event_users_key, rsvp_yes_user_ids);
            pipe.query_async(con)
        })
        .map(|(_, ())| ())
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
                        None => Box::new(future::err(
                            SimpleError::new("Redis transaction failed").into(),
                        )) as BoxedFuture<_>,
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
