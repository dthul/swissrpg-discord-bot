use warp::Filter;

pub fn create_routes(
    // redis_client: redis::Client,
    _discord_cache_http: lib::discord::CacheAndHttp,
    _stripe_webhook_secret: String,
) -> impl warp::Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    let get_route = {
        warp::post()
            .and(warp::path!("webhooks" / "stripe"))
            .and(warp::header::<String>("Stripe-Signature"))
            .and_then(move |_signature| {
                println!("Webhook!");
                async { Ok::<_, warp::Rejection>("Webhook!") }
            })
    };
    get_route
}
