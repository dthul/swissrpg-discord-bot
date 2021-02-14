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
use stripe::Error as StripeError;
use tokio::{task::JoinError, time::error::Elapsed};
use url::ParseError as UrlParseError;

#[derive(Debug)]
pub struct BoxedError {
    pub inner: Box<dyn std::error::Error + Send + Sync>,
    pub backtrace: Backtrace,
}

impl std::fmt::Display for BoxedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Encountered the following error:\n{:#?}\nBacktrace:\n{:?}",
            self.inner, self.backtrace
        )
    }
}

impl std::error::Error for BoxedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // Dunno
        Some(self.inner.as_ref())
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
            inner: Box::new(err),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<RedisError> for BoxedError {
    fn from(err: RedisError) -> Self {
        BoxedError {
            inner: Box::new(err),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<ChronoParseError> for BoxedError {
    fn from(err: ChronoParseError) -> Self {
        BoxedError {
            inner: Box::new(err),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<UrlParseError> for BoxedError {
    fn from(err: UrlParseError) -> Self {
        BoxedError {
            inner: Box::new(err),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<SerenityError> for BoxedError {
    fn from(err: SerenityError) -> Self {
        BoxedError {
            inner: Box::new(err),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<ReqwestError> for BoxedError {
    fn from(err: ReqwestError) -> Self {
        BoxedError {
            inner: Box::new(err),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<HttpError> for BoxedError {
    fn from(err: HttpError) -> Self {
        BoxedError {
            inner: Box::new(err),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<AskamaError> for BoxedError {
    fn from(err: AskamaError) -> Self {
        BoxedError {
            inner: Box::new(err),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<RegexError> for BoxedError {
    fn from(err: RegexError) -> Self {
        BoxedError {
            inner: Box::new(err),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<ParseIntError> for BoxedError {
    fn from(err: ParseIntError) -> Self {
        BoxedError {
            inner: Box::new(err),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<JoinError> for BoxedError {
    fn from(err: JoinError) -> Self {
        BoxedError {
            inner: Box::new(err),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<StripeError> for BoxedError {
    fn from(err: StripeError) -> Self {
        // TODO: stripe::Error is not Sync
        let simple_error = SimpleError::new(format!("{:#?}", err));
        BoxedError {
            inner: Box::new(simple_error),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<Elapsed> for BoxedError {
    fn from(err: Elapsed) -> Self {
        BoxedError {
            inner: Box::new(err),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<sqlx::Error> for BoxedError {
    fn from(err: sqlx::Error) -> Self {
        BoxedError {
            inner: Box::new(err),
            backtrace: Backtrace::new(),
        }
    }
}

impl From<crate::meetup::Error> for BoxedError {
    fn from(err: crate::meetup::Error) -> Self {
        BoxedError {
            inner: Box::new(err),
            backtrace: Backtrace::new(),
        }
    }
}
