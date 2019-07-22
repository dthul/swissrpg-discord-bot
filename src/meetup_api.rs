use reqwest::header::{HeaderMap, AUTHORIZATION};
use reqwest::{Method, Request};
use serde::Deserialize;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const BASE_URL: &'static str = "https://api.meetup.com";

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

    pub fn get_user(&self, id: u64) -> Result<Option<User>> {
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
}
