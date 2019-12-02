use futures_util::{compat::Future01CompatExt, FutureExt};
use rand::Rng;
use redis::{Commands, PipelineCommands};

pub struct ScheduleSessionFlow {
    pub id: u64,
    pub event_series_id: String,
}

impl ScheduleSessionFlow {
    pub fn new(
        redis_client: &mut redis::Client,
        event_series_id: String,
    ) -> Result<Self, crate::meetup::Error> {
        let id: u64 = rand::thread_rng().gen();
        let redis_key = format!("flow:schedule_session:{}", id);
        let mut pipe = redis::pipe();
        let _: () = pipe
            .hset(&redis_key, "event_series_id", &event_series_id)
            .ignore()
            .expire(&redis_key, 10 * 60)
            .query(redis_client)?;
        Ok(ScheduleSessionFlow {
            id: id,
            event_series_id: event_series_id,
        })
    }

    pub async fn retrieve(
        redis_connection: redis::aio::Connection,
        id: u64,
    ) -> Result<(redis::aio::Connection, Option<Self>), crate::meetup::Error> {
        let redis_key = format!("flow:schedule_session:{}", id);
        let mut pipe = redis::pipe();
        pipe.hget(&redis_key, "event_series_id");
        let (redis_connection, (event_series_id,)): (_, (Option<String>,)) =
            pipe.query_async(redis_connection).compat().await?;
        let flow = event_series_id.map(|event_series_id| ScheduleSessionFlow {
            id: id,
            event_series_id: event_series_id,
        });
        Ok((redis_connection, flow))
    }

    pub async fn schedule(
        self,
        redis_client: &mut redis::Client,
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
        let mut events =
            crate::meetup::util::get_events_for_series(redis_client, &self.event_series_id)?;
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
            redis_client,
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
        let _: redis::RedisResult<()> = redis_client.del(&redis_key);
        let sync_future = {
            let new_event = new_event.clone();
            let mut redis_client = redis_client.clone();
            let rsvps = rsvp_result.as_ref().map(|res| res.cloned_rsvps.clone());
            async move {
                let event_id = new_event.id.clone();
                crate::meetup::sync::sync_event(new_event, &mut redis_client).await?;
                if let Some(rsvps) = rsvps {
                    crate::meetup::sync::sync_rsvps(&event_id, rsvps, &mut redis_client).await?;
                }
                Ok::<_, crate::meetup::Error>(())
            }
        };
        crate::ASYNC_RUNTIME.spawn(sync_future.map(|res| {
            if let Err(err) = res {
                eprintln!("Could not sync the newly scheduled event:\n{:#?}", err);
            }
        }));
        Ok((new_event, rsvp_result))
    }

    pub async fn delete(
        self,
        redis_connection: redis::aio::Connection,
    ) -> Result<redis::aio::Connection, crate::meetup::Error> {
        let redis_key = format!("flow:schedule_session:{}", self.id);
        let (redis_connection, ()) = redis::cmd("DEL")
            .arg(&redis_key)
            .query_async(redis_connection)
            .compat()
            .await?;
        Ok(redis_connection)
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
        let description = crate::meetup::sync::ONLINE_REGEX.replace_all(&description, "");
        let mut description = crate::meetup::sync::CHANNEL_REGEX
            .replace_all(&description, "")
            .into_owned();
        // Add an event series shortcode if there is none yet
        if !crate::meetup::sync::EVENT_SERIES_REGEX.is_match(&description) {
            description.push_str(&format!("\n[campaign {}]", old_event_id));
        }
        // Increase the Session number
        let mut name = new_event.name.clone();
        let title_captures = crate::meetup::sync::SESSION_REGEX.captures_iter(&new_event.name);
        // Match the rightmost occurence of "Session X" in the title
        if let Some(capture) = title_captures.last() {
            // If there is a match, increase the number
            // Extract the current number from the title
            let session_number = capture.name("number").unwrap().as_str();
            // Try to parse the session number
            let session_number = session_number.parse::<i32>()?;
            // Find the range of the "Session X" match
            let session_x_match = capture.get(0).unwrap();
            // Replace the text "Session X" by "Session X+1"
            name.replace_range(
                session_x_match.start()..session_x_match.end(),
                &format!("Session {}", session_number + 1),
            );
        } else {
            // If there is no match, append a Session number
            name.push_str(&format!(" Session 2"));
        }
        new_event.name = name;
        new_event.description = description;
        new_event.time = new_date_time;
        Ok(new_event)
    }
}
