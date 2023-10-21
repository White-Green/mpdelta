use lru::LruCache;
use mpdelta_renderer::{MPDeltaRenderingController, MPDeltaRenderingControllerBuilder, RenderingControllerItem};
use std::num::NonZeroUsize;
use std::sync::Mutex;

#[derive(Default)]
pub struct LRUCacheRenderingControllerBuilder {}

impl LRUCacheRenderingControllerBuilder {
    pub fn new() -> LRUCacheRenderingControllerBuilder {
        LRUCacheRenderingControllerBuilder {}
    }
}

impl MPDeltaRenderingControllerBuilder for LRUCacheRenderingControllerBuilder {
    type Controller<F: Fn(RenderingControllerItem) + Send + Sync> = LRUCacheRenderingController<F>;

    fn create<F: Fn(RenderingControllerItem) + Send + Sync>(&self, f: F) -> Self::Controller<F> {
        LRUCacheRenderingController::new(f)
    }
}

pub struct LRUCacheRenderingController<F> {
    f: F,
    lru: Mutex<LruCache<usize, ()>>,
}

impl<F> LRUCacheRenderingController<F> {
    fn new(f: F) -> LRUCacheRenderingController<F> {
        LRUCacheRenderingController {
            f,
            lru: Mutex::new(LruCache::new(NonZeroUsize::new(5).unwrap())),
        }
    }
}

impl<F: Fn(RenderingControllerItem) + Send + Sync> MPDeltaRenderingController for LRUCacheRenderingController<F> {
    fn on_request_render(&self, frame: usize) {
        let mut guard = self.lru.lock().unwrap();
        let Some((drop_frame, _)) = guard.push(frame, ()) else {
            return;
        };
        drop(guard);
        (self.f)(RenderingControllerItem::RemoveCache { frame: drop_frame });
    }
}
