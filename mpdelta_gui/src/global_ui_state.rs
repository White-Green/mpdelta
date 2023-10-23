use crate::message_router::handler::IntoFunctionHandler;
use crate::message_router::handler::MessageHandlerBuilder;
use crate::message_router::{MessageHandler, MessageRouter};
use crate::view_model_util::use_arc;
use crate::viewmodel::ViewModelParams;
use mpdelta_async_runtime::AsyncRuntime;
use mpdelta_core::component::instance::ComponentInstanceHandle;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::project::{RootComponentClass, RootComponentClassHandle};
use mpdelta_core::ptr::StaticPointer;
use std::mem;
use std::sync::atomic;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::{Arc, Mutex, RwLock as StdRwLock};
use std::time::Instant;

#[derive(Debug)]
pub enum GlobalUIEvent<K: 'static, T: ParameterValueType> {
    BeginRenderFrame,
    EndRenderFrame,
    SelectRootComponentClass(Option<RootComponentClassHandle<K, T>>),
    SelectComponentInstance(Option<ComponentInstanceHandle<K, T>>),
}

impl<K, T> Clone for GlobalUIEvent<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    fn clone(&self) -> Self {
        match self {
            GlobalUIEvent::BeginRenderFrame => GlobalUIEvent::BeginRenderFrame,
            GlobalUIEvent::EndRenderFrame => GlobalUIEvent::EndRenderFrame,
            GlobalUIEvent::SelectRootComponentClass(target) => GlobalUIEvent::SelectRootComponentClass(target.clone()),
            GlobalUIEvent::SelectComponentInstance(target) => GlobalUIEvent::SelectComponentInstance(target.clone()),
        }
    }
}

impl<K, T> PartialEq for GlobalUIEvent<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    fn eq(&self, other: &Self) -> bool {
        if mem::discriminant(self) != mem::discriminant(other) {
            return false;
        }
        match (self, other) {
            (GlobalUIEvent::BeginRenderFrame, GlobalUIEvent::BeginRenderFrame) => true,
            (GlobalUIEvent::EndRenderFrame, GlobalUIEvent::EndRenderFrame) => true,
            (GlobalUIEvent::SelectRootComponentClass(target1), GlobalUIEvent::SelectRootComponentClass(target2)) => target1 == target2,
            (GlobalUIEvent::SelectComponentInstance(target1), GlobalUIEvent::SelectComponentInstance(target2)) => target1 == target2,
            _ => unreachable!(),
        }
    }
}

impl<K, T> Eq for GlobalUIEvent<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
}

#[derive(Clone, PartialEq, Eq)]
pub enum Message {
    BeginRenderFrame,
    EndRenderFrame,
}

pub trait GlobalUIEventHandler<K: 'static, T: ParameterValueType> {
    fn handle(&self, event: GlobalUIEvent<K, T>);
}

pub trait GlobalUIState<K: 'static, T: ParameterValueType>: Send + Sync + 'static {
    fn register_global_ui_event_handler(&self, handler: Arc<impl GlobalUIEventHandler<K, T> + Send + Sync + 'static>);
    fn begin_render_frame(&self);
    fn end_render_frame(&self);
    fn playing(&self) -> bool;
    fn play(&self);
    fn pause(&self);
    fn seek(&self) -> usize;
    fn set_seek(&self, _seek: usize);
    fn select_root_component_class(&self, target: &RootComponentClassHandle<K, T>);
    fn unselect_root_component_class(&self);
    fn select_component_instance(&self, target: &ComponentInstanceHandle<K, T>);
}

pub struct GlobalUIStateImpl<K: 'static, T, H, Runtime> {
    request_play: Arc<AtomicBool>,
    playing: Arc<AtomicBool>,
    seek: Arc<AtomicUsize>,
    handlers: StdRwLock<Vec<Arc<dyn GlobalUIEventHandler<K, T> + Send + Sync>>>,
    message_router: MessageRouter<H, Runtime>,
}

impl<K: 'static, T: ParameterValueType> GlobalUIStateImpl<K, T, (), ()> {
    pub fn new<P: ViewModelParams<K, T>>(params: &P) -> GlobalUIStateImpl<K, T, impl MessageHandler<Message, P::AsyncRuntime> + Send + Sync, P::AsyncRuntime> {
        let request_play = Arc::new(AtomicBool::new(false));
        let playing = Arc::new(AtomicBool::new(false));
        let seek = Arc::new(AtomicUsize::new(0));
        let play_start_seek = AtomicUsize::new(0);
        let play_start_at = Mutex::new(Instant::now());
        let message_router = MessageRouter::builder()
            .handle(|handler| {
                handler.filter(|message| *message == Message::BeginRenderFrame).handle({
                    use_arc!(request_play, playing, seek);
                    move |_| {
                        let now = Instant::now();
                        let mut lock = play_start_at.lock().unwrap();
                        if request_play.compare_exchange(true, false, atomic::Ordering::AcqRel, atomic::Ordering::Acquire).is_ok() && playing.compare_exchange(false, true, atomic::Ordering::AcqRel, atomic::Ordering::Acquire).is_ok() {
                            *lock = now;
                            play_start_seek.store(seek.load(atomic::Ordering::Acquire), atomic::Ordering::Release);
                        }
                        if playing.load(atomic::Ordering::Acquire) {
                            seek.store((((now - *lock).as_secs_f64() * 60.) as usize + play_start_seek.load(atomic::Ordering::Acquire)) % 600, atomic::Ordering::Release);
                        }
                    }
                })
            })
            .build(params.runtime().clone());
        GlobalUIStateImpl {
            request_play,
            playing,
            seek,
            handlers: StdRwLock::new(Vec::new()),
            message_router,
        }
    }
}

impl<K: 'static, T: ParameterValueType, H: MessageHandler<Message, Runtime> + Send + Sync + 'static, Runtime> GlobalUIStateImpl<K, T, H, Runtime> {
    fn handle_by(handlers: &StdRwLock<Vec<Arc<dyn GlobalUIEventHandler<K, T> + Send + Sync>>>, event: GlobalUIEvent<K, T>) {
        for handler in handlers.read().unwrap().iter() {
            handler.handle(event.clone());
        }
    }

    fn handle(&self, event: GlobalUIEvent<K, T>) {
        Self::handle_by(&self.handlers, event);
    }
}

impl<K, T, H, Runtime> GlobalUIState<K, T> for GlobalUIStateImpl<K, T, H, Runtime>
where
    K: 'static,
    T: ParameterValueType,
    H: MessageHandler<Message, Runtime> + Send + Sync + 'static,
    Runtime: AsyncRuntime<()> + Clone + 'static,
{
    fn register_global_ui_event_handler(&self, handler: Arc<impl GlobalUIEventHandler<K, T> + Send + Sync + 'static>) {
        self.handlers.write().unwrap().push(handler as Arc<dyn GlobalUIEventHandler<K, T> + Send + Sync + 'static>);
    }

    fn begin_render_frame(&self) {
        self.message_router.handle(Message::BeginRenderFrame);
        self.handle(GlobalUIEvent::BeginRenderFrame);
    }

    fn end_render_frame(&self) {
        self.message_router.handle(Message::EndRenderFrame);
        self.handle(GlobalUIEvent::EndRenderFrame);
    }

    fn playing(&self) -> bool {
        self.playing.load(atomic::Ordering::Acquire)
    }

    fn play(&self) {
        self.request_play.store(true, atomic::Ordering::Release);
    }

    fn pause(&self) {
        self.playing.store(false, atomic::Ordering::Release);
    }

    fn seek(&self) -> usize {
        self.seek.load(atomic::Ordering::Acquire)
    }

    fn set_seek(&self, seek: usize) {
        self.seek.store(seek, atomic::Ordering::Release);
    }

    fn select_root_component_class(&self, target: &StaticPointer<tokio::sync::RwLock<RootComponentClass<K, T>>>) {
        self.handle(GlobalUIEvent::SelectRootComponentClass(Some(target.clone())));
    }

    fn unselect_root_component_class(&self) {
        self.handle(GlobalUIEvent::SelectRootComponentClass(None));
        self.handle(GlobalUIEvent::SelectComponentInstance(None));
    }

    fn select_component_instance(&self, target: &ComponentInstanceHandle<K, T>) {
        self.handle(GlobalUIEvent::SelectComponentInstance(Some(target.clone())));
    }
}
