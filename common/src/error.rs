use crate::meetup_api::Error as MeetupApiError;
use askama::Error as AskamaError;
use backtrace::Backtrace;
use chrono::format::ParseError as ChronoParseError;
use hyper::http::Error as HttpError;
use redis::RedisError;
use regex::Error as RegexError;
use reqwest::Error as ReqwestError;
use serenity::Error as SerenityError;
use simple_error::SimpleError;
use std::num::ParseIntError;
use tokio::timer::Error as TokioTimerError;
use url::ParseError as UrlParseError;
 
type RequestTokenError = oauth2::RequestTokenError<
    crate::oauth2_async_http_client::Error,
    oauth2::StandardErrorResponse<oauth2::basic::BasicErrorResponseType>,
>;

#[derive(Debug)]
pub enum ErrorVariant {
    OAuth2RequestTokenError(Box<RequestTokenError>),
    Other(Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug)]
pub struct BoxedError {
    pub inner: ErrorVariant,
    pub backtrace: Backtrace,
}

impl std::fmt::Display for BoxedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner {
            ErrorVariant::Other(err) => write!(
                f,
                "Encountered the following error:\n{:#?}\nBacktrace:\n{:?}",
                err, self.backtrace
            ),
            ErrorVariant::OAuth2RequestTokenError(err) => match &**err {
                oauth2::RequestTokenError::Other(string) => write!(
                    f,
                    "OAuth2::RequestTokenError::Other:\n{:#?}\nBacktrace:\n{:?}",
                    string, self.backtrace
                ),
                oauth2::RequestTokenError::Parse(serde_err, bytes) => write!(
                    f,
                    "OAuth2::RequestTokenError::Parse:\n{:#?}\nBytes:\n{:?}\nBacktrace:\n{:?}",
                    serde_err, bytes, self.backtrace
                ),
                oauth2::RequestTokenError::Request(req_err) => write!(
                    f,
                    "OAuth2::RequestTokenError::Request:\n{:#?}\nBacktrace:\n{:?}",
                    req_err, self.backtrace
                ),
                oauth2::RequestTokenError::ServerResponse(err) => write!(
                    f,
                    "OAuth2::RequestTokenError::ServerResponse:\n{:#?}\nBacktrace:\n{:?}",
                    err, self.backtrace
                ),
            },
        }
    }
}

impl std::error::Error for BoxedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // Dunno
        None
    }
}

// impl<E: std::error::Error + Send + Sync + 'static> From<E> for BoxedError {
//     fn from(err: E) -> Self {
//         BoxedError {
//             inner: Box::new(err),
//             backtrace: Backtrace::new()
//         }
//     }
// }

impl From<SimpleError> for BoxedError {
    fn from(err: SimpleError) -> Self {
        BoxedError {
            inner: ErrorVariant::Other(Box::new(err)),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<RedisError> for BoxedError {
    fn from(err: RedisError) -> Self {
        BoxedError {
            inner: ErrorVariant::Other(Box::new(err)),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<MeetupApiError> for BoxedError {
    fn from(err: MeetupApiError) -> Self {
        BoxedError {
            inner: ErrorVariant::Other(Box::new(err)),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<ChronoParseError> for BoxedError {
    fn from(err: ChronoParseError) -> Self {
        BoxedError {
            inner: ErrorVariant::Other(Box::new(err)),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<UrlParseError> for BoxedError {
    fn from(err: UrlParseError) -> Self {
        BoxedError {
            inner: ErrorVariant::Other(Box::new(err)),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<SerenityError> for BoxedError {
    fn from(err: SerenityError) -> Self {
        BoxedError {
            inner: ErrorVariant::Other(Box::new(err)),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<ReqwestError> for BoxedError {
    fn from(err: ReqwestError) -> Self {
        BoxedError {
            inner: ErrorVariant::Other(Box::new(err)),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<HttpError> for BoxedError {
    fn from(err: HttpError) -> Self {
        BoxedError {
            inner: ErrorVariant::Other(Box::new(err)),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<TokioTimerError> for BoxedError {
    fn from(err: TokioTimerError) -> Self {
        BoxedError {
            inner: ErrorVariant::Other(Box::new(err)),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<AskamaError> for BoxedError {
    fn from(err: AskamaError) -> Self {
        BoxedError {
            inner: ErrorVariant::Other(Box::new(err)),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<RegexError> for BoxedError {
    fn from(err: RegexError) -> Self {
        BoxedError {
            inner: ErrorVariant::Other(Box::new(err)),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<ParseIntError> for BoxedError {
    fn from(err: ParseIntError) -> Self {
        BoxedError {
            inner: ErrorVariant::Other(Box::new(err)),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<RequestTokenError> for BoxedError {
    fn from(err: RequestTokenError) -> Self {
        BoxedError {
            inner: ErrorVariant::OAuth2RequestTokenError(Box::new(err)),
            backtrace: Backtrace::new(),
        }
    }
}
