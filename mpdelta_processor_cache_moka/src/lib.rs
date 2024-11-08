use moka::future::Cache;
use mpdelta_core::component::processor::{CacheKey, ProcessorCache};
use std::any::Any;
use std::future::Future;
use std::sync::Arc;

#[derive(Clone)]
pub struct MokaCache {
    cache: Cache<Arc<dyn CacheKey>, Arc<dyn Any + Send + Sync>>,
}

impl Default for MokaCache {
    fn default() -> Self {
        Self::new()
    }
}

impl MokaCache {
    pub fn new() -> MokaCache {
        MokaCache { cache: Cache::new(128) }
    }
}

impl ProcessorCache for MokaCache {
    fn insert(&self, key: Arc<dyn CacheKey>, value: Arc<dyn Any + Send + Sync>) -> impl Future<Output = ()> + Send + '_ {
        self.cache.insert(key, value)
    }

    fn get<'a>(&'a self, key: &'a Arc<dyn CacheKey>) -> impl Future<Output = Option<Arc<dyn Any + Send + Sync>>> + Send + 'a {
        self.cache.get(key)
    }

    fn invalidate<'life0, 'life1, 'async_trait>(&'life0 self, key: &'life1 Arc<dyn CacheKey>) -> impl Future<Output = ()> + Send + 'async_trait
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        self.cache.invalidate(key)
    }
}
