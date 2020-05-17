// While syncing upcoming Meetup events, the code in this file is used to build
// a list of events with free spots and post those to Discord.

use crate::meetup::api::Event;
use geo::{euclidean_distance::EuclideanDistance, Point};
use lazy_static::lazy_static;
use serenity::model::id::ChannelId;
use std::{collections::HashMap, fmt::Write};

pub const CLOSED_PATTERN: &'static str = r"(?i)\[\s*closed\s*\]";

lazy_static! {
    pub static ref CLOSED_REGEX: regex::Regex = regex::Regex::new(CLOSED_PATTERN).unwrap();
}

#[derive(Debug, Clone)]
pub struct EventCollector {
    // List of upcoming events and the number of free spots
    events: Vec<(Event, u16)>,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
enum Location {
    Online,
    Zurich,
    Basel,
    Luzern,
    Lugano,
    Geneva,
    Lausanne,
    Bern,
    Aarau,
    Chur,
}

// The ordering here is used for the ordering of posted messages
static ALL_LOCATIONS: &[Location] = &[
    Location::Zurich,
    Location::Basel,
    Location::Luzern,
    Location::Lugano,
    Location::Geneva,
    Location::Lausanne,
    Location::Bern,
    Location::Aarau,
    Location::Chur,
    Location::Online,
];

// Locations with a latitude and longitude
static PHYSICAL_LOCATIONS: &[Location] = &[
    Location::Zurich,
    Location::Basel,
    Location::Luzern,
    Location::Lugano,
    Location::Geneva,
    Location::Lausanne,
    Location::Bern,
    Location::Aarau,
    Location::Chur,
];

impl Location {
    pub fn lat_lon(&self) -> Option<Point<f64>> {
        match self {
            Location::Online => None,
            Location::Zurich => Some(Point::new(8.541694, 47.376888)),
            Location::Basel => Some(Point::new(7.588576, 47.559601)),
            Location::Luzern => Some(Point::new(8.308010, 47.045540)),
            Location::Lugano => Some(Point::new(8.953620, 46.003601)),
            Location::Geneva => Some(Point::new(6.143158, 46.204391)),
            Location::Lausanne => Some(Point::new(6.6345432, 46.519316)),
            Location::Bern => Some(Point::new(7.4433158, 46.9489217)),
            Location::Aarau => Some(Point::new(8.0606556, 47.3934732)),
            Location::Chur => Some(Point::new(9.5275838, 46.8533507)),
        }
    }

    // Finds the closest location to the given point
    pub fn closest(p: Point<f64>) -> Location {
        let mut min_loc_dist = (
            PHYSICAL_LOCATIONS[0],
            PHYSICAL_LOCATIONS[0]
                .lat_lon()
                .unwrap()
                .euclidean_distance(&p),
        );
        for &location in &PHYSICAL_LOCATIONS[1..] {
            let dist = location.lat_lon().unwrap().euclidean_distance(&p);
            if dist < min_loc_dist.1 {
                min_loc_dist = (location, dist)
            }
        }
        min_loc_dist.0
    }

    pub fn name(&self) -> &'static str {
        match self {
            Location::Online => "Online",
            Location::Zurich => "Zürich",
            Location::Basel => "Basel",
            Location::Luzern => "Luzern",
            Location::Lugano => "Lugano",
            Location::Geneva => "Geneva",
            Location::Lausanne => "Lausanne",
            Location::Bern => "Bern",
            Location::Aarau => "Aarau",
            Location::Chur => "Chur",
        }
    }

    pub fn flag_name(&self) -> &'static str {
        match self {
            Location::Online => "Online",
            Location::Zurich => "Zurich",
            Location::Basel => "Basel",
            Location::Luzern => "Luzern",
            Location::Lugano => "Ticino",
            Location::Geneva => "Geneva",
            Location::Lausanne => "Vaud",
            Location::Bern => "Bern",
            Location::Aarau => "Aargau",
            Location::Chur => "Graubunden",
        }
    }

    pub fn color(&self) -> (u8, u8, u8) {
        // Discord seems to turn perfect white and black into their
        // counterparts depending on the user's color scheme, so we don't use
        // perfect white and black here
        match self {
            Location::Online => (255, 23, 68),
            Location::Zurich => (0, 115, 229),
            Location::Basel => (1, 2, 2),
            Location::Luzern => (38, 139, 204),
            Location::Lugano => (232, 66, 63),
            Location::Geneva => (232, 66, 63),
            Location::Lausanne => (22, 167, 78),
            Location::Bern => (255, 215, 48),
            Location::Aarau => (38, 139, 204),
            Location::Chur => (254, 254, 254),
        }
    }

    pub fn meetup_group_link(&self) -> &'static str {
        match self {
            Location::Online | Location::Zurich => "https://www.meetup.com/SwissRPG-Zurich/",
            Location::Basel
            | Location::Luzern
            | Location::Lugano
            | Location::Bern
            | Location::Aarau
            | Location::Chur => "https://www.meetup.com/SwissRPG-Central/",
            Location::Geneva | Location::Lausanne => "https://www.meetup.com/SwissRPG-Romandie/",
        }
    }
}

impl EventCollector {
    pub fn new() -> Self {
        EventCollector { events: vec![] }
    }

    pub fn add_event(&mut self, event: &Event) {
        // Do a quick pre-filtering.
        // Discard events which don't have free spots
        let num_free_spots = match (event.rsvp_limit, event.yes_rsvp_count) {
            (Some(rsvp_limit), Some(yes_rsvp_count)) if rsvp_limit > yes_rsvp_count => {
                rsvp_limit - yes_rsvp_count
            }
            _ => return,
        };
        // Filter out events for which RSVPs are not open
        let is_closed_event = event
            .rsvp_rules
            .as_ref()
            .map(|rules| rules.closed)
            .unwrap_or(false)
            || CLOSED_REGEX.is_match(&event.description);
        if is_closed_event {
            return;
        }
        // Keep no events which are too far in the future
        if event.time > chrono::Utc::now() + chrono::Duration::days(30) {
            return;
        }
        self.events.push((event.clone(), num_free_spots));
    }

    pub async fn update_channel(
        &self,
        discord_api: &crate::discord::CacheAndHttp,
        channel_id: ChannelId,
        static_file_prefix: &str,
    ) -> Result<(), crate::meetup::Error> {
        let mut latest_messages = channel_id
            .messages(&discord_api.http, |query| {
                query.limit(2 * ALL_LOCATIONS.len() as u64)
            })
            .await?;
        let mut localized_events = self.localized_events();
        for location in ALL_LOCATIONS {
            let events: &mut [(Event, u16)] = localized_events
                .get_mut(location)
                .map(Vec::as_mut_slice)
                .unwrap_or(&mut []);
            events.sort_unstable_by_key(|(event, _)| event.time);
            // Try to find an existing message that corresponds to this location
            let embed_author = location.name();
            let location_message = latest_messages.iter_mut().find(|message| {
                message
                    .embeds
                    .first()
                    .and_then(|embed| embed.author.as_ref())
                    .map(|author| author.name == embed_author)
                    .unwrap_or(false)
            });
            if let Some(message) = location_message {
                // Edit the existing message
                message
                    .edit(discord_api, |message| {
                        message.embed(|embed| {
                            Self::build_embed(static_file_prefix, *location, events, embed)
                        })
                    })
                    .await?;
            } else {
                // Post a new message
                channel_id
                    .send_message(&discord_api.http, |message| {
                        message.embed(|embed| {
                            Self::build_embed(static_file_prefix, *location, events, embed)
                        })
                    })
                    .await?;
            }
        }
        Ok(())
    }

    fn build_embed<'a>(
        static_file_prefix: &'_ str,
        location: Location,
        events: &'_ [(Event, u16)],
        embed_builder: &'a mut serenity::builder::CreateEmbed,
    ) -> &'a mut serenity::builder::CreateEmbed {
        let footer_text = chrono::Utc::now()
            .with_timezone(&chrono_tz::Europe::Zurich)
            .format("Last update at %H:%M")
            .to_string();
        let mut description = "Updated every 15 minutes".to_string();
        for (event, free_spots) in events {
            description.push_str("\n\n");
            write!(
                &mut description,
                "**{}**\n",
                // TODO: proper escaping
                &event.name.replace("*", r"\*")
            )
            .ok();
            description.push_str(&event.time.format("_%a, %b %-d_").to_string());
            if *free_spots == 1 {
                write!(&mut description, " — {} spot\n", free_spots).ok();
            } else {
                write!(&mut description, " — {} spots\n", free_spots).ok();
            }
            write!(&mut description, "[Sign up on Meetup](<{}>)", &event.link).ok();
        }
        embed_builder
            .author(|author| {
                author
                    .name(location.name())
                    .icon_url(format!("{}SwissRPG-logo-128.png", static_file_prefix))
                    .url(location.meetup_group_link())
            })
            .thumbnail(format!(
                "{}thumbnail_{}.png",
                static_file_prefix,
                location.flag_name()
            ))
            .title(if events.is_empty() {
                "All games are fully booked. Roll for initiative!"
            } else {
                "List of games and events that have open spots"
            })
            .description(description)
            .colour(location.color())
            .footer(|footer| footer.text(&footer_text))
    }

    // Returns all events for which a location can be determined, grouped by
    // their respective locations
    fn localized_events(&self) -> HashMap<Location, Vec<(Event, u16)>> {
        // Try to assign each event to one of our cities or the online category
        let mut location_events: HashMap<Location, Vec<(Event, u16)>> = HashMap::new();
        for (event, num_free_spots) in &self.events {
            if let Some(location) = Self::event_location(event) {
                location_events
                    .entry(location)
                    .or_default()
                    .push((event.clone(), *num_free_spots));
            }
        }
        location_events
    }

    // Figure out which location (if any) an event belongs to
    fn event_location(event: &Event) -> Option<Location> {
        let venue = match &event.venue {
            Some(venue) => venue,
            None => {
                // Event doesn't have a venue? Assume that it's online
                return Some(Location::Online);
            }
        };
        // Is this event online?
        if venue.name.to_lowercase() == "online game"
            || crate::meetup::sync::ONLINE_REGEX.is_match(&event.description)
        {
            return Some(Location::Online);
        }
        // Doesn't seem to be an online event. We will use latitude and
        // longitude to figure out the city instead
        let (lat, lon) = match (venue.lat, venue.lon) {
            (Some(lat), Some(lon)) => (lat, lon),
            _ => {
                // No latitude and longitude?
                // No idea where it is then.
                return None;
            }
        };
        let point = Point::new(lon, lat);
        let location = Location::closest(point);
        Some(location)
    }
}
