use lazy_static::lazy_static;

// TODO: move to URL module
#[cfg(feature = "bottest")]
pub const DOMAIN: &'static str = "bottest.swissrpg.ch";
#[cfg(feature = "bottest")]
pub const BASE_URL: &'static str = "https://bottest.swissrpg.ch";
#[cfg(not(feature = "bottest"))]
pub const DOMAIN: &'static str = "bot.swissrpg.ch";
#[cfg(not(feature = "bottest"))]
pub const BASE_URL: &'static str = "https://bot.swissrpg.ch";
lazy_static! {
    pub static ref LINK_URL_REGEX: regex::Regex =
        regex::Regex::new(r"^/link/(?P<id>[a-zA-Z0-9\-_]+)$").unwrap();
    pub static ref LINK_REDIRECT_URL_REGEX: regex::Regex =
        regex::Regex::new(r"^/link/(?P<id>[a-zA-Z0-9\-_]+)/(?P<type>rsvp|norsvp)/redirect$")
            .unwrap();
}

// Meetup API
pub const MEETUP_OAUTH2_AUTH_URL: &'static str = "https://secure.meetup.com/oauth2/authorize";
pub const MEETUP_OAUTH2_TOKEN_URL: &'static str = "https://secure.meetup.com/oauth2/access";
