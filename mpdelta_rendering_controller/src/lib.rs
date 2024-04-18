use mpdelta_renderer::{MPDeltaRenderingController, MPDeltaRenderingControllerBuilder, RenderingControllerItem};
use std::collections::BTreeSet;
use std::sync::Mutex;

#[derive(Default)]
pub struct LookaheadRenderingControllerBuilder {}

impl LookaheadRenderingControllerBuilder {
    pub fn new() -> LookaheadRenderingControllerBuilder {
        LookaheadRenderingControllerBuilder {}
    }
}

impl MPDeltaRenderingControllerBuilder for LookaheadRenderingControllerBuilder {
    type Controller<F: Fn(RenderingControllerItem) + Send + Sync + 'static> = LookaheadRenderingController<F>;

    fn create<F: Fn(RenderingControllerItem) + Send + Sync + 'static>(&self, f: F) -> Self::Controller<F> {
        LookaheadRenderingController::new(f)
    }
}

pub struct LookaheadRenderingController<F> {
    f: F,
    in_cache: Mutex<BTreeSet<usize>>,
}

impl<F> LookaheadRenderingController<F> {
    fn new(f: F) -> LookaheadRenderingController<F> {
        LookaheadRenderingController { f, in_cache: Mutex::new(BTreeSet::new()) }
    }
}

impl<F: Fn(RenderingControllerItem) + Send + Sync + 'static> MPDeltaRenderingController for LookaheadRenderingController<F> {
    fn on_request_render(&self, frame: usize) {
        let mut in_cache = self.in_cache.lock().unwrap();
        let remove_frames = in_cache.range(..frame.saturating_sub(20)).copied().collect::<Vec<_>>();
        remove_frames.iter().copied().for_each(|frame| (self.f)(RenderingControllerItem::RemoveCache { frame }));
        remove_frames.iter().for_each(|frame| {
            in_cache.remove(frame);
        });
        let new_frames = (frame..).take(40).filter(|f| !in_cache.contains(f)).collect::<Vec<_>>();
        new_frames.iter().copied().for_each(|frame| (self.f)(RenderingControllerItem::RequestRender { frame }));
        in_cache.extend(new_frames);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_lookahead_rendering_controller() {
        let add_frames = Arc::new(Mutex::new(Vec::new()));
        let remove_frames = Arc::new(Mutex::new(Vec::new()));
        let af = Arc::clone(&add_frames);
        let rf = Arc::clone(&remove_frames);
        let controller = LookaheadRenderingController::new(move |item| match item {
            RenderingControllerItem::RequestRender { frame } => {
                add_frames.lock().unwrap().push(frame);
            }
            RenderingControllerItem::RemoveCache { frame } => {
                remove_frames.lock().unwrap().push(frame);
            }
        });
        controller.on_request_render(0);
        assert_eq!(*af.lock().unwrap(), (0..40).collect::<Vec<_>>());
        assert_eq!(*rf.lock().unwrap(), []);
        controller.on_request_render(1);
        assert_eq!(*af.lock().unwrap(), (0..41).collect::<Vec<_>>());
        assert_eq!(*rf.lock().unwrap(), []);
        controller.on_request_render(35);
        assert_eq!(*af.lock().unwrap(), (0..75).collect::<Vec<_>>());
        assert_eq!(*rf.lock().unwrap(), (0..15).collect::<Vec<_>>());
    }
}
