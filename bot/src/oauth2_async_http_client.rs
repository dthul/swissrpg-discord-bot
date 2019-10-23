// This is code copied from the oauth2 crate.
// Since oauth2 (used to) use reqwest 0.9 but we already use reqwest 0.10,
// I copied the code such that we don't have two of reqwest, hyper and tokio.
// Can be removed as soon as oauth2 updates to the same reqwest version we use.
use failure::Fail;
// use futures_01::{Future, IntoFuture};
use futures_util::compat::Compat;
use futures_util::FutureExt;
use oauth2::{HttpRequest, HttpResponse};
use reqwest::Client as AsyncClient;
use reqwest::RedirectPolicy;

///
/// Error type returned by failed reqwest HTTP requests.
///
#[derive(Debug, Fail)]
pub enum Error {
    /// Error returned by reqwest crate.
    #[fail(display = "request failed")]
    Reqwest(#[cause] reqwest::Error),
    /// Non-reqwest HTTP error.
    #[fail(display = "HTTP error")]
    Http(#[cause] http::Error),
    /// I/O error.
    #[fail(display = "I/O error")]
    Io(#[cause] std::io::Error),
    /// Other error.
    #[fail(display = "Other error: {}", _0)]
    Other(String),
}

///
/// Asynchronous HTTP client.
///
pub fn async_http_client(
    request: HttpRequest,
) -> impl futures_01::Future<Item = HttpResponse, Error = Error> {
    let future = async move {
        let client = AsyncClient::builder()
            // Following redirects opens the client up to SSRF vulnerabilities.
            .redirect(RedirectPolicy::none())
            .build()
            .map_err(Error::Reqwest)?;
        let mut request_builder = client
            .request(request.method, request.url.as_str())
            .body(request.body);
        for (name, value) in &request.headers {
            request_builder = request_builder.header(name, value);
        }
        let request = request_builder.build().map_err(Error::Reqwest)?;
        let response = client.execute(request).await.map_err(Error::Reqwest)?;
        let status_code = response.status();
        let headers = response.headers().clone();
        let body = response.bytes().await.map_err(Error::Reqwest)?;
        Ok(HttpResponse {
            status_code,
            headers,
            body: body.to_vec(),
        })
    };
    Compat::new(future.boxed())
}
