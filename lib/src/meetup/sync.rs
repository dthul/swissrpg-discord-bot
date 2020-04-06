use futures_util::{lock::Mutex, stream::StreamExt, FutureExt};
use lazy_static::lazy_static;
use redis::{self, AsyncCommands};
use simple_error::SimpleError;
use std::sync::Arc;

pub const NEW_ADVENTURE_PATTERN: &'static str = r"(?i)[\[\(]\s*new\s*adventure\s*[\]\)]";
pub const NEW_CAMPAIGN_PATTERN: &'static str = r"(?i)[\[\(]\s*new\s*campaign\s*[\]\)]";
pub const EVENT_SERIES_PATTERN: &'static str =
    r"(?i)[\[\(]\s*campaign\s*(?P<event_id>[a-zA-Z0-9]+)\s*[\]\)]";
pub const CHANNEL_PATTERN: &'static str = r"(?i)[\[\(]\s*channel\s*(?P<channel_id>[0-9]+)\s*[\]\)]";
pub const SESSION_PATTERN: &'static str = r"(?i)\s*session\s*(?P<number>[0-9]+)";
pub const ONLINE_PATTERN: &'static str = r"(?i)[\[\(]\s*online\s*[\]\)]";

lazy_static! {
    pub static ref NEW_ADVENTURE_REGEX: regex::Regex =
        regex::Regex::new(NEW_ADVENTURE_PATTERN).unwrap();
    pub static ref NEW_CAMPAIGN_REGEX: regex::Regex =
        regex::Regex::new(NEW_CAMPAIGN_PATTERN).unwrap();
    pub static ref EVENT_SERIES_REGEX: regex::Regex =
        regex::Regex::new(EVENT_SERIES_PATTERN).unwrap();
    pub static ref CHANNEL_REGEX: regex::Regex = regex::Regex::new(CHANNEL_PATTERN).unwrap();
    pub static ref SESSION_REGEX: regex::Regex = regex::Regex::new(SESSION_PATTERN).unwrap();
    pub static ref ONLINE_REGEX: regex::Regex = regex::Regex::new(ONLINE_PATTERN).unwrap();
}

// TODO: Introduce a type like "Meetup connection" that contains
// an Arc<Mutex<Option<MeetupClient>>> internally and has the same
// methods as MeetupClient (so we don't need to match on the Option
// every time we want to use the client)
pub async fn sync_task(
    meetup_client: Arc<Mutex<Option<Arc<super::api::AsyncClient>>>>,
    redis_connection: &mut redis::aio::Connection,
) -> Result<(), super::Error> {
    let meetup_client = {
        let guard = meetup_client.lock().await;
        match *guard {
            Some(ref meetup_client) => meetup_client.clone(),
            None => return Err(SimpleError::new("Meetup API unavailable").into()),
        }
        // The Mutex guard will be dropped here
    };
    let upcoming_events = meetup_client.get_upcoming_events_all_groups();
    futures::pin_mut!(upcoming_events);
    // Sync events
    // For loops for streams not supported (yet?)
    while let Some(event) = upcoming_events.next().await {
        match event {
            Err(err) => eprintln!("Couldn't query upcoming event: {}", err),
            Ok(event) => match sync_event(event, redis_connection).await {
                Err(err) => eprintln!("Event sync failed: {}", err),
                _ => (),
            },
        }
    }
    // Sync event series
    let event_series: Vec<String> = redis_connection.smembers("event_series").await?;
    for series_id in event_series {
        match sync_event_series(series_id, meetup_client.as_ref(), redis_connection).await {
            Err(err) => eprintln!("Series sync failed: {}", err),
            _ => (),
        };
        // Add a 250ms delay between each item as a naive rate limit for the Meetup API
        tokio::time::delay_for(std::time::Duration::from_millis(250)).await;
    }
    Ok(())
}

// This function is supposed to be idempotent, so calling it with the same
// event is fine.
pub async fn sync_event(
    event: super::api::Event,
    redis_connection: &mut redis::aio::Connection,
) -> Result<(), super::Error> {
    let is_new_adventure = NEW_ADVENTURE_REGEX.is_match(&event.description);
    let is_new_campaign = NEW_CAMPAIGN_REGEX.is_match(&event.description);
    let is_online = ONLINE_REGEX.is_match(&event.description);
    let event_series_captures = EVENT_SERIES_REGEX.captures(&event.description);
    let channel_captures = CHANNEL_REGEX.captures(&event.description);
    let indicated_channel_id = match channel_captures {
        Some(captures) => match captures.name("channel_id") {
            Some(id) => match id.as_str().parse::<u64>() {
                Ok(id) => Some(id),
                _ => return Err(SimpleError::new("Invalid channel id").into()),
            },
            _ => return Err(SimpleError::new("Internal error parsing channel id").into()),
        },
        _ => None,
    };
    if indicated_channel_id.is_some() && !(is_new_adventure || is_new_campaign) {
        return Err(SimpleError::new(format!(
            "Skipping event \"{}\" since it indicates a channel to be connected with but is not \
             the start of a new series",
            event.name
        ))
        .into());
    }
    // Either: new adventure, new campaign, or continuation (event series)
    if !(is_new_adventure || is_new_campaign || event_series_captures.is_some()) {
        println!("Syncing task: Ignoring event \"{}\"", event.name);
        return Ok(());
    } else {
        println!("Syncing task: found event \"{}\"", event.name);
    }
    if event_series_captures.is_some()
        && (is_new_adventure || is_new_campaign || indicated_channel_id.is_some())
    {
        eprintln!(
            "Syncing task: Event \"{}\" specifies a series as well as a new adventure/campaign \
             tag, ignoring",
            event.name
        );
        return Ok(());
    }
    // If this is part of an event series, figure out which
    let indicated_event_series_id = if let Some(event_series_captures) = event_series_captures {
        // This is the event ID of an event that belongs to this series
        let series_event_id = match event_series_captures.name("event_id") {
            Some(id) => id.as_str(),
            None => {
                eprintln!("Syncing task: error capturing event_id");
                return Ok(());
            }
        };
        // Query Redis for that event's series ID
        let redis_series_event_series_key =
            format!("meetup_event:{}:event_series", series_event_id);
        let event_series_id: redis::RedisResult<Option<String>> =
            redis_connection.get(&redis_series_event_series_key).await;
        let event_series_id = match event_series_id {
            Ok(id) => match id {
                Some(id) => Some(id),
                None => {
                    eprintln!(
                        "Event \"{}\" indicates that it wants to be in the same series as event \
                         {} but the latter does not belong to an event series yet",
                        event.name, series_event_id
                    );
                    return Ok(());
                }
            },
            Err(err) => {
                eprintln!(
                    "Syncing task: error querying Redis for event series: {}",
                    err
                );
                return Ok(());
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
    let transaction_fn = {
        // There are no non-move async closures yet, so if we want to capture
        // by reference, we have to do it manually
        let event = &event;
        let indicated_event_series_id = &indicated_event_series_id;
        let redis_event_hosts_key = &redis_event_hosts_key;
        let redis_event_series_key = &redis_event_series_key;
        let redis_event_key = &redis_event_key;
        let redis_channel_series_key = &redis_channel_series_key;
        crate::redis::closure_type_helper(move |con, mut pipe: redis::Pipeline| {
            let event = event.clone();
            let indicated_event_series_id = indicated_event_series_id.clone();
            let redis_event_hosts_key = redis_event_hosts_key.clone();
            let redis_event_series_key = redis_event_series_key.clone();
            let redis_event_key = redis_event_key.clone();
            let redis_channel_series_key = redis_channel_series_key.clone();
            let fut = async move {
                let mut query = redis::pipe();
                query
                    .get(&redis_event_series_key)
                    .get(&redis_channel_series_key);
                let (existing_series_id, indicated_channel_series): (
                    Option<String>,
                    Option<String>,
                ) = query.query_async(con).await?;
                if existing_series_id.is_none() {
                    // If this event has no series ID yet but also
                    // doesn't indicate that it is the start of a
                    // new series or belongs to an existing series, do nothing
                    if !(is_new_adventure || is_new_campaign || indicated_event_series_id.is_some())
                    {
                        println!("Syncing task: Ignoring event \"{}\"", event.name);
                        return pipe.query_async(con).await;
                    }
                    // If this event has no series ID yet, but the channel
                    // it wants to be associated with does, then something is fishy
                    if indicated_channel_series.is_some() {
                        println!(
                            "Event \"{}\" wants to be associated with a certain channel but that \
                             channel already belongs to an event series",
                            event.name
                        );
                        return pipe.query_async(con).await;
                    }
                }
                // Use the existing series ID or create a new one
                let series_id = match existing_series_id {
                    Some(existing_series_id) => {
                        // This event was already synced before and as such already has an event series ID.

                        // If this event's series ID does not match the channel's series ID, something is fishy
                        if let Some(channel_series) = indicated_channel_series {
                            if &channel_series != &existing_series_id {
                                eprintln!(
                                    "Event \"{}\" wants to be associated with a certain channel \
                                     but that channel already belongs to a different event series",
                                    event.name
                                );
                                return pipe.query_async(con).await;
                            }
                        }
                        // If this event's series ID does not match the indicated event series ID, issue a warning
                        if let Some(indicated_event_series_id) = indicated_event_series_id {
                            if &existing_series_id != &indicated_event_series_id {
                                eprintln!(
                                    "Warning: Event \"{}\" indicates event series {} but is \
                                     already associated with event series {}.",
                                    event.name, indicated_event_series_id, existing_series_id
                                );
                                return pipe.query_async(con).await;
                            }
                        }
                        existing_series_id
                    }
                    None => {
                        // This event has not been synced before and we either create a new event series ID
                        // for new campaigns/adventures or we connect it to an existing event series
                        if (is_new_adventure || is_new_campaign)
                            && indicated_event_series_id.is_none()
                        {
                            let new_series_id = crate::new_random_id(16);
                            if let Some(channel_id) = indicated_channel_id {
                                // If this event wants to be associated with a channel but that channel already
                                // has an event series ID, something is fishy
                                if indicated_channel_series.is_some() {
                                    eprintln!(
                                        "Event \"{}\" wants to be associated with a certain \
                                         channel but that channel already belongs to a different \
                                         event series",
                                        event.name
                                    );
                                    return pipe.query_async(con).await;
                                } else {
                                    // The event wants to be associated with a channel and that channel is not
                                    // associated to anything else yet, looking good!
                                    println!(
                                        "Associating event \"{}\" with Discord channel {}",
                                        event.name, channel_id
                                    );
                                    let redis_series_channel_key =
                                        format!("event_series:{}:discord_channel", &new_series_id);
                                    pipe.sadd("discord_channels", channel_id);
                                    pipe.set(&redis_channel_series_key, &new_series_id);
                                    pipe.set(&redis_series_channel_key, channel_id);
                                }
                            }
                            new_series_id
                        } else if let Some(indicated_event_series_id) = indicated_event_series_id {
                            indicated_event_series_id.clone()
                        } else {
                            // Something went wrong
                            eprintln!(
                                "Syncing task: internal error (event has no series id yet, but is \
                                 neither a new adventure/campaign nor does it belong to a session"
                            );
                            return pipe.query_async(con).await;
                        }
                    }
                };
                // If the [online] shortcode has been set (even if this is not
                // the first event in the series), mark the series as online
                if is_online {
                    let redis_series_online_key = format!("event_series:{}:is_online", &series_id);
                    pipe.set(&redis_series_online_key, "true");
                }
                let redis_series_events_key = format!("event_series:{}:meetup_events", &series_id);
                let host_user_ids: Vec<_> = event.event_hosts.iter().map(|user| user.id).collect();
                let event_time = event.time.to_rfc3339();
                let event_hash = &[
                    ("name", &event.name),
                    ("time", &event_time),
                    ("link", &event.link),
                    ("urlname", &event.group.urlname),
                ];
                if is_new_adventure || is_new_campaign {
                    let redis_series_type_key = format!("event_series:{}:type", &series_id);
                    let series_type = if is_new_campaign {
                        "campaign"
                    } else {
                        "adventure"
                    };
                    pipe.set(&redis_series_type_key, series_type);
                }
                pipe.sadd(redis_events_key, &event.id)
                    .sadd(redis_series_key, &series_id)
                    .sadd(&redis_event_hosts_key, host_user_ids)
                    .set(&redis_event_series_key, &series_id)
                    .sadd(&redis_series_events_key, &event.id);
                for &(field, value) in event_hash {
                    // Do not use hset_multiple, it deletes existing fields!
                    pipe.hset(&redis_event_key, field, value);
                }
                pipe.query_async(con).await
            };
            fut.boxed()
        })
    };
    let transaction_keys = &[
        &redis_event_series_key,
        &redis_event_key,
        &redis_channel_series_key,
    ];
    crate::redis::async_redis_transaction::<_, (), _>(
        redis_connection,
        transaction_keys,
        transaction_fn,
    )
    .await?;
    println!("Event syncing task: Synced event \"{}\"", event_name);
    Ok(())
}

async fn sync_event_series(
    series_id: String,
    meetup_client: &super::api::AsyncClient,
    redis_connection: &mut redis::aio::Connection,
) -> Result<(), super::Error> {
    // Get all events belonging to this event series
    let events = super::util::get_events_for_series(redis_connection, &series_id).await?;
    // Filter past events
    let now = chrono::Utc::now();
    let mut upcoming: Vec<_> = events
        .into_iter()
        .filter(|event| event.time > now)
        .collect();
    // Sort by date
    upcoming.sort_unstable_by_key(|event| event.time);
    // We loop since the next event might have been deleted on Meetup.
    // So we just continue until we find one that has not been deleted or the list is exhausted.
    for next_event in upcoming {
        // The first element in this vector will be the next upcoming event
        let next_event_id = next_event.id.clone();
        let next_event_name = &next_event.name;
        let group_urlname = &next_event.urlname;
        println!(
            "Syncing task: Querying RSVPs for event \"{}\"",
            next_event_name
        );
        // Query the RSVPs for that event
        let rsvps = match meetup_client.get_rsvps(group_urlname, &next_event_id).await {
            Err(super::api::Error::ResourceNotFound) => {
                // Remove this event from Redis
                eprintln!(
                    "Event {} was deleted from Meetup, removing from database...",
                    &next_event.id
                );
                crate::redis::delete_event(redis_connection, &next_event.id).await?;
                eprintln!("Removed event {} from database", &next_event.id);
                continue;
            }
            Err(err) => return Err(err.into()),
            Ok(rsvps) => rsvps,
        };
        // Sync the RSVPs
        println!("Syncing task: Found {} RSVPs", rsvps.len());
        return sync_rsvps(&next_event_id, rsvps, redis_connection).await;
    }
    Ok(())
}

pub async fn sync_rsvps(
    event_id: &str,
    rsvps: Vec<super::api::RSVP>,
    redis_connection: &mut redis::aio::Connection,
) -> Result<(), super::Error> {
    let rsvp_yes_user_ids: Vec<_> = rsvps
        .iter()
        .filter_map(|rsvp| {
            if rsvp.response == super::api::RSVPResponse::Yes {
                Some(rsvp.member.id)
            } else {
                None
            }
        })
        .collect();
    let rsvp_no_user_ids: Vec<_> = rsvps
        .iter()
        .filter_map(|rsvp| match rsvp.response {
            super::api::RSVPResponse::No | super::api::RSVPResponse::Waitlist => {
                Some(rsvp.member.id)
            }
            _ => None,
        })
        .collect();
    let redis_event_users_key = format!("meetup_event:{}:meetup_users", event_id);
    let mut pipe = redis::pipe();
    if rsvp_yes_user_ids.len() > 0 {
        pipe.sadd(&redis_event_users_key, rsvp_yes_user_ids);
    }
    if rsvp_no_user_ids.len() > 0 {
        pipe.srem(&redis_event_users_key, rsvp_no_user_ids);
    }
    let _: () = pipe.query_async(redis_connection).await?;
    Ok(())
}
