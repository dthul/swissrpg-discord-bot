use std::fmt::Write;

use serenity::model::id::UserId;

use crate::{
    db::{self, EventId, EventSeriesId, MeetupEventId},
    meetup::{newapi::AlphaNumericId, sync::EventSyncResult},
};

pub fn update_event_title(title: &str, with_session_number: bool) -> String {
    // Increase the Session number
    let title_captures = crate::meetup::sync::SESSION_REGEX.captures_iter(title);
    if with_session_number {
        // Match the rightmost occurence of " Session X" in the event name.
        // Returns the event name without the session number (title_only) and
        // the current session number
        let (title_only, session_number) = if let Some(capture) = title_captures.last() {
            // If there is a match, increase the number
            // Extract the current number from the title
            let session_number = capture.name("number").unwrap().as_str();
            // Try to parse the session number
            let session_number = session_number.parse::<i32>().unwrap_or(1);
            // Find the range of the " Session X" match and remove it from the string
            let session_x_match = capture.get(0).unwrap();
            let mut title_only = title.to_string();
            title_only.truncate(session_x_match.start());
            (title_only, session_number)
        } else {
            // If there is no match, return the whole name and Session number 1
            (title.to_string(), 1)
        };
        // Create a new " Session X+1" suffix
        let new_session_suffix = format!(" Session {}", session_number + 1);
        // Check if the concatenation of event title and session suffix is short enough
        let new_event_title = if title_only.encode_utf16().count()
            + new_session_suffix.encode_utf16().count()
            <= crate::meetup::MAX_EVENT_NAME_UTF16_LEN
        {
            title_only + &new_session_suffix
        } else {
            // Event title and session prefix together are too long.
            // Shorten the event title and add an ellipsis.
            let ellipsis = "…";
            let ellipsis_utf16_len = ellipsis.encode_utf16().count();
            let max_title_utf16_len = crate::meetup::MAX_EVENT_NAME_UTF16_LEN
                - new_session_suffix.encode_utf16().count()
                - ellipsis_utf16_len;
            let shortened_title =
                crate::meetup::util::truncate_str(title_only, max_title_utf16_len);
            shortened_title + ellipsis + &new_session_suffix
        };
        new_event_title
    } else {
        // Remove any possibly existing session number
        if let Some(capture) = title_captures.last() {
            let mut title = title.to_string();
            title.replace_range(capture.get(0).unwrap().range(), "");
            title
        } else {
            title.to_string()
        }
    }
}

pub fn update_event_description(
    description: &str,
    some_event_id: EventId,
    is_open_event: bool,
) -> String {
    // Remove unnecessary shortcodes from follow-up sessions
    let description = crate::meetup::sync::NEW_ADVENTURE_REGEX.replace_all(&description, "");
    let description = crate::meetup::sync::NEW_CAMPAIGN_REGEX.replace_all(&description, "");
    let description = crate::meetup::sync::ONLINE_REGEX.replace_all(&description, "");
    let description = crate::meetup::sync::CHANNEL_REGEX.replace_all(&description, "");
    let description = crate::meetup::sync::EVENT_REGEX.replace_all(&description, "");
    // Update event series shortcode
    let mut description = description.into_owned();
    let series_shortcode_ranges = crate::meetup::sync::EVENT_SERIES_REGEX
        .find_iter(&description)
        .map(|m| m.range())
        .collect::<Vec<_>>();
    match series_shortcode_ranges.as_slice() {
        [] => {
            // Insert a new shortcode
            description.push_str(&format!("[campaign {}]", some_event_id.0));
        }
        [range] => {
            // Replace the existing shortcode
            description.replace_range(range.clone(), &format!("[campaign {}]", some_event_id.0));
        }
        _ => {
            // More than one capture, remove all existing ones and add a new one
            let mut updated_description = crate::meetup::sync::EVENT_SERIES_REGEX
                .replace_all(&description, "")
                .into_owned();
            updated_description.push_str(&format!("[campaign {}]", some_event_id.0));
            description = updated_description;
        }
    }
    // If this event is an "open event", make sure that there is no [closed] shortcode.
    // (We don't add it automatically here for closed events though)
    if is_open_event {
        crate::free_spots::CLOSED_REGEX
            .replace_all(&description, "")
            .into_owned()
    } else {
        description
    }
}

pub struct ScheduleSessionResult {
    pub event_id: EventId,
    pub meetup_event: Option<crate::meetup::newapi::create_event_mutation::EventData>,
    pub meetup_event_id: Option<MeetupEventId>,
}

pub async fn schedule_session(
    event_series_id: EventSeriesId,
    participant_limit: u16,
    with_session_number: bool,
    start_time: chrono::DateTime<chrono::Utc>,
    duration: chrono::Duration,
    db_connection: &sqlx::PgPool,
    discord_api: &crate::discord::CacheAndHttp,
    meetup_client: &crate::meetup::newapi::AsyncClient,
    redis_connection: &mut redis::aio::Connection,
    bot_id: UserId,
) -> Result<ScheduleSessionResult, crate::meetup::Error> {
    // We use the latest event as a blueprint for the newly scheduled one
    let last_event = match db::get_last_event_in_series(db_connection, event_series_id).await? {
        Some(event) => event,
        None => {
            return Err(simple_error::SimpleError::new(
                "Can not schedule a follow up session because there is not latest event in this \
                 event series",
            )
            .into())
        }
    };

    let title = update_event_title(&last_event.title, with_session_number);
    let description = update_event_description(
        &last_event.description,
        last_event.id,
        participant_limit > 0,
    );

    // Create a new event in the database
    let new_event_id = sqlx::query!(
        r#"
        INSERT INTO event (event_series_id, start_time, end_time, title, description, is_online, discord_category_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id"#,
        event_series_id.0,
        start_time,
        start_time + duration,
        title,
        description,
        last_event.is_online,
        last_event.discord_category.map(|id| id.0 as i64)
    ).map(|row| EventId(row.id)).fetch_one(db_connection).await?;

    // Below this point we do not want any early returns because we want to
    // return the new_event_id even if something else fails.
    // To reduce the chance of this happening the rest of the code is in another method

    // If there is a positive participant limit we publish the game on Meetup for new signups
    let (meetup_event, meetup_event_db_id) = if participant_limit <= 0 {
        (None, None)
    } else {
        match push_event_to_meetup(
            new_event_id,
            participant_limit,
            db_connection,
            meetup_client,
        )
        .await
        {
            Ok(PushEventResult {
                meetup_event,
                meetup_event_db_id,
            }) => (Some(meetup_event), meetup_event_db_id),
            Err(err) => {
                eprintln!(
                    "Schedule session: could not push event to Meetup:\n{:#?}",
                    err
                );
                (None, None)
            }
        }
    };

    if let Err(err) = schedule_session_postprocess(
        new_event_id,
        event_series_id,
        meetup_event.clone(),
        db_connection,
        discord_api,
        redis_connection,
        bot_id,
    )
    .await
    {
        eprintln!("Schedule session postprocessing failed:\n{:#?}", err);
    }

    Ok(ScheduleSessionResult {
        event_id: new_event_id,
        meetup_event: meetup_event,
        meetup_event_id: meetup_event_db_id,
    })
}

async fn schedule_session_postprocess(
    event_id: EventId,
    event_series_id: EventSeriesId,
    meetup_event: Option<crate::meetup::newapi::create_event_mutation::EventData>,
    db_connection: &sqlx::PgPool,
    discord_api: &crate::discord::CacheAndHttp,
    redis_connection: &mut redis::aio::Connection,
    bot_id: UserId,
) -> Result<(), crate::meetup::Error> {
    // Remove any possibly existing channel snoozes
    if let Err(err) = (|| async {
        let mut tx = db_connection.begin().await?;
        if let Some(channel_id) = crate::get_event_text_channel(event_id, &mut tx).await? {
            sqlx::query!(
                r#"UPDATE event_series_text_channel SET snooze_until = NULL WHERE discord_id = $1"#,
                channel_id.0 as i64
            )
            .execute(&mut tx)
            .await?;
            tx.commit().await?;
        }
        Ok::<(), crate::meetup::Error>(())
    })()
    .await
    {
        eprintln!("Failed to update channel snooze:\n{:#?}", err);
    }

    if let Err(err) = crate::discord::sync::sync_event_series(
        event_series_id,
        redis_connection,
        db_connection,
        discord_api,
        bot_id,
    )
    .await
    {
        eprintln!("Schedule session: sync event series failed:\n{:#?}", err);
    }

    // If a Meetup event was created, announce the new session in the bot alerts channel
    if let Some(meetup_event) = meetup_event {
        let message = format!(
            "<@&{organiser_id}>, a new session has been scheduled:\n{link}.\nPlease announce this \
             session for new players to join. Don't forget to **open RSVPs** when you do that.",
            organiser_id = crate::discord::sync::ids::ORGANISER_ID.0,
            link = &meetup_event.event_url,
        );
        if let Err(err) =
            crate::discord::util::say_in_bot_alerts_channel(&message, discord_api).await
        {
            eprintln!(
                "Encountered an error when trying to announce a new session in the bot alerts \
                 channel:\n{:#?}",
                err
            );
        }
    }
    Ok(())
}

struct PushEventResult {
    meetup_event: crate::meetup::newapi::create_event_mutation::EventData,
    meetup_event_db_id: Option<MeetupEventId>,
}

async fn push_event_to_meetup(
    event_id: EventId,
    participant_limit: u16,
    db_connection: &sqlx::PgPool,
    meetup_client: &crate::meetup::newapi::AsyncClient,
) -> Result<PushEventResult, crate::meetup::Error> {
    let event_data = sqlx::query!(
        r#"SELECT title, description, start_time, end_time FROM event WHERE id = $1"#,
        event_id.0
    )
    .fetch_one(db_connection)
    .await?;

    let title = event_data.title;
    let mut description = event_data.description;
    let start_time = event_data.start_time;
    let end_time = event_data.end_time;
    let duration = if let Some(end_time) = end_time {
        Some(end_time - start_time)
    } else {
        None
    };

    // Add event shortcode to the description (TODO: remove old one if present)
    write!(description, "\n[event {}]", event_id.0).expect("Can write into String");

    if let Some((meetup_event_db_id, meetup_event_id)) = sqlx::query!(
        "SELECT id, meetup_id FROM meetup_event WHERE event_id = $1",
        event_id.0
    )
    .map(|row| (MeetupEventId(row.id), AlphaNumericId(row.meetup_id)))
    .fetch_optional(db_connection)
    .await?
    {
        match meetup_client.get_event(meetup_event_id.clone()).await {
            Ok(meetup_event) => {
                return Ok(PushEventResult {
                    meetup_event: meetup_event.into(),
                    meetup_event_db_id: Some(meetup_event_db_id),
                })
            }
            Err(crate::meetup::newapi::Error::ResourceNotFound) => {
                // It seems like the event was deleted from Meetup, remove it from the database
                sqlx::query!(
                    "DELETE FROM meetup_event WHERE id = $1",
                    meetup_event_db_id.0
                )
                .execute(db_connection)
                .await?;
            }
            Err(err) => return Err(err.into()),
        }
    }

    // We go from the latest event to the oldest event until we find one
    // that has not been deleted to use as a template
    let event_series_id = sqlx::query!(
        "SELECT event_series_id FROM event WHERE id = $1",
        event_id.0
    )
    .map(|row| EventSeriesId(row.event_series_id))
    .fetch_one(db_connection)
    .await?;
    let events = crate::db::get_events_for_series(db_connection, event_series_id).await?;
    let mut new_meetup_event = None;
    for event in events {
        let meetup_event = if let Some(meetup_event) = event.meetup_event {
            meetup_event
        } else {
            continue;
        };
        let title = title.clone();
        let description = description.clone();
        let new_event_hook = Box::new(|mut new_event: crate::meetup::newapi::NewEvent| {
            new_event.title = title;
            new_event.description = description;
            new_event.startDateTime = crate::meetup::newapi::DateTime(start_time);
            new_event.duration = duration.map(Into::into);
            new_event.publishStatus =
                Some(crate::meetup::newapi::create_event_mutation::PublishStatus::PUBLISHED);
            new_event.rsvpSettings = match new_event.rsvpSettings {
                Some(mut rsvp_settings) => {
                    rsvp_settings.rsvpLimit = Some(participant_limit as i64);
                    Some(rsvp_settings)
                }
                None => Some(crate::meetup::newapi::create_event_mutation::RsvpSettings {
                    rsvpOpenTime: None,
                    rsvpCloseTime: None,
                    rsvpOpenDuration: None,
                    rsvpCloseDuration: None,
                    rsvpLimit: Some(participant_limit as i64),
                    guestLimit: Some(1), // TODO: what to set here? Zero?
                }),
            };
            println!(
                "Trying to create a Meetup event with the following details:\n{:#?}",
                new_event
            );
            Ok(new_event)
        }) as _;
        new_meetup_event = match crate::meetup::util::clone_event(
            &meetup_event.urlname,
            meetup_event.meetup_id,
            &meetup_client,
            Some(new_event_hook),
        )
        .await
        {
            Err(crate::meetup::Error::NewAPIError(
                crate::meetup::newapi::Error::ResourceNotFound,
            )) => {
                // Event was deleted, try the next one
                continue;
            }
            Err(err) => return Err(err.into()),
            Ok(new_event) => Some(new_event),
        };
        break;
    }
    let mut new_meetup_event = if let Some(new_meetup_event) = new_meetup_event {
        new_meetup_event
    } else {
        return Err(simple_error::SimpleError::new(
            "Could not find a previous Meetup event to clone",
        )
        .into());
    };

    // Close the RSVPs, ignoring errors
    if let Err(err) = meetup_client
        .close_rsvps(new_meetup_event.id.0.clone())
        .await
    {
        eprintln!(
            "RSVPs for event {} could not be closed:\n{:#?}",
            &new_meetup_event.id, err
        );
    } else {
        new_meetup_event.rsvp_settings = Some(
            crate::meetup::newapi::create_event_mutation::EventDataRsvpSettings {
                rsvps_closed: Some(true),
            },
        );
    };

    // Sync the Meetup event
    // TODO: retry on serialization error
    let meetup_event_db_id =
        match crate::meetup::sync::sync_event(new_meetup_event.clone().into(), db_connection).await
        {
            Ok(EventSyncResult::Synced(synced_event_id, meetup_event_db_id)) => {
                // The synced event ID must be the same as the event ID used throughout this method.
                // If this is not the case we try to delete the Meetup event
                if synced_event_id != event_id {
                    eprintln!(
                        "Schedule session: the synced event ID does not match the expected event \
                         ID"
                    );
                    // TODO: try to delete Meetup event
                    return Ok(PushEventResult {
                        meetup_event: new_meetup_event,
                        meetup_event_db_id: None,
                    });
                }
                meetup_event_db_id
            }
            Ok(EventSyncResult::CouldNotSync) => {
                eprintln!("Schedule session: Meetup event could not be synced");
                return Ok(PushEventResult {
                    meetup_event: new_meetup_event,
                    meetup_event_db_id: None,
                });
            }
            Err(err) => {
                eprintln!(
                    "Schedule session: error when syncing Meetup event:\n{:#?}",
                    err
                );
                return Ok(PushEventResult {
                    meetup_event: new_meetup_event,
                    meetup_event_db_id: None,
                });
            }
        };

    Ok(PushEventResult {
        meetup_event: new_meetup_event,
        meetup_event_db_id: Some(meetup_event_db_id),
    })
}
