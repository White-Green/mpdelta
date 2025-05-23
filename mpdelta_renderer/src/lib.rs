use crate::heartbeat::{HeartbeatController, HeartbeatMonitor};
use crate::lazy_init::LazyInit;
use crate::render::Renderer;
use arc_swap::ArcSwap;
use async_trait::async_trait;
use crossbeam_utils::atomic::AtomicCell;
use futures::FutureExt;
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::instance::{ComponentInstance, ComponentInstanceId};
use mpdelta_core::component::link::MarkerLink;
use mpdelta_core::component::marker_pin::{MarkerPinId, MarkerTime};
use mpdelta_core::component::parameter::{ImageRequiredParamsFixed, ImageRequiredParamsTransformFixed, Parameter, ParameterSelect, ParameterType, ParameterValueType};
use mpdelta_core::component::processor::{DynGatherNativeParameter, ProcessorCache};
use mpdelta_core::core::{ComponentEncoder, ComponentRendererBuilder};
use mpdelta_core::time::TimelineTime;
use mpdelta_core::usecase::RealtimeComponentRenderer;
use mpdelta_differential::CollectCachedTimeError;
use rpds::{RedBlackTreeMap, RedBlackTreeMapSync};
use std::convert::Infallible;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut, Range};
use std::sync::{Arc, RwLock as StdRwLock};
use thiserror::Error;
use tokio::runtime::Handle;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

mod heartbeat;
mod invalidate_range;
mod lazy_init;
mod render;
#[cfg(test)]
mod tests;
mod time_stretch;

pub use invalidate_range::InvalidateRange;
pub use time_stretch::{GlobalTime, LocalTime, TimeStretch, TimeStretchSegment};

pub struct ImageCombinerRequest {
    pub size_request: ImageSizeRequest,
    pub transform: Option<ImageRequiredParamsTransformFixed>,
}

impl From<ImageSizeRequest> for ImageCombinerRequest {
    fn from(size_request: ImageSizeRequest) -> Self {
        ImageCombinerRequest { size_request, transform: None }
    }
}

pub type ImageCombinerParam = ImageRequiredParamsFixed;
pub struct AudioCombinerRequest {
    pub length: TimelineTime,
    pub invert_time_map: Option<Arc<TimeStretch<LocalTime, GlobalTime>>>,
}

#[derive(Clone)]
pub struct AudioCombinerParam {
    pub volume: Arc<[DynGatherNativeParameter<f64>]>,
    pub time_map: Arc<TimeStretch<GlobalTime, LocalTime>>,
    pub invalidate_range: InvalidateRange<TimelineTime>,
}

impl AudioCombinerParam {
    pub fn new(volume: Arc<[DynGatherNativeParameter<f64>]>, time_map: Arc<TimeStretch<GlobalTime, LocalTime>>, invalidate_range: InvalidateRange<TimelineTime>) -> AudioCombinerParam {
        AudioCombinerParam { volume, time_map, invalidate_range }
    }
}

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

pub struct MPDeltaRendererBuilder<C, ImageCombinerBuilder, AudioCombinerBuilder, Cache> {
    controller_builder: Arc<C>,
    image_combiner_builder: Arc<ImageCombinerBuilder>,
    audio_combiner_builder: Arc<AudioCombinerBuilder>,
    cache: Cache,
    runtime: Handle,
}

impl<C, ImageCombinerBuilder, AudioCombinerBuilder, Cache> MPDeltaRendererBuilder<C, ImageCombinerBuilder, AudioCombinerBuilder, Cache> {
    pub fn new(image_combiner_builder: Arc<ImageCombinerBuilder>, controller_builder: Arc<C>, audio_combiner_builder: Arc<AudioCombinerBuilder>, cache: Cache, runtime: Handle) -> MPDeltaRendererBuilder<C, ImageCombinerBuilder, AudioCombinerBuilder, Cache> {
        MPDeltaRendererBuilder {
            controller_builder,
            image_combiner_builder,
            audio_combiner_builder,
            cache,
            runtime,
        }
    }
}

#[async_trait]
impl<T, C, ImageCombinerBuilder, AudioCombinerBuilder, Cache> ComponentRendererBuilder<T> for MPDeltaRendererBuilder<C, ImageCombinerBuilder, AudioCombinerBuilder, Cache>
where
    T: ParameterValueType + 'static,
    C: MPDeltaRenderingControllerBuilder + 'static,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
    Cache: ProcessorCache + Clone + 'static,
{
    type Err = Infallible;
    type Renderer = MPDeltaRenderer<T, C, ImageCombinerBuilder, AudioCombinerBuilder, Cache>;

    async fn create_renderer(&self, component: Arc<ComponentInstance<T>>) -> Result<Self::Renderer, Self::Err> {
        let (controller, loop_heartbeat) = heartbeat::heartbeat();
        let images = Arc::new(ArcSwap::new(Arc::new(RedBlackTreeMap::new_sync())));
        let (sender, component_length, future) = rendering_loop(
            component.clone(),
            &*self.controller_builder,
            Arc::clone(&self.image_combiner_builder),
            Arc::clone(&self.audio_combiner_builder),
            self.cache.clone(),
            Handle::current(),
            controller,
            Arc::clone(&images),
        );
        let component_natural_length = AtomicCell::new(component_length);
        self.runtime.spawn(future);
        Ok(MPDeltaRenderer {
            component,
            component_natural_length,
            controller_builder: Arc::clone(&self.controller_builder),
            image_combiner_builder: Arc::clone(&self.image_combiner_builder),
            audio_combiner_builder: Arc::clone(&self.audio_combiner_builder),
            cache: self.cache.clone(),
            runtime: self.runtime.clone(),
            images,
            loop_heartbeat: StdRwLock::new(loop_heartbeat),
            loop_sender: ArcSwap::new(Arc::new(sender)),
        })
    }
}

#[derive(Error)]
pub enum EncodeError<E> {
    #[error("render error: {0}")]
    RenderError(#[from] RenderError),
    #[error("encoder error: {0}")]
    EncoderError(E),
}

impl<E> Debug for EncodeError<E>
where
    E: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::RenderError(e) => f.debug_tuple("RenderError").field(e).finish(),
            EncodeError::EncoderError(e) => f.debug_tuple("EncoderError").field(e).finish(),
        }
    }
}

impl<T, C, ImageCombinerBuilder, AudioCombinerBuilder, Cache, Encoder> ComponentEncoder<T, Encoder> for MPDeltaRendererBuilder<C, ImageCombinerBuilder, AudioCombinerBuilder, Cache>
where
    T: ParameterValueType + 'static,
    C: MPDeltaRenderingControllerBuilder + 'static,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
    Cache: ProcessorCache + Clone + 'static,
    Encoder: VideoEncoderBuilder<T::Image, T::Audio> + 'static,
{
    type Err = EncodeError<Encoder::Err>;

    async fn render_and_encode<'life0, 'async_trait>(&'life0 self, component: Arc<ComponentInstance<T>>, mut encoder: Encoder) -> Result<(), Self::Err>
    where
        'life0: 'async_trait,
    {
        let mut encoder = encoder.build().map_err(EncodeError::EncoderError)?;
        let renderer = Arc::new(Renderer::new(component.clone(), self.runtime.clone(), Arc::clone(&self.image_combiner_builder), Arc::clone(&self.audio_combiner_builder), self.cache.clone()));
        let length = renderer.component_length();
        if encoder.requires_audio() {
            match renderer.render(TimelineTime::ZERO, ParameterType::Audio(())).await {
                Ok(Parameter::Audio(value)) => encoder.set_audio(value),
                Ok(other) => {
                    return Err(RenderError::OutputTypeMismatch {
                        component: *component.id(),
                        expect: Parameter::Audio(()),
                        actual: other.select(),
                    }
                    .into());
                }
                Err(err) => return Err(err.into()),
            }
        }
        if encoder.requires_image() {
            let (i, n) = length.value().deconstruct_with_round(60);
            let length_frames = i as i64 * 60 + n as i64;
            for f in 0..length_frames {
                match renderer.render(TimelineTime::new(MixedFraction::from_fraction(f, 60)), ParameterType::Image(())).await {
                    Ok(Parameter::Image(value)) => encoder.push_frame(value),
                    Ok(other) => {
                        return Err(RenderError::OutputTypeMismatch {
                            component: *component.id(),
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

enum RenderingMessage<T: ParameterValueType> {
    RequestRenderFrame { frame: usize },
    RequestConstructAudio { ret: oneshot::Sender<RenderResult<T::Audio>> },
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

type Images<T> = RedBlackTreeMapSync<usize, LazyInit<Result<<T as ParameterValueType>::Image, RenderError>>>;

// TODO: あとでなんとかするかも
#[allow(clippy::too_many_arguments)]
fn rendering_loop<T, C, ImageCombinerBuilder, AudioCombinerBuilder, Cache>(
    component: Arc<ComponentInstance<T>>,
    controller_builder: &C,
    image_combiner_builder: Arc<ImageCombinerBuilder>,
    audio_combiner_builder: Arc<AudioCombinerBuilder>,
    cache: Cache,
    runtime: Handle,
    heartbeat_controller: HeartbeatController,
    images: Arc<ArcSwap<Images<T>>>,
) -> (UnboundedSender<RenderingMessage<T>>, MarkerTime, impl Future<Output = ()> + Send + 'static)
where
    T: ParameterValueType + 'static,
    C: MPDeltaRenderingControllerBuilder,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
    Cache: ProcessorCache + 'static,
{
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let (controller_sender, mut controller_receiver) = tokio::sync::mpsc::unbounded_channel();
    let renderer = Arc::new(Renderer::new(component.clone(), runtime.clone(), image_combiner_builder, audio_combiner_builder, cache));
    let component_length = renderer.component_length();
    let controller = controller_builder.create(move |message| {
        let _ = controller_sender.send(message);
    });
    images.store(Arc::new(RedBlackTreeMap::new_sync()));
    #[allow(unreachable_code)] // heartbeat_controllerはdropされるときの通知を担当するので、panicしない場合ずっとdropされずにいなければならない
    let future = async move {
        let _heartbeat_controller = heartbeat_controller;
        loop {
            tokio::select! {
                message = receiver.recv() => {
                    let Some(message) = message else { return; };
                    match message {
                        RenderingMessage::RequestRenderFrame { frame } => controller.on_request_render(frame),
                        RenderingMessage::RequestConstructAudio {ret} => {
                            let renderer = Arc::clone(&renderer);
                            let component = component.clone();
                            runtime.spawn(async move {
                                let result = match renderer.render(TimelineTime::ZERO, ParameterType::Audio(())).await {
                                    Ok(Parameter::Audio(value)) => Ok(value),
                                    Ok(value) => Err(RenderError::OutputTypeMismatch {
                                        component: *component.id(),
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
                            let component_id = *component.id();
                            let result = LazyInit::new(renderer.render(TimelineTime::new(MixedFraction::from_fraction(frame as i64, 60)), ParameterType::Image(()))
                                    .map(move |result| match result {
                                        Ok(Parameter::Image(value)) => Ok(value),
                                        Ok(value) => Err(RenderError::OutputTypeMismatch {
                                          component: component_id,
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
    (sender, component_length, future)
}

pub struct MPDeltaRenderer<T: ParameterValueType, C, ImageCombinerBuilder, AudioCombinerBuilder, Cache> {
    component: Arc<ComponentInstance<T>>,
    component_natural_length: AtomicCell<MarkerTime>,
    controller_builder: Arc<C>,
    image_combiner_builder: Arc<ImageCombinerBuilder>,
    audio_combiner_builder: Arc<AudioCombinerBuilder>,
    cache: Cache,
    runtime: Handle,
    images: Arc<ArcSwap<Images<T>>>,
    loop_heartbeat: StdRwLock<HeartbeatMonitor>,
    loop_sender: ArcSwap<UnboundedSender<RenderingMessage<T>>>,
}

#[derive(Error)]
pub enum RenderError {
    #[error("invalid component: {0:?}")]
    InvalidComponent(ComponentInstanceId),
    #[error("the output type by {component:?} is mismatch; expected: {expect:?}, but got {actual:?}")]
    OutputTypeMismatch { component: ComponentInstanceId, expect: Parameter<ParameterSelect>, actual: Parameter<ParameterSelect> },
    #[error("invalid link graph")]
    InvalidLinkGraph,
    #[error("invalid marker: {0:?}")]
    InvalidMarker(MarkerPinId),
    #[error("invalid marker: {0:?}")]
    InvalidMarkerLink(MarkerLink),
    #[error("{index}-th variable parameter of {component:?} is invalid")]
    InvalidVariableParameter { component: ComponentInstanceId, index: usize },
    #[error("time {at:?} is out of range {range:?}")]
    RenderTargetTimeOutOfRange { component: ComponentInstanceId, range: Range<TimelineTime>, at: TimelineTime },
    #[error("required type value is not provided")]
    NotProvided,
    #[error("timeout")]
    Timeout,
    #[error("unsupported parameter type")]
    UnsupportedParameterType,
    #[error("{0}")]
    UnknownError(#[from] Arc<dyn Error + Send + Sync + 'static>),
}

pub type RenderResult<Ok> = Result<Ok, RenderError>;

impl From<CollectCachedTimeError> for RenderError {
    fn from(_: CollectCachedTimeError) -> Self {
        // TODO:
        RenderError::InvalidLinkGraph
    }
}

impl Debug for RenderError {
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
            RenderError::UnsupportedParameterType => f.debug_struct("UnsupportedParameterType").finish(),
            RenderError::UnknownError(error) => f.debug_tuple("UnknownError").field(error).finish(),
        }
    }
}

impl Clone for RenderError {
    fn clone(&self) -> Self {
        match self {
            RenderError::InvalidComponent(handle) => RenderError::InvalidComponent(*handle),
            RenderError::OutputTypeMismatch { component, expect, actual } => RenderError::OutputTypeMismatch {
                component: *component,
                expect: *expect,
                actual: *actual,
            },
            RenderError::InvalidLinkGraph => RenderError::InvalidLinkGraph,
            RenderError::InvalidMarker(handle) => RenderError::InvalidMarker(*handle),
            RenderError::InvalidMarkerLink(handle) => RenderError::InvalidMarkerLink(handle.clone()),
            RenderError::InvalidVariableParameter { component, index } => RenderError::InvalidVariableParameter { component: *component, index: *index },
            RenderError::RenderTargetTimeOutOfRange { component, range, at } => RenderError::RenderTargetTimeOutOfRange { component: *component, range: range.clone(), at: *at },
            RenderError::NotProvided => RenderError::NotProvided,
            RenderError::Timeout => RenderError::Timeout,
            RenderError::UnsupportedParameterType => RenderError::UnsupportedParameterType,
            RenderError::UnknownError(error) => RenderError::UnknownError(error.clone()),
        }
    }
}

impl<T, C, ImageCombinerBuilder, AudioCombinerBuilder, Cache> RealtimeComponentRenderer<T> for MPDeltaRenderer<T, C, ImageCombinerBuilder, AudioCombinerBuilder, Cache>
where
    T: ParameterValueType + 'static,
    C: MPDeltaRenderingControllerBuilder + 'static,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
    Cache: ProcessorCache + Clone + 'static,
{
    type Err = RenderError;

    fn get_component_length(&self) -> Option<MarkerTime> {
        Some(self.component_natural_length.load())
    }

    fn render_frame(&self, frame: usize) -> Result<T::Image, Self::Err> {
        let result = self.images.load().get(&frame).and_then(|image| image.get().as_deref().cloned()).unwrap_or(Err(RenderError::Timeout));
        if !self.loop_heartbeat.read().unwrap().is_live() {
            let mut heartbeat_guard = self.loop_heartbeat.write().unwrap();
            if heartbeat_guard.is_live() {
                return result;
            }
            let (heartbeat_controller, new_monitor) = heartbeat::heartbeat();
            let (new_loop_sender, component_length, fut) = rendering_loop(
                self.component.clone(),
                &*self.controller_builder,
                Arc::clone(&self.image_combiner_builder),
                Arc::clone(&self.audio_combiner_builder),
                self.cache.clone(),
                self.runtime.clone(),
                heartbeat_controller,
                Arc::clone(&self.images),
            );
            self.component_natural_length.store(component_length);
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
                        let (new_loop_sender, component_length, fut) = rendering_loop(
                            self.component.clone(),
                            &*self.controller_builder,
                            Arc::clone(&self.image_combiner_builder),
                            Arc::clone(&self.audio_combiner_builder),
                            self.cache.clone(),
                            self.runtime.clone(),
                            heartbeat_controller,
                            Arc::clone(&self.images),
                        );
                        self.component_natural_length.store(component_length);
                        *loop_heartbeat = new_monitor;
                        self.runtime.spawn(fut);
                        self.loop_sender.store(Arc::new(new_loop_sender));
                    }
                };
            }
            let result = receiver.await;
            match result {
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
    fn collect<'async_trait>(self) -> impl Future<Output = Data> + Send + 'async_trait
    where
        Self: 'async_trait,
        Data: 'async_trait;
}

impl<Data, O> CombinerBuilder<Data> for O
where
    O: Deref + Send + Sync,
    O::Target: CombinerBuilder<Data>,
{
    type Request = <O::Target as CombinerBuilder<Data>>::Request;
    type Param = <O::Target as CombinerBuilder<Data>>::Param;
    type Combiner = <O::Target as CombinerBuilder<Data>>::Combiner;
    fn new_combiner(&self, request: Self::Request) -> Self::Combiner {
        <O::Target as CombinerBuilder<Data>>::new_combiner(self.deref(), request)
    }
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
