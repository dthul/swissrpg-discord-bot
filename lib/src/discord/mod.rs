pub mod sync;

use std::sync::Arc;

#[derive(Clone)]
pub struct CacheAndHttp {
    pub cache: serenity::cache::CacheRwLock,
    pub http: Arc<serenity::http::raw::Http>,
}

impl serenity::http::CacheHttp for CacheAndHttp {
    fn cache(&self) -> Option<&serenity::cache::CacheRwLock> {
        Some(&self.cache)
    }
    fn http(&self) -> &serenity::http::raw::Http {
        &self.http
    }
}

impl serenity::http::CacheHttp for &CacheAndHttp {
    fn cache(&self) -> Option<&serenity::cache::CacheRwLock> {
        Some(&self.cache)
    }
    fn http(&self) -> &serenity::http::raw::Http {
        &self.http
    }
}
