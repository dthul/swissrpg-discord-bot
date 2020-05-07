use warp::Filter;

pub fn create_routes(
    discord_cache_http: lib::discord::CacheAndHttp,
    api_key: String,
) -> impl warp::Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    // Leak the API key's memory to get a &'static str
    let api_key = Box::leak(api_key.into_boxed_str());
    let get_route = {
        warp::get()
            .and(warp::path!("api" / "check_discord_username"))
            .and(warp::header::exact("Api-Key", api_key))
            .and(warp::header::<String>("Discord-Username"))
            .and_then(move |discord_username: String| {
                let discord_cache_http = discord_cache_http.clone();
                async move {
                    let id = lib::tasks::subscription_roles::discord_username_to_id(
                        &discord_cache_http,
                        &discord_username,
                    )?;
                    if id.is_none() {
                        // The username seems to be invalid, return a 204 HTTP code
                        Ok::<_, warp::Rejection>(warp::http::StatusCode::NO_CONTENT)
                    } else {
                        // The username could be matched to an ID, return a 200 HTTP code
                        Ok::<_, warp::Rejection>(warp::http::StatusCode::OK)
                    }
                }
            })
    };
    get_route
}
