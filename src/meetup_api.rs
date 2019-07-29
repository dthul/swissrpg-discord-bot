use chrono::TimeZone;
use futures::future;
use futures::stream;
use futures::{Future, Stream};
use reqwest::header::{HeaderMap, AUTHORIZATION};
use reqwest::{Method, Request};
use serde::de::Error as _;
use serde::Deserialize;
use std::collections::HashMap;

const BASE_URL: &'static str = "https://api.meetup.com";
pub const URLNAME: &'static str = "SwissRPG-Zurich";

pub struct Client {
    client: reqwest::Client,
}

pub struct AsyncClient {
    client: reqwest::r#async::Client,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Photo {
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
    pub id: u64,
    pub name: String,
    pub time: i64, // UTC start time of the event, in milliseconds since the epoch
    pub event_hosts: Vec<User>,
    pub link: String,
}

impl Event {
    pub fn get_time(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        chrono::Utc.timestamp_millis_opt(self.time).earliest()
    }
}

type EventList = HashMap<String, Event>;

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
    No,
    Waitlist,
}

#[derive(Debug, Clone)]
pub struct RSVP {
    pub user: User,
    pub response: RSVPResponse,
}

#[derive(Debug, Deserialize, Clone)]
struct _RSVP {
    response: RSVPResponse,
}

#[derive(Debug, Deserialize, Clone)]
struct MemberRSVP {
    member: User,
    rsvp: _RSVP,
}

type MemberRSVPList = HashMap<String, MemberRSVP>;

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
            _ => Err(D::Error::invalid_value(
                serde::de::Unexpected::Enum,
                &"one of [yes, no, waitlist]",
            )),
        }
    }
}

impl Client {
    pub fn new(access_token: &str) -> Client {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            format!("Bearer {}", access_token).parse().unwrap(),
        );
        Client {
            client: reqwest::Client::builder()
                .default_headers(headers)
                .build()
                .expect("Could not initialize the reqwest client"),
        }
    }

    pub fn get_group_profile(&self, id: Option<u64>) -> crate::Result<Option<User>> {
        let url = match id {
            Some(id) => format!(
                "{}/{}/members/{}?&sign=true&photo-host=public&only=id,name,photo,group_profile&omit=group_profile.group,group_profile.answers",
                BASE_URL, URLNAME, id
            ),
            _ => format!(
                "{}/{}/members/self?&sign=true&photo-host=public&only=id,name,photo,group_profile&omit=group_profile.group,group_profile.answers",
                BASE_URL, URLNAME
            ),
        };
        let url = url.parse()?;
        let mut response = self.client.execute(Request::new(Method::GET, url))?;
        if let Ok(user) = response.json::<User>() {
            return Ok(Some(user));
        } else {
            return Ok(None);
        }
    }

    pub fn get_member_profile(&self, id: Option<u64>) -> crate::Result<Option<User>> {
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
        let url = url.parse()?;
        let mut response = self.client.execute(Request::new(Method::GET, url))?;
        if let Ok(user) = response.json::<User>() {
            return Ok(Some(user));
        } else {
            return Ok(None);
        }
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
            client: reqwest::r#async::Client::builder()
                .default_headers(headers)
                .build()
                .expect("Could not initialize the reqwest client"),
        }
    }

    // Gets the user with the specified ID
    pub fn get_group_profile(
        &self,
        id: Option<u64>,
    ) -> impl Future<Item = Option<User>, Error = crate::BoxedError> {
        let url = match id {
            Some(id) => format!(
                "{}/{}/members/{}?&sign=true&photo-host=public&only=id,name,photo,group_profile&omit=group_profile.group,group_profile.answers",
                BASE_URL, URLNAME, id
            ),
            _ => format!(
                "{}/{}/members/self?&sign=true&photo-host=public&only=id,name,photo,group_profile&omit=group_profile.group,group_profile.answers",
                BASE_URL, URLNAME
            ),
        };
        self.client
            .get(&url)
            .send()
            .from_err::<crate::BoxedError>()
            .and_then(|mut response| {
                response.json::<User>().then(|user| match user {
                    Ok(user) => future::ok(Some(user)),
                    _ => future::ok(None),
                })
            })
    }

    // Gets the user with the specified ID
    pub fn get_member_profile(
        &self,
        id: Option<u64>,
    ) -> impl Future<Item = Option<User>, Error = crate::BoxedError> {
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
        self.client
            .get(&url)
            .send()
            .from_err::<crate::BoxedError>()
            .and_then(|mut response| {
                response.json::<User>().then(|user| match user {
                    Ok(user) => future::ok(Some(user)),
                    _ => future::ok(None),
                })
            })
    }

    // Doesn't implement pagination. But since Meetup returns 200 elements per page,
    // this does not matter for us anyway
    pub fn get_upcoming_events(&self) -> impl Stream<Item = Event, Error = crate::BoxedError> {
        let url = format!("{}/{}/events?&sign=true&photo-host=public&page=200&fields=event_hosts&has_ended=false&status=upcoming&only=event_hosts.id,event_hosts.name,id,link,time,name", BASE_URL, URLNAME);
        self.client
            .get(&url)
            .send()
            .from_err::<crate::BoxedError>()
            .and_then(|mut response| response.json::<EventList>().from_err::<crate::BoxedError>())
            .map(|event_list| stream::iter_ok(event_list.into_iter().map(|(_, event)| event)))
            .flatten_stream()
    }

    // Get members that RSVP'd yes
    pub fn get_rsvps(&self, id: u64) -> impl Future<Item = Vec<RSVP>, Error = crate::BoxedError> {
        let url = format!("{}/{}/events/{}/attendance?&sign=true&photo-host=public&page=200&omit=member.photo,member.event_context,member.role,rsvp.guests,rsvp.id,rsvp.updated", BASE_URL, URLNAME, id);
        self.client
            .get(&url)
            .send()
            .from_err::<crate::BoxedError>()
            .and_then(|mut response| {
                response
                    .json::<MemberRSVPList>()
                    .from_err::<crate::BoxedError>()
            })
            .map(|member_rsvp_list| {
                member_rsvp_list
                    .into_iter()
                    .map(|(_, member_rsvp)| RSVP {
                        user: member_rsvp.member,
                        response: member_rsvp.rsvp.response,
                    })
                    .collect()
            })
    }
}
