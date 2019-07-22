//!
//! This example showcases the Github OAuth2 process for requesting access to the user's public repos and
//! email address.
//!
//! Before running it, you'll need to generate your own Github OAuth2 credentials.
//!
//! In order to run the example call:
//!
//! ```sh
//! GITHUB_CLIENT_ID=xxx GITHUB_CLIENT_SECRET=yyy cargo run --example github
//! ```
//!
//! ...and follow the instructions.
//!

use hyper::rt::Future;
use hyper::service::service_fn_ok;
use hyper::{Body, Method, Request, Response, Server};
use oauth2::basic::BasicClient;
use oauth2::curl::http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope,
    TokenResponse, TokenUrl,
};
use redis::{Commands, RedisResult};
use serenity::prelude::Mutex;
use std::sync::Arc;
use url::Url;

const BASE_URL: &'static str = "http://bot.8na.de";

fn meetup_auth(
    oauth_client: &BasicClient,
    csrf_token: &Arc<Mutex<Option<CsrfToken>>>,
    req: Request<Body>,
) -> Response<Body> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => {
            // Generate the authorization URL to which we'll redirect the user.
            let (authorize_url, csrf_state) = oauth_client
                .authorize_url(CsrfToken::new_random)
                // This example is requesting access to the user's public repos and email.
                .add_scope(Scope::new("ageless".to_string()))
                .add_scope(Scope::new("basic".to_string()))
                .add_scope(Scope::new("event_management".to_string()))
                .url();
            // Store the generated CSRF token so we can compare it to the one
            // returned by Meetup later
            *csrf_token.lock() = Some(csrf_state);
            let html_body = format!("<a href=\"{}\">Login with Meetup</a>", authorize_url);
            Response::new(html_body.into())
        }
        (&Method::GET, "/redirect") => {
            let full_uri = format!("{}{}", BASE_URL, &req.uri().to_string());
            let req_url = Url::parse(&full_uri).unwrap();
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
                return Response::new(format!("OAuth error: {}", error).into());
            }
            match (code, state) {
                (Some(code), Some(state)) => {
                    if let Some(ref csrf_state) = *csrf_token.lock() {
                        // Compare the CSRF state that was returned by Meetup to the one
                        // we have saved
                        if csrf_state.secret() == state {
                            // Exchange the code with a token.
                            let code = AuthorizationCode::new(code.to_string());
                            let token_res = oauth_client.exchange_code(code).request(http_client);
                            match token_res {
                                Ok(token_res) => {
                                    println!("Access token: {}", token_res.access_token().secret());
                                    println!(
                                        "Refresh token: {:?}",
                                        token_res.refresh_token().map(|t| t.secret())
                                    );
                                    return Response::new("Thanks for logging in :)".into());
                                }
                                Err(err) => {
                                    eprintln!("Request token error: {:?}", err);
                                    return Response::new(
                                        "Could not exchange code for an access token".into(),
                                    );
                                }
                            };
                        } else {
                            return Response::new(
                                format!(
                                    "CSRF tokens do not match: {} vs {}",
                                    csrf_state.secret(),
                                    state
                                )
                                .into(),
                            );
                        }
                    } else {
                        return Response::new("No CSRF token on server".into());
                    }
                }
                _ => return Response::new("Request parameters missing".into()),
            };
        }
        _ => Response::new("Unknown route".into()),
    }
}

pub struct OAuth2Consumer {
    client: Arc<BasicClient>,
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
        let client = BasicClient::new(
            meetup_client_id,
            Some(meetup_client_secret),
            auth_url,
            Some(token_url),
        )
        .set_auth_type(oauth2::AuthType::RequestBody)
        // This example will be running its own server at localhost:8080.
        // See below for the server implementation.
        .set_redirect_url(RedirectUrl::new(
            Url::parse(format!("{}/redirect", BASE_URL).as_str()).unwrap(),
        ));
        let client = Arc::new(client);

        OAuth2Consumer { client: client }
    }

    pub fn create_auth_server(
        &self,
        addr: std::net::SocketAddr,
        _redis_client: &redis::Client,
    ) -> impl Future<Item = (), Error = ()> + Send + 'static {
        let csrf_token = Arc::new(Mutex::new(None));
        // And a MakeService to handle each connection...
        let client = self.client.clone();
        let make_service = move || {
            let client = client.clone();
            let csrf_token = csrf_token.clone();
            service_fn_ok(move |req| meetup_auth(&client, &csrf_token, req))
        };
        let server = Server::bind(&addr).serve(make_service).map_err(|e| {
            eprintln!("server error: {}", e);
        });

        server
    }

    pub fn token_refresh_task(
        &self,
        mut redis_connection: redis::Connection,
        meetup_client: Arc<Mutex<Option<crate::meetup_api::Client>>>,
    ) -> impl FnMut(&mut white_rabbit::Context) -> white_rabbit::DateResult + Send + Sync + 'static
    {
        let oauth2_client = self.client.clone();
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

    pub fn client(&self) -> &Arc<BasicClient> {
        &self.client
    }
}
