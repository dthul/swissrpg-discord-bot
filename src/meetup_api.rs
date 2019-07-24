use futures::future::Future;
use reqwest::header::{HeaderMap, AUTHORIZATION};
use reqwest::{Method, Request};
use serde::de::Error as _;
use serde::Deserialize;

const BASE_URL: &'static str = "https://api.meetup.com";
pub const URLNAME: &'static str = "SwissRPG-Zurich";

pub struct Client {
    client: reqwest::Client,
}

pub struct AsyncClient {
    client: reqwest::r#async::Client,
}

#[derive(Deserialize)]
pub struct Photo {
    pub thumb_link: String,
}

#[derive(Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub photo: Option<Photo>,
}

pub enum UserStatus {
    None,
    Pending,
    PendingPayment,
    Active,
    Blocked,
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

#[derive(Deserialize)]
pub struct UserInfo {
    pub status: UserStatus,
    pub role: Option<String>,
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

    // Gets the user with the specified ID
    pub fn get_user(&self, id: u64) -> crate::Result<Option<User>> {
        let url = format!(
            "{}/members/{}?&sign=true&photo-host=public&only=id,name,photo",
            BASE_URL, id
        );
        let url = url.parse()?;
        let mut response = self.client.execute(Request::new(Method::GET, url))?;
        if let Ok(user) = response.json::<User>() {
            return Ok(Some(user));
        } else {
            return Ok(None);
        }
    }

    // Gets the currently authenticated user's membership info
    pub fn get_user_info(&self) -> crate::Result<Option<UserInfo>> {
        let url = format!(
            "{}/{}?&only=self&omit=self.actions,self.membership_dues,self.previous_membership_dues,self.visited",
            BASE_URL, URLNAME
        );
        let url = url.parse()?;
        let mut response = self.client.execute(Request::new(Method::GET, url))?;
        println!("get_user_info: {:?}", &response);
        if let Ok(user_info) = response.json::<UserInfo>() {
            return Ok(Some(user_info));
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
    pub fn get_user(
        &self,
        id: u64,
    ) -> impl futures::Future<Item = Option<User>, Error = crate::BoxedError> {
        let url = format!(
            "{}/members/{}?&sign=true&photo-host=public&only=id,name,photo",
            BASE_URL, id
        );
        self.client
            .get(&url)
            .send()
            .from_err::<crate::BoxedError>()
            .and_then(|mut response| {
                response.json::<User>().then(|user| match user {
                    Ok(user) => futures::future::ok(Some(user)),
                    _ => futures::future::ok(None),
                })
            })
    }

    // Gets the currently authenticated user's membership info
    pub fn get_user_info(
        &self,
    ) -> impl futures::Future<Item = Option<UserInfo>, Error = crate::BoxedError> {
        let url = format!(
            "{}/{}?&only=self&omit=self.actions,self.membership_dues,self.previous_membership_dues,self.visited",
            BASE_URL, URLNAME
        );
        self.client
            .get(&url)
            .send()
            .from_err::<crate::BoxedError>()
            .and_then(|mut response| {
                println!("get_user_info: {:?}", &response);
                response
                    .json::<UserInfo>()
                    .then(|user_info| match user_info {
                        Ok(user_info) => futures::future::ok(Some(user_info)),
                        _ => futures::future::ok(None),
                    })
            })
    }
}
