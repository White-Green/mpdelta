use crate::view_model_util::use_arc;
use crate::viewmodel::ViewModelParams;
use crate::AudioTypePlayer;
use crossbeam_utils::atomic::AtomicCell;
use mpdelta_async_runtime::AsyncRuntime;
use mpdelta_core::common::mixed_fraction::atomic::AtomicMixedFraction;
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::instance::ComponentInstanceId;
use mpdelta_core::component::marker_pin::MarkerTime;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::project::{RootComponentClass, RootComponentClassHandle};
use mpdelta_core::ptr::StaticPointer;
use mpdelta_core::time::TimelineTime;
use mpdelta_message_router::handler::{IntoFunctionHandler, MessageHandlerBuilder};
use mpdelta_message_router::{MessageHandler, MessageRouter};
use std::mem;
use std::sync::atomic::AtomicBool;
use std::sync::{atomic, Arc, Mutex, RwLock as StdRwLock};
use std::time::Instant;

#[derive(Debug)]
pub enum GlobalUIEvent<T: ParameterValueType> {
    BeginRenderFrame,
    EndRenderFrame,
    SelectRootComponentClass(Option<RootComponentClassHandle<T>>),
    SelectComponentInstance(Option<ComponentInstanceId>),
}

impl<T> Clone for GlobalUIEvent<T>
where
    T: ParameterValueType,
{
    fn clone(&self) -> Self {
        match self {
            GlobalUIEvent::BeginRenderFrame => GlobalUIEvent::BeginRenderFrame,
            GlobalUIEvent::EndRenderFrame => GlobalUIEvent::EndRenderFrame,
            GlobalUIEvent::SelectRootComponentClass(target) => GlobalUIEvent::SelectRootComponentClass(target.clone()),
            GlobalUIEvent::SelectComponentInstance(target) => GlobalUIEvent::SelectComponentInstance(*target),
        }
    }
}

impl<T> PartialEq for GlobalUIEvent<T>
where
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

impl<T> Eq for GlobalUIEvent<T> where T: ParameterValueType {}

#[derive(Clone, PartialEq, Eq)]
pub enum Message {
    BeginRenderFrame,
    EndRenderFrame,
}

pub trait GlobalUIEventHandler<T: ParameterValueType> {
    fn handle(&self, event: GlobalUIEvent<T>);
}

pub trait GlobalUIState<T: ParameterValueType>: Send + Sync + 'static {
    fn register_global_ui_event_handler(&self, handler: Arc<impl GlobalUIEventHandler<T> + Send + Sync + 'static>);
    fn begin_render_frame(&self);
    fn end_render_frame(&self);
    fn playing(&self) -> bool;
    fn play(&self);
    fn pause(&self);
    fn component_length(&self) -> Option<MarkerTime>;
    fn set_component_length(&self, length: MarkerTime);
    fn seek(&self) -> MarkerTime;
    fn set_seek(&self, seek: MarkerTime);
    fn select_root_component_class(&self, target: &RootComponentClassHandle<T>);
    fn unselect_root_component_class(&self);
    fn select_component_instance(&self, target: &ComponentInstanceId);
}

pub struct GlobalUIStateImpl<T, A, H, Runtime> {
    request_play: Arc<AtomicBool>,
    playing: Arc<AtomicBool>,
    component_length: Arc<AtomicCell<Option<MarkerTime>>>,
    seek: Arc<AtomicMixedFraction>,
    audio_player: Arc<A>,
    handlers: StdRwLock<Vec<Arc<dyn GlobalUIEventHandler<T> + Send + Sync>>>,
    message_router: MessageRouter<H, Runtime>,
}

impl<T: ParameterValueType> GlobalUIStateImpl<T, (), (), ()> {
    pub fn new<P: ViewModelParams<T>>(params: &P) -> GlobalUIStateImpl<T, P::AudioPlayer, impl MessageHandler<Message, P::AsyncRuntime> + Send + Sync, P::AsyncRuntime> {
        let request_play = Arc::new(AtomicBool::new(false));
        let playing = Arc::new(AtomicBool::new(false));
        let component_length = Arc::new(AtomicCell::new(None::<MarkerTime>));
        let seek = Arc::new(AtomicMixedFraction::new(MixedFraction::ZERO));
        let play_start_seek = AtomicMixedFraction::new(MixedFraction::ZERO);
        let play_start_at = Mutex::new(Instant::now());
        let message_router = MessageRouter::builder()
            .handle(|handler| {
                handler.filter(|message| *message == Message::BeginRenderFrame).handle({
                    use_arc!(request_play, playing, component_length, seek);
                    move |_| {
                        let now = Instant::now();
                        let mut lock = play_start_at.lock().unwrap();
                        if request_play.compare_exchange(true, false, atomic::Ordering::AcqRel, atomic::Ordering::Acquire).is_ok() && playing.compare_exchange(false, true, atomic::Ordering::AcqRel, atomic::Ordering::Acquire).is_ok() {
                            *lock = now;
                            play_start_seek.store(seek.load(atomic::Ordering::Acquire), atomic::Ordering::Release);
                        }
                        if playing.load(atomic::Ordering::Acquire) {
                            seek.store(
                                (MixedFraction::from_f64((now - *lock).as_secs_f64()) + play_start_seek.load(atomic::Ordering::Acquire)) % component_length.load().map_or(MixedFraction::from_integer(10), |time| time.value()),
                                atomic::Ordering::Release,
                            );
                        }
                    }
                })
            })
            .build(params.runtime().clone());
        GlobalUIStateImpl {
            request_play,
            playing,
            component_length,
            seek,
            audio_player: Arc::clone(params.audio_player()),
            handlers: StdRwLock::new(Vec::new()),
            message_router,
        }
    }
}

impl<T, A, H, Runtime> GlobalUIStateImpl<T, A, H, Runtime>
where
    T: ParameterValueType,
    A: AudioTypePlayer<T::Audio> + 'static,
    H: MessageHandler<Message, Runtime> + Send + Sync + 'static,
{
    fn handle_by(handlers: &StdRwLock<Vec<Arc<dyn GlobalUIEventHandler<T> + Send + Sync>>>, event: GlobalUIEvent<T>) {
        for handler in handlers.read().unwrap().iter() {
            handler.handle(event.clone());
        }
    }

    fn handle(&self, event: GlobalUIEvent<T>) {
        Self::handle_by(&self.handlers, event);
    }
}

impl<T, A, H, Runtime> GlobalUIState<T> for GlobalUIStateImpl<T, A, H, Runtime>
where
    T: ParameterValueType,
    A: AudioTypePlayer<T::Audio> + 'static,
    H: MessageHandler<Message, Runtime> + Send + Sync + 'static,
    Runtime: AsyncRuntime<()> + Clone + 'static,
{
    fn register_global_ui_event_handler(&self, handler: Arc<impl GlobalUIEventHandler<T> + Send + Sync + 'static>) {
        self.handlers.write().unwrap().push(handler as Arc<dyn GlobalUIEventHandler<T> + Send + Sync + 'static>);
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
        self.audio_player.play();
    }

    fn pause(&self) {
        self.playing.store(false, atomic::Ordering::Release);
        self.audio_player.pause();
        self.audio_player.seek(TimelineTime::new(self.seek().value()));
    }

    fn component_length(&self) -> Option<MarkerTime> {
        self.component_length.load()
    }

    fn set_component_length(&self, length: MarkerTime) {
        self.component_length.store(Some(length));
    }

    fn seek(&self) -> MarkerTime {
        MarkerTime::new(self.seek.load(atomic::Ordering::Acquire)).unwrap()
    }

    fn set_seek(&self, seek: MarkerTime) {
        self.seek.store(seek.value(), atomic::Ordering::Release);
        self.audio_player.seek(TimelineTime::new(seek.value()));
    }

    fn select_root_component_class(&self, target: &StaticPointer<tokio::sync::RwLock<RootComponentClass<T>>>) {
        self.handle(GlobalUIEvent::SelectRootComponentClass(Some(target.clone())));
    }

    fn unselect_root_component_class(&self) {
        self.handle(GlobalUIEvent::SelectRootComponentClass(None));
        self.handle(GlobalUIEvent::SelectComponentInstance(None));
    }

    fn select_component_instance(&self, target: &ComponentInstanceId) {
        self.handle(GlobalUIEvent::SelectComponentInstance(Some(*target)));
    }
}
