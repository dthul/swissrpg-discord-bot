use std::{future::Future, sync::Arc};

use axum::{http::StatusCode, routing::get_service, AddExtensionLayer, Router};
use futures_util::lock::Mutex;
use tower_http::services::ServeDir;

use super::{api, linking, schedule_session, stripe_webhook_endpoint};

pub struct State {
    pub oauth2_consumer: Arc<lib::meetup::oauth2::OAuth2Consumer>,
    pub redis_client: redis::Client,
    pub pool: sqlx::PgPool,
    pub async_meetup_client: Arc<Mutex<Option<Arc<lib::meetup::newapi::AsyncClient>>>>,
    pub discord_cache_http: lib::discord::CacheAndHttp,
    pub bot_name: String,
    pub stripe_webhook_secret: Option<String>,
    pub stripe_client: Arc<stripe::Client>,
    pub api_key: Option<String>,
}

pub fn create_server(
    oauth2_consumer: Arc<lib::meetup::oauth2::OAuth2Consumer>,
    addr: std::net::SocketAddr,
    redis_client: redis::Client,
    pool: sqlx::PgPool,
    async_meetup_client: Arc<Mutex<Option<Arc<lib::meetup::newapi::AsyncClient>>>>,
    discord_cache_http: lib::discord::CacheAndHttp,
    bot_name: String,
    stripe_webhook_secret: Option<String>,
    stripe_client: Arc<stripe::Client>,
    api_key: Option<String>,
    shutdown_signal: impl Future<Output = ()> + Send + 'static,
) -> impl Future<Output = ()> + Send + 'static {
    let state = Arc::new(State {
        oauth2_consumer,
        redis_client,
        pool,
        async_meetup_client,
        discord_cache_http,
        bot_name,
        stripe_webhook_secret,
        stripe_client,
        api_key,
    });
    let linking_routes = linking::create_routes();
    let schedule_session_routes = schedule_session::create_routes();
    let stripe_webhook_routes = stripe_webhook_endpoint::create_routes();
    let api_routes = api::create_routes();
    let static_route: Router = Router::new().nest(
        "/static",
        get_service(
            ServeDir::new("ui/src/web/html/static").append_index_html_on_directories(false),
        )
        .handle_error(|err: std::io::Error| async move {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal server error:\n{:#?}", err),
            )
        }),
    );
    let router = linking_routes
        .merge(schedule_session_routes)
        .merge(stripe_webhook_routes)
        .nest("api", api_routes)
        .merge(static_route)
        .layer(AddExtensionLayer::new(state));
    async move {
        if let Err(err) = axum::Server::bind(&addr)
            .serve(router.into_make_service())
            .with_graceful_shutdown(shutdown_signal)
            .await
        {
            eprintln!("Web server exited with an error:\n{:#?}", err);
        }
    }
}
