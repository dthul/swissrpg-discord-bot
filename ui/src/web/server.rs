use askama::Template;
use futures_util::{compat::Future01CompatExt, lock::Mutex, TryFutureExt};
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server,
};
use oauth2::basic::BasicClient;
use std::pin::Pin;
use std::{borrow::Cow, future::Future, sync::Arc};

type RequestHandler = for<'a> fn(
    redis_connection: redis::aio::Connection,
    oauth2_authorization_client: &'a BasicClient,
    oauth2_link_client: &'a BasicClient,
    _discord_http: &'a serenity::CacheAndHttp,
    async_meetup_client: &'a Arc<Mutex<Option<Arc<lib::meetup::api::AsyncClient>>>>,
    req: &'a Request<Body>,
    bot_name: String,
) -> Pin<
    Box<
        dyn Future<
                Output = Result<
                    (
                        redis::aio::Connection,
                        Option<super::server::HandlerResponse>,
                    ),
                    lib::meetup::Error,
                >,
            > + Send
            + 'a,
    >,
>;

pub enum HandlerResponse {
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

const REQUEST_HANDLERS: &'static [RequestHandler] = &[super::linking::meetup_http_handler_boxed];
pub fn create_server(
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
                        let mut redis_connection =
                            redis_client.get_async_connection().compat().await?;
                        let mut handler_response = None;
                        for request_handler in REQUEST_HANDLERS {
                            match request_handler(
                                redis_connection,
                                &authorization_client,
                                &link_client,
                                &discord_http,
                                &async_meetup_client,
                                &req,
                                bot_name.clone(),
                            )
                            .await
                            {
                                Err(err) => {
                                    handler_response = Some(Err(err));
                                    break;
                                }
                                Ok((con, None)) => {
                                    redis_connection = con;
                                    continue;
                                }
                                Ok((_con, Some(response))) => {
                                    handler_response = Some(Ok(response));
                                    break;
                                }
                            }
                        }
                        match handler_response {
                            None => Ok(Response::new("Unknown route".into())),
                            Some(Ok(handler_response)) => match handler_response {
                                HandlerResponse::Response(response) => {
                                    Ok::<_, lib::BoxedError>(response)
                                }
                                HandlerResponse::Message {
                                    title,
                                    content,
                                    safe_content,
                                    img_url,
                                } => {
                                    let html_body = super::MessageTemplate {
                                        title: &title,
                                        content: content.as_ref().map(Cow::as_ref),
                                        safe_content: safe_content.as_ref().map(Cow::as_ref),
                                        img_url: img_url.as_ref().map(Cow::as_ref),
                                    }
                                    .render()?;
                                    Ok(Response::new(html_body.into()))
                                }
                            },
                            Some(Err(err)) => {
                                // Catch all errors and don't let the details of internal server erros leak
                                // TODO: replace HandlerError with the never type "!" once it
                                // is available on stable, since this function will never return an error
                                eprintln!("Error in meetup_authorize: {}", err);
                                let message_template = super::MessageTemplate {
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
