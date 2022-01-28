use redis::AsyncCommands;
use std::{
    future::Future,
    io::{self, Write},
};
use unicode_segmentation::UnicodeSegmentation;

async fn try_with_token_refresh<
    T,
    Ret: Future<Output = Result<T, super::newapi::Error>>,
    F: Fn(super::newapi::AsyncClient) -> Ret,
>(
    f: F,
    user_id: u64,
    redis_connection: &mut redis::aio::Connection,
    oauth2_consumer: &super::oauth2::OAuth2Consumer,
) -> Result<T, super::Error> {
    // Look up the Meetup access token for this user
    println!("Looking up the oauth access token");
    io::stdout().flush().unwrap();
    let redis_meetup_user_oauth_tokens_key = format!("meetup_user:{}:oauth2_tokens", user_id);
    let access_token: Option<String> = redis_connection
        .hget(&redis_meetup_user_oauth_tokens_key, "access_token")
        .await?;
    let access_token = match access_token {
        Some(access_token) => access_token,
        None => {
            // There is no access token: try to obtain a new one
            println!("No access token, calling oauth2_consumer.refresh_oauth_tokens");
            io::stdout().flush().unwrap();
            let access_token = oauth2_consumer
                .refresh_oauth_tokens(super::oauth2::TokenType::User(user_id), redis_connection)
                .await?;
            println!("Got an access token!");
            io::stdout().flush().unwrap();
            access_token.secret().clone()
        }
    };
    let mut meetup_api_user_client = super::newapi::AsyncClient::new(&access_token);
    // Check if the authentication works
    if let Err(err) = meetup_api_user_client.get_self().await {
        // Authentication failed
        println!("Authentication failed:\n{:#?}", err);
        // The request seems to have failed due to invalid credentials.
        // Try to obtain a new access token and re-run the provided function.
        println!("Calling oauth2_consumer.refresh_oauth_tokens");
        io::stdout().flush().unwrap();
        let access_token = oauth2_consumer
            .refresh_oauth_tokens(super::oauth2::TokenType::User(user_id), redis_connection)
            .await?;
        println!("Got an access token!");
        io::stdout().flush().unwrap();
        meetup_api_user_client = super::newapi::AsyncClient::new(access_token.secret());
    }
    // Run the provided function
    println!("Running the provided function");
    io::stdout().flush().unwrap();
    match f(meetup_api_user_client).await {
        Err(err) => {
            println!("Got an error");
            io::stdout().flush().unwrap();
            eprintln!("Meetup API error: {:#?}", err);
            io::stdout().flush().unwrap();
            return Err(err.into());
        }
        Ok(t) => {
            println!("Everything is fine!");
            io::stdout().flush().unwrap();
            Ok(t)
        }
    }
}

// pub async fn rsvp_user_to_event(
//     user_id: u64,
//     urlname: &str,
//     event_id: &str,
//     redis_connection: &mut redis::aio::Connection,
//     oauth2_consumer: &super::oauth2::OAuth2Consumer,
// ) -> Result<super::api::RSVP, super::Error> {
//     let rsvp_fun = |async_meetup_user_client: super::api::AsyncClient| async move {
//         async_meetup_user_client.rsvp(urlname, event_id, true).await
//     };
//     try_with_token_refresh(rsvp_fun, user_id, redis_connection, oauth2_consumer).await
// }

pub async fn clone_event<'a>(
    urlname: &'a str,
    event_id: &'a str,
    meetup_client: &'a super::newapi::AsyncClient,
    hook: Option<
        Box<
            dyn FnOnce(super::newapi::NewEvent) -> Result<super::newapi::NewEvent, super::Error>
                + Send
                + 'a,
        >,
    >,
) -> Result<super::newapi::NewEventResponse, super::Error> {
    let event = meetup_client.get_event(event_id.into()).await?;
    let new_event = super::newapi::NewEvent {
        groupUrlname: urlname.into(),
        title: event.title.unwrap_or_else(|| "No title".into()),
        description: event
            .description
            .unwrap_or_else(|| "Missing description".into()),
        startDateTime: event.date_time.into(),
        duration: None,
        rsvpSettings: Some(super::newapi::NewEventRsvpSettings {
            rsvpLimit: Some(event.max_tickets),
            guestLimit: Some(event.number_of_allowed_guests),
            rsvpOpenTime: None,
            rsvpCloseTime: None,
            rsvpOpenDuration: None,
            rsvpCloseDuration: None,
        }),
        eventHosts: Some(
            event
                .hosts
                .unwrap_or(vec![])
                .iter()
                .map(|host| host.id.0 as i64)
                .collect(),
        ),
        venueId: if event.is_online {
            Some("online".into())
        } else {
            event.venue.map(|venue| venue.id.0)
        },
        selfRsvp: Some(false),
        howToFindUs: if event.is_online {
            None
        } else {
            event.how_to_find_us
        },
        question: None,
        featuredPhotoId: Some(event.image.id.0 as i64),
        publishStatus: Some(super::newapi::NewEventPublishStatus::DRAFT),
    };
    // If there is a hook specified, let it modify the new event before
    // publishing it to Meetup
    let new_event = match hook {
        Some(hook) => hook(new_event)?,
        None => new_event,
    };
    // Post the event on Meetup
    let new_event = meetup_client.create_event(new_event).await?;
    return Ok(new_event);
}

// #[derive(Debug)]
// pub struct CloneRSVPResult {
//     pub cloned_rsvps: Vec<super::api::RSVP>,
//     pub num_success: u16,
//     pub num_failure: u16,
//     pub latest_error: Option<super::Error>,
// }

// pub async fn clone_rsvps(
//     urlname: &str,
//     src_event_id: &str,
//     dst_event_id: &str,
//     redis_connection: &mut redis::aio::Connection,
//     meetup_client: &super::api::AsyncClient,
//     oauth2_consumer: &super::oauth2::OAuth2Consumer,
// ) -> Result<CloneRSVPResult, super::Error> {
//     // First, query the source event's RSVPs and filter them by "yes" responses
//     let rsvps: Vec<_> = meetup_client
//         .get_rsvps(urlname, src_event_id)
//         .await?
//         .into_iter()
//         .filter(|rsvp| rsvp.response == super::api::RSVPResponse::Yes)
//         .collect();
//     // Now, try to RSVP each user to the destination event
//     let mut result = CloneRSVPResult {
//         cloned_rsvps: Vec::with_capacity(rsvps.len()),
//         num_success: 0,
//         num_failure: 0,
//         latest_error: None,
//     };
//     for rsvp in &rsvps {
//         match rsvp_user_to_event(
//             rsvp.member.id,
//             urlname,
//             dst_event_id,
//             redis_connection,
//             oauth2_consumer,
//         )
//         .await
//         {
//             Ok(rsvp) => {
//                 result.cloned_rsvps.push(rsvp);
//                 result.num_success += 1;
//             }
//             Err(err) => {
//                 eprintln!(
//                     "Could not RSVP user {} to event {}:\n{:#?}",
//                     rsvp.member.id, dst_event_id, err
//                 );
//                 result.latest_error = Some(err);
//                 result.num_failure += 1;
//             }
//         }
//     }
//     Ok(result)
// }

// TODO: move to redis module?
pub struct Event {
    pub id: String,
    pub name: String,
    pub time: chrono::DateTime<chrono::Utc>,
    pub link: String,
    pub urlname: String,
    pub is_online: bool,
}

// Queries all events belonging to the specified series from Redis,
// ignoring errors that might arise querying specific events
// (and not returning them instead)
pub async fn get_events_for_series(
    redis_connection: &mut redis::aio::Connection,
    series_id: &str,
) -> Result<Vec<Event>, super::Error> {
    let redis_series_events_key = format!("event_series:{}:meetup_events", &series_id);
    // Get all events belonging to this event series
    let event_ids: Vec<String> = redis_connection.smembers(&redis_series_events_key).await?;
    let mut events = Vec::with_capacity(event_ids.len());
    for event_id in event_ids {
        let redis_event_key = format!("meetup_event:{}", event_id);
        let res: redis::RedisResult<(String, String, String, String, Option<String>)> =
            redis_connection
                .hget(
                    &redis_event_key,
                    &["time", "name", "link", "urlname", "is_online"],
                )
                .await;
        if let Ok((time, name, link, urlname, is_online)) = res {
            if let Ok(time) = chrono::DateTime::parse_from_rfc3339(&time) {
                events.push(Event {
                    id: event_id,
                    name,
                    time: time.with_timezone(&chrono::Utc),
                    link,
                    urlname,
                    is_online: is_online.map(|v| v == "true").unwrap_or(false),
                });
            }
        }
    }
    Ok(events)
}

pub async fn get_group_memberships(
    meetup_api: super::newapi::AsyncClient,
) -> Result<Vec<super::newapi::GroupMembership>, super::newapi::Error> {
    let mut memberships = Vec::with_capacity(super::newapi::URLNAMES.len());
    for urlname in super::newapi::URLNAMES {
        let membership = meetup_api.get_group_membership(urlname.to_string()).await?;
        memberships.push(membership);
    }
    Ok(memberships)
}

// Truncates the string to the given maximum length.
// The "length" of a Unicode string is no well-defined concept. Meetup (at least
// the form on the website) seems to use the number of UTF-16 code units as the
// length of the string, so this is what this method uses (see
// https://hsivonen.fi/string-length/ for some details on Unicode string lengths).
pub fn truncate_str(mut string: String, max_len: usize) -> String {
    // Count the number of characters that are allowed
    let mut utf16_length = 0;
    let mut utf8_length = 0;
    for grapheme in UnicodeSegmentation::graphemes(string.as_str(), /*extended*/ true) {
        // Compute the length of this grapheme in UTF-16
        let grapheme_utf16_length = grapheme.encode_utf16().count();
        if utf16_length + grapheme_utf16_length <= max_len {
            utf16_length += grapheme_utf16_length;
            utf8_length += grapheme.len();
        } else {
            // We have reached the maximum length
            break;
        }
    }
    string.truncate(utf8_length);
    string
}
