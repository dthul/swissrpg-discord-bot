use reqwest::header::{HeaderMap, AUTHORIZATION};
use reqwest::{Method, Request};
use serde::Deserialize;
use serde::de::Error as _;

const BASE_URL: &'static str = "https://api.meetup.com";
pub const URLNAME: &'static str = "SwissRPG-Zurich";

pub struct Client {
    client: reqwest::Client,
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
        where D: serde::Deserializer<'de>
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "none" => Ok(UserStatus::None),
            "pending" => Ok(UserStatus::Pending),
            "pending_payment" => Ok(UserStatus::PendingPayment),
            "active" => Ok(UserStatus::Active),
            "blocked" => Ok(UserStatus::Blocked),
            _ => Err(D::Error::invalid_value(serde::de::Unexpected::Enum, &"one of [none, pending, pending_payment, active, blocked]")),
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
