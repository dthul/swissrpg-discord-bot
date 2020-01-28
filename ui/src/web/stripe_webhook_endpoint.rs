pub fn create_routes(
    // redis_client: redis::Client,
    discord_cache_http: lib::discord::CacheAndHttp,
    stripe_webhook_secret: String,
) -> impl warp::Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    let get_route = {
        let redis_client = redis_client.clone();
        warp::post()
            .and(warp::path!("webhooks" / "stripe"))
            .and(warp::header::<String>("Stripe-Signature"))
            .and_then(move |signature| {
                async move {
                    let mut redis_connection = redis_client
                        .get_async_connection()
                        .err_into::<lib::meetup::Error>()
                        .await?;
                    handle_schedule_session(&mut redis_connection, flow_id)
                        .err_into::<warp::Rejection>()
                        .await
                }
            })
    };
    get_route
}