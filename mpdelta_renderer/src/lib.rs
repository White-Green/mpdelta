use crate::heartbeat::{HeartbeatController, HeartbeatMonitor};
use crate::lazy_init::LazyInit;
use crate::render::Renderer;
use arc_swap::ArcSwap;
use archery::ArcTK;
use async_trait::async_trait;
use crossbeam_utils::atomic::AtomicCell;
use futures::FutureExt;
use mpdelta_core::component::instance::ComponentInstanceHandle;
use mpdelta_core::component::link::MarkerLinkHandle;
use mpdelta_core::component::marker_pin::{MarkerPinHandle, MarkerTime};
use mpdelta_core::component::parameter::{ImageRequiredParamsFixed, Parameter, ParameterSelect, ParameterType, ParameterValueType};
use mpdelta_core::core::{ComponentEncoder, ComponentRendererBuilder};
use mpdelta_core::time::TimelineTime;
use mpdelta_core::usecase::RealtimeComponentRenderer;
use mpdelta_differential::CollectCachedTimeError;
use qcell::TCellOwner;
use rpds::RedBlackTreeMap;
use std::convert::Infallible;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::ops::{DerefMut, Range};
use std::sync::{Arc, RwLock as StdRwLock};
use thiserror::Error;
use tokio::runtime::Handle;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::{oneshot, RwLock};

mod heartbeat;
mod lazy_init;
mod render;
mod thread_cancel;

pub use render::{TimeMap, TimeMapSegment};

type ImageCombinerRequest = ImageSizeRequest;
type ImageCombinerParam = ImageRequiredParamsFixed;
type AudioCombinerRequest = TimelineTime;
type AudioCombinerParam = TimeMap;

pub struct DynError(Box<dyn Error + Send + 'static>);

impl Debug for DynError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl Display for DynError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl Error for DynError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.0)
    }
}

pub struct MPDeltaRendererBuilder<K: 'static, C, ImageCombinerBuilder, AudioCombinerBuilder> {
    controller_builder: Arc<C>,
    image_combiner_builder: Arc<ImageCombinerBuilder>,
    audio_combiner_builder: Arc<AudioCombinerBuilder>,
    key: Arc<RwLock<TCellOwner<K>>>,
    runtime: Handle,
}

impl<K, C, ImageCombinerBuilder, AudioCombinerBuilder> MPDeltaRendererBuilder<K, C, ImageCombinerBuilder, AudioCombinerBuilder> {
    pub fn new(image_combiner_builder: Arc<ImageCombinerBuilder>, controller_builder: Arc<C>, audio_combiner_builder: Arc<AudioCombinerBuilder>, key: Arc<RwLock<TCellOwner<K>>>, runtime: Handle) -> MPDeltaRendererBuilder<K, C, ImageCombinerBuilder, AudioCombinerBuilder> {
        MPDeltaRendererBuilder {
            controller_builder,
            image_combiner_builder,
            audio_combiner_builder,
            key,
            runtime,
        }
    }
}

#[async_trait]
impl<K, T, C, ImageCombinerBuilder, AudioCombinerBuilder> ComponentRendererBuilder<K, T> for MPDeltaRendererBuilder<K, C, ImageCombinerBuilder, AudioCombinerBuilder>
where
    K: Send + Sync + 'static,
    T: ParameterValueType + 'static,
    C: MPDeltaRenderingControllerBuilder + 'static,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
{
    type Err = Infallible;
    type Renderer = MPDeltaRenderer<K, T, C, ImageCombinerBuilder, AudioCombinerBuilder>;

    async fn create_renderer(&self, component: &ComponentInstanceHandle<K, T>) -> Result<Self::Renderer, Self::Err> {
        let component_natural_length = Arc::new(AtomicCell::new(None));
        let (controller, loop_heartbeat) = heartbeat::heartbeat();
        let images = Arc::new(ArcSwap::new(Arc::new(RedBlackTreeMap::new_sync())));
        let (sender, future) = rendering_loop(
            Arc::clone(&self.key),
            component.clone(),
            Arc::clone(&component_natural_length),
            &*self.controller_builder,
            Arc::clone(&self.image_combiner_builder),
            Arc::clone(&self.audio_combiner_builder),
            Handle::current(),
            controller,
            Arc::clone(&images),
        );
        self.runtime.spawn(future);
        Ok(MPDeltaRenderer {
            key: Arc::clone(&self.key),
            component: component.clone(),
            component_natural_length,
            controller_builder: Arc::clone(&self.controller_builder),
            image_combiner_builder: Arc::clone(&self.image_combiner_builder),
            audio_combiner_builder: Arc::clone(&self.audio_combiner_builder),
            runtime: self.runtime.clone(),
            images,
            loop_heartbeat: StdRwLock::new(loop_heartbeat),
            loop_sender: ArcSwap::new(Arc::new(sender)),
        })
    }
}

#[derive(Error)]
pub enum EncodeError<K: 'static, T: ParameterValueType, E> {
    #[error("render error: {0}")]
    RenderError(#[from] RenderError<K, T>),
    #[error("encoder error: {0}")]
    EncoderError(E),
}

impl<K, T, E> Debug for EncodeError<K, T, E>
where
    K: 'static,
    T: ParameterValueType,
    E: Debug,
    RenderError<K, T>: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::RenderError(e) => f.debug_tuple("RenderError").field(e).finish(),
            EncodeError::EncoderError(e) => f.debug_tuple("EncoderError").field(e).finish(),
        }
    }
}

impl<K, T, C, ImageCombinerBuilder, AudioCombinerBuilder, Encoder> ComponentEncoder<K, T, Encoder> for MPDeltaRendererBuilder<K, C, ImageCombinerBuilder, AudioCombinerBuilder>
where
    K: Send + Sync + 'static,
    T: ParameterValueType + 'static,
    C: MPDeltaRenderingControllerBuilder + 'static,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
    Encoder: VideoEncoderBuilder<T::Image, T::Audio> + 'static,
{
    type Err = EncodeError<K, T, Encoder::Err>;

    async fn render_and_encode<'life0, 'life1, 'async_trait>(&'life0 self, component: &'life1 ComponentInstanceHandle<K, T>, mut encoder: Encoder) -> Result<(), Self::Err>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        let mut encoder = encoder.build().map_err(EncodeError::EncoderError)?;
        let renderer = Arc::new(Renderer::new(self.runtime.clone(), component.clone(), Arc::clone(&self.image_combiner_builder), Arc::clone(&self.audio_combiner_builder)));
        let key = Arc::new(Arc::clone(&self.key).read_owned().await);
        let length = renderer.calc_natural_length(&**key).await?;
        if encoder.requires_audio() {
            match renderer.render(0, ParameterType::Audio(()), Arc::clone(&key)).await {
                Ok(Parameter::Audio(value)) => encoder.set_audio(value),
                Ok(other) => {
                    return Err(RenderError::OutputTypeMismatch {
                        component: component.clone(),
                        expect: Parameter::Audio(()),
                        actual: other.select(),
                    }
                    .into());
                }
                Err(err) => return Err(err.into()),
            }
        }
        if encoder.requires_image() {
            let length_frames = length.map_or(600, |length| {
                let (i, n) = length.value().deconstruct_with_round(60);
                i as usize * 60 + n as usize
            });
            for f in 0..length_frames {
                match renderer.render(f, ParameterType::Image(()), Arc::clone(&key)).await {
                    Ok(Parameter::Image(value)) => encoder.push_frame(value),
                    Ok(other) => {
                        return Err(RenderError::OutputTypeMismatch {
                            component: component.clone(),
                            expect: Parameter::Image(()),
                            actual: other.select(),
                        }
                        .into());
                    }
                    Err(err) => return Err(err.into()),
                }
                if f % 60 == 59 {
                    println!("{} of {length_frames} frames rendered", f + 1);
                }
            }
            println!("{length_frames} for {length_frames} frames rendered");
        }
        println!("waiting for encode");
        encoder.finish();
        println!("encode finished");
        Ok(())
    }
}

pub trait VideoEncoderBuilder<Image, Audio>: Send + Sync {
    type Err: Error + Send + 'static;
    type Encoder: VideoEncoder<Image, Audio>;
    fn build(&mut self) -> Result<Self::Encoder, Self::Err>;
}

impl<Image, Audio, O> VideoEncoderBuilder<Image, Audio> for O
where
    O: DerefMut + Send + Sync,
    O::Target: VideoEncoderBuilder<Image, Audio>,
{
    type Err = <O::Target as VideoEncoderBuilder<Image, Audio>>::Err;
    type Encoder = <O::Target as VideoEncoderBuilder<Image, Audio>>::Encoder;
    fn build(&mut self) -> Result<Self::Encoder, Self::Err> {
        self.deref_mut().build()
    }
}

pub trait VideoEncoderBuilderDyn<Image, Audio>: Send + Sync {
    fn build_dyn(&mut self) -> Result<Box<dyn VideoEncoder<Image, Audio>>, Box<dyn Error + Send + 'static>>;
}

impl<Image, Audio, O> VideoEncoderBuilderDyn<Image, Audio> for O
where
    O: VideoEncoderBuilder<Image, Audio>,
    O::Encoder: 'static,
{
    fn build_dyn(&mut self) -> Result<Box<dyn VideoEncoder<Image, Audio>>, Box<dyn Error + Send + 'static>> {
        match self.build() {
            Ok(encoder) => Ok(Box::new(encoder)),
            Err(err) => Err(Box::new(err)),
        }
    }
}

impl<Image, Audio> VideoEncoderBuilder<Image, Audio> for dyn VideoEncoderBuilderDyn<Image, Audio> {
    type Err = DynError;
    type Encoder = Box<dyn VideoEncoder<Image, Audio>>;

    fn build(&mut self) -> Result<Self::Encoder, Self::Err> {
        self.build_dyn().map_err(DynError)
    }
}

pub trait VideoEncoder<Image, Audio>: Send + Sync {
    fn requires_image(&self) -> bool;
    fn push_frame(&mut self, frame: Image);
    fn requires_audio(&self) -> bool;
    fn set_audio(&mut self, audio: Audio);
    fn finish(&mut self);
}

impl<Image, Audio, O> VideoEncoder<Image, Audio> for O
where
    O: DerefMut + Send + Sync,
    O::Target: VideoEncoder<Image, Audio>,
{
    fn requires_image(&self) -> bool {
        self.deref().requires_image()
    }

    fn push_frame(&mut self, frame: Image) {
        self.deref_mut().push_frame(frame)
    }

    fn requires_audio(&self) -> bool {
        self.deref().requires_audio()
    }

    fn set_audio(&mut self, audio: Audio) {
        self.deref_mut().set_audio(audio)
    }

    fn finish(&mut self) {
        self.deref_mut().finish()
    }
}

enum RenderingMessage<K: 'static, T: ParameterValueType> {
    RequestRenderFrame { frame: usize },
    RequestConstructAudio { ret: oneshot::Sender<RenderResult<T::Audio, K, T>> },
}

pub enum RenderingControllerItem {
    RequestRender { frame: usize },
    RemoveCache { frame: usize },
}

pub trait MPDeltaRenderingControllerBuilder: Send + Sync {
    type Controller<F: Fn(RenderingControllerItem) + Send + Sync + 'static>: MPDeltaRenderingController;
    fn create<F: Fn(RenderingControllerItem) + Send + Sync + 'static>(&self, f: F) -> Self::Controller<F>;
}

pub trait MPDeltaRenderingController: Send + Sync + 'static {
    fn on_request_render(&self, frame: usize);
}

type Images<K, T> = RedBlackTreeMap<usize, LazyInit<Result<<T as ParameterValueType>::Image, RenderError<K, T>>>, ArcTK>;

// TODO: あとでなんとかするかも
#[allow(clippy::too_many_arguments)]
fn rendering_loop<K, T, C, ImageCombinerBuilder, AudioCombinerBuilder>(
    key: Arc<RwLock<TCellOwner<K>>>,
    component: ComponentInstanceHandle<K, T>,
    component_natural_length: Arc<AtomicCell<Option<MarkerTime>>>,
    controller_builder: &C,
    image_combiner_builder: Arc<ImageCombinerBuilder>,
    audio_combiner_builder: Arc<AudioCombinerBuilder>,
    runtime: Handle,
    heartbeat_controller: HeartbeatController,
    images: Arc<ArcSwap<Images<K, T>>>,
) -> (UnboundedSender<RenderingMessage<K, T>>, impl Future<Output = ()> + Send + 'static)
where
    K: 'static + Send + Sync,
    T: ParameterValueType + 'static,
    C: MPDeltaRenderingControllerBuilder,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
{
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let (controller_sender, mut controller_receiver) = tokio::sync::mpsc::unbounded_channel();
    let renderer = Arc::new(Renderer::new(runtime.clone(), component.clone(), image_combiner_builder, audio_combiner_builder));
    let controller = controller_builder.create(move |message| {
        let _ = controller_sender.send(message);
    });
    images.store(Arc::new(RedBlackTreeMap::new_sync()));
    #[allow(unreachable_code)] // heartbeat_controllerはdropされるときの通知を担当するので、panicしない場合ずっとdropされずにいなければならない
    let future = async move {
        let _heartbeat_controller = heartbeat_controller;
        let calc_natural_length = || {
            let renderer = Arc::clone(&renderer);
            let key = Arc::clone(&key);
            runtime.spawn(key.read_owned().then(move |key| renderer.calc_natural_length(key)))
        };
        let mut fut = calc_natural_length();
        loop {
            tokio::select! {
                biased;
                result = &mut fut => {
                    match result {
                        Ok(Ok(natural_length)) => {
                            component_natural_length.store(natural_length);
                        }
                        Ok(Err(e)) => eprintln!("{}", e),
                        Err(e) => eprintln!("{}", e),
                    }
                    fut = calc_natural_length();
                }
                message = receiver.recv() => {
                    let Some(message) = message else { return; };
                    match message {
                        RenderingMessage::RequestRenderFrame { frame } => controller.on_request_render(frame),
                        RenderingMessage::RequestConstructAudio {ret} => {
                            let renderer = Arc::clone(&renderer);
                            let component = component.clone();
                            let key = Arc::clone(&key);
                            runtime.spawn(async move {
                                let result = match renderer.render(0, ParameterType::Audio(()), Arc::new(key.read_owned().await)).await {
                                    Ok(Parameter::Audio(value)) => Ok(value),
                                    Ok(value) => Err(RenderError::OutputTypeMismatch{
                                        component,
                                        expect: Parameter::Audio(()),
                                        actual: value.select(),
                                    }),
                                    Err(e) => Err(e),
                                };
                                let _ = ret.send(result);
                            });
                        }
                    }
                }
                Some(message) = controller_receiver.recv() => {
                    match message {
                        RenderingControllerItem::RequestRender {frame} => {
                            let renderer = Arc::clone(&renderer);
                            let component = component.clone();
                            let key = Arc::clone(&key);
                            let result = LazyInit::new(key.read_owned().then(move |key| renderer.render(frame, ParameterType::Image(()), Arc::new(key)))
                                    .map(|result| match result {
                                        Ok(Parameter::Image(value)) => Ok(value),
                                        Ok(value) => Err(RenderError::OutputTypeMismatch {
                                            component,
                                            expect: Parameter::Image(()),
                                            actual: value.select(),
                                        }),
                                        Err(e) => Err(e),
                                    }), &runtime);
                            let i = images.load().insert(frame, result);
                            images.store(Arc::new(i));
                        }
                        RenderingControllerItem::RemoveCache {frame} => {
                            let i = images.load().remove(&frame);
                            images.store(Arc::new(i));
                        }
                    }
                }
            }
        }
        drop(heartbeat_controller);
    };
    (sender, future)
}

pub struct MPDeltaRenderer<K: 'static, T: ParameterValueType, C, ImageCombinerBuilder, AudioCombinerBuilder> {
    key: Arc<RwLock<TCellOwner<K>>>,
    component: ComponentInstanceHandle<K, T>,
    component_natural_length: Arc<AtomicCell<Option<MarkerTime>>>,
    controller_builder: Arc<C>,
    image_combiner_builder: Arc<ImageCombinerBuilder>,
    audio_combiner_builder: Arc<AudioCombinerBuilder>,
    runtime: Handle,
    images: Arc<ArcSwap<Images<K, T>>>,
    loop_heartbeat: StdRwLock<HeartbeatMonitor>,
    loop_sender: ArcSwap<UnboundedSender<RenderingMessage<K, T>>>,
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
    InvalidMarkerLink(MarkerLinkHandle<K>),
    #[error("{index}-th variable parameter of {component:?} is invalid")]
    InvalidVariableParameter { component: ComponentInstanceHandle<K, T>, index: usize },
    #[error("time {at:?} is out of range {range:?}")]
    RenderTargetTimeOutOfRange { component: ComponentInstanceHandle<K, T>, range: Range<TimelineTime>, at: TimelineTime },
    #[error("required type value is not provided")]
    NotProvided,
    #[error("timeout")]
    Timeout,
}

pub type RenderResult<Ok, K, T> = Result<Ok, RenderError<K, T>>;

impl<K, T: ParameterValueType> From<CollectCachedTimeError<K, T>> for RenderError<K, T> {
    fn from(value: CollectCachedTimeError<K, T>) -> Self {
        match value {
            CollectCachedTimeError::InvalidComponentInstance(component) => RenderError::InvalidComponent(component),
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

impl<K, T: ParameterValueType> Clone for RenderError<K, T> {
    fn clone(&self) -> Self {
        match self {
            RenderError::InvalidComponent(handle) => RenderError::InvalidComponent(handle.clone()),
            RenderError::OutputTypeMismatch { component, expect, actual } => RenderError::OutputTypeMismatch {
                component: component.clone(),
                expect: *expect,
                actual: *actual,
            },
            RenderError::InvalidLinkGraph => RenderError::InvalidLinkGraph,
            RenderError::InvalidMarker(handle) => RenderError::InvalidMarker(handle.clone()),
            RenderError::InvalidMarkerLink(handle) => RenderError::InvalidMarkerLink(handle.clone()),
            RenderError::InvalidVariableParameter { component, index } => RenderError::InvalidVariableParameter { component: component.clone(), index: *index },
            RenderError::RenderTargetTimeOutOfRange { component, range, at } => RenderError::RenderTargetTimeOutOfRange {
                component: component.clone(),
                range: range.clone(),
                at: *at,
            },
            RenderError::NotProvided => RenderError::NotProvided,
            RenderError::Timeout => RenderError::Timeout,
        }
    }
}

impl<K, T, C, ImageCombinerBuilder, AudioCombinerBuilder> RealtimeComponentRenderer<T> for MPDeltaRenderer<K, T, C, ImageCombinerBuilder, AudioCombinerBuilder>
where
    K: Send + Sync,
    T: ParameterValueType + 'static,
    C: MPDeltaRenderingControllerBuilder + 'static,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
{
    type Err = RenderError<K, T>;

    fn get_component_length(&self) -> Option<MarkerTime> {
        self.component_natural_length.load()
    }

    fn render_frame(&self, frame: usize) -> Result<T::Image, Self::Err> {
        let result = self.images.load().get(&frame).and_then(|image| image.get().as_deref().cloned()).unwrap_or(Err(RenderError::Timeout));
        if !self.loop_heartbeat.read().unwrap().is_live() {
            let mut heartbeat_guard = self.loop_heartbeat.write().unwrap();
            if heartbeat_guard.is_live() {
                return result;
            }
            let (heartbeat_controller, new_monitor) = heartbeat::heartbeat();
            dbg!();
            let (new_loop_sender, fut) = rendering_loop(
                Arc::clone(&self.key),
                self.component.clone(),
                Arc::clone(&self.component_natural_length),
                &*self.controller_builder,
                Arc::clone(&self.image_combiner_builder),
                Arc::clone(&self.audio_combiner_builder),
                self.runtime.clone(),
                heartbeat_controller,
                Arc::clone(&self.images),
            );
            self.runtime.spawn(fut);
            *heartbeat_guard = new_monitor;
            self.loop_sender.store(Arc::new(new_loop_sender));
        }
        let _ = self.loop_sender.load().send(RenderingMessage::RequestRenderFrame { frame });
        result
    }

    fn sampling_rate(&self) -> u32 {
        48_000
    }

    async fn mix_audio(&self, _offset: usize, _length: usize) -> Result<T::Audio, Self::Err> {
        loop {
            let (sender, receiver) = oneshot::channel();
            let mut message = Some(RenderingMessage::RequestConstructAudio { ret: sender });
            loop {
                match self.loop_sender.load().send(message.take().unwrap()) {
                    Ok(()) => break,
                    Err(SendError(failed_message)) => {
                        message = Some(failed_message);
                        let mut loop_heartbeat = self.loop_heartbeat.write().unwrap();
                        if loop_heartbeat.is_live() {
                            continue;
                        }
                        let (heartbeat_controller, new_monitor) = heartbeat::heartbeat();
                        dbg!();
                        let (new_loop_sender, fut) = rendering_loop(
                            Arc::clone(&self.key),
                            self.component.clone(),
                            Arc::clone(&self.component_natural_length),
                            &*self.controller_builder,
                            Arc::clone(&self.image_combiner_builder),
                            Arc::clone(&self.audio_combiner_builder),
                            self.runtime.clone(),
                            heartbeat_controller,
                            Arc::clone(&self.images),
                        );
                        *loop_heartbeat = new_monitor;
                        self.runtime.spawn(fut);
                        self.loop_sender.store(Arc::new(new_loop_sender));
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
