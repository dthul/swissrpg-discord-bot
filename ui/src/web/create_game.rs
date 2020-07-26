use lib::asana::tags::Color;
use serde::{de, de::Error, Deserialize, Deserializer};
use std::sync::Arc;
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
    #[serde(rename = "33", deserialize_with = "bool_from_yes_no")]
    dndbeyond_access: bool,
    #[serde(rename = "40")]
    character_creation: CharacterCreation,
    #[serde(rename = "17", deserialize_with = "from_str")]
    max_players: usize,
    #[serde(rename = "37.1", deserialize_with = "bool_from_yes_no")]
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
    #[serde(rename = "48")]
    venue_name: String,
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

fn bool_from_yes_no<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    match String::deserialize(deserializer)?.as_str() {
        "Yes" => Ok(true),
        "No" | "" => Ok(false),
        other => Err(de::Error::invalid_value(
            de::Unexpected::Str(other),
            &"'Yes' or 'No'",
        )),
    }
}

fn from_str<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    T::from_str(&s).map_err(de::Error::custom)
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
    asana_client: Arc<lib::asana::api::AsyncClient>,
) -> impl warp::Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    let post_route = {
        warp::post()
            .and(warp::path!("webhooks" / "gf"))
            .and(warp::body::content_length_limit(1024 * 512))
            .and(warp::body::json())
            .and_then(move |body: serde_json::Value| {
                println!("Got GF form submission:\n{:#?}", body);
                let asana_client = asana_client.clone();
                async move {
                    match serde_json::from_value::<FormContent>(body) {
                        Ok(form_content) => {
                            println!("Creating Asana task");
                            if let Err(err) = create_asana_task(&asana_client, form_content).await {
                                eprintln!("Asana error:\n{:#?}", err);
                            } else {
                                println!("Created Asana task");
                            }
                            Ok::<_, warp::Rejection>(warp::http::StatusCode::OK)
                        }
                        Err(err) => {
                            eprintln!("Couldn't parse form content:\n{:#?}", err);
                            Ok::<_, warp::Rejection>(warp::http::StatusCode::BAD_REQUEST)
                        }
                    }
                }
            })
    };
    post_route
}

async fn create_asana_task(
    asana_client: &lib::asana::api::AsyncClient,
    form_content: FormContent,
) -> Result<lib::asana::task::Task, lib::meetup::Error> {
    // Turn the form into an Asana task
    // Step 1: get the venue Tag
    let tag = match form_content.venue {
        Venue::Online => {
            asana_client
                .get_or_create_tag_by_name("Online", Some(&Color::DarkPurple))
                .await?
        }
        Venue::SwissRPG | Venue::Own => {
            asana_client
                .get_or_create_tag_by_name(&form_content.venue_name, Some(&Color::LightTeal))
                .await?
        }
    };
    let new_task = lib::asana::task::CreateTask {
        name: form_content.title,
        project_ids: Some(vec![lib::asana::ids::PUBLICATIONS_PROJECT_ID.clone()]),
        tag_ids: Some(vec![tag.id]),
        notes: Some(form_content.description),
    };
    asana_client
        .create_task(&new_task)
        .await
        .map_err(Into::into)
}
