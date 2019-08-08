use crate::meetup_api::Error as MeetupApiError;
use askama::Error as AskamaError;
use backtrace::Backtrace;
use chrono::format::ParseError as ChronoParseError;
use hyper::http::Error as HttpError;
use redis::RedisError;
use reqwest::Error as ReqwestError;
use serenity::Error as SerenityError;
use simple_error::SimpleError;
use tokio::timer::Error as TokioTimerError;
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
            "Encountered the following error:\n{}\nBacktrace:\n{:?}",
            self.inner, self.backtrace
        )
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

impl From<MeetupApiError> for BoxedError {
    fn from(err: MeetupApiError) -> Self {
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

impl From<TokioTimerError> for BoxedError {
    fn from(err: TokioTimerError) -> Self {
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
