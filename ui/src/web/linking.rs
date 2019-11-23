use askama::Template;
use cookie::Cookie;
use futures_util::{compat::Future01CompatExt, lock::Mutex, TryFutureExt};
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server,
};
use oauth2::{basic::BasicClient, AuthorizationCode, CsrfToken, RedirectUrl, Scope, TokenResponse};
use redis::PipelineCommands;
use std::{
    borrow::Cow,
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use url::Url;

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

async fn generate_csrf_cookie(
    redis_connection: redis::aio::Connection,
    csrf_state: &str,
) -> Result<(redis::aio::Connection, Cookie<'static>), lib::meetup::Error> {
    let random_csrf_user_id = lib::new_random_id(16);
    let redis_csrf_key = format!("csrf:{}", &random_csrf_user_id);
    let mut pipe = redis::pipe();
    pipe.set_ex(&redis_csrf_key, csrf_state, 3600);
    let (redis_connection, _): (_, ()) = pipe.query_async(redis_connection).compat().await?;
    Ok((
        redis_connection,
        Cookie::build("csrf_user_id", random_csrf_user_id)
            .domain(lib::urls::DOMAIN)
            .http_only(true)
            .same_site(cookie::SameSite::Lax)
            .max_age(time::Duration::hours(1))
            .finish(),
    ))
}

async fn check_csrf_cookie(
    redis_connection: redis::aio::Connection,
    headers: &hyper::HeaderMap<hyper::header::HeaderValue>,
    csrf_state: &str,
) -> Result<(redis::aio::Connection, bool), lib::meetup::Error> {
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
        None => return Ok((redis_connection, false)),
        Some(csrf_user_id_cookie) => csrf_user_id_cookie,
    };
    let redis_csrf_key = format!("csrf:{}", csrf_user_id_cookie.value());
    let mut pipe = redis::pipe();
    pipe.get(&redis_csrf_key);
    let (redis_connection, (csrf_stored_state,)): (_, (Option<String>,)) =
        pipe.query_async(redis_connection).compat().await?;
    let csrf_stored_state: String = match csrf_stored_state {
        None => return Ok((redis_connection, false)),
        Some(csrf_stored_state) => csrf_stored_state,
    };
    Ok((redis_connection, csrf_state == csrf_stored_state))
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
    pub fn from_template(template: impl Template) -> Result<Self, lib::BoxedError> {
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

async fn meetup_http_handler(
    redis_connection: redis::aio::Connection,
    oauth2_authorization_client: &BasicClient,
    oauth2_link_client: &BasicClient,
    _discord_http: &serenity::CacheAndHttp,
    async_meetup_client: &Arc<Mutex<Option<Arc<lib::meetup::api::AsyncClient>>>>,
    req: Request<Body>,
    bot_name: String,
) -> Result<HandlerResponse, lib::meetup::Error> {
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
        let (_redis_connection, csrf_cookie) =
            generate_csrf_cookie(redis_connection, csrf_state.secret()).await?;
        let html_body = format!("<a href=\"{}\">Login with Meetup</a>", authorize_url);
        Response::builder()
            .header(hyper::header::SET_COOKIE, csrf_cookie.to_string())
            .body(html_body.into())
            .map_err(|err| err.into())
            .map(|response| HandlerResponse::Response(response))
    } else if let (&Method::GET, "/authorize/redirect") = (method, path) {
        let full_uri = format!("{}{}", lib::urls::BASE_URL, &req.uri().to_string());
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
            return Ok(("OAuth2 error", error.to_string()).into());
        }
        let (code, csrf_state) = match (code, state) {
            (Some(code), Some(state)) => (code, state),
            _ => return Ok(("Request parameters missing", "").into()),
        };
        // Compare the CSRF state that was returned by Meetup to the one
        // we have saved
        let (redis_connection, csrf_is_valid) =
            check_csrf_cookie(redis_connection, req.headers(), &csrf_state).await?;
        if !csrf_is_valid {
            return Ok((
                "CSRF check failed",
                "Please go back to the first page, reload, and repeat the process",
            )
                .into());
        }
        // Exchange the code with a token.
        let code = AuthorizationCode::new(code.to_string());
        let async_meetup_client = async_meetup_client.clone();
        let token_res = oauth2_authorization_client
            .exchange_code(code)
            .request_async(lib::meetup::oauth2_async_http_client::async_http_client)
            .compat()
            .await?;
        // Check that this token belongs to an organizer of all our Meetup groups
        let new_async_meetup_client =
            lib::meetup::api::AsyncClient::new(token_res.access_token().secret());
        let user_profiles =
            lib::meetup::util::get_group_profiles(new_async_meetup_client.clone()).await?;
        let is_organizer = user_profiles.iter().all(|profile| {
            let is_organizer = match profile {
                Some(lib::meetup::api::User {
                    group_profile:
                        Some(lib::meetup::api::GroupProfile {
                            status: lib::meetup::api::UserStatus::Active,
                            role: Some(role),
                        }),
                    ..
                }) => {
                    *role == lib::meetup::api::LeadershipRole::Organizer
                        || *role == lib::meetup::api::LeadershipRole::Coorganizer
                        || *role == lib::meetup::api::LeadershipRole::AssistantOrganizer
                }
                _ => false,
            };
            is_organizer
        });
        if !is_organizer {
            return Ok(("Only the organizer can log in", "").into());
        }
        // Store the new access and refresh tokens in Redis
        let transaction_fn = {
            let token_res = &token_res;
            move |con: redis::aio::Connection, mut pipe: redis::Pipeline| {
                async move {
                    match token_res.refresh_token() {
                        Some(refresh_token) => {
                            pipe.set("meetup_access_token", token_res.access_token().secret())
                                .set("meetup_refresh_token", refresh_token.secret());
                            pipe.query_async(con).compat().await
                        }
                        None => {
                            // Don't delete the (possibly existing) old refresh token
                            pipe.set("meetup_access_token", token_res.access_token().secret());
                            pipe.query_async(con).compat().await
                        }
                    }
                }
            }
        };
        let (_redis_connection, _): (_, ()) = lib::redis::async_redis_transaction(
            redis_connection,
            &["meetup_access_token", "meetup_refresh_token"],
            transaction_fn,
        )
        .await?;
        // Replace the meetup client
        *async_meetup_client.lock().await = Some(Arc::new(new_async_meetup_client));
        Ok(("Thanks for logging in :)", "").into())
    } else if let (&Method::GET, Some(captures)) =
        (method, lib::urls::LINK_URL_REGEX.captures(path))
    {
        // The linking ID was stored in Redis when the linking link was created.
        // Check that it is still valid
        let linking_id = match captures.name("id") {
            Some(id) => id.as_str(),
            _ => return Ok(("Invalid Request", "").into()),
        };
        let redis_key = format!("meetup_linking:{}:discord_user", &linking_id);
        let mut pipe = redis::pipe();
        pipe.expire(&redis_key, 600).ignore().get(&redis_key);
        let (redis_connection, (discord_id,)): (_, (Option<u64>,)) =
            pipe.query_async(redis_connection).compat().await?;
        if discord_id.is_none() {
            return Ok((
                lib::strings::OAUTH2_LINK_EXPIRED_TITLE,
                lib::strings::OAUTH2_LINK_EXPIRED_CONTENT,
            )
                .into());
        }
        // TODO: check that this Discord ID is not linked yet before generating an authorization URL
        // Generate the authorization URL to which we'll redirect the user.
        // Two versions: One with just the "basic" scope to identify the user.
        // The second with the "rsvp" scope that will allow us to RSVP the user to events.
        let csrf_state = CsrfToken::new_random();
        let (_authorize_url_basic, csrf_state) = oauth2_link_client
            .clone()
            .set_redirect_url(RedirectUrl::new(
                Url::parse(
                    format!(
                        "{}/link/{}/norsvp/redirect",
                        lib::urls::BASE_URL,
                        linking_id
                    )
                    .as_str(),
                )
                .unwrap(),
            ))
            .authorize_url(|| csrf_state)
            .add_scope(Scope::new("basic".to_string()))
            .url();
        let (authorize_url_rsvp, csrf_state) = oauth2_link_client
            .clone()
            .set_redirect_url(RedirectUrl::new(
                Url::parse(
                    format!("{}/link/{}/rsvp/redirect", lib::urls::BASE_URL, linking_id).as_str(),
                )
                .unwrap(),
            ))
            .authorize_url(|| csrf_state)
            .add_scope(Scope::new("basic".to_string()))
            .add_scope(Scope::new("rsvp".to_string()))
            .url();
        // Store the generated CSRF token so we can compare it to the one
        // returned by Meetup later
        let (_redis_connection, csrf_cookie) =
            generate_csrf_cookie(redis_connection, csrf_state.secret()).await?;
        let linking_template = LinkingTemplate {
            authorize_url: authorize_url_rsvp.as_str(),
        };
        let html_body = linking_template.render()?;
        let response = Response::builder()
            .header(hyper::header::SET_COOKIE, csrf_cookie.to_string())
            .body(html_body.into())?;
        Ok(HandlerResponse::Response(response))
    } else if let (&Method::GET, Some(captures)) =
        (method, lib::urls::LINK_REDIRECT_URL_REGEX.captures(path))
    {
        // The linking ID was stored in Redis when the linking link was created.
        // Check that it is still valid
        let linking_id = match captures.name("id") {
            Some(id) => id.as_str(),
            _ => return Ok(("Invalid request", "").into()),
        };
        let with_rsvp_scope = match captures.name("type") {
            Some(r#type) => r#type.as_str() == "rsvp",
            _ => false,
        };
        let redis_key = format!("meetup_linking:{}:discord_user", &linking_id);
        // This is a one-time use link. Expire it now.
        let mut pipe = redis::pipe();
        pipe.get(&redis_key).del(&redis_key);
        let (redis_connection, (discord_id, _)): (_, (Option<u64>, u32)) =
            pipe.query_async(redis_connection).compat().await?;
        let discord_id = match discord_id {
            Some(id) => id,
            None => {
                return Ok((
                    lib::strings::OAUTH2_LINK_EXPIRED_TITLE,
                    lib::strings::OAUTH2_LINK_EXPIRED_CONTENT,
                )
                    .into())
            }
        };
        let full_uri = format!("{}{}", lib::urls::BASE_URL, &req.uri().to_string());
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
            if error == "access_denied" {
                // The user did not grant access
                // Give them the chance to do it again
                let (_redis_connection, linking_url) =
                    lib::meetup::oauth2::generate_meetup_linking_link(redis_connection, discord_id)
                        .await?;
                return Ok(HandlerResponse::Message {
                    title: Cow::Borrowed("Linking Failure"),
                    content: None,
                    img_url: None,
                    safe_content: Some(Cow::Owned(lib::strings::OAUTH2_AUTHORISATION_DENIED(
                        &linking_url,
                    ))),
                });
            } else {
                // Some other error occured
                eprintln!("Received an OAuth2 error code from Meetup: {}", error);
                return Ok(("OAuth2 error", error.to_string()).into());
            }
        }
        let (code, csrf_state) = match (code, state) {
            (Some(code), Some(state)) => (code, state),
            _ => return Ok(("Request parameters missing", "").into()),
        };
        // Compare the CSRF state that was returned by Meetup to the one
        // we have saved
        let (redis_connection, csrf_is_valid) =
            check_csrf_cookie(redis_connection, req.headers(), &csrf_state).await?;
        if !csrf_is_valid {
            return Ok((
                "CSRF check failed",
                "Please go back to the first page, reload, and repeat the process",
            )
                .into());
        }
        // Exchange the code with a token.
        let code = AuthorizationCode::new(code.to_string());
        let redirect_url = RedirectUrl::new(
            Url::parse(format!("{}{}", lib::urls::BASE_URL, path).as_str()).unwrap(),
        );
        let token_res = oauth2_link_client
            .clone()
            .set_redirect_url(redirect_url)
            .exchange_code(code)
            .request_async(lib::meetup::oauth2_async_http_client::async_http_client)
            .compat()
            .await?;
        // Get the user's Meetup ID
        let async_user_meetup_client =
            lib::meetup::api::AsyncClient::new(token_res.access_token().secret());
        let meetup_user = async_user_meetup_client.get_member_profile(None).await?;
        let meetup_user = match meetup_user {
            Some(info) => info,
            _ => {
                return Ok(("Could not find Meetup ID", "").into());
            }
        };
        let redis_key_d2m = format!("discord_user:{}:meetup_user", discord_id);
        let redis_key_m2d = format!("meetup_user:{}:discord_user", meetup_user.id);
        // Check that the Discord ID has not been linked yet
        let (redis_connection, existing_meetup_id): (_, Option<u64>) = redis::cmd("GET")
            .arg(&redis_key_d2m)
            .query_async(redis_connection)
            .compat()
            .await?;
        match existing_meetup_id {
            Some(existing_meetup_id) => {
                if existing_meetup_id == meetup_user.id {
                    return Ok((
                        lib::strings::OAUTH2_ALREADY_LINKED_SUCCESS_TITLE,
                        lib::strings::OAUTH2_ALREADY_LINKED_SUCCESS_CONTENT,
                    )
                        .into());
                } else {
                    return Ok((
                        lib::strings::OAUTH2_DISCORD_ALREADY_LINKED_FAILURE_TITLE,
                        lib::strings::OAUTH2_DISCORD_ALREADY_LINKED_FAILURE_CONTENT(&bot_name),
                    )
                        .into());
                }
            }
            _ => (),
        }
        // Check that the Meetup ID has not been linked to some other Discord ID yet
        let (redis_connection, existing_discord_id): (_, Option<u64>) = redis::cmd("GET")
            .arg(&redis_key_m2d)
            .query_async(redis_connection)
            .compat()
            .await?;
        match existing_discord_id {
            Some(_) => {
                return Ok((
                    lib::strings::OAUTH2_MEETUP_ALREADY_LINKED_FAILURE_TITLE,
                    lib::strings::OAUTH2_MEETUP_ALREADY_LINKED_FAILURE_CONTENT(&bot_name),
                )
                    .into());
            }
            _ => (),
        }
        // Create the link between the Discord and the Meetup ID
        let successful = AtomicBool::new(false);
        let (_redis_connection, _): (_, ()) = {
            let mut redis_connection = redis_connection;
            // If the "rsvp" scope is part of the token result, store the tokens as well
            if with_rsvp_scope {
                if let Some(refresh_token) = token_res.refresh_token() {
                    let redis_user_tokens_key =
                        format!("meetup_user:{}:oauth2_tokens", meetup_user.id);
                    let fields = &[
                        ("access_token", token_res.access_token().secret()),
                        ("refresh_token", refresh_token.secret()),
                    ];
                    let mut pipe = redis::pipe();
                    pipe.hset_multiple(&redis_user_tokens_key, fields);
                    let (new_redis_connection, _): (_, ()) =
                        pipe.query_async(redis_connection).compat().await?;
                    redis_connection = new_redis_connection;
                }
            }
            let transaction_fn = {
                let redis_key_d2m = &redis_key_d2m;
                let redis_key_m2d = &redis_key_m2d;
                let meetup_user_id = meetup_user.id;
                let successful = &successful;
                move |con: redis::aio::Connection, mut pipe: redis::Pipeline| {
                    async move {
                        let (con, linked_meetup_id): (_, Option<u64>) = redis::cmd("GET")
                            .arg(redis_key_d2m)
                            .query_async(con)
                            .compat()
                            .await?;
                        let (con, linked_discord_id): (_, Option<u64>) = redis::cmd("GET")
                            .arg(redis_key_m2d)
                            .query_async(con)
                            .compat()
                            .await?;
                        if linked_meetup_id.is_some() || linked_discord_id.is_some() {
                            // The meetup id was linked in the meantime, abort
                            successful.store(false, Ordering::Release);
                            // Execute empty transaction just to get out of the closure
                            pipe.query_async(con).compat().await
                        } else {
                            pipe.sadd("meetup_users", meetup_user_id)
                                .sadd("discord_users", discord_id)
                                .set(redis_key_d2m, meetup_user_id)
                                .set(redis_key_m2d, discord_id);
                            successful.store(true, Ordering::Release);
                            pipe.query_async(con).compat().await
                        }
                    }
                }
            };
            let transaction_keys = &[&redis_key_d2m, &redis_key_m2d];
            lib::redis::async_redis_transaction(redis_connection, transaction_keys, transaction_fn)
                .await?
        };
        if !successful.load(Ordering::Acquire) {
            return Ok((
                "Linking Failure",
                "Could not assign meetup id (timing error)",
            )
                .into());
        }
        if let Some(photo) = meetup_user.photo {
            Ok(HandlerResponse::Message {
                title: Cow::Borrowed(lib::strings::OAUTH2_LINKING_SUCCESS_TITLE),
                content: Some(Cow::Owned(lib::strings::OAUTH2_LINKING_SUCCESS_CONTENT(
                    &meetup_user.name,
                ))),
                safe_content: None,
                img_url: Some(Cow::Owned(photo.thumb_link)),
            }
            .into())
        } else {
            Ok((
                lib::strings::OAUTH2_LINKING_SUCCESS_TITLE,
                lib::strings::OAUTH2_LINKING_SUCCESS_CONTENT(&meetup_user.name),
            )
                .into())
        }
    } else {
        Ok(("Unknown route", "").into())
    }
}

pub fn create_auth_server(
    oauth2_consumer: &lib::meetup::oauth2::OAuth2Consumer,
    addr: std::net::SocketAddr,
    redis_client: redis::Client,
    discord_http: Arc<serenity::CacheAndHttp>,
    async_meetup_client: Arc<Mutex<Option<Arc<lib::meetup::api::AsyncClient>>>>,
    bot_name: String,
) -> impl Future<Output = ()> + Send + 'static {
    // And a MakeService to handle each connection...
    let make_meetup_service = {
        let authorization_client = oauth2_consumer.authorization_client.clone();
        let link_client = oauth2_consumer.link_client.clone();
        make_service_fn(move |_| {
            let authorization_client = authorization_client.clone();
            let link_client = link_client.clone();
            let discord_http = discord_http.clone();
            let async_meetup_client = async_meetup_client.clone();
            let bot_name = bot_name.clone();
            let redis_client = redis_client.clone();
            let request_to_response_fn = {
                move |req| {
                    let authorization_client = authorization_client.clone();
                    let link_client = link_client.clone();
                    let discord_http = discord_http.clone();
                    let async_meetup_client = async_meetup_client.clone();
                    let bot_name = bot_name.clone();
                    let redis_client = redis_client.clone();
                    async move {
                        // Create a new Redis connection for each request.
                        // Not optimal...
                        let redis_connection = redis_client.get_async_connection().compat().await?;
                        let handler_response = meetup_http_handler(
                            redis_connection,
                            &authorization_client,
                            &link_client,
                            &discord_http,
                            &async_meetup_client,
                            req,
                            bot_name.clone(),
                        )
                        .await;
                        match handler_response {
                            Ok(handler_response) => match handler_response {
                                HandlerResponse::Response(response) => {
                                    Ok::<_, lib::BoxedError>(response)
                                }
                                HandlerResponse::Message {
                                    title,
                                    content,
                                    safe_content,
                                    img_url,
                                } => {
                                    let html_body = LinkingMessageTemplate {
                                        title: &title,
                                        content: content.as_ref().map(Cow::as_ref),
                                        safe_content: safe_content.as_ref().map(Cow::as_ref),
                                        img_url: img_url.as_ref().map(Cow::as_ref),
                                    }
                                    .render()?;
                                    Ok(Response::new(html_body.into()))
                                }
                            },
                            Err(err) => {
                                // Catch all errors and don't let the details of internal server erros leak
                                // TODO: replace HandlerError with the never type "!" once it
                                // is available on stable, since this function will never return an error
                                eprintln!("Error in meetup_authorize: {}", err);
                                let message_template = LinkingMessageTemplate {
                                    title: lib::strings::INTERNAL_SERVER_ERROR,
                                    content: None,
                                    safe_content: None,
                                    img_url: None,
                                };
                                let html_body = message_template.render()?;
                                Ok(Response::new(html_body.into()))
                            }
                        }
                    }
                }
            };
            async { Ok::<_, lib::BoxedError>(service_fn(request_to_response_fn)) }
        })
    };
    let server = Server::bind(&addr)
        .serve(make_meetup_service)
        .unwrap_or_else(|err| {
            eprintln!("server error: {}", err);
        });

    server
}
