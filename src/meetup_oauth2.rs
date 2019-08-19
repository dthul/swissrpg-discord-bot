use crate::meetup_api;
use crate::strings;
use askama::Template;
use cookie::Cookie;
use futures::future;
use futures::stream;
use futures::stream::Stream;
use futures::Future;
use hyper::service::service_fn;
use hyper::{Body, Method, Request, Response, Server};
use lazy_static::lazy_static;
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope,
    TokenResponse, TokenUrl,
};
use rand::Rng;
use redis::{Commands, PipelineCommands, RedisResult};
use serenity::prelude::{Mutex, RwLock};
use simple_error::SimpleError;
use std::borrow::Cow;
use std::sync::Arc;
use url::Url;

const DOMAIN: &'static str = "bot.swissrpg.ch";
const BASE_URL: &'static str = "https://bot.swissrpg.ch";
lazy_static! {
    static ref LINK_URL_REGEX: regex::Regex =
        regex::Regex::new(r"^/link/(?P<id>[a-zA-Z0-9\-_]+)$").unwrap();
    static ref LINK_REDIRECT_URL_REGEX: regex::Regex =
        regex::Regex::new(r"^/link/(?P<id>[a-zA-Z0-9\-_]+)/(?P<type>rsvp|norsvp)/redirect$")
            .unwrap();
}

#[derive(Template)]
#[template(path = "link.html")]
struct LinkingTemplate<'a> {
    authorize_url: &'a str,
}

#[derive(Template)]
#[template(path = "link_message.html")]
struct LinkingMessageTemplate<'a> {
    title: &'a str,
    content: Option<&'a str>,
    safe_content: Option<&'a str>,
    img_url: Option<&'a str>,
}

pub fn new_random_id(num_bytes: u32) -> String {
    let random_bytes: Vec<u8> = (0..num_bytes)
        .map(|_| rand::thread_rng().gen::<u8>())
        .collect();
    base64::encode_config(&random_bytes, base64::URL_SAFE_NO_PAD)
}

fn generate_csrf_cookie(
    redis_connection_mutex: &Mutex<redis::Connection>,
    csrf_state: &str,
) -> crate::Result<Cookie<'static>> {
    let random_csrf_user_id = new_random_id(16);
    let redis_csrf_key = format!("csrf:{}", &random_csrf_user_id);
    let _: () = redis_connection_mutex
        .lock()
        .set_ex(&redis_csrf_key, csrf_state, 3600)?;
    Ok(Cookie::build("csrf_user_id", random_csrf_user_id)
        .domain(DOMAIN)
        .http_only(true)
        .same_site(cookie::SameSite::Lax)
        .max_age(time::Duration::hours(1))
        .finish())
}

fn check_csrf_cookie(
    redis_connection_mutex: &Mutex<redis::Connection>,
    headers: &hyper::HeaderMap<hyper::header::HeaderValue>,
    csrf_state: &str,
) -> crate::Result<bool> {
    let csrf_user_id_cookie =
        headers
            .get_all(hyper::header::COOKIE)
            .iter()
            .find_map(|header_value| {
                if let Ok(header_value) = header_value.to_str() {
                    if let Ok(cookie) = Cookie::parse(header_value) {
                        if cookie.name() == "csrf_user_id" {
                            return Some(cookie);
                        }
                    }
                }
                None
            });
    let csrf_user_id_cookie = match csrf_user_id_cookie {
        None => return Ok(false),
        Some(csrf_user_id_cookie) => csrf_user_id_cookie,
    };
    let redis_csrf_key = format!("csrf:{}", csrf_user_id_cookie.value());
    let csrf_stored_state: String = match redis_connection_mutex.lock().get(&redis_csrf_key)? {
        None => return Ok(false),
        Some(csrf_stored_state) => csrf_stored_state,
    };
    Ok(csrf_state == csrf_stored_state)
}

pub fn generate_meetup_linking_link(
    redis_connection: &Mutex<redis::Connection>,
    discord_id: u64,
) -> crate::Result<String> {
    let linking_id = new_random_id(16);
    let redis_key = format!("meetup_linking:{}:discord_user", &linking_id);
    let _: () = match redis_connection.lock().set(&redis_key, discord_id) {
        Ok(id) => id,
        Err(err) => {
            return Err(SimpleError::new(format!(
                "Redis error when trying to generate Meetup linking link: {}",
                err
            ))
            .into())
        }
    };
    return Ok(format!("{}/link/{}", BASE_URL, &linking_id));
}

enum HandlerResponse {
    Response(Response<Body>),
    Message {
        title: Cow<'static, str>,
        content: Option<Cow<'static, str>>,
        safe_content: Option<Cow<'static, str>>,
        img_url: Option<Cow<'static, str>>,
    },
}

impl HandlerResponse {
    pub fn from_template(template: impl Template) -> Result<Self, crate::BoxedError> {
        template
            .render()
            .map_err(Into::into)
            .map(|html_body| HandlerResponse::Response(Response::new(html_body.into())))
    }
}

impl From<(&'static str, &'static str)> for HandlerResponse {
    fn from((title, content): (&'static str, &'static str)) -> Self {
        HandlerResponse::Message {
            title: Cow::Borrowed(title),
            content: Some(Cow::Borrowed(content)),
            safe_content: None,
            img_url: None,
        }
    }
}

impl From<(String, &'static str)> for HandlerResponse {
    fn from((title, content): (String, &'static str)) -> Self {
        HandlerResponse::Message {
            title: Cow::Owned(title),
            content: Some(Cow::Borrowed(content)),
            safe_content: None,
            img_url: None,
        }
    }
}

impl From<(&'static str, String)> for HandlerResponse {
    fn from((title, content): (&'static str, String)) -> Self {
        HandlerResponse::Message {
            title: Cow::Borrowed(title),
            content: Some(Cow::Owned(content)),
            safe_content: None,
            img_url: None,
        }
    }
}

impl From<(String, String)> for HandlerResponse {
    fn from((title, content): (String, String)) -> Self {
        HandlerResponse::Message {
            title: Cow::Owned(title),
            content: Some(Cow::Owned(content)),
            safe_content: None,
            img_url: None,
        }
    }
}

fn get_group_profiles(
    meetup_api: meetup_api::AsyncClient,
) -> impl Future<Item = Vec<Option<meetup_api::User>>, Error = crate::meetup_api::Error> {
    stream::iter_ok::<_, crate::meetup_api::Error>(&meetup_api::URLNAMES)
        .and_then(move |urlname| meetup_api.get_group_profile(None, urlname))
        .collect()
}

// TODO: switch to future-aware mutexes
// TODO: switch to async Redis
type ResponseFuture = Box<dyn Future<Item = HandlerResponse, Error = crate::BoxedError> + Send>;
fn meetup_http_handler(
    redis_connection_mutex: &Arc<Mutex<redis::Connection>>,
    oauth2_authorization_client: &BasicClient,
    oauth2_link_client: &BasicClient,
    _discord_http: &serenity::CacheAndHttp,
    meetup_client: &Arc<RwLock<Option<meetup_api::Client>>>,
    async_meetup_client: &Arc<RwLock<Option<meetup_api::AsyncClient>>>,
    req: Request<Body>,
    bot_name: String,
) -> ResponseFuture {
    let (method, path) = (req.method(), req.uri().path());
    if let (&Method::GET, "/authorize") = (method, path) {
        // Generate the authorization URL to which we'll redirect the user.
        let (authorize_url, csrf_state) = oauth2_authorization_client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new("ageless".to_string()))
            .add_scope(Scope::new("basic".to_string()))
            .add_scope(Scope::new("event_management".to_string()))
            .url();
        // Store the generated CSRF token so we can compare it to the one
        // returned by Meetup later
        let csrf_cookie = match generate_csrf_cookie(redis_connection_mutex, csrf_state.secret()) {
            Ok(csrf_cookie) => csrf_cookie,
            Err(err) => return Box::new(future::err(err.into())),
        };
        let html_body = format!("<a href=\"{}\">Login with Meetup</a>", authorize_url);
        Box::new(future::result(
            Response::builder()
                .header(hyper::header::SET_COOKIE, csrf_cookie.to_string())
                .body(html_body.into())
                .map_err(|err| err.into())
                .map(|response| HandlerResponse::Response(response)),
        ))
    } else if let (&Method::GET, "/authorize/redirect") = (method, path) {
        let full_uri = format!("{}{}", BASE_URL, &req.uri().to_string());
        let req_url = match Url::parse(&full_uri) {
            Ok(url) => url,
            Err(err) => return Box::new(future::err(err.into())),
        };
        let params: Vec<_> = req_url.query_pairs().collect();
        let code = params
            .iter()
            .find_map(|(key, value)| if key == "code" { Some(value) } else { None });
        let state = params
            .iter()
            .find_map(|(key, value)| if key == "state" { Some(value) } else { None });
        let error = params
            .iter()
            .find_map(|(key, value)| if key == "error" { Some(value) } else { None });
        if let Some(error) = error {
            return Box::new(future::ok(("OAuth2 error", error.to_string()).into()));
        }
        let (code, csrf_state) = match (code, state) {
            (Some(code), Some(state)) => (code, state),
            _ => return Box::new(future::ok(("Request parameters missing", "").into())),
        };
        // Compare the CSRF state that was returned by Meetup to the one
        // we have saved
        let csrf_is_valid =
            match check_csrf_cookie(redis_connection_mutex, req.headers(), &csrf_state) {
                Ok(is_valid) => is_valid,
                Err(err) => return Box::new(future::err(err.into())),
            };
        if !csrf_is_valid {
            return Box::new(future::ok(
                (
                    "CSRF check failed",
                    "Please go back to the first page, reload, and repeat the process",
                )
                    .into(),
            ));
        }
        // Exchange the code with a token.
        let code = AuthorizationCode::new(code.to_string());
        let redis_connection_mutex = redis_connection_mutex.clone();
        let meetup_client = meetup_client.clone();
        let async_meetup_client = async_meetup_client.clone();
        let future = oauth2_authorization_client
            .exchange_code(code)
            .request_async(async_http_client)
            .map_err(|err| SimpleError::new(format!("RequestTokenError: {}", err)).into())
            .and_then(|token_res| {
                // Check that this token belongs to an organizer of all our Meetup groups
                let new_async_meetup_client =
                    meetup_api::AsyncClient::new(token_res.access_token().secret());
                get_group_profiles(new_async_meetup_client.clone())
                    .from_err::<crate::BoxedError>()
                    .and_then(move |user_profiles| {
                        let is_organizer = user_profiles.iter().all(|profile| {
                            let is_organizer = match profile {
                                Some(meetup_api::User {
                                    group_profile:
                                        Some(meetup_api::GroupProfile {
                                            status: meetup_api::UserStatus::Active,
                                            role: Some(role),
                                        }),
                                    ..
                                }) => {
                                    *role == meetup_api::LeadershipRole::Organizer
                                        || *role == meetup_api::LeadershipRole::Coorganizer
                                        || *role == meetup_api::LeadershipRole::AssistantOrganizer
                                }
                                _ => false,
                            };
                            is_organizer
                        });
                        if !is_organizer {
                            return future::ok(("Only the organizer can log in", "").into());
                        }
                        // Store the new access and refresh tokens in Redis
                        let res: RedisResult<()> = redis::transaction(
                            &mut *redis_connection_mutex.lock(),
                            &["meetup_access_token", "meetup_refresh_token"],
                            |con, pipe| match token_res.refresh_token() {
                                Some(refresh_token) => pipe
                                    .set("meetup_access_token", token_res.access_token().secret())
                                    .set("meetup_refresh_token", refresh_token.secret())
                                    .query(con),
                                None => pipe
                                    .set("meetup_access_token", token_res.access_token().secret())
                                    .del("meetup_refresh_token")
                                    .query(con),
                            },
                        );
                        if let Err(err) = res {
                            return future::err(err.into());
                        }
                        // Replace the meetup client
                        let new_blocking_meetup_client =
                            meetup_api::Client::new(token_res.access_token().secret());
                        *meetup_client.write() = Some(new_blocking_meetup_client);
                        *async_meetup_client.write() = Some(new_async_meetup_client);
                        future::ok(("Thanks for logging in :)", "").into())
                    })
            });
        Box::new(future)
    } else if let (&Method::GET, Some(captures)) = (method, LINK_URL_REGEX.captures(path)) {
        // The linking ID was stored in Redis when the linking link was created.
        // Check that it is still valid
        let linking_id = match captures.name("id") {
            Some(id) => id.as_str(),
            _ => return Box::new(future::ok(("Invalid Request", "").into())),
        };
        let redis_key = format!("meetup_linking:{}:discord_user", &linking_id);
        let (discord_id,): (Option<u64>,) = match redis::pipe()
            .expire(&redis_key, 600)
            .ignore()
            .get(&redis_key)
            .query(&mut *redis_connection_mutex.lock())
        {
            Ok(id) => id,
            Err(err) => return Box::new(future::err(err.into())),
        };
        if discord_id.is_none() {
            return Box::new(future::ok(
                (
                    strings::OAUTH2_LINK_EXPIRED_TITLE,
                    strings::OAUTH2_LINK_EXPIRED_CONTENT,
                )
                    .into(),
            ));
        }
        // TODO: check that this Discord ID is not linked yet before generating an authorization URL
        // Generate the authorization URL to which we'll redirect the user.
        // Two versions: One with just the "basic" scope to identify the user.
        // The second with the "rsvp" scope that will allow us to RSVP the user to events.
        let csrf_state = CsrfToken::new_random();
        let (_authorize_url_basic, csrf_state) = oauth2_link_client
            .clone()
            .set_redirect_url(RedirectUrl::new(
                Url::parse(format!("{}/link/{}/norsvp/redirect", BASE_URL, linking_id).as_str())
                    .unwrap(),
            ))
            .authorize_url(|| csrf_state)
            .add_scope(Scope::new("basic".to_string()))
            .url();
        let (authorize_url_rsvp, csrf_state) = oauth2_link_client
            .clone()
            .set_redirect_url(RedirectUrl::new(
                Url::parse(format!("{}/link/{}/rsvp/redirect", BASE_URL, linking_id).as_str())
                    .unwrap(),
            ))
            .authorize_url(|| csrf_state)
            .add_scope(Scope::new("basic".to_string()))
            .add_scope(Scope::new("rsvp".to_string()))
            .url();
        // Store the generated CSRF token so we can compare it to the one
        // returned by Meetup later
        let csrf_cookie = match generate_csrf_cookie(redis_connection_mutex, csrf_state.secret()) {
            Ok(csrf_cookie) => csrf_cookie,
            Err(err) => return Box::new(future::err(err.into())),
        };
        let linking_template = LinkingTemplate {
            authorize_url: authorize_url_rsvp.as_str(),
        };
        Box::new(future::result(
            linking_template
                .render()
                .map_err(Into::into)
                .and_then(|html_body| {
                    Response::builder()
                        .header(hyper::header::SET_COOKIE, csrf_cookie.to_string())
                        .body(html_body.into())
                        .map_err(Into::into)
                })
                .map(|response| HandlerResponse::Response(response)),
        ))
    } else if let (&Method::GET, Some(captures)) = (method, LINK_REDIRECT_URL_REGEX.captures(path))
    {
        // The linking ID was stored in Redis when the linking link was created.
        // Check that it is still valid
        let linking_id = match captures.name("id") {
            Some(id) => id.as_str(),
            _ => return Box::new(future::ok(("Invalid request", "").into())),
        };
        let with_rsvp_scope = match captures.name("type") {
            Some(r#type) => r#type.as_str() == "rsvp",
            _ => false,
        };
        let redis_key = format!("meetup_linking:{}:discord_user", &linking_id);
        // This is a one-time use link. Expire it now.
        let (discord_id, _): (Option<u64>, u32) = match redis::pipe()
            .get(&redis_key)
            .del(&redis_key)
            .query(&mut *redis_connection_mutex.lock())
        {
            Ok(id) => id,
            Err(err) => return Box::new(future::err(err.into())),
        };
        let discord_id = match discord_id {
            Some(id) => id,
            None => {
                return Box::new(future::ok(
                    (
                        strings::OAUTH2_LINK_EXPIRED_TITLE,
                        strings::OAUTH2_LINK_EXPIRED_CONTENT,
                    )
                        .into(),
                ))
            }
        };
        let full_uri = format!("{}{}", BASE_URL, &req.uri().to_string());
        let req_url = match Url::parse(&full_uri) {
            Ok(url) => url,
            Err(err) => return Box::new(future::err(err.into())),
        };
        let params: Vec<_> = req_url.query_pairs().collect();
        let code = params
            .iter()
            .find_map(|(key, value)| if key == "code" { Some(value) } else { None });
        let state = params
            .iter()
            .find_map(|(key, value)| if key == "state" { Some(value) } else { None });
        let error = params
            .iter()
            .find_map(|(key, value)| if key == "error" { Some(value) } else { None });
        if let Some(error) = error {
            if error == "access_denied" {
                // The user did not grant access
                // Give them the chance to do it again
                let linking_url =
                    match generate_meetup_linking_link(&redis_connection_mutex, discord_id) {
                        Err(err) => return Box::new(future::err(err)),
                        Ok(url) => url,
                    };
                return Box::new(future::ok(HandlerResponse::Message {
                    title: Cow::Borrowed("Linking Failure"),
                    content: None,
                    img_url: None,
                    safe_content: Some(Cow::Owned(strings::OAUTH2_AUTHORISATION_DENIED(
                        &linking_url,
                    ))),
                }));
            } else {
                // Some other error occured
                eprintln!("Received an OAuth2 error code from Meetup: {}", error);
                return Box::new(future::ok(("OAuth2 error", error.to_string()).into()));
            }
        }
        let (code, csrf_state) = match (code, state) {
            (Some(code), Some(state)) => (code, state),
            _ => return Box::new(future::ok(("Request parameters missing", "").into())),
        };
        // Compare the CSRF state that was returned by Meetup to the one
        // we have saved
        let csrf_is_valid =
            match check_csrf_cookie(redis_connection_mutex, req.headers(), &csrf_state) {
                Ok(is_valid) => is_valid,
                Err(err) => return Box::new(future::err(err.into())),
            };
        if !csrf_is_valid {
            return Box::new(future::ok(
                (
                    "CSRF check failed",
                    "Please go back to the first page, reload, and repeat the process",
                )
                    .into(),
            ));
        }
        // Exchange the code with a token.
        let code = AuthorizationCode::new(code.to_string());
        let redis_connection_mutex = redis_connection_mutex.clone();
        let future = oauth2_link_client
            .clone()
            .set_redirect_url(RedirectUrl::new(
                Url::parse(format!("{}{}", BASE_URL, path).as_str()).unwrap(),
            ))
            .exchange_code(code)
            .request_async(async_http_client)
            .map_err(|err| SimpleError::new(format!("RequestTokenError: {}", err)).into())
            .and_then(move |token_res| {
                // Get the user's Meetup ID
                let async_user_meetup_client =
                    meetup_api::AsyncClient::new(token_res.access_token().secret());
                async_user_meetup_client
                    .get_member_profile(None)
                    .from_err::<crate::BoxedError>()
                    .and_then(move |meetup_user| {
                        let meetup_user = match meetup_user {
                            Some(info) => info,
                            _ => {
                                return future::ok(("Could not find Meetup ID", "").into());
                            }
                        };
                        let redis_key_d2m = format!("discord_user:{}:meetup_user", discord_id);
                        let redis_key_m2d = format!("meetup_user:{}:discord_user", meetup_user.id);
                        // Check that the Discord ID has not been linked yet
                        let existing_meetup_id: RedisResult<Option<u64>> =
                            redis_connection_mutex.lock().get(&redis_key_d2m);
                        match existing_meetup_id {
                            Ok(Some(existing_meetup_id)) => {
                                if existing_meetup_id == meetup_user.id {
                                    return future::ok(
                                        (
                                            strings::OAUTH2_ALREADY_LINKED_SUCCESS_TITLE,
                                            strings::OAUTH2_ALREADY_LINKED_SUCCESS_CONTENT,
                                        )
                                            .into(),
                                    );
                                } else {
                                    return future::ok(
                                        (
                                            strings::OAUTH2_DISCORD_ALREADY_LINKED_FAILURE_TITLE,
                                            strings::OAUTH2_DISCORD_ALREADY_LINKED_FAILURE_CONTENT(
                                                &bot_name,
                                            ),
                                        )
                                            .into(),
                                    );
                                }
                            }
                            Err(err) => return future::err(err.into()),
                            _ => (),
                        }
                        // Check that the Meetup ID has not been linked to some other Discord ID yet
                        let existing_discord_id: RedisResult<Option<u64>> =
                            redis_connection_mutex.lock().get(&redis_key_m2d);
                        match existing_discord_id {
                            Ok(Some(_)) => {
                                return future::ok(
                                    (
                                        strings::OAUTH2_MEETUP_ALREADY_LINKED_FAILURE_TITLE,
                                        strings::OAUTH2_MEETUP_ALREADY_LINKED_FAILURE_CONTENT(
                                            &bot_name,
                                        ),
                                    )
                                        .into(),
                                );
                            }
                            Err(err) => return future::err(err.into()),
                            _ => (),
                        }
                        // Create the link between the Discord and the Meetup ID
                        let mut successful = false;
                        let res: RedisResult<()> = {
                            let mut redis_connection = redis_connection_mutex.lock();
                            // If the "rsvp" scope is part of the token result, store the tokens as well
                            if with_rsvp_scope {
                                if let Some(refresh_token) = token_res.refresh_token() {
                                    let redis_user_tokens_key =
                                        format!("meetup_user:{}:oauth2_tokens", meetup_user.id);
                                    let fields = &[
                                        ("access_token", token_res.access_token().secret()),
                                        ("refresh_token", refresh_token.secret()),
                                    ];
                                    let _: redis::RedisResult<()> = redis::pipe()
                                        .hset_multiple(&redis_user_tokens_key, fields)
                                        .query(&mut *redis_connection);
                                }
                            }
                            redis::transaction(
                                &mut *redis_connection,
                                &[&redis_key_d2m, &redis_key_m2d],
                                |con, pipe| {
                                    let linked_meetup_id: Option<u64> = con.get(&redis_key_d2m)?;
                                    let linked_discord_id: Option<u64> = con.get(&redis_key_m2d)?;
                                    if linked_meetup_id.is_some() || linked_discord_id.is_some() {
                                        // The meetup id was linked in the meantime, abort
                                        successful = false;
                                        // Execute empty transaction just to get out of the closure
                                        pipe.query(con)
                                    } else {
                                        pipe.sadd("meetup_users", meetup_user.id)
                                            .sadd("discord_users", discord_id)
                                            .set(&redis_key_d2m, meetup_user.id)
                                            .set(&redis_key_m2d, discord_id);
                                        successful = true;
                                        pipe.query(con)
                                    }
                                },
                            )
                        };
                        if let Err(err) = res {
                            return future::err(err.into());
                        }
                        if !successful {
                            return future::ok(
                                (
                                    "Linking Failure",
                                    "Could not assign meetup id (timing error)",
                                )
                                    .into(),
                            );
                        }
                        if let Some(photo) = meetup_user.photo {
                            future::ok(
                                HandlerResponse::Message {
                                    title: Cow::Borrowed(strings::OAUTH2_LINKING_SUCCESS_TITLE),
                                    content: Some(Cow::Owned(
                                        strings::OAUTH2_LINKING_SUCCESS_CONTENT(&meetup_user.name),
                                    )),
                                    safe_content: None,
                                    img_url: Some(Cow::Owned(photo.thumb_link)),
                                }
                                .into(),
                            )
                        } else {
                            future::ok(
                                (
                                    strings::OAUTH2_LINKING_SUCCESS_TITLE,
                                    strings::OAUTH2_LINKING_SUCCESS_CONTENT(&meetup_user.name),
                                )
                                    .into(),
                            )
                        }
                    })
            });
        Box::new(future)
    } else {
        Box::new(future::ok(("Unknown route", "").into()))
    }
}

pub struct OAuth2Consumer {
    authorization_client: Arc<BasicClient>,
    link_client: Arc<BasicClient>,
}

impl OAuth2Consumer {
    pub fn new(meetup_client_id: String, meetup_client_secret: String) -> Self {
        let meetup_client_id = ClientId::new(meetup_client_id);
        let meetup_client_secret = ClientSecret::new(meetup_client_secret);
        let auth_url =
            AuthUrl::new(Url::parse("https://secure.meetup.com/oauth2/authorize").unwrap());
        let token_url =
            TokenUrl::new(Url::parse("https://secure.meetup.com/oauth2/access").unwrap());

        // Set up the config for the Github OAuth2 process.
        let authorization_client = BasicClient::new(
            meetup_client_id,
            Some(meetup_client_secret),
            auth_url,
            Some(token_url),
        )
        .set_auth_type(oauth2::AuthType::RequestBody)
        .set_redirect_url(RedirectUrl::new(
            Url::parse(format!("{}/authorize/redirect", BASE_URL).as_str()).unwrap(),
        ));
        let link_client = authorization_client
            .clone()
            .set_redirect_url(RedirectUrl::new(
                Url::parse(format!("{}/link/redirect", BASE_URL).as_str()).unwrap(),
            ));
        let authorization_client = Arc::new(authorization_client);
        let link_client = Arc::new(link_client);

        OAuth2Consumer {
            authorization_client: authorization_client,
            link_client: link_client,
        }
    }

    pub fn create_auth_server(
        &self,
        addr: std::net::SocketAddr,
        redis_connection: redis::Connection,
        discord_http: Arc<serenity::CacheAndHttp>,
        meetup_client: Arc<RwLock<Option<meetup_api::Client>>>,
        async_meetup_client: Arc<RwLock<Option<meetup_api::AsyncClient>>>,
        bot_name: String,
    ) -> impl Future<Item = (), Error = ()> + Send + 'static {
        let redis_connection_mutex = Arc::new(Mutex::new(redis_connection));
        // And a MakeService to handle each connection...
        let make_meetup_service = {
            let authorization_client = self.authorization_client.clone();
            let link_client = self.link_client.clone();
            let redis_connection_mutex = redis_connection_mutex.clone();
            let meetup_client = meetup_client.clone();
            let async_meetup_client = async_meetup_client.clone();
            move || {
                let authorization_client = authorization_client.clone();
                let link_client = link_client.clone();
                let redis_connection_mutex = redis_connection_mutex.clone();
                let discord_http = discord_http.clone();
                let meetup_client = meetup_client.clone();
                let async_meetup_client = async_meetup_client.clone();
                let bot_name = bot_name.clone();
                service_fn(move |req| {
                    meetup_http_handler(
                        &redis_connection_mutex,
                        &authorization_client,
                        &link_client,
                        &discord_http,
                        &meetup_client,
                        &async_meetup_client,
                        req,
                        bot_name.clone(),
                    )
                    .and_then(|handler_response| match handler_response {
                        HandlerResponse::Response(response) => future::ok(response),
                        HandlerResponse::Message {
                            title,
                            content,
                            safe_content,
                            img_url,
                        } => {
                            let rendered_template = LinkingMessageTemplate {
                                title: &title,
                                content: content.as_ref().map(Cow::as_ref),
                                safe_content: safe_content.as_ref().map(Cow::as_ref),
                                img_url: img_url.as_ref().map(Cow::as_ref),
                            }
                            .render();
                            future::result(
                                rendered_template
                                    .map_err(Into::into)
                                    .map(|html_body| Response::new(html_body.into())),
                            )
                        }
                    })
                    // Catch all errors and don't let the details of internal server erros leak
                    // TODO: replace HandlerError with the never type "!" once it
                    // is available on stable, since this function will never return an error
                    .or_else(
                        |err| -> Result<Response<Body>, crate::BoxedError> {
                            eprintln!("Error in meetup_authorize: {}", err);
                            let message_template = LinkingMessageTemplate {
                                title: strings::INTERNAL_SERVER_ERROR,
                                content: None,
                                safe_content: None,
                                img_url: None,
                            };
                            Ok(message_template
                                .render()
                                .map(|html_body| Response::new(html_body.into()))
                                .unwrap_or_else(|_| {
                                    Response::new(strings::INTERNAL_SERVER_ERROR.into())
                                }))
                        },
                    )
                })
            }
        };
        let server = Server::bind(&addr).serve(make_meetup_service).map_err(|e| {
            eprintln!("server error: {}", e);
        });

        server
    }

    // Refreshes the authorization token
    pub fn token_refresh_task(
        &self,
        mut redis_connection: redis::Connection,
        meetup_client: Arc<RwLock<Option<meetup_api::Client>>>,
    ) -> impl FnMut(&mut white_rabbit::Context) -> white_rabbit::DateResult + Send + Sync + 'static
    {
        let oauth2_client = self.authorization_client.clone();
        let refresh_meetup_access_token_task =
            move |_context: &mut white_rabbit::Context| -> white_rabbit::DateResult {
                // Try to get the refresh token from Redis
                let refresh_token: String = match redis_connection.get("meetup_refresh_token") {
                    Ok(refresh_token_option) => match refresh_token_option {
                        Some(refresh_token) => refresh_token,
                        None => {
                            eprintln!("Could not refresh the Meetup access token since there is no refresh token available");
                            // Try to refresh again in an hour
                            return white_rabbit::DateResult::Repeat(
                                white_rabbit::Utc::now() + white_rabbit::Duration::hours(1),
                            );
                        }
                    },
                    Err(err) => {
                        eprintln!(
                            "Could not refresh the Meetup access token. Redis error: {}",
                            err
                        );
                        // Try to refresh again in an hour
                        return white_rabbit::DateResult::Repeat(
                            white_rabbit::Utc::now() + white_rabbit::Duration::hours(1),
                        );
                    }
                };
                // Try to exchange the refresh token for fresh access and refresh tokens
                let refresh_token = oauth2::RefreshToken::new(refresh_token);
                let refresh_token_response = match oauth2_client
                    .exchange_refresh_token(&refresh_token)
                    .request(oauth2::curl::http_client)
                {
                    Ok(refresh_token_response) => refresh_token_response,
                    Err(err) => {
                        eprintln!(
                            "Could not refresh the Meetup access token. OAuth2 error: {}",
                            err
                        );
                        // Try to refresh again in an hour
                        return white_rabbit::DateResult::Repeat(
                            white_rabbit::Utc::now() + white_rabbit::Duration::hours(1),
                        );
                    }
                };
                let (new_access_token, new_refresh_token) = match refresh_token_response
                    .refresh_token()
                {
                    Some(refresh_token) => (refresh_token_response.access_token(), refresh_token),
                    None => {
                        eprintln!("Error during Meetup access token refresh. Meetup did not return a new refresh token");
                        // Try to refresh again in an hour
                        return white_rabbit::DateResult::Repeat(
                            white_rabbit::Utc::now() + white_rabbit::Duration::hours(1),
                        );
                    }
                };
                *meetup_client.write() = Some(meetup_api::Client::new(new_access_token.secret()));
                // Store the new tokens in Redis
                let res: RedisResult<()> = redis_connection.set_multiple(&[
                    ("meetup_access_token", new_access_token.secret()),
                    ("meetup_refresh_token", new_refresh_token.secret()),
                ]);
                if let Err(err) = res {
                    eprintln!("Error storing new Meetup tokens in Redis: {}", err);
                }
                // Refresh the access token in two days from now
                let next_refresh = white_rabbit::Utc::now() + white_rabbit::Duration::days(2);
                // Store refresh date in Redis, ignore failures
                let _: redis::RedisResult<()> = redis_connection.set(
                    "meetup_access_token_refresh_time",
                    next_refresh.to_rfc3339(),
                );
                // Re-schedule this task
                white_rabbit::DateResult::Repeat(next_refresh)
            };
        refresh_meetup_access_token_task
    }
}
