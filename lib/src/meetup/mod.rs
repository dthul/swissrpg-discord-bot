pub mod api;
pub mod error;
pub mod oauth2;
pub mod sync;
pub mod util;

pub use error::Error;

pub const MAX_EVENT_NAME_UTF16_LEN: usize = 80; // Maximum length in UTF-16 code units
