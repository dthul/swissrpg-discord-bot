use chrono::serde::ts_milliseconds;
use futures::{stream::StreamExt, Stream};
use futures_util::{stream, TryFutureExt};
use reqwest::header::{HeaderMap, AUTHORIZATION};
use serde::{de::Error as _, Deserialize};

const BASE_URL: &'static str = "https://api.meetup.com";
pub const URLNAMES: [&'static str; 3] =
    ["SwissRPG-Zurich", "SwissRPG-Central", "SwissRPG-Romandie"];

#[derive(Debug, Clone)]
pub struct AsyncClient {
    client: reqwest::Client,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Photo {
    pub id: u64,
    pub thumb_link: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupProfile {
    pub role: Option<LeadershipRole>,
    pub status: UserStatus,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub photo: Option<Photo>,
    pub group_profile: Option<GroupProfile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Group {
    pub urlname: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum UserStatus {
    None,
    Pending,
    PendingPayment,
    Active,
    Blocked,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LeadershipRole {
    AssistantOrganizer,
    Coorganizer,
    EventOrganizer,
    Organizer,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub id: String, // yeah, Event IDs seem to be the only ones that are alphanumeric...
    pub name: String,
    #[serde(with = "ts_milliseconds")]
    pub time: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "duration")]
    pub duration_ms: Option<u64>,
    pub event_hosts: Vec<User>,
    pub featured_photo: Option<Photo>,
    pub link: String,
    pub group: Group,
    pub description: String,
    pub rsvp_limit: Option<u16>,
    pub yes_rsvp_count: Option<u16>,
    pub simple_html_description: Option<String>,
    pub how_to_find_us: Option<String>,
    pub venue: Option<Venue>,
    pub rsvp_rules: Option<RSVPRules>,
    pub is_online_event: Option<bool>,
}

impl Event {
    pub fn num_free_spots(&self) -> u16 {
        match (self.rsvp_limit, self.yes_rsvp_count) {
            (Some(rsvp_limit), Some(yes_rsvp_count)) if rsvp_limit > yes_rsvp_count => {
                rsvp_limit - yes_rsvp_count
            }
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Venue {
    pub id: u64,
    pub name: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub address_1: Option<String>,
    pub address_2: Option<String>,
    pub address_3: Option<String>,
    pub city: Option<String>,
}

pub struct NewEvent {
    pub description: String,
    pub duration_ms: Option<u64>,
    pub featured_photo_id: Option<u64>,
    pub hosts: Vec<u64>,
    pub how_to_find_us: Option<String>,
    // lat / long?
    pub name: String,
    pub rsvp_limit: Option<u16>,
    pub time: chrono::DateTime<chrono::Utc>,
    pub venue_id: u64,
    pub guest_limit: Option<u16>,
    pub published: bool,
}

impl<'de> Deserialize<'de> for UserStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "none" => Ok(UserStatus::None),
            "pending" => Ok(UserStatus::Pending),
            "pending_payment" => Ok(UserStatus::PendingPayment),
            "active" => Ok(UserStatus::Active),
            "blocked" => Ok(UserStatus::Blocked),
            _ => Err(D::Error::invalid_value(
                serde::de::Unexpected::Enum,
                &"one of [none, pending, pending_payment, active, blocked]",
            )),
        }
    }
}

impl<'de> Deserialize<'de> for LeadershipRole {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "assistant_organizer" => Ok(LeadershipRole::AssistantOrganizer),
            "coorganizer" => Ok(LeadershipRole::Coorganizer),
            "event_organizer" => Ok(LeadershipRole::EventOrganizer),
            "organizer" => Ok(LeadershipRole::Organizer),
            _ => Err(D::Error::invalid_value(
                serde::de::Unexpected::Enum,
                &"one of [assistant_organizer, coorganizer, event_organizer, organizer]",
            )),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct UserInfo {
    pub status: UserStatus,
    pub role: Option<String>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RSVPResponse {
    Yes,
    YesPendingPayment,
    No,
    Waitlist,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RSVP {
    pub member: User,
    pub response: RSVPResponse,
}

impl<'de> Deserialize<'de> for RSVPResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "yes" => Ok(RSVPResponse::Yes),
            "no" => Ok(RSVPResponse::No),
            "waitlist" => Ok(RSVPResponse::Waitlist),
            "yes_pending_payment" => Ok(RSVPResponse::YesPendingPayment),
            _ => Err(D::Error::invalid_value(
                serde::de::Unexpected::Enum,
                &"one of [yes, yes_pending_payment, no, waitlist]",
            )),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub struct RSVPRules {
    pub guest_limit: u16,
    pub closed: bool,
}

#[derive(Debug)]
pub enum Error {
    AuthenticationFailure,
    Reqwest(reqwest::Error),
    Serde {
        error: serde_json::Error,
        input: String,
    },
    ResourceNotFound,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::AuthenticationFailure => {
                write!(f, "Meetup Client Error (Authentication Failure)")
            }
            Error::Reqwest(error) => write!(f, "Meetup Client Error (Reqwest Error):\n{:?}", error),
            Error::Serde { error, input } => write!(
                f,
                "Meetup Client Error (Deserialization Error):\n{:?}\nInput was:\n{}",
                error, input
            ),
            Error::ResourceNotFound => write!(f, "Meetup Client Error: Resource not found"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::AuthenticationFailure => None,
            Error::Reqwest(err) => Some(err),
            Error::Serde { error: err, .. } => Some(err),
            Error::ResourceNotFound => None,
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Error::Reqwest(err)
    }
}

impl AsyncClient {
    pub fn new(access_token: &str) -> AsyncClient {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            format!("Bearer {}", access_token).parse().unwrap(),
        );
        AsyncClient {
            client: reqwest::Client::builder()
                .default_headers(headers)
                .build()
                .expect("Could not initialize the reqwest client"),
        }
    }

    // Gets the user with the specified ID
    // TODO: currently we cannot distinguish between a non-existing user or some other kind of error
    pub async fn get_group_profile(
        &self,
        id: Option<u64>,
        urlname: &str,
    ) -> Result<Option<User>, Error> {
        let url = match id {
            Some(id) => format!(
                "{}/{}/members/{}?&sign=true&photo-host=public&only=id,name,photo,group_profile&\
                 omit=group_profile.group,group_profile.answers",
                BASE_URL, urlname, id
            ),
            _ => format!(
                "{}/{}/members/self?&sign=true&photo-host=public&only=id,name,photo,group_profile&\
                 omit=group_profile.group,group_profile.answers",
                BASE_URL, urlname
            ),
        };
        let res = self.client.get(&url).send().await?;
        let user_res = Self::try_deserialize(res).await;
        match user_res {
            Ok(user) => Ok(Some(user)),
            Err(err) => {
                // Dirty hack: instead of properly parsing the errors returned
                // by the Meetup API to figure out whether it is just a "404",
                // just look at the error text instead
                if let Error::Serde { input, .. } = &err {
                    if input.contains("member_error") {
                        return Ok(None);
                    }
                }
                Err(err)
            }
        }
    }

    // Gets the user with the specified ID
    // TODO: currently we cannot distinguish between a non-existing user or some other kind of error
    pub async fn get_member_profile(&self, id: Option<u64>) -> Result<Option<User>, Error> {
        let url = match id {
            Some(id) => format!(
                "{}/members/{}?&sign=true&photo-host=public&only=id,name,photo",
                BASE_URL, id
            ),
            _ => format!(
                "{}/members/self?&sign=true&photo-host=public&only=id,name,photo",
                BASE_URL
            ),
        };
        let res = self.client.get(&url).send().await?;
        let user_res = Self::try_deserialize(res).await;
        match user_res {
            Ok(user) => Ok(Some(user)),
            Err(err) => {
                // Dirty hack: instead of properly parsing the errors returned
                // by the Meetup API to figure out whether it is just a "404",
                // just look at the error text instead
                if let Error::Serde { input, .. } = &err {
                    if input.contains("member_error") {
                        return Ok(None);
                    }
                }
                Err(err)
            }
        }
    }

    // Doesn't implement pagination. But since Meetup returns 200 elements per page,
    // this does not matter for us anyway
    pub fn get_upcoming_events(&self, urlname: &str) -> impl Stream<Item = Result<Event, Error>> {
        let url = format!(
            "{}/{}/events?&sign=true&photo-host=public&page=200&fields=event_hosts,rsvp_rules&\
             has_ended=false&status=upcoming&only=description,event_hosts.id,event_hosts.name,id,\
             link,time,name,group.urlname,rsvp_limit,yes_rsvp_count,venue,rsvp_rules,is_online_event",
            BASE_URL, urlname
        );
        let request = self.client.get(&url);
        request
            .send()
            .err_into::<Error>()
            .and_then(Self::try_deserialize)
            .map_ok(|event_list: Vec<Event>| stream::iter(event_list.into_iter().map(Ok)))
            .try_flatten_stream()
    }

    pub fn get_upcoming_events_all_groups(&self) -> impl Stream<Item = Result<Event, Error>> {
        let streams: Vec<_> = URLNAMES
            .iter()
            .map(|urlname| self.get_upcoming_events(urlname))
            .collect();
        stream::iter(streams).flatten()
    }

    pub async fn get_event(&self, urlname: &str, event_id: &str) -> Result<Event, Error> {
        let url = format!(
            "{}/{}/events/{}?&fields=simple_html_description,featured_photo,event_hosts",
            BASE_URL, urlname, event_id
        );
        let res = self.client.get(&url).send().await?;
        let event_res = Self::try_deserialize(res).await;
        match event_res {
            Ok(event) => Ok(event),
            Err(err) => {
                // Dirty hack: instead of properly parsing the errors returned
                // by the Meetup API to figure out whether it is just a "404",
                // just look at the error text instead
                if let Error::Serde { input, .. } = &err {
                    if input.contains("event_error") {
                        return Err(Error::ResourceNotFound);
                    }
                }
                Err(err)
            }
        }
    }

    // Get members that RSVP'd yes
    pub async fn get_rsvps(&self, urlname: &str, event_id: &str) -> Result<Vec<RSVP>, Error> {
        let url = format!(
            "{}/{}/events/{}/rsvps?&sign=true&photo-host=public&page=200&only=response,member&\
             omit=member.photo,member.event_context",
            BASE_URL, urlname, event_id
        );
        let res = self.client.get(&url).send().await?;
        let rsvp_res = Self::try_deserialize(res).await;
        match rsvp_res {
            Ok(rsvps) => Ok(rsvps),
            Err(err) => {
                // Dirty hack: instead of properly parsing the errors returned
                // by the Meetup API to figure out whether it is just a "404",
                // just look at the error text instead
                if let Error::Serde { input, .. } = &err {
                    if input.contains("event_error") {
                        return Err(Error::ResourceNotFound);
                    }
                }
                Err(err)
            }
        }
    }

    pub async fn rsvp(
        &self,
        urlname: &str,
        event_id: &str,
        attending: bool,
    ) -> Result<RSVP, Error> {
        let url = format!(
            "{}/{}/events/{}/rsvps?&sign=true&photo-host=public&page=200&only=response,member&\
             omit=member.photo,member.event_context",
            BASE_URL, urlname, event_id
        );
        let res = self
            .client
            .post(&url)
            .query(&[("response", if attending { "yes" } else { "no" })])
            .send()
            .await?;
        let rsvp_res = Self::try_deserialize(res).await;
        match rsvp_res {
            Ok(rsvp) => Ok(rsvp),
            Err(err) => {
                // Dirty hack: instead of properly parsing the errors returned
                // by the Meetup API to figure out whether it is a credentials
                // error by just looking at the error text instead
                if let Error::Serde { input, .. } = &err {
                    if input.contains("auth_fail") {
                        return Err(Error::AuthenticationFailure);
                    }
                }
                Err(err)
            }
        }
    }

    pub async fn create_event(&self, urlname: &str, event: NewEvent) -> Result<Event, Error> {
        let url = format!(
            "{}/{}/events?&fields=simple_html_description,featured_photo,event_hosts",
            BASE_URL, urlname
        );
        let host_ids = itertools::join(&event.hosts, ",");
        let mut query = vec![
            ("description", event.description),
            ("event_hosts", host_ids),
            ("name", event.name),
            ("rsvp_limit", event.rsvp_limit.unwrap_or(0).to_string()),
            ("guest_limit", event.guest_limit.unwrap_or(0).to_string()),
            ("self_rsvp", "false".into()),
            ("time", event.time.timestamp_millis().to_string()),
            ("venue_id", event.venue_id.to_string()),
            (
                "publish_status",
                if event.published {
                    "published".to_string()
                } else {
                    "draft".to_string()
                },
            ),
        ];
        if let Some(duration_ms) = event.duration_ms {
            query.push(("duration", duration_ms.to_string()));
        }
        if let Some(featured_photo_id) = event.featured_photo_id {
            query.push(("featured_photo_id", featured_photo_id.to_string()));
        }
        if let Some(how_to_find_us) = event.how_to_find_us {
            query.push(("how_to_find_us", how_to_find_us));
        }
        let res = self.client.post(&url).query(&query).send().await?;
        Self::try_deserialize(res).await
    }

    pub async fn close_rsvps(&self, urlname: &str, event_id: &str) -> Result<(), Error> {
        let url = format!("{}/{}/events/{}/rsvps/close", BASE_URL, urlname, event_id);
        let _res = self.client.post(&url).send().await?;
        Ok(())
    }

    async fn try_deserialize<T: serde::de::DeserializeOwned>(
        response: reqwest::Response,
    ) -> Result<T, Error> {
        let text = response.text().await?;
        let value: T = serde_json::from_str(&text).map_err(|err| Error::Serde {
            error: err,
            input: text,
        })?;
        Ok(value)
    }
}
