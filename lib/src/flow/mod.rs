use futures_util::FutureExt;
use rand::Rng;
use redis::AsyncCommands;

pub struct ScheduleSessionFlow {
    pub id: u64,
    pub event_series_id: String,
}

impl ScheduleSessionFlow {
    pub fn new(
        redis_connection: &mut redis::Connection,
        event_series_id: String,
    ) -> Result<Self, crate::meetup::Error> {
        let id: u64 = rand::thread_rng().gen();
        let redis_key = format!("flow:schedule_session:{}", id);
        let mut pipe = redis::pipe();
        let _: () = pipe
            .hset(&redis_key, "event_series_id", &event_series_id)
            .ignore()
            .expire(&redis_key, 10 * 60)
            .query(redis_connection)?;
        Ok(ScheduleSessionFlow {
            id: id,
            event_series_id: event_series_id,
        })
    }

    pub async fn retrieve(
        redis_connection: &mut redis::aio::Connection,
        id: u64,
    ) -> Result<Option<Self>, crate::meetup::Error> {
        let redis_key = format!("flow:schedule_session:{}", id);
        let event_series_id: Option<String> =
            redis_connection.hget(&redis_key, "event_series_id").await?;
        let flow = event_series_id.map(|event_series_id| ScheduleSessionFlow {
            id: id,
            event_series_id: event_series_id,
        });
        Ok(flow)
    }

    pub async fn schedule(
        self,
        mut redis_connection: redis::aio::Connection,
        meetup_client: &crate::meetup::api::AsyncClient,
        oauth2_consumer: &crate::meetup::oauth2::OAuth2Consumer,
        date_time: chrono::DateTime<chrono::Utc>,
    ) -> Result<
        (
            crate::meetup::api::Event,
            Option<crate::meetup::util::CloneRSVPResult>,
        ),
        crate::meetup::Error,
    > {
        // Query the latest event in the series
        let mut events = crate::meetup::util::get_events_for_series(
            &mut redis_connection,
            &self.event_series_id,
        )
        .await?;
        // Sort by time
        events.sort_unstable_by_key(|event| event.time);
        // Take the latest event as a template to copy from
        let latest_event = match events.last() {
            Some(event) => event,
            None => {
                return Err(simple_error::SimpleError::new(
                    "There is no event to use as a template",
                )
                .into())
            }
        };
        // Clone the event
        let new_event_hook =
            Box::new(|new_event| Self::new_event_hook(new_event, date_time, &latest_event.id));
        let new_event = crate::meetup::util::clone_event(
            &latest_event.urlname,
            &latest_event.id,
            meetup_client,
            Some(new_event_hook),
        )
        .await?;
        // Try to transfer the RSVPs to the new event
        let rsvp_result = match crate::meetup::util::clone_rsvps(
            &latest_event.urlname,
            &latest_event.id,
            &new_event.id,
            &mut redis_connection,
            meetup_client,
            oauth2_consumer,
        )
        .await
        {
            Ok(result) => Some(result),
            Err(err) => {
                eprintln!("Could not transfer all RSVPs to the new event.\n{:#?}", err);
                None
            }
        };
        let redis_key = format!("flow:schedule_session:{}", self.id);
        let _: redis::RedisResult<()> = redis_connection.del(&redis_key).await;
        let sync_future = {
            let new_event = new_event.clone();
            let rsvps = rsvp_result.as_ref().map(|res| res.cloned_rsvps.clone());
            async move {
                let event_id = new_event.id.clone();
                crate::meetup::sync::sync_event(new_event, &mut redis_connection).await?;
                if let Some(rsvps) = rsvps {
                    crate::meetup::sync::sync_rsvps(&event_id, rsvps, &mut redis_connection)
                        .await?;
                }
                Ok::<_, crate::meetup::Error>(())
            }
        };
        tokio::spawn(sync_future.map(|res| {
            if let Err(err) = res {
                eprintln!("Could not sync the newly scheduled event:\n{:#?}", err);
            }
        }));
        Ok((new_event, rsvp_result))
    }

    pub async fn delete(
        self,
        redis_connection: &mut redis::aio::Connection,
    ) -> Result<(), crate::meetup::Error> {
        let redis_key = format!("flow:schedule_session:{}", self.id);
        let () = redis_connection.del(&redis_key).await?;
        Ok(())
    }

    pub fn new_event_hook(
        mut new_event: crate::meetup::api::NewEvent,
        new_date_time: chrono::DateTime<chrono::Utc>,
        old_event_id: &str,
    ) -> Result<crate::meetup::api::NewEvent, crate::meetup::Error> {
        // Remove unnecessary shortcodes from follow-up sessions
        let description = new_event.description;
        let description = crate::meetup::sync::NEW_ADVENTURE_REGEX.replace_all(&description, "");
        let description = crate::meetup::sync::NEW_CAMPAIGN_REGEX.replace_all(&description, "");
        // We don't remove the [online] shortcode from descriptions anymore,
        // such that the "free game spots" feature has an easy way to tell
        // whether an event is online or not. This is mostly due to the fact
        // that at the time of this writing, we can not use the official Meetup
        // feature (yet?) for marking events as being online.
        // let description = crate::meetup::sync::ONLINE_REGEX.replace_all(&description, "");
        let mut description = crate::meetup::sync::CHANNEL_REGEX
            .replace_all(&description, "")
            .into_owned();
        // Add an event series shortcode if there is none yet
        if !crate::meetup::sync::EVENT_SERIES_REGEX.is_match(&description) {
            description.push_str(&format!("\n[campaign {}]", old_event_id));
        }
        // Increase the Session number
        let name = new_event.name.clone();
        let title_captures = crate::meetup::sync::SESSION_REGEX.captures_iter(&new_event.name);
        // Match the rightmost occurence of " Session X" in the event name.
        // Returns the event name without the session number (title_only) and
        // the current session number
        let (title_only, session_number) = if let Some(capture) = title_captures.last() {
            // If there is a match, increase the number
            // Extract the current number from the title
            let session_number = capture.name("number").unwrap().as_str();
            // Try to parse the session number
            let session_number = session_number.parse::<i32>()?;
            // Find the range of the " Session X" match and remove it from the string
            let session_x_match = capture.get(0).unwrap();
            let mut title_only = name;
            title_only.truncate(session_x_match.start());
            (title_only, session_number)
        } else {
            // If there is no match, return the whole name and Session number 1
            (name, 1)
        };
        // Create a new " Session X+1" suffix
        let new_session_suffix = format!(" Session {}", session_number + 1);
        // Check if the concatenation of event title and session suffix is short enough
        let new_event_name = if title_only.encode_utf16().count()
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
        new_event.name = new_event_name;
        new_event.description = description;
        new_event.time = new_date_time;
        Ok(new_event)
    }
}
