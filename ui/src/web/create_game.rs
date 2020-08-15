use chrono::{Duration, NaiveDate, NaiveTime, TimeZone};
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
    #[serde(rename = "34", deserialize_with = "date_yyyymmdd")]
    date: NaiveDate,
    #[serde(rename = "35", deserialize_with = "time_hhmm")]
    time: NaiveTime,
    #[serde(rename = "38", deserialize_with = "duration_hhmm")]
    duration: Duration,
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

fn time_hhmm<'de, D>(deserializer: D) -> Result<NaiveTime, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match NaiveTime::parse_from_str(&s, "%H:%M") {
        Ok(time) => Ok(time),
        Err(_) => Err(D::Error::invalid_value(
            serde::de::Unexpected::Str(s.as_str()),
            &"time string in HH:MM format",
        )),
    }
}

fn duration_hhmm<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let regex = match regex::Regex::new("^([0-9]{1,2}):([0-9]{2})$") {
        Ok(regex) => regex,
        Err(err) => {
            eprintln!("Error when trying to compile duration regex:\n{:#?}", err);
            return Err(D::Error::custom("Regex compile error"));
        }
    };
    if let Some(captures) = regex.captures(&s) {
        let hours = captures
            .get(1)
            .map(|m| m.as_str().parse::<u8>().ok())
            .flatten();
        let minutes = captures
            .get(2)
            .map(|m| m.as_str().parse::<u8>().ok())
            .flatten();
        if let (Some(hours), Some(minutes)) = (hours, minutes) {
            return Ok(Duration::minutes(60 * hours as i64 + minutes as i64));
        }
    }
    Err(D::Error::invalid_value(
        serde::de::Unexpected::Str(s.as_str()),
        &"duration string in HH:MM format",
    ))
}

fn date_yyyymmdd<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
        Ok(date) => Ok(date),
        Err(_) => Err(D::Error::invalid_value(
            serde::de::Unexpected::Str(s.as_str()),
            &"date string in YYYY-MM-DD format",
        )),
    }
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
                            if let Err(err) = create_asana_task(&asana_client, &form_content).await
                            {
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

async fn crate_meetup_event(
    meetup_client: &lib::meetup::api::AsyncClient,
    form_content: &FormContent,
) -> Result<(), lib::meetup::Error> {
    let time = chrono_tz::Europe::Zurich
        .from_local_datetime(&form_content.date.and_time(form_content.time))
        .earliest()
        .ok_or(simple_error::SimpleError::new(
            "Couldn't convert to local Zurich time",
        ))?
        .with_timezone(&chrono::Utc);
    let new_event = lib::meetup::api::NewEvent {
        description: form_content.description.clone(),
        duration_ms: Some(form_content.duration.num_milliseconds() as u64),
        featured_photo_id: None,
        hosts: vec![],
        how_to_find_us: None,
        name: form_content.title.clone(),
        rsvp_limit: Some(form_content.max_players as u16),
        time,
        venue_id: 0, // TODO
        guest_limit: None,
        published: false,
    };
    Ok(())
}

async fn create_asana_task(
    asana_client: &lib::asana::api::AsyncClient,
    form_content: &FormContent,
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
        name: form_content.title.clone(),
        project_ids: Some(vec![lib::asana::ids::PUBLICATIONS_PROJECT_ID.clone()]),
        tag_ids: Some(vec![tag.id]),
        notes: Some(form_content.description.clone()),
    };
    asana_client
        .create_task(&new_task)
        .await
        .map_err(Into::into)
}
