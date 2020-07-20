use serde::{de::Error, Deserialize};
use warp::Filter;

#[derive(Clone, Debug, Deserialize)]
struct FormContent {
    #[serde(rename = "4")]
    email: String,
    #[serde(rename = "5.3")]
    name_first: String,
    #[serde(rename = "29.3")]
    name_last: String,
    #[serde(rename = "6")]
    discord_user_name: String,
    #[serde(rename = "9")]
    meetup_profile_url: String,
    #[serde(rename = "11")]
    game_type: GameType,
    #[serde(rename = "15")]
    rpg_system: RPGSystem,
    // TODO: bool from "Yes" / "No" string?
    #[serde(rename = "33")]
    dndbeyond_access: bool,
    #[serde(rename = "33")]
    character_creation: CharacterCreation,
    #[serde(rename = "17")]
    max_players: usize,
    // TODO: bool from "Yes" / "No" string?
    #[serde(rename = "37.1")]
    beginner_friendly: bool,
    #[serde(rename = "19")]
    title: String,
    #[serde(rename = "20")]
    description: String,
    // TODO: YYYY-MM-DD format
    #[serde(rename = "34")]
    date: String,
    // TODO: HH:MM format (24h)
    #[serde(rename = "35")]
    time: String,
    // TODO: HH:MM format
    #[serde(rename = "38")]
    duration: String,
    #[serde(rename = "42")]
    city: City,
    #[serde(rename = "45")]
    zip: String,
    // TODO: merge venue from fields 43 and 48 (and later others?)
    #[serde(rename = "43")]
    venue: Venue,
    #[serde(rename = "26")]
    comments: String,
}

#[derive(Clone, Copy, Debug)]
enum GameType {
    OneShot,
    OneShotSeries,
    MiniCampaign,
    Campaign,
    IntroGame,
}

#[derive(Clone, Debug)]
enum RPGSystem {
    DnD5e,
    CoC7e,
    Shadowrun6e,
    Runequest,
    Other(String),
}

#[derive(Clone, Copy, Debug)]
enum CharacterCreation {
    Before,
    During,
    Pregen,
}

#[derive(Clone, Debug)]
enum City {
    Zurich,
    Basel,
    Bern,
    Geneva,
    Lausanne,
    Lugano,
    Other(String),
}

#[derive(Clone, Debug)]
enum Venue {
    Own,
    SwissRPG,
    Online,
}

impl<'de> Deserialize<'de> for GameType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "One-Shot" => Ok(GameType::OneShot),
            "One-Shot Series" => Ok(GameType::OneShotSeries),
            "Mini campaign" => Ok(GameType::MiniCampaign),
            "Campaign" => Ok(GameType::Campaign),
            "Initiation intro game (for pure beginners)" => Ok(GameType::IntroGame),
            _ => Err(D::Error::invalid_value(
                serde::de::Unexpected::Enum,
                &"one of [\"One-Shot\", \"One-Shot Series\", \"Mini Campaign\", \"Campaign\", \"Initiation intro game (for pure beginners)\"]",
            )),
        }
    }
}

impl<'de> Deserialize<'de> for RPGSystem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "Dungeons & Dragons 5e" => Ok(RPGSystem::DnD5e),
            "Call of Cthulhu 7e" => Ok(RPGSystem::CoC7e),
            "Shadowrun 6e" => Ok(RPGSystem::Shadowrun6e),
            "Runequest" => Ok(RPGSystem::Runequest),
            _ => Ok(RPGSystem::Other(s)),
        }
    }
}

impl<'de> Deserialize<'de> for CharacterCreation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "Before game (in my Discord channel)" => Ok(CharacterCreation::Before),
            "During the game session" => Ok(CharacterCreation::During),
            "Pre-generated characters" => Ok(CharacterCreation::Pregen),
            _ => Err(D::Error::invalid_value(
                serde::de::Unexpected::Enum,
                &"one of [\"Before game (in my Discord channel)\", \"During the game session\", \"Pre-generated characters\"]",
            )),
        }
    }
}

impl<'de> Deserialize<'de> for Venue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "My place" => Ok(Venue::Own),
            "SwissRPG venue" => Ok(Venue::SwissRPG),
            "Online" => Ok(Venue::Online),
            _ => Err(D::Error::invalid_value(
                serde::de::Unexpected::Enum,
                &"one of [\"My place\", \"SwissRPG venue\", \"Online\"]",
            )),
        }
    }
}

impl<'de> Deserialize<'de> for City {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "Zürich" => Ok(City::Zurich),
            "Basel" => Ok(City::Basel),
            "Bern" => Ok(City::Bern),
            "Geneva" => Ok(City::Geneva),
            "Lausanne" => Ok(City::Lausanne),
            "Lugano" => Ok(City::Lugano),
            _ => Ok(City::Other(s)),
        }
    }
}

pub fn create_routes(
) -> impl warp::Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    let post_route = {
        warp::post()
            .and(warp::path!("webhooks" / "gf"))
            .and(warp::body::content_length_limit(1024 * 512))
            .and(warp::body::json())
            .and_then(move |body: serde_json::Value| {
                println!("Got GF form submission:\n{:#?}", body);
                async move {
                    // let mut redis_connection = redis_client
                    //     .get_async_connection()
                    //     .err_into::<lib::meetup::Error>()
                    //     .await?;
                    Ok::<_, warp::Rejection>(warp::http::StatusCode::OK)
                }
            })
    };
    post_route
}
