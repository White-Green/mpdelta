use crate::render::Renderer;
use async_trait::async_trait;
use dashmap::DashMap;
use futures::FutureExt;
use mpdelta_core::component::instance::ComponentInstanceHandle;
use mpdelta_core::component::link::MarkerLink;
use mpdelta_core::component::marker_pin::MarkerPinHandle;
use mpdelta_core::component::parameter::{AudioRequiredParamsFixed, ImageRequiredParamsFixed, Parameter, ParameterSelect, ParameterType, ParameterValueType};
use mpdelta_core::core::{ComponentRendererBuilder, IdGenerator};
use mpdelta_core::ptr::StaticPointer;
use mpdelta_core::time::TimelineTime;
use mpdelta_core::usecase::RealtimeComponentRenderer;
use mpdelta_differential::CollectCachedTimeError;
use qcell::{TCell, TCellOwner};
use std::convert::Infallible;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;
use tokio::runtime::Handle;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::{oneshot, RwLock};

mod render;
mod thread_cancel;

pub use render::TimeMap;

pub struct MPDeltaRendererBuilder<K: 'static, Id, C, ImageCombinerBuilder, AudioCombinerBuilder> {
    id: Arc<Id>,
    controller_builder: Arc<C>,
    image_combiner_builder: Arc<ImageCombinerBuilder>,
    audio_combiner_builder: Arc<AudioCombinerBuilder>,
    key: Arc<RwLock<TCellOwner<K>>>,
    runtime: Handle,
}

impl<K, Id, C, ImageCombinerBuilder, AudioCombinerBuilder> MPDeltaRendererBuilder<K, Id, C, ImageCombinerBuilder, AudioCombinerBuilder> {
    pub fn new(id: Arc<Id>, image_combiner_builder: Arc<ImageCombinerBuilder>, controller_builder: Arc<C>, audio_combiner_builder: Arc<AudioCombinerBuilder>, key: Arc<RwLock<TCellOwner<K>>>, runtime: Handle) -> MPDeltaRendererBuilder<K, Id, C, ImageCombinerBuilder, AudioCombinerBuilder> {
        MPDeltaRendererBuilder {
            id,
            controller_builder,
            image_combiner_builder,
            audio_combiner_builder,
            key,
            runtime,
        }
    }
}

#[async_trait]
impl<K, T, C, ImageCombinerBuilder, AudioCombinerBuilder, Id> ComponentRendererBuilder<K, T> for MPDeltaRendererBuilder<K, Id, C, ImageCombinerBuilder, AudioCombinerBuilder>
where
    K: Send + Sync + 'static,
    T: ParameterValueType + 'static,
    C: MPDeltaRenderingControllerBuilder + 'static,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageSizeRequest, Param = ImageRequiredParamsFixed> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = (), Param = AudioRequiredParamsFixed> + 'static,
    Id: IdGenerator + 'static,
{
    type Err = Infallible;
    type Renderer = MPDeltaRenderer<K, T, C, ImageCombinerBuilder, AudioCombinerBuilder>;

    async fn create_renderer(&self, component: &ComponentInstanceHandle<K, T>) -> Result<Self::Renderer, Self::Err> {
        let (sender, future) = rendering_loop(Arc::clone(&self.key), component.clone(), &*self.controller_builder, Arc::clone(&self.image_combiner_builder), Arc::clone(&self.audio_combiner_builder), Handle::current());
        self.runtime.spawn(future);
        Ok(MPDeltaRenderer {
            key: Arc::clone(&self.key),
            component: component.clone(),
            controller_builder: Arc::clone(&self.controller_builder),
            image_combiner_builder: Arc::clone(&self.image_combiner_builder),
            audio_combiner_builder: Arc::clone(&self.audio_combiner_builder),
            runtime: self.runtime.clone(),
            loop_sender: Mutex::new(sender),
        })
    }
}

enum RenderingMessage<K: 'static, T: ParameterValueType> {
    RequestRenderFrame { frame: usize, ret: oneshot::Sender<Result<T::Image, RenderError<K, T>>> },
}

struct RenderingCache<T> {
    map: DashMap<usize, tokio::sync::OnceCell<T>>,
}

impl<T> Default for RenderingCache<T> {
    fn default() -> Self {
        RenderingCache::new()
    }
}

impl<T> RenderingCache<T> {
    fn new() -> RenderingCache<T> {
        RenderingCache { map: DashMap::new() }
    }

    async fn get_or_insert_with<'a, F1, Fut, F2, Ret>(&'a self, frame: usize, f: F1, ret: F2) -> Ret
    where
        F1: FnOnce() -> Fut + 'a,
        Fut: Future<Output = T> + 'a,
        F2: FnOnce(&T) -> Ret,
    {
        let cell = self.map.entry(frame).or_default().downgrade();
        cell.get_or_init(f).map(ret).await
    }

    async fn get_or_try_insert_with<F1, Fut, F2, Ret, Err>(&self, frame: usize, f: F1, ret: F2) -> Ret
    where
        F1: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, Err>>,
        F2: FnOnce(Result<&T, Err>) -> Ret,
    {
        let cell = self.map.entry(frame).or_default().downgrade();
        cell.get_or_try_init(f).map(ret).await
    }

    fn remove(&self, frame: usize) {
        self.map.remove(&frame);
    }
}

pub enum RenderingControllerItem {
    RequestRender { frame: usize },
    RemoveCache { frame: usize },
}

pub trait MPDeltaRenderingControllerBuilder: Send + Sync {
    type Controller<F: Fn(RenderingControllerItem) + Send + Sync>: MPDeltaRenderingController;
    fn create<F: Fn(RenderingControllerItem) + Send + Sync>(&self, f: F) -> Self::Controller<F>;
}

pub trait MPDeltaRenderingController {
    fn on_request_render(&self, frame: usize);
}

fn rendering_loop<K, T, C, ImageCombinerBuilder, AudioCombinerBuilder>(
    key: Arc<RwLock<TCellOwner<K>>>,
    component: ComponentInstanceHandle<K, T>,
    controller_builder: &C,
    image_combiner_builder: Arc<ImageCombinerBuilder>,
    audio_combiner_builder: Arc<AudioCombinerBuilder>,
    runtime: Handle,
) -> (UnboundedSender<RenderingMessage<K, T>>, impl Future<Output = ()> + Send + 'static)
where
    K: 'static + Send + Sync,
    T: ParameterValueType + 'static,
    C: MPDeltaRenderingControllerBuilder,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageSizeRequest, Param = ImageRequiredParamsFixed> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = (), Param = AudioRequiredParamsFixed> + 'static,
{
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let (controller_sender, mut controller_receiver) = tokio::sync::mpsc::unbounded_channel();
    let renderer = Arc::new(Renderer::new(runtime.clone(), component.clone(), image_combiner_builder, audio_combiner_builder));
    let cache = Arc::new(RenderingCache::new());
    let _controller = controller_builder.create(move |message| {
        let _ = controller_sender.send(message);
    });
    let future = async move {
        loop {
            tokio::select! {
                biased;
                message = receiver.recv() => {
                    let Some(message) = message else { return; };
                    match message {
                        RenderingMessage::RequestRenderFrame { frame, ret } => {
                            let renderer = Arc::clone(&renderer);
                            let cache = Arc::clone(&cache);
                            let component = component.clone();
                            let key = Arc::clone(&key);
                            runtime.spawn(async move {
                                cache.get_or_try_insert_with(frame, || key.read_owned().then(|key| renderer.render(frame, ParameterType::Image(()), Arc::new(key)))
                                    .map(|result| match result {
                                        Ok(Parameter::Image(value)) => Ok(value),
                                        Ok(value) => Err(RenderError::OutputTypeMismatch {
                                            component,
                                            expect: Parameter::Image(()),
                                            actual: value.select(),
                                        }),
                                        Err(e) => Err(e),
                                    }), move |result| { let _ = ret.send(result.cloned()); }).await
                            });
                        }
                    }
                }
                Some(message) = controller_receiver.recv() => {
                    match message {
                        RenderingControllerItem::RequestRender {frame} => {
                            let renderer = Arc::clone(&renderer);
                            let cache = Arc::clone(&cache);
                            let component = component.clone();
                            let key = Arc::clone(&key);
                            runtime.spawn(async move {
                                cache.get_or_try_insert_with(frame, || key.read_owned().then(|key| renderer.render(frame, ParameterType::Image(()), Arc::new(key)))
                                    .map(|result| match result {
                                        Ok(Parameter::Image(value)) => Ok(value),
                                        Ok(value) => Err(RenderError::OutputTypeMismatch {
                                            component,
                                            expect: Parameter::Image(()),
                                            actual: value.select(),
                                        }),
                                        Err(e) => Err(e),
                                    }), |_| ()).await
                            });
                        }
                        RenderingControllerItem::RemoveCache {frame} => {
                            cache.remove(frame);
                        }
                    }
                }
            }
        }
    };
    (sender, future)
}

pub struct MPDeltaRenderer<K: 'static, T: ParameterValueType, C, ImageCombinerBuilder, AudioCombinerBuilder> {
    key: Arc<RwLock<TCellOwner<K>>>,
    component: ComponentInstanceHandle<K, T>,
    controller_builder: Arc<C>,
    image_combiner_builder: Arc<ImageCombinerBuilder>,
    audio_combiner_builder: Arc<AudioCombinerBuilder>,
    runtime: Handle,
    loop_sender: Mutex<UnboundedSender<RenderingMessage<K, T>>>,
}

#[derive(Error)]
pub enum RenderError<K: 'static, T: ParameterValueType> {
    #[error("invalid component: {0:?}")]
    InvalidComponent(ComponentInstanceHandle<K, T>),
    #[error("the output type by {component:?} is mismatch; expected: {expect:?}, but got {actual:?}")]
    OutputTypeMismatch {
        component: ComponentInstanceHandle<K, T>,
        expect: Parameter<ParameterSelect>,
        actual: Parameter<ParameterSelect>,
    },
    #[error("invalid link graph")]
    InvalidLinkGraph,
    #[error("invalid marker: {0:?}")]
    InvalidMarker(MarkerPinHandle<K>),
    #[error("invalid marker: {0:?}")]
    InvalidMarkerLink(StaticPointer<TCell<K, MarkerLink<K>>>),
    #[error("{index}-th variable parameter of {component:?} is invalid")]
    InvalidVariableParameter { component: ComponentInstanceHandle<K, T>, index: usize },
    #[error("time {at:?} is out of range {range:?}")]
    RenderTargetTimeOutOfRange { component: ComponentInstanceHandle<K, T>, range: Range<TimelineTime>, at: TimelineTime },
    #[error("required type value is not provided")]
    NotProvided,
    #[error("timeout")]
    Timeout,
}

impl<K, T: ParameterValueType> From<CollectCachedTimeError<K>> for RenderError<K, T> {
    fn from(value: CollectCachedTimeError<K>) -> Self {
        match value {
            CollectCachedTimeError::InvalidMarkerLink(link) => RenderError::InvalidMarkerLink(link),
            CollectCachedTimeError::InvalidMarker(pin) => RenderError::InvalidMarker(pin),
            CollectCachedTimeError::InvalidLinkGraph => RenderError::InvalidLinkGraph,
        }
    }
}

impl<K, T: ParameterValueType> Debug for RenderError<K, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::InvalidComponent(c) => f.debug_tuple("InvalidComponent").field(c).finish(),
            RenderError::OutputTypeMismatch { component, expect, actual } => f.debug_struct("OutputTypeMismatch").field("component", component).field("expect", expect).field("actual", actual).finish(),
            RenderError::InvalidLinkGraph => f.debug_struct("InvalidLinkGraph").finish(),
            RenderError::InvalidMarker(m) => f.debug_tuple("InvalidMarker").field(m).finish(),
            RenderError::InvalidMarkerLink(l) => f.debug_tuple("InvalidMarkerLink").field(l).finish(),
            RenderError::InvalidVariableParameter { component, index } => f.debug_struct("InvalidVariableParameter").field("component", component).field("index", index).finish(),
            RenderError::RenderTargetTimeOutOfRange { component, range, at } => f.debug_struct("FrameOutOfRange").field("component", component).field("range", range).field("at", at).finish(),
            RenderError::NotProvided => f.debug_struct("NotProvided").finish(),
            RenderError::Timeout => f.debug_struct("Timeout").finish(),
        }
    }
}

impl<K, T, C, ImageCombinerBuilder, AudioCombinerBuilder> RealtimeComponentRenderer<T> for MPDeltaRenderer<K, T, C, ImageCombinerBuilder, AudioCombinerBuilder>
where
    K: Send + Sync,
    T: ParameterValueType + 'static,
    C: MPDeltaRenderingControllerBuilder + 'static,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageSizeRequest, Param = ImageRequiredParamsFixed> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = (), Param = AudioRequiredParamsFixed> + 'static,
{
    type Err = RenderError<K, T>;

    fn get_frame_count(&self) -> usize {
        600
    }

    fn render_frame(&self, frame: usize) -> Result<T::Image, Self::Err> {
        let mut loop_sender = self.loop_sender.lock().unwrap();
        self.runtime
            .block_on(async {
                tokio::time::timeout(Duration::from_millis(16), async {
                    loop {
                        let (sender, receiver) = oneshot::channel();
                        let mut message = Some(RenderingMessage::RequestRenderFrame { frame, ret: sender });
                        loop {
                            match loop_sender.send(message.take().unwrap()) {
                                Ok(()) => break,
                                Err(SendError(failed_message)) => {
                                    message = Some(failed_message);
                                    let (new_loop_sender, fut) = rendering_loop(Arc::clone(&self.key), self.component.clone(), &*self.controller_builder, Arc::clone(&self.image_combiner_builder), Arc::clone(&self.audio_combiner_builder), self.runtime.clone());
                                    self.runtime.spawn(fut);
                                    *loop_sender = new_loop_sender;
                                }
                            };
                        }
                        match receiver.await {
                            Ok(Ok(result)) => break Ok(result),
                            Ok(Err(result)) => {
                                eprintln!("{}", result);
                                break Err(result);
                            }
                            Err(_) => {}
                        }
                    }
                })
                .await
            })
            .unwrap_or(Err(RenderError::Timeout))
    }

    fn sampling_rate(&self) -> u32 {
        48_000
    }

    fn mix_audio(&self, _offset: usize, _length: usize) -> Result<T::Audio, Self::Err> {
        todo!()
    }

    fn render_param(&self, _param: Parameter<ParameterSelect>) -> Result<Parameter<T>, Self::Err> {
        todo!()
    }
}

pub trait CombinerBuilder<Data>: Send + Sync {
    type Request;
    type Param;
    type Combiner: Combiner<Data, Param = Self::Param>;
    fn new_combiner(&self, request: Self::Request) -> Self::Combiner;
}

pub trait Combiner<Data>: Send + Sync {
    type Param;
    fn add(&mut self, data: Data, param: Self::Param);
    fn collect(self) -> Data;
}

#[derive(Debug, Clone, Copy)]
pub struct ImageSizeRequest {
    pub width: f32,
    pub height: f32,
}

impl PartialEq for ImageSizeRequest {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width && self.height == other.height
    }
}

impl Eq for ImageSizeRequest {}

impl Hash for ImageSizeRequest {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.width.to_ne_bytes().hash(state);
        self.height.to_ne_bytes().hash(state);
    }
}
