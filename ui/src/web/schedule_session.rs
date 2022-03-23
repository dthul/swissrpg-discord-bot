use std::{collections::HashMap, sync::Arc};

use askama::Template;
use axum::{
    extract::{ContentLengthLimit, Extension, Form, Path},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use chrono::{offset::TimeZone, Datelike, Timelike};
use chrono_tz::Europe;
use lib::{db, schedule_session::ScheduleSessionResult};

use super::{server::State, MessageTemplate, WebError};

pub fn create_routes() -> Router {
    let routes = Router::new().route(
        "/schedule_session/:flow_id",
        get(schedule_session_handler).post(schedule_session_post_handler),
    );
    // The following routes are just to be able to take a look at the scheduling
    // and success templates without using an actual flow
    #[cfg(feature = "bottest")]
    let routes = routes
        .route(
            "/schedule_session/test",
            get(|| {
                let local_time = chrono::Utc::now().with_timezone(&Europe::Zurich);
                let template = ScheduleSessionTemplate {
                    day: local_time.day() as u8,
                    month: local_time.month() as u8,
                    year: local_time.year() as u16,
                    hour: local_time.hour() as u8,
                    minute: local_time.minute() as u8,
                    selectable_years: &[local_time.year() as u16, local_time.year() as u16 + 1],
                    duration: 150,
                    with_session_number: true,
                    title: "Test event",
                    link: Some("https://meetup.com/"),
                };
                futures::future::ready(template.into_response())
            }),
        )
        .route(
            "/schedule_session/test/success",
            get(|| {
                let template = ScheduleSessionSuccessTemplate {
                    title: "Test event",
                    link: Some("https://meetup.com/"),
                    closed_rsvps: Some(true),
                };
                futures::future::ready(template.into_response())
            }),
        );
    routes
}

#[derive(Template)]
#[template(path = "schedule_session.html")]
struct ScheduleSessionTemplate<'a> {
    day: u8, // In Europe::Zurich timezone
    month: u8,
    year: u16,
    hour: u8,
    minute: u8,
    selectable_years: &'a [u16],
    duration: u16, // In minutes
    title: &'a str,
    link: Option<&'a str>,
    with_session_number: bool,
}

#[derive(Template)]
#[template(path = "schedule_session_success.html")]
struct ScheduleSessionSuccessTemplate<'a> {
    title: &'a str,
    link: Option<&'a str>,
    closed_rsvps: Option<bool>,
}

pub mod filters {
    pub fn isequal<T: num_traits::PrimInt>(num: &T, val: &T) -> Result<bool, askama::Error> {
        Ok(num == val)
    }

    pub fn format_minutes_to_hhmm(minutes: &u16) -> Result<String, askama::Error> {
        Ok(format!("{}:{:02}", minutes / 60, minutes % 60))
    }
}

async fn schedule_session_handler(
    Extension(state): Extension<Arc<State>>,
    Path(flow_id): Path<u64>,
) -> Result<Response, WebError> {
    let mut redis_connection = state.redis_client.get_async_connection().await?;
    eprintln!("Retrieving flow...");
    let flow = lib::flow::ScheduleSessionFlow::retrieve(&mut redis_connection, flow_id).await?;
    let flow = match flow {
        Some(flow) => flow,
        None => {
            let template: MessageTemplate = ("Link expired", "Please request a new link").into();
            return Ok(template.into_response());
        }
    };
    eprintln!("... got it!\nRetrieving last event...");
    let event = db::get_last_event_in_series(&state.pool, flow.event_series_id).await?;
    eprintln!("... got it!");
    match event {
        None => {
            let template: MessageTemplate = (
                "No prior event found",
                "Cannot schedule a continuation session without an initial event",
            )
                .into();
            Ok(template.into_response())
        }
        Some(event) => {
            // Assume Swiss time
            let local_time = event.time.with_timezone(&Europe::Zurich);
            // We don't just add 7 * 24 hours, since that might break across
            // daylight saving time boundaries
            let mut next_event_local_datetime = match (local_time.date()
                + chrono::Duration::weeks(1))
            .and_time(local_time.time())
            {
                Some(time) => time,
                None => local_time,
            };
            // If the proposed next event time is in the past, propose a time in the future instead
            let now = chrono::Utc::now().with_timezone(&Europe::Zurich);
            if next_event_local_datetime < now {
                next_event_local_datetime = now
                    .with_timezone(&Europe::Zurich)
                    .date()
                    .and_time(local_time.time())
                    .unwrap_or(now + chrono::Duration::days(1));
            }
            let template = ScheduleSessionTemplate {
                day: next_event_local_datetime.day() as u8,
                month: next_event_local_datetime.month() as u8,
                year: next_event_local_datetime.year() as u16,
                hour: next_event_local_datetime.hour() as u8,
                minute: next_event_local_datetime.minute() as u8,
                selectable_years: &[
                    next_event_local_datetime.year() as u16,
                    next_event_local_datetime.year() as u16 + 1,
                ],
                duration: 4 * 60,
                title: &event.title,
                link: event
                    .meetup_event
                    .as_ref()
                    .map(|meetup_event| meetup_event.url.as_str()),
                with_session_number: lib::meetup::sync::SESSION_REGEX.is_match(&event.title),
            };
            Ok(template.into_response())
        }
    }
}

async fn schedule_session_post_handler(
    _: ContentLengthLimit<(), 32768>,
    Extension(state): Extension<Arc<State>>,
    Path(flow_id): Path<u64>,
    Form(form_data): Form<HashMap<String, String>>,
) -> Result<Response, WebError> {
    let meetup_client = match *(state.async_meetup_client).lock().await {
        Some(ref meetup_client) => meetup_client.clone(),
        None => {
            let template: MessageTemplate =
                ("Meetup API unavailable", "Please try again later").into();
            return Ok(template.into_response());
        }
    };
    let mut redis_connection = state.redis_client.get_async_connection().await?;
    let flow = lib::flow::ScheduleSessionFlow::retrieve(&mut redis_connection, flow_id).await?;
    let flow = match flow {
        Some(flow) => flow,
        None => {
            let template: MessageTemplate = ("Link expired", "Please request a new link").into();
            return Ok(template.into_response());
        }
    };
    // Check that the form contains all necessary data
    let participant_limit = form_data
        .get("participant_limit")
        .map(|value| value.parse::<u16>())
        .unwrap_or(Ok(0))?;
    let with_session_number = form_data
        .get("with_session_number")
        .map(|value| value == "yes")
        .unwrap_or(false);
    let (year, month, day, hour, minute) = match (
        form_data.get("year"),
        form_data.get("month"),
        form_data.get("day"),
        form_data.get("hour"),
        form_data.get("minute"),
    ) {
        (Some(year), Some(month), Some(day), Some(hour), Some(minute)) => {
            (year, month, day, hour, minute)
        }
        _ => {
            let template: MessageTemplate = (
                "Invalid data",
                "Seems like the submitted data is incomplete",
            )
                .into();
            return Ok(template.into_response());
        }
    };
    let duration = match form_data.get("duration") {
        None => 4 * 60,
        Some(duration) => match duration.parse::<i64>() {
            Ok(duration) => duration.clamp(30, 12 * 60),
            Err(_) => 4 * 60,
        },
    };
    let duration = chrono::Duration::minutes(duration);
    // Try to convert the supplied data to a DateTime
    let date_time = match (
        year.parse::<i32>(),
        month.parse::<u32>(),
        day.parse::<u32>(),
        hour.parse::<u32>(),
        minute.parse::<u32>(),
    ) {
        (Ok(year), Ok(month), Ok(day), Ok(hour), Ok(minute)) => {
            match chrono::NaiveDate::from_ymd_opt(year, month, day) {
                Some(date) => match date.and_hms_opt(hour, minute, 0) {
                    Some(naive_date_time) => {
                        match Europe::Zurich.from_local_datetime(&naive_date_time) {
                            chrono::LocalResult::Single(date_time) => date_time,
                            _ => {
                                let template: MessageTemplate = (
                                    "Invalid data",
                                    "Seems like the specified time is ambiguous or non-existent",
                                )
                                    .into();
                                return Ok(template.into_response());
                            }
                        }
                    }
                    _ => {
                        let template: MessageTemplate =
                            ("Invalid data", "Seems like the specified time is invalid").into();
                        return Ok(template.into_response());
                    }
                },
                _ => {
                    let template: MessageTemplate =
                        ("Invalid data", "Seems like the specified date is invalid").into();
                    return Ok(template.into_response());
                }
            }
        }
        _ => {
            let template: MessageTemplate = (
                "Invalid data",
                "Seems like the submitted data has an invalid format",
            )
                .into();
            return Ok(template.into_response());
        }
    };
    // Convert time to UTC
    let date_time = date_time.with_timezone(&chrono::Utc);

    let ScheduleSessionResult {
        event_id,
        meetup_event,
        ..
    } = lib::schedule_session::schedule_session(
        flow.event_series_id,
        participant_limit,
        with_session_number,
        date_time,
        duration,
        &state.pool,
        &state.discord_cache_http,
        &meetup_client,
        &mut redis_connection,
        state.bot_id,
    )
    .await?;

    // Delete the flow, ignoring errors
    if let Err(err) = flow.delete(&mut redis_connection).await {
        eprintln!(
            "Encountered an error when trying to delete a schedule session flow:\n{:#?}",
            err
        );
    }

    let event = lib::db::get_event(&state.pool, event_id).await?;

    let template = ScheduleSessionSuccessTemplate {
        title: &event.title,
        link: meetup_event
            .as_ref()
            .map(|meetup_event| meetup_event.event_url.as_str()),
        closed_rsvps: meetup_event.as_ref().map(|meetup_event| {
            meetup_event
                .rsvp_settings
                .as_ref()
                .and_then(|rsvp_settings| rsvp_settings.rsvps_closed)
                .unwrap_or(false)
        }),
    };

    // let template: MessageTemplate = (
    //     "No prior event found",
    //     "Cannot schedule a continuation session without an initial event",
    // )
    //     .into();

    Ok(template.into_response())
}
