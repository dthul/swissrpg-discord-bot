// This is code copied from the oauth2 crate.
// Since oauth2 (used to) use reqwest 0.9 but we already use reqwest 0.10,
// I copied the code such that we don't have two of reqwest, hyper and tokio.
// Can be removed as soon as oauth2 updates to the same reqwest version we use.
use failure::Fail;
use oauth2::{HttpRequest, HttpResponse};
use reqwest::{redirect::Policy, Client as AsyncClient};
use std::convert::TryFrom;

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
// TODO: all these conversions (as_xxx, from_xxx) between http 0.1 and http 0.2
// types can be removed once oauth2-rs uses http 0.2
pub async fn async_http_client(request: HttpRequest) -> Result<HttpResponse, Error> {
    let client = AsyncClient::builder()
        // Following redirects opens the client up to SSRF vulnerabilities.
        .redirect(Policy::none())
        .build()
        .map_err(Error::Reqwest)?;
    let mut request_builder = client
        .request(
            reqwest::Method::try_from(request.method.as_str()).expect("Invalid HTTP method"),
            request.url.as_str(),
        )
        .body(request.body);
    for (name, value) in &request.headers {
        request_builder = request_builder.header(
            name.as_str(),
            reqwest::header::HeaderValue::from_bytes(value.as_bytes())
                .expect("Invalid header value"),
        );
    }
    let request = request_builder.build().map_err(Error::Reqwest)?;
    let response = client.execute(request).await.map_err(Error::Reqwest)?;
    let status_code = response.status();
    let headers: http::HeaderMap = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                http::header::HeaderName::from_bytes(name.as_ref()).expect("Invalid header name"),
                http::HeaderValue::from_bytes(value.as_bytes()).expect("Invalid header value"),
            )
        })
        .collect();
    let body = response.bytes().await.map_err(Error::Reqwest)?;
    Ok(HttpResponse {
        status_code: http::StatusCode::from_u16(status_code.as_u16()).expect("Invalid status code"),
        headers: headers,
        body: body.to_vec(),
    })
}
