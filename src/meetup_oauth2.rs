use cookie::Cookie;
use hyper::rt::Future;
use hyper::service::service_fn_ok;
use hyper::{Body, Method, Request, Response, Server};
use lazy_static::lazy_static;
use oauth2::basic::BasicClient;
use oauth2::curl::http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope,
    TokenResponse, TokenUrl,
};
use rand::Rng;
use redis::{Commands, PipelineCommands, RedisResult};
use serenity::prelude::Mutex;
use simple_error::SimpleError;
use std::sync::Arc;
use url::Url;

const DOMAIN: &'static str = "bot.swissrpg.ch";
const BASE_URL: &'static str = "https://bot.swissrpg.ch";
lazy_static! {
    static ref LINK_URL_REGEX: regex::Regex =
        regex::Regex::new(r"^/link/(?P<id>[a-zA-Z0-9\-_]+)$").unwrap();
    static ref LINK_REDIRECT_URL_REGEX: regex::Regex =
        regex::Regex::new(r"^/link/(?P<id>[a-zA-Z0-9\-_]+)/redirect$").unwrap();
}

fn new_random_id(num_bytes: u32) -> String {
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

fn meetup_http_handler(
    redis_connection_mutex: &Mutex<redis::Connection>,
    oauth2_authorization_client: &BasicClient,
    oauth2_link_client: &BasicClient,
    _discord_http: &serenity::CacheAndHttp,
    meetup_client: &Mutex<Option<crate::meetup_api::Client>>,
    req: Request<Body>,
) -> crate::Result<Response<Body>> {
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
        let csrf_cookie = generate_csrf_cookie(redis_connection_mutex, csrf_state.secret())?;
        let html_body = format!("<a href=\"{}\">Login with Meetup</a>", authorize_url);
        Ok(Response::builder()
            .header(hyper::header::SET_COOKIE, csrf_cookie.to_string())
            .body(html_body.into())?)
    } else if let (&Method::GET, "/authorize/redirect") = (method, path) {
        let full_uri = format!("{}{}", BASE_URL, &req.uri().to_string());
        let req_url = Url::parse(&full_uri)?;
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
            return Ok(Response::new(format!("OAuth error: {}", error).into()));
        }
        match (code, state) {
            (Some(code), Some(csrf_state)) => {
                // Compare the CSRF state that was returned by Meetup to the one
                // we have saved
                let csrf_is_valid =
                    check_csrf_cookie(redis_connection_mutex, req.headers(), &csrf_state)?;
                if !csrf_is_valid {
                    return Ok(Response::new(
                                "CSRF check failed. Please go back to the first page, reload, and repeat the process.".into()
                            ));
                }
                // Exchange the code with a token.
                let code = AuthorizationCode::new(code.to_string());
                let token_res = oauth2_authorization_client
                    .exchange_code(code)
                    .request(http_client)
                    .map_err(|err| SimpleError::new(format!("RequestTokenError: {}", err)))?;
                // Check that this token belongs to an organizer
                let new_meetup_client =
                    crate::meetup_api::Client::new(token_res.access_token().secret());
                let is_organizer = match new_meetup_client.get_user_info()? {
                    Some(crate::meetup_api::UserInfo {
                        role: Some(_role), ..
                    }) => {
                        // TODO: check role
                        false
                    }
                    _ => false,
                };
                if !is_organizer {
                    return Ok(Response::new("Only organizers can log in".into()));
                }
                // Store the new access and refresh tokens in Redis
                let _: () = redis::transaction(
                    &mut *redis_connection_mutex.lock(),
                    &["meetup_access_token", "meetup_refresh_token"],
                    |con, pipe| match token_res.refresh_token() {
                        Some(refresh_token) => pipe
                            .set("meetup_access_token", token_res.access_token().secret())
                            .set("meetup_refresh_token", refresh_token.secret())
                            .ignore()
                            .query(con),
                        None => pipe
                            .set("meetup_access_token", token_res.access_token().secret())
                            .del("meetup_refresh_token")
                            .ignore()
                            .query(con),
                    },
                )?;
                // Replace the meetup client
                *meetup_client.lock() = Some(new_meetup_client);
                return Ok(Response::new("Thanks for logging in :)".into()));
            }
            _ => return Ok(Response::new("Request parameters missing".into())),
        };
    } else if let (&Method::GET, Some(captures)) = (method, LINK_URL_REGEX.captures(path)) {
        // The linking ID was stored in Redis when the linking link was created.
        // Check that it is still valid
        let linking_id = match captures.name("id") {
            Some(id) => id.as_str(),
            _ => return Ok(Response::new("Invalid request".into())),
        };
        let redis_key = format!("meetup_linking:{}:discord_user", &linking_id);
        let discord_id: Option<String> = redis::pipe()
            .expire(&redis_key, 600)
            .ignore()
            .get(&redis_key)
            .query(&mut *redis_connection_mutex.lock())?;
        if discord_id.is_none() {
            return Ok(Response::new("This link seems to have expired. Get a new link from the bot with the \"link meetup\" command".into()));
        }
        // Generate the authorization URL to which we'll redirect the user.
        let (authorize_url, csrf_state) = oauth2_link_client
            .clone()
            .set_redirect_url(RedirectUrl::new(
                Url::parse(format!("{}/link/{}/redirect", BASE_URL, linking_id).as_str()).unwrap(),
            ))
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new("ageless".to_string()))
            .add_scope(Scope::new("basic".to_string()))
            .url();
        // Store the generated CSRF token so we can compare it to the one
        // returned by Meetup later
        let csrf_cookie = generate_csrf_cookie(redis_connection_mutex, csrf_state.secret())?;
        let html_body = format!("<a href=\"{}\">Link with Meetup</a>", authorize_url);
        Ok(Response::builder()
            .header(hyper::header::SET_COOKIE, csrf_cookie.to_string())
            .body(html_body.into())?)
    } else if let (&Method::GET, Some(captures)) = (method, LINK_REDIRECT_URL_REGEX.captures(path))
    {
        // The linking ID was stored in Redis when the linking link was created.
        // Check that it is still valid
        let linking_id = match captures.name("id") {
            Some(id) => id.as_str(),
            _ => return Ok(Response::new("Invalid request".into())),
        };
        let redis_key = format!("meetup_linking:{}:discord_user", &linking_id);
        let discord_id: Option<String> = redis::pipe()
            .expire(&redis_key, 600)
            .ignore()
            .get(&redis_key)
            .query(&mut *redis_connection_mutex.lock())?;
        let _discord_id = match discord_id {
            Some(id) => id,
            None => return Ok(Response::new("This link seems to have expired. Get a new link from the bot with the \"link meetup\" command".into()))
        };
        let full_uri = format!("{}{}", BASE_URL, &req.uri().to_string());
        let req_url = Url::parse(&full_uri)?;
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
            return Ok(Response::new(format!("OAuth error: {}", error).into()));
        }
        match (code, state) {
            (Some(code), Some(csrf_state)) => {
                // Compare the CSRF state that was returned by Meetup to the one
                // we have saved
                let csrf_is_valid =
                    check_csrf_cookie(redis_connection_mutex, req.headers(), &csrf_state)?;
                if !csrf_is_valid {
                    return Ok(Response::new(
                                "CSRF check failed. Please go back to the first page, reload and repeat the process.".into()
                            ));
                }
                // Exchange the code with a token.
                let code = AuthorizationCode::new(code.to_string());
                let token_res = oauth2_authorization_client
                    .exchange_code(code)
                    .request(http_client)
                    .map_err(|err| SimpleError::new(format!("RequestTokenError: {}", err)))?;
                // TODO: check that this token does belong to an organizer,
                // store it in Redis and replace the meetup_api::Client
                println!("Access token: {}", token_res.access_token().secret());
                println!(
                    "Refresh token: {:?}",
                    token_res.refresh_token().map(|t| t.secret())
                );
                return Ok(Response::new("Thanks for logging in :)".into()));
            }
            _ => return Ok(Response::new("Request parameters missing".into())),
        };
    } else {
        Ok(Response::new("Unknown route".into()))
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
        meetup_client: Arc<Mutex<Option<crate::meetup_api::Client>>>,
    ) -> impl Future<Item = (), Error = ()> + Send + 'static {
        let redis_connection_mutex = Arc::new(Mutex::new(redis_connection));
        // And a MakeService to handle each connection...
        let make_meetup_service = {
            let authorization_client = self.authorization_client.clone();
            let link_client = self.link_client.clone();
            let redis_connection_mutex = redis_connection_mutex.clone();
            let meetup_client = meetup_client.clone();
            move || {
                let authorization_client = authorization_client.clone();
                let link_client = link_client.clone();
                let redis_connection_mutex = redis_connection_mutex.clone();
                let discord_http = discord_http.clone();
                let meetup_client = meetup_client.clone();
                service_fn_ok(move |req| {
                    match meetup_http_handler(
                        &redis_connection_mutex,
                        &authorization_client,
                        &link_client,
                        &discord_http,
                        &meetup_client,
                        req,
                    ) {
                        Ok(response) => response,
                        Err(err) => {
                            eprintln!("Error in meetup_authorize: {}", err);
                            Response::new("Internal Server Error".into())
                        }
                    }
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
        meetup_client: Arc<Mutex<Option<crate::meetup_api::Client>>>,
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
                // Try to exchange the refresh token for fresh access and refresh tokens.
                // Lock the Meetup client in the meantime, such that other code does not
                // try to use a stale access token
                let mut meetup_client_lock = meetup_client.lock();
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
                *meetup_client_lock =
                    Some(crate::meetup_api::Client::new(new_access_token.secret()));
                drop(meetup_client_lock);
                // Store the new tokens in Redis
                let res: RedisResult<()> = redis_connection.set_multiple(&[
                    ("meetup_access_token", new_access_token.secret()),
                    ("meetup_refresh_token", new_refresh_token.secret()),
                ]);
                if let Err(err) = res {
                    eprintln!("Error storing new Meetup tokens in Redis: {}", err);
                }
                // Refresh the access token in a week from now
                let next_refresh = white_rabbit::Utc::now() + white_rabbit::Duration::weeks(1);
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
