use serenity::model::id::ChannelId;

// macro_rules! create_wrapped_type {
//     ($Wrapper:ident, $T:ident) => {
//         #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
//         pub struct $Wrapper(pub $T);

//         impl<'r, DB: ::sqlx::Database> ::sqlx::Decode<'r, DB> for $Wrapper
//         where
//             $T: ::sqlx::Decode<'r, DB>,
//         {
//             fn decode(
//                 value: <DB as ::sqlx::database::HasValueRef<'r>>::ValueRef,
//             ) -> Result<$Wrapper, Box<dyn ::std::error::Error + 'static + Send + Sync>> {
//                 let value = <$T as ::sqlx::Decode<DB>>::decode(value)?;
//                 Ok($Wrapper(value))
//             }
//         }

//         impl<'q, DB: ::sqlx::Database> ::sqlx::Encode<'q, DB> for $Wrapper
//         where
//             $T: ::sqlx::Encode<'q, DB>,
//         {
//             #[inline]
//             fn encode(
//                 self,
//                 buf: &mut <DB as ::sqlx::database::HasArguments<'q>>::ArgumentBuffer,
//             ) -> ::sqlx::encode::IsNull {
//                 <$T as ::sqlx::Encode<DB>>::encode(self.0, buf)
//             }

//             #[inline]
//             fn encode_by_ref(
//                 &self,
//                 buf: &mut <DB as ::sqlx::database::HasArguments<'q>>::ArgumentBuffer,
//             ) -> ::sqlx::encode::IsNull {
//                 <$T as ::sqlx::Encode<DB>>::encode(self.0, buf)
//             }

//             #[inline]
//             fn produces(&self) -> Option<DB::TypeInfo> {
//                 self.0.produces()
//             }

//             #[inline]
//             fn size_hint(&self) -> usize {
//                 self.0.size_hint()
//             }
//         }
//     };
// }

// create_wrapped_type!(EventSeriesId, i32);
// create_wrapped_type!(DiscordUserId, u64);

#[derive(sqlx::Type, Debug, Clone, Copy)]
#[sqlx(transparent)]
pub struct EventSeriesId(pub i32);

#[derive(sqlx::Type, Debug, Clone, Copy)]
#[sqlx(transparent)]
pub struct EventId(pub i32);

#[derive(sqlx::Type, Debug, Clone, Copy)]
#[sqlx(transparent)]
pub struct DiscordChannelId(pub u64);

pub struct MeetupEvent {
    pub meetup_id: String,
    pub url: String,
    pub urlname: String,
}

pub struct Event {
    pub id: EventId,
    pub title: String,
    pub description: String,
    pub time: chrono::DateTime<chrono::Utc>,
    pub is_online: bool,
    pub discord_category: Option<ChannelId>,
    pub meetup_event: Option<MeetupEvent>,
}

struct EventQueryHelper {
    event_id: i32,
    start_time: chrono::DateTime<chrono::Utc>,
    title: String,
    description: String,
    is_online: bool,
    discord_category_id: Option<i64>,
    meetup_event_id: Option<i32>,
    meetup_event_meetup_id: Option<String>,
    meetup_event_url: Option<String>,
    meetup_event_urlname: Option<String>,
}

impl From<EventQueryHelper> for Event {
    fn from(row: EventQueryHelper) -> Self {
        let meetup_event = match (
            row.meetup_event_id,
            row.meetup_event_meetup_id,
            row.meetup_event_url,
            row.meetup_event_urlname,
        ) {
            (Some(id), Some(meetup_id), Some(url), Some(urlname)) => Some(MeetupEvent {
                meetup_id: meetup_id,
                url: url,
                urlname: urlname,
            }),
            _ => None,
        };
        Event {
            id: EventId(row.event_id),
            title: row.title,
            description: row.description,
            time: row.start_time,
            is_online: row.is_online,
            discord_category: row.discord_category_id.map(|id| ChannelId(id as u64)),
            meetup_event: meetup_event,
        }
    }
}

pub async fn get_next_event_in_series(
    db_connection: &sqlx::PgPool,
    series_id: EventSeriesId,
) -> Result<Option<Event>, crate::meetup::Error> {
    let next_event = sqlx::query_as!(
        EventQueryHelper,
        r#"SELECT event.id as event_id, event.start_time, event.title, event.description, event.is_online, event.discord_category_id, meetup_event.id as "meetup_event_id?", meetup_event.meetup_id as "meetup_event_meetup_id?", meetup_event.url as "meetup_event_url?", meetup_event.urlname as "meetup_event_urlname?"
        FROM event
        LEFT OUTER JOIN meetup_event ON event.id = meetup_event.event_id
        WHERE event_series_id = $1 AND start_time > now()
        ORDER BY start_time
        FETCH FIRST ROW ONLY"#,
        series_id.0
    )
    .fetch_optional(db_connection)
    .await?;
    Ok(next_event.map(Into::into))
}

pub async fn get_last_event_in_series(
    db_connection: &sqlx::PgPool,
    series_id: EventSeriesId,
) -> Result<Option<Event>, crate::meetup::Error> {
    let last_event = sqlx::query_as!(
        EventQueryHelper,
        r#"SELECT event.id as event_id, event.start_time, event.title, event.description, event.is_online, event.discord_category_id, meetup_event.id as "meetup_event_id?", meetup_event.meetup_id as "meetup_event_meetup_id?", meetup_event.url as "meetup_event_url?", meetup_event.urlname as "meetup_event_urlname?"
        FROM event
        LEFT OUTER JOIN meetup_event ON event.id = meetup_event.event_id
        WHERE event_series_id = $1
        ORDER BY start_time DESC
        FETCH FIRST ROW ONLY"#,
        series_id.0
    )
    .fetch_optional(db_connection)
    .await?;
    Ok(last_event.map(Into::into))
}

// Queries all events belonging to the specified series from latest to oldest
pub async fn get_events_for_series(
    db_connection: &sqlx::PgPool,
    series_id: EventSeriesId,
) -> Result<Vec<Event>, crate::meetup::Error> {
    let events = sqlx::query_as!(
        EventQueryHelper,
        r#"SELECT event.id as event_id, event.start_time, event.title, event.description, event.is_online, event.discord_category_id, meetup_event.id as "meetup_event_id?", meetup_event.meetup_id as "meetup_event_meetup_id?", meetup_event.url as "meetup_event_url?", meetup_event.urlname as "meetup_event_urlname?"
        FROM event
        LEFT OUTER JOIN meetup_event ON event.id = meetup_event.event_id
        WHERE event_series_id = $1
        ORDER BY start_time DESC"#,
        series_id.0
    )
    .map(Into::into)
    .fetch_all(db_connection)
    .await?;
    Ok(events)
}
