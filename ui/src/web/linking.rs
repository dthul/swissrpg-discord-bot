use askama::Template;
use cookie::Cookie;
use futures_util::{lock::Mutex, FutureExt, TryFutureExt};
use hyper::Response;
use oauth2::{basic::BasicClient, AuthorizationCode, CsrfToken, RedirectUrl, Scope, TokenResponse};
use redis::AsyncCommands;
use serde::Deserialize;
use std::{borrow::Cow, sync::Arc};
use warp::Filter;

#[derive(Deserialize)]
struct LinkQuery {
    code: String,
    state: String,
    error: Option<String>,
}

pub fn create_routes(
    redis_client: redis::Client,
    oauth2_consumer: Arc<lib::meetup::oauth2::OAuth2Consumer>,
    async_meetup_client: Arc<Mutex<Option<Arc<lib::meetup::api::AsyncClient>>>>,
    bot_name: String,
) -> impl warp::Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    let authorize_route = {
        let redis_client = redis_client.clone();
        let oauth2_consumer = oauth2_consumer.clone();
        warp::get()
            .and(warp::path("authorize"))
            .and(warp::path::end())
            .and_then(move || {
                let redis_client = redis_client.clone();
                let oauth2_consumer = oauth2_consumer.clone();
                async move {
                    let mut redis_connection = redis_client
                        .get_async_connection()
                        .err_into::<lib::meetup::Error>()
                        .await?;
                    handle_authorize(&mut redis_connection, &oauth2_consumer.authorization_client)
                        .err_into::<warp::Rejection>()
                        .await
                }
            })
    };
    let authorize_redirect_route = {
        let redis_client = redis_client.clone();
        let oauth2_consumer = oauth2_consumer.clone();
        let async_meetup_client = async_meetup_client.clone();
        warp::get()
            .and(warp::path!("authorize" / "redirect"))
            .and(warp::query())
            .and(warp::header::headers_cloned())
            .and_then(move |query: LinkQuery, headers| {
                let redis_client = redis_client.clone();
                let oauth2_consumer = oauth2_consumer.clone();
                let async_meetup_client = async_meetup_client.clone();
                async move {
                    let mut redis_connection = redis_client
                        .get_async_connection()
                        .err_into::<lib::meetup::Error>()
                        .await?;
                    handle_authorize_redirect(
                        &mut redis_connection,
                        &oauth2_consumer.authorization_client,
                        &async_meetup_client,
                        query,
                        &headers,
                    )
                    .err_into::<warp::Rejection>()
                    .await
                }
            })
    };
    let link_route = {
        let redis_client = redis_client.clone();
        let oauth2_consumer = oauth2_consumer.clone();
        warp::get()
            .and(warp::path!("link" / String))
            .and_then(move |linking_id: String| {
                let redis_client = redis_client.clone();
                let oauth2_consumer = oauth2_consumer.clone();
                async move {
                    let mut redis_connection = redis_client
                        .get_async_connection()
                        .err_into::<lib::meetup::Error>()
                        .await?;
                    handle_link(
                        &mut redis_connection,
                        &oauth2_consumer.link_client,
                        &linking_id,
                    )
                    .err_into::<warp::Rejection>()
                    .await
                }
            })
    };
    let link_redirect_route = {
        let redis_client = redis_client.clone();
        let oauth2_consumer = oauth2_consumer.clone();
        let bot_name = bot_name.clone();
        warp::get()
            .and(warp::path!("link" / String / ..))
            .and(
                warp::path("rsvp")
                    .map(|| true)
                    .or(warp::path("norsvp").map(|| false))
                    .unify(),
            )
            .and(warp::path("redirect"))
            .and(warp::path::end())
            .and(warp::path::full())
            .and(warp::query())
            .and(warp::header::headers_cloned())
            .and_then(
                move |linking_id: String,
                      with_rsvp,
                      path: warp::path::FullPath,
                      query: LinkQuery,
                      headers| {
                    let redis_client = redis_client.clone();
                    let oauth2_consumer = oauth2_consumer.clone();
                    let bot_name = bot_name.clone();
                    async move {
                        let mut redis_connection = redis_client
                            .get_async_connection()
                            .err_into::<lib::meetup::Error>()
                            .await?;
                        handle_link_redirect(
                            &mut redis_connection,
                            &oauth2_consumer.link_client,
                            &bot_name,
                            path.as_str(),
                            query,
                            &headers,
                            &linking_id,
                            with_rsvp,
                        )
                        .err_into::<warp::Rejection>()
                        .await
                    }
                },
            )
    };
    authorize_route
        .or(authorize_redirect_route)
        .or(link_route)
        .or(link_redirect_route)
}

#[derive(Template)]
#[template(path = "link.html")]
struct LinkingTemplate<'a> {
    authorize_url: &'a str,
}

async fn generate_csrf_cookie(
    redis_connection: &mut redis::aio::Connection,
    csrf_state: &str,
) -> Result<Cookie<'static>, lib::meetup::Error> {
    let random_csrf_user_id = lib::new_random_id(16);
    let redis_csrf_key = format!("csrf:{}", &random_csrf_user_id);
    let _: () = redis_connection
        .set_ex(&redis_csrf_key, csrf_state, 3600)
        .await?;
    Ok(Cookie::build("csrf_user_id", random_csrf_user_id)
        .domain(lib::urls::DOMAIN)
        .http_only(true)
        .same_site(cookie::SameSite::Lax)
        .max_age(time::Duration::hours(1))
        .finish())
}

async fn check_csrf_cookie(
    redis_connection: &mut redis::aio::Connection,
    headers: &hyper::HeaderMap<hyper::header::HeaderValue>,
    csrf_state: &str,
) -> Result<bool, lib::meetup::Error> {
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
    let csrf_stored_state: Option<String> = redis_connection.get(&redis_csrf_key).await?;
    let csrf_stored_state: String = match csrf_stored_state {
        None => return Ok(false),
        Some(csrf_stored_state) => csrf_stored_state,
    };
    Ok(csrf_state == csrf_stored_state)
}

async fn handle_authorize(
    redis_connection: &mut redis::aio::Connection,
    oauth2_authorization_client: &BasicClient,
) -> Result<super::server::HandlerResponse, lib::meetup::Error> {
    // Generate the authorization URL to which we'll redirect the user.
    let (authorize_url, csrf_state) = oauth2_authorization_client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("ageless".to_string()))
        .add_scope(Scope::new("basic".to_string()))
        .add_scope(Scope::new("event_management".to_string()))
        .url();
    // Store the generated CSRF token so we can compare it to the one
    // returned by Meetup later
    let csrf_cookie = generate_csrf_cookie(redis_connection, csrf_state.secret()).await?;
    let html_body = format!("<a href=\"{}\">Login with Meetup</a>", authorize_url);
    Response::builder()
        .header(hyper::header::SET_COOKIE, csrf_cookie.to_string())
        .body(html_body.into())
        .map_err(|err| err.into())
        .map(|response| super::server::HandlerResponse::Response(response))
}

async fn handle_authorize_redirect(
    redis_connection: &mut redis::aio::Connection,
    oauth2_authorization_client: &BasicClient,
    async_meetup_client: &Arc<Mutex<Option<Arc<lib::meetup::api::AsyncClient>>>>,
    query: LinkQuery,
    headers: &hyper::HeaderMap<hyper::header::HeaderValue>,
) -> Result<super::server::HandlerResponse, lib::meetup::Error> {
    if let Some(error) = query.error {
        return Ok(("OAuth2 error", error.to_string()).into());
    }
    // Compare the CSRF state that was returned by Meetup to the one
    // we have saved
    let csrf_is_valid = check_csrf_cookie(redis_connection, headers, &query.state).await?;
    if !csrf_is_valid {
        return Ok((
            "CSRF check failed",
            "Please go back to the first page, reload, and repeat the process",
        )
            .into());
    }
    // Exchange the code with a token.
    let code = AuthorizationCode::new(query.code);
    let async_meetup_client = async_meetup_client.clone();
    let token_res = oauth2_authorization_client
        .exchange_code(code)
        .request_async(oauth2::reqwest::async_http_client)
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
        // let token_res = &token_res;
        lib::redis::closure_type_helper(
            move |con: &mut redis::aio::Connection, mut pipe: redis::Pipeline| {
                let token_res = token_res.clone();
                async move {
                    match token_res.refresh_token() {
                        Some(refresh_token) => {
                            pipe.set("meetup_access_token", token_res.access_token().secret())
                                .set("meetup_refresh_token", refresh_token.secret());
                            pipe.query_async(con).await
                        }
                        None => {
                            // Don't delete the (possibly existing) old refresh token
                            pipe.set("meetup_access_token", token_res.access_token().secret());
                            pipe.query_async(con).await
                        }
                    }
                }
                .boxed()
            },
        )
    };
    let _: () = lib::redis::async_redis_transaction(
        redis_connection,
        &["meetup_access_token", "meetup_refresh_token"],
        transaction_fn,
    )
    .await?;
    // Replace the meetup client
    *async_meetup_client.lock().await = Some(Arc::new(new_async_meetup_client));
    Ok(("Thanks for logging in :)", "").into())
}

async fn handle_link(
    redis_connection: &mut redis::aio::Connection,
    oauth2_link_client: &BasicClient,
    linking_id: &str,
) -> Result<super::server::HandlerResponse, lib::meetup::Error> {
    // The linking ID was stored in Redis when the linking link was created.
    // Check that it is still valid
    let redis_key = format!("meetup_linking:{}:discord_user", linking_id);
    let mut pipe = redis::pipe();
    pipe.expire(&redis_key, 600).ignore().get(&redis_key);
    let (discord_id,): (Option<u64>,) = pipe.query_async(redis_connection).await?;
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
        .set_redirect_uri(RedirectUrl::new(format!(
            "{}/link/{}/norsvp/redirect",
            lib::urls::BASE_URL,
            linking_id
        ))?)
        .authorize_url(|| csrf_state)
        .add_scope(Scope::new("basic".to_string()))
        .url();
    let (authorize_url_rsvp, csrf_state) = oauth2_link_client
        .clone()
        .set_redirect_uri(RedirectUrl::new(format!(
            "{}/link/{}/rsvp/redirect",
            lib::urls::BASE_URL,
            linking_id
        ))?)
        .authorize_url(|| csrf_state)
        .add_scope(Scope::new("basic".to_string()))
        .add_scope(Scope::new("rsvp".to_string()))
        .url();
    // Store the generated CSRF token so we can compare it to the one
    // returned by Meetup later
    let csrf_cookie = generate_csrf_cookie(redis_connection, csrf_state.secret()).await?;
    let linking_template = LinkingTemplate {
        authorize_url: authorize_url_rsvp.as_str(),
    };
    let html_body = linking_template.render()?;
    let response = Response::builder()
        .header(hyper::header::SET_COOKIE, csrf_cookie.to_string())
        .body(html_body.into())?;
    Ok(super::server::HandlerResponse::Response(response))
}

async fn handle_link_redirect(
    redis_connection: &mut redis::aio::Connection,
    oauth2_link_client: &BasicClient,
    bot_name: &str,
    path: &str,
    query: LinkQuery,
    _headers: &hyper::HeaderMap<hyper::header::HeaderValue>,
    linking_id: &str,
    with_rsvp_scope: bool,
) -> Result<super::server::HandlerResponse, lib::meetup::Error> {
    // The linking ID was stored in Redis when the linking link was created.
    // Check that it is still valid
    let redis_key = format!("meetup_linking:{}:discord_user", &linking_id);
    // This is a one-time use link. Expire it now.
    let mut pipe = redis::pipe();
    pipe.get(&redis_key).del(&redis_key);
    let (discord_id, _): (Option<u64>, u32) = pipe.query_async(redis_connection).await?;
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
    if let Some(error) = query.error {
        if error == "access_denied" {
            // The user did not grant access
            // Give them the chance to do it again
            let linking_url =
                lib::meetup::oauth2::generate_meetup_linking_link(redis_connection, discord_id)
                    .await?;
            return Ok(super::server::HandlerResponse::Message {
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
    // Compare the CSRF state that was returned by Meetup to the one
    // we have saved
    // let csrf_is_valid = check_csrf_cookie(redis_connection, headers, &query.state).await?;
    // if !csrf_is_valid {
    //     return Ok((
    //         "CSRF check failed",
    //         "Please go back to the first page, reload, and repeat the process",
    //     )
    //         .into());
    // }
    // Exchange the code with a token.
    let code = AuthorizationCode::new(query.code);
    let redirect_url = RedirectUrl::new(format!("{}{}", lib::urls::BASE_URL, path))?;
    let token_res = oauth2_link_client
        .clone()
        .set_redirect_uri(redirect_url)
        .exchange_code(code)
        .request_async(oauth2::reqwest::async_http_client)
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
    let existing_meetup_id: Option<u64> = redis_connection.get(&redis_key_d2m).await?;
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
    let existing_discord_id: Option<u64> = redis_connection.get(&redis_key_m2d).await?;
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
    let successful = Arc::new(Mutex::new(false));
    let _: () = {
        // If the "rsvp" scope is part of the token result, store the tokens as well
        if with_rsvp_scope {
            if let Some(refresh_token) = token_res.refresh_token() {
                let redis_user_tokens_key = format!("meetup_user:{}:oauth2_tokens", meetup_user.id);
                let mut pipe = redis::pipe();
                pipe.hset(
                    &redis_user_tokens_key,
                    "access_token",
                    token_res.access_token().secret(),
                );
                pipe.hset(
                    &redis_user_tokens_key,
                    "refresh_token",
                    refresh_token.secret(),
                );
                let _: () = pipe.query_async(redis_connection).await?;
            }
        }
        let transaction_fn = {
            let redis_key_d2m = redis_key_d2m.clone();
            let redis_key_m2d = redis_key_m2d.clone();
            let meetup_user_id = meetup_user.id;
            let successful = successful.clone();
            lib::redis::closure_type_helper(
                move |con: &mut redis::aio::Connection, mut pipe: redis::Pipeline| {
                    let redis_key_d2m = redis_key_d2m.clone();
                    let redis_key_m2d = redis_key_m2d.clone();
                    let successful = successful.clone();
                    async move {
                        let linked_meetup_id: Option<u64> = con.get(&redis_key_d2m).await?;
                        let linked_discord_id: Option<u64> = con.get(&redis_key_m2d).await?;
                        if linked_meetup_id.is_some() || linked_discord_id.is_some() {
                            // The meetup id was linked in the meantime, abort
                            *successful.lock().await = false;
                            // Execute empty transaction just to get out of the closure
                            pipe.query_async(con).await
                        } else {
                            pipe.sadd("meetup_users", meetup_user_id)
                                .sadd("discord_users", discord_id)
                                .set(&redis_key_d2m, meetup_user_id)
                                .set(&redis_key_m2d, discord_id);
                            *successful.lock().await = true;
                            pipe.query_async(con).await
                        }
                    }
                    .boxed()
                },
            )
        };
        let transaction_keys = &[&redis_key_d2m, &redis_key_m2d];
        lib::redis::async_redis_transaction(redis_connection, transaction_keys, transaction_fn)
            .await?
    };
    if !*successful.lock().await {
        return Ok((
            "Linking Failure",
            "Could not assign meetup id (timing error)",
        )
            .into());
    }
    if let Some(photo) = meetup_user.photo {
        Ok(super::server::HandlerResponse::Message {
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
}
