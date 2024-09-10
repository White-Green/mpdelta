use crate::thread_cancel::{AutoCancellable, CancellationGuard};
use crate::{AudioCombinerParam, AudioCombinerRequest, Combiner, CombinerBuilder, ImageCombinerParam, ImageCombinerRequest, ImageSizeRequest, RenderError, RenderResult};
use cgmath::Vector3;
use futures::{stream, StreamExt, TryStreamExt};
use moka::future::Cache;
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerPinId, MarkerTime};
use mpdelta_core::component::parameter::value::DynEditableSingleValueMarker;
use mpdelta_core::component::parameter::{
    AbstractFile, AudioRequiredParams, AudioRequiredParamsFixed, ImageRequiredParams, ImageRequiredParamsFixed, ImageRequiredParamsTransform, ImageRequiredParamsTransformFixed, Opacity, Parameter, ParameterType, ParameterValueRaw, ParameterValueType, VariableParameterValue,
};
use mpdelta_core::component::processor::{CacheKey, ComponentProcessorWrapper, NativeProcessorInput};
use mpdelta_core::time::TimelineTime;
use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::Arc;
use std::{convert, future, iter, mem, panic};
use tokio::runtime::Handle;

mod evaluate_parameter;

pub(crate) struct Renderer<T: ParameterValueType, ImageCombinerBuilder, AudioCombinerBuilder> {
    runtime: Handle,
    component: ComponentInstance<T>,
    image_combiner_builder: Arc<ImageCombinerBuilder>,
    audio_combiner_builder: Arc<AudioCombinerBuilder>,
    cache: Cache<Box<dyn CacheKey>, Arc<dyn Any + Send + Sync>>,
}

impl<T, ImageCombinerBuilder, AudioCombinerBuilder> Renderer<T, ImageCombinerBuilder, AudioCombinerBuilder>
where
    T: ParameterValueType,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
{
    pub(crate) fn new(runtime: Handle, component: ComponentInstance<T>, image_combiner_builder: Arc<ImageCombinerBuilder>, audio_combiner_builder: Arc<AudioCombinerBuilder>) -> Self {
        Renderer {
            runtime,
            component,
            image_combiner_builder,
            audio_combiner_builder,
            cache: Cache::new(128),
        }
    }

    pub(crate) fn render(&self, at: usize, ty: ParameterType) -> impl Future<Output = RenderResult<ParameterValueRaw<T::Image, T::Audio>>> + Send + 'static {
        let ctx = RenderContext {
            runtime: self.runtime.clone(),
            image_combiner_builder: Arc::clone(&self.image_combiner_builder),
            audio_combiner_builder: Arc::clone(&self.audio_combiner_builder),
            cache: self.cache.clone(),
            _phantom: PhantomData,
        };
        let component = self.component.clone();
        async move {
            let right = TimelineTime::new(component.marker_right().locked_component_time().unwrap().value());
            let time_map = HashMap::from([(*component.marker_left().id(), TimelineTime::ZERO), (*component.marker_right().id(), right)]);
            render_inner(&component, TimelineTime::new(MixedFraction::from_fraction(at as i64, 60)) /* TODO: */, &ty, &ctx, &Arc::new(time_map)).await.map(into_parameter_value_fixed)
        }
    }

    pub(crate) fn calc_natural_length<'a>(&self) -> impl Future<Output = Result<Option<MarkerTime>, RenderError>> + Send + 'a {
        let component = self.component.clone();
        let render_cache = self.cache.clone();
        async move {
            let fixed_parameters = component
                .fixed_parameters()
                .iter()
                .map(|value| match value {
                    Parameter::None => ParameterValueRaw::None,
                    Parameter::Image(value) => ParameterValueRaw::Image(value.get_value()),
                    Parameter::Audio(value) => ParameterValueRaw::Audio(value.get_value()),
                    Parameter::Binary(value) => ParameterValueRaw::Binary(value.get_value()),
                    Parameter::String(value) => ParameterValueRaw::String(value.get_value()),
                    Parameter::Integer(value) => ParameterValueRaw::Integer(value.get_value()),
                    Parameter::RealNumber(value) => ParameterValueRaw::RealNumber(value.get_value()),
                    Parameter::Boolean(value) => ParameterValueRaw::Boolean(value.get_value()),
                    Parameter::Dictionary(value) => ParameterValueRaw::Dictionary(value.get_value()),
                    Parameter::Array(value) => ParameterValueRaw::Array(value.get_value()),
                    Parameter::ComponentClass(_value) => ParameterValueRaw::ComponentClass(()),
                })
                .collect::<Vec<_>>();
            match component.processor() {
                ComponentProcessorWrapper::Native(processor) => {
                    let cache_key = processor.whole_component_cache_key(&fixed_parameters);
                    let mut cache = if let Some(key) = &cache_key { render_cache.get(key).await } else { None };

                    let result = Ok(processor.natural_length(&fixed_parameters, &mut cache).await);
                    if let (Some(key), Some(value)) = (cache_key, cache) {
                        render_cache.insert(key, value).await;
                    }
                    result
                }
                ComponentProcessorWrapper::Component(processor) => Ok(Some(processor.natural_length(&fixed_parameters).await)),
                ComponentProcessorWrapper::GatherNative(_) => unimplemented!(),
                ComponentProcessorWrapper::GatherComponent(_) => unimplemented!(),
            }
        }
    }
}

struct RenderContext<T, ImageCombinerBuilder, AudioCombinerBuilder> {
    runtime: Handle,
    image_combiner_builder: Arc<ImageCombinerBuilder>,
    audio_combiner_builder: Arc<AudioCombinerBuilder>,
    cache: Cache<Box<dyn CacheKey>, Arc<dyn Any + Send + Sync>>,
    _phantom: PhantomData<T>,
}

impl<T, ImageCombinerBuilder, AudioCombinerBuilder> Clone for RenderContext<T, ImageCombinerBuilder, AudioCombinerBuilder> {
    fn clone(&self) -> Self {
        let RenderContext {
            runtime,
            image_combiner_builder,
            audio_combiner_builder,
            cache,
            _phantom,
        } = self;
        RenderContext {
            runtime: runtime.clone(),
            image_combiner_builder: Arc::clone(image_combiner_builder),
            audio_combiner_builder: Arc::clone(audio_combiner_builder),
            cache: cache.clone(),
            _phantom: PhantomData,
        }
    }
}

struct RenderOutput<Image, Audio>(PhantomData<(Image, Audio)>);

unsafe impl<Image, Audio> Send for RenderOutput<Image, Audio> {}

unsafe impl<Image, Audio> Sync for RenderOutput<Image, Audio> {}

impl<Image, Audio> ParameterValueType for RenderOutput<Image, Audio>
where
    Image: Send + Sync + Clone + 'static,
    Audio: Send + Sync + Clone + 'static,
{
    type Image = (Image, ImageCombinerParam);
    type Audio = (Audio, AudioCombinerParam);
    type Binary = AbstractFile;
    type String = String;
    type Integer = i64;
    type RealNumber = f64;
    type Boolean = bool;
    type Dictionary = HashMap<String, ParameterValueRaw<Image, Audio>>;
    type Array = Vec<ParameterValueRaw<Image, Audio>>;
    type ComponentClass = ();
}

fn into_parameter_value_fixed<Image, Audio>(value: Parameter<RenderOutput<Image, Audio>>) -> ParameterValueRaw<Image, Audio>
where
    Image: Send + Sync + Clone + 'static,
    Audio: Send + Sync + Clone + 'static,
{
    match value {
        Parameter::None => ParameterValueRaw::None,
        Parameter::Image((value, _)) => ParameterValueRaw::Image(value),
        Parameter::Audio((value, _)) => ParameterValueRaw::Audio(value),
        Parameter::Binary(value) => ParameterValueRaw::Binary(value),
        Parameter::String(value) => ParameterValueRaw::String(value),
        Parameter::Integer(value) => ParameterValueRaw::Integer(value),
        Parameter::RealNumber(value) => ParameterValueRaw::RealNumber(value),
        Parameter::Boolean(value) => ParameterValueRaw::Boolean(value),
        Parameter::Dictionary(value) => ParameterValueRaw::Dictionary(value),
        Parameter::Array(value) => ParameterValueRaw::Array(value),
        Parameter::ComponentClass(value) => ParameterValueRaw::ComponentClass(value),
    }
}

fn from_parameter_value_fixed<Image, Audio>(value: ParameterValueRaw<Image, Audio>) -> Parameter<RenderOutput<Image, Audio>>
where
    Image: Send + Sync + Clone + 'static,
    Audio: Send + Sync + Clone + 'static,
{
    match value {
        ParameterValueRaw::None => Parameter::None,
        ParameterValueRaw::Image(_) => unreachable!(),
        ParameterValueRaw::Audio(_) => unreachable!(),
        ParameterValueRaw::Binary(value) => Parameter::Binary(value),
        ParameterValueRaw::String(value) => Parameter::String(value),
        ParameterValueRaw::Integer(value) => Parameter::Integer(value),
        ParameterValueRaw::RealNumber(value) => Parameter::RealNumber(value),
        ParameterValueRaw::Boolean(value) => Parameter::Boolean(value),
        ParameterValueRaw::Dictionary(value) => Parameter::Dictionary(value),
        ParameterValueRaw::Array(value) => Parameter::Array(value),
        ParameterValueRaw::ComponentClass(value) => Parameter::ComponentClass(value),
    }
}

#[allow(clippy::manual_async_fn)]
fn render_inner<'a, T, ImageCombinerBuilder, AudioCombinerBuilder>(
    component: &'a ComponentInstance<T>,
    at: TimelineTime,
    ty: &'a ParameterType,
    ctx: &'a RenderContext<T, ImageCombinerBuilder, AudioCombinerBuilder>,
    timeline_time: &'a Arc<HashMap<MarkerPinId, TimelineTime>>,
) -> impl Future<Output = RenderResult<Parameter<RenderOutput<T::Image, T::Audio>>>> + Send + 'a
where
    T: ParameterValueType,
    ComponentInstance<T>: Clone,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
{
    async move {
        let fixed_parameters = component
            .fixed_parameters()
            .iter()
            .map(|value| match value {
                Parameter::None => ParameterValueRaw::None,
                Parameter::Image(value) => ParameterValueRaw::Image(value.get_value()),
                Parameter::Audio(value) => ParameterValueRaw::Audio(value.get_value()),
                Parameter::Binary(value) => ParameterValueRaw::Binary(value.get_value()),
                Parameter::String(value) => ParameterValueRaw::String(value.get_value()),
                Parameter::Integer(value) => ParameterValueRaw::Integer(value.get_value()),
                Parameter::RealNumber(value) => ParameterValueRaw::RealNumber(value.get_value()),
                Parameter::Boolean(value) => ParameterValueRaw::Boolean(value.get_value()),
                Parameter::Dictionary(value) => ParameterValueRaw::Dictionary(value.get_value()),
                Parameter::Array(value) => ParameterValueRaw::Array(value.get_value()),
                Parameter::ComponentClass(_value) => ParameterValueRaw::ComponentClass(()),
            })
            .collect::<Vec<_>>();
        let variable_parameters = async {
            let cancellation_guard = CancellationGuard::new();
            let variable_parameters = ctx
                .runtime
                .spawn_blocking({
                    let component = component.clone();
                    let ctx = ctx.clone();
                    let timeline_time = Arc::clone(timeline_time);
                    let cancellation_token = cancellation_guard.token();
                    move || {
                        let variable_parameters = component.variable_parameters();
                        let variable_parameters_type = component.variable_parameters_type();
                        if variable_parameters.len() != variable_parameters_type.len() {
                            return Err(RenderError::InvalidVariableParameter {
                                component: *component.id(),
                                index: variable_parameters.len().min(variable_parameters_type.len()),
                            });
                        }
                        variable_parameters_type
                            .par_iter()
                            .enumerate()
                            .map(|(i, (_, ty))| (i, &variable_parameters[i], ty))
                            .map(|(i, param, ty)| evaluate_parameter::evaluate_parameter(param, ty, at, &ctx, &timeline_time, &cancellation_token).unwrap_or_else(|| Err(RenderError::InvalidVariableParameter { component: *component.id(), index: i })))
                            .try_fold(Vec::new, |mut acc, result| {
                                acc.push(result?);
                                Ok(acc)
                            })
                            .try_reduce_with(|mut a, mut b| {
                                if a.len() < b.len() {
                                    mem::swap(&mut a, &mut b);
                                }
                                a.extend(b);
                                Ok(a)
                            })
                            .unwrap_or_else(|| Ok(Vec::new()))
                    }
                })
                .await;
            drop(cancellation_guard);
            match variable_parameters {
                Ok(value) => value,
                Err(err) => panic::resume_unwind(err.into_panic()),
            }
        };
        let image_required_params = || {
            let &ImageRequiredParams {
                ref transform,
                background_color,
                ref opacity,
                ref blend_mode,
                ref composite_operation,
            } = component.image_required_params().unwrap();
            let cancellation_guard = CancellationGuard::new();
            let cancellation_token = cancellation_guard.token();
            let eval_at = |value: &VariableParameterValue<_>| evaluate_parameter::evaluate_parameter_f64(value, at, ctx, timeline_time, &cancellation_token).unwrap().map(|result| result.into_real_number().unwrap());
            let transform = match &**transform {
                ImageRequiredParamsTransform::Params {
                    size,
                    scale,
                    translate,
                    rotate,
                    scale_center,
                    rotate_center,
                } => ImageRequiredParamsTransformFixed::Params {
                    size: Vector3 {
                        x: eval_at(&size.x)?,
                        y: eval_at(&size.y)?,
                        z: eval_at(&size.z)?,
                    },
                    scale: Vector3 {
                        x: eval_at(&scale.x)?,
                        y: eval_at(&scale.y)?,
                        z: eval_at(&scale.z)?,
                    },
                    translate: Vector3 {
                        x: eval_at(&translate.x)?,
                        y: eval_at(&translate.y)?,
                        z: eval_at(&translate.z)?,
                    },
                    rotate: evaluate_parameter::evaluate_time_split_value_at(rotate, at, timeline_time).unwrap()?,
                    scale_center: Vector3 {
                        x: eval_at(&scale_center.x)?,
                        y: eval_at(&scale_center.y)?,
                        z: eval_at(&scale_center.z)?,
                    },
                    rotate_center: Vector3 {
                        x: eval_at(&rotate_center.x)?,
                        y: eval_at(&rotate_center.y)?,
                        z: eval_at(&rotate_center.z)?,
                    },
                },
                ImageRequiredParamsTransform::Free { left_top, right_top, left_bottom, right_bottom } => ImageRequiredParamsTransformFixed::Free {
                    left_top: Vector3 {
                        x: eval_at(&left_top.x)?,
                        y: eval_at(&left_top.y)?,
                        z: eval_at(&left_top.z)?,
                    },
                    right_top: Vector3 {
                        x: eval_at(&right_top.x)?,
                        y: eval_at(&right_top.y)?,
                        z: eval_at(&right_top.z)?,
                    },
                    left_bottom: Vector3 {
                        x: eval_at(&left_bottom.x)?,
                        y: eval_at(&left_bottom.y)?,
                        z: eval_at(&left_bottom.z)?,
                    },
                    right_bottom: Vector3 {
                        x: eval_at(&right_bottom.x)?,
                        y: eval_at(&right_bottom.y)?,
                        z: eval_at(&right_bottom.z)?,
                    },
                },
            };
            Ok::<_, RenderError>(ImageRequiredParamsFixed {
                transform,
                background_color,
                opacity: Opacity::new(evaluate_parameter::evaluate_time_split_value_at(opacity, at, timeline_time).unwrap()?).unwrap_or(Opacity::OPAQUE),
                blend_mode: evaluate_parameter::evaluate_time_split_value_at(blend_mode, at, timeline_time).unwrap()?,
                composite_operation: evaluate_parameter::evaluate_time_split_value_at(composite_operation, at, timeline_time).unwrap()?,
            })
        };
        let _audio_required_params = || {
            let AudioRequiredParams { volume } = component.audio_required_params().unwrap();
            let cancellation_guard = CancellationGuard::new();
            let cancellation_token = cancellation_guard.token();
            Ok::<_, RenderError>(AudioRequiredParamsFixed {
                volume: volume
                    .iter()
                    .map(|volume| evaluate_parameter::evaluate_parameter_f64(volume, at, ctx, timeline_time, &cancellation_token).unwrap().map(|result| result.into_real_number().unwrap()))
                    .collect::<Result<Vec<_>, _>>()?,
            })
        };
        let time_map = TimeMap::new(component.marker_left(), component.markers(), component.marker_right(), timeline_time)?;

        // これは再生位置によらずAudioだけはCombinerに読ませるための特殊処理(できれば消したい)
        let internal_at = if ty.equals_type(&ParameterType::Audio(())) {
            TimelineTime::ZERO
        } else {
            time_map.map(at).ok_or_else(|| RenderError::RenderTargetTimeOutOfRange {
                component: *component.id(),
                range: time_map.left()..time_map.right(),
                at,
            })?
        };

        match component.processor() {
            ComponentProcessorWrapper::Native(processor) => {
                let whole_cache_key = processor.whole_component_cache_key(&fixed_parameters);
                let mut whole_cache_value = if let Some(cache_key) = &whole_cache_key { ctx.cache.get(cache_key).await } else { None };
                if !processor.supports_output_type(&fixed_parameters, ty.select(), &mut whole_cache_value).await {
                    return Err(RenderError::NotProvided);
                }
                let params = NativeProcessorInput {
                    fixed_parameters: &fixed_parameters,
                    variable_parameters: &variable_parameters.await?,
                    variable_parameter_type: component.variable_parameters_type(),
                };
                let mut image_params = None;
                let request = match ty.select() {
                    Parameter::None => Parameter::None,
                    Parameter::Image(()) => {
                        let p = image_required_params()?;
                        let ImageRequiredParamsTransformFixed::Params { size, .. } = image_params.get_or_insert(p).transform else {
                            panic!();
                        };
                        Parameter::Image(((1920. * size.x).abs().ceil() as u32, (1080. * size.y).abs().ceil() as u32))
                    }
                    Parameter::Audio(()) => Parameter::Audio(()),
                    Parameter::Binary(()) => Parameter::Binary(()),
                    Parameter::String(()) => Parameter::String(()),
                    Parameter::Integer(()) => Parameter::Integer(()),
                    Parameter::RealNumber(()) => Parameter::RealNumber(()),
                    Parameter::Boolean(()) => Parameter::Boolean(()),
                    Parameter::Dictionary(()) => Parameter::Dictionary(()),
                    Parameter::Array(()) => Parameter::Array(()),
                    Parameter::ComponentClass(()) => Parameter::ComponentClass(()),
                };
                let framed_cache_key = processor.framed_cache_key(params, internal_at, request.select());
                let mut framed_cache_value = if let Some(cache_key) = &framed_cache_key { ctx.cache.get(cache_key).await } else { None };
                let result = match processor.process(params, internal_at, request, &mut whole_cache_value, &mut framed_cache_value).await {
                    Parameter::Image(image) => Ok(Parameter::Image((image, image_params.unwrap()))),
                    Parameter::Audio(audio) => Ok(Parameter::Audio((audio, time_map.clone()))),
                    other => Ok(from_parameter_value_fixed(other)),
                };
                if let (Some(cache_key), Some(cache_value)) = (whole_cache_key, whole_cache_value) {
                    ctx.cache.insert(cache_key, cache_value).await;
                }
                if let (Some(cache_key), Some(cache_value)) = (framed_cache_key, framed_cache_value) {
                    ctx.cache.insert(cache_key, cache_value).await;
                }
                result
            }
            ComponentProcessorWrapper::Component(processor) => {
                let pair = processor.process(&fixed_parameters, &[], &[/* TODO */], component.variable_parameters_type()).await;
                let timeline_time_inner = mpdelta_differential::collect_cached_time(&*pair)?;
                let timeline_time_inner = Arc::new(timeline_time_inner);
                match ty {
                    ParameterType::None => Ok(Parameter::None),
                    ParameterType::Image(_) => {
                        let image = stream::iter(pair.components_dyn())
                            .map(assume_general_lifetime(|component: &ComponentInstance<T>| {
                                ctx.runtime
                                    .spawn({
                                        let component = component.clone();
                                        let ty = ty.clone();
                                        let ctx: RenderContext<_, _, _> = ctx.clone();
                                        let timeline_time_inner = Arc::clone(&timeline_time_inner);
                                        async move {
                                            match render_inner(&component, internal_at, &ty, &ctx, &timeline_time_inner).await {
                                                Err(RenderError::NotProvided) => None,
                                                Err(RenderError::RenderTargetTimeOutOfRange { component: c, .. }) if &c == component.id() => None,
                                                other => Some((other, component)),
                                            }
                                        }
                                    })
                                    .auto_cancel()
                            }))
                            .buffered(10)
                            .map(|join_result| join_result.unwrap_or_else(|err| panic::resume_unwind(err.into_panic())))
                            .filter_map(future::ready)
                            .map(|(result, component)| result.map(|result| (result, component)))
                            .try_fold(ctx.image_combiner_builder.new_combiner(ImageSizeRequest { width: 1920., height: 1080. }), |mut combiner, (result, component)| {
                                let Parameter::Image((image, image_required_params)) = result else {
                                    return future::ready(Err(RenderError::OutputTypeMismatch {
                                        component: *component.id(),
                                        expect: ty.select(),
                                        actual: result.select(),
                                    }));
                                };
                                combiner.add(image, image_required_params);
                                future::ready(Ok(combiner))
                            })
                            .await?
                            .collect();
                        Ok(Parameter::Image((image, image_required_params()?)))
                    }
                    ParameterType::Audio(_) => {
                        let left = timeline_time[component.marker_left().id()];
                        let right = timeline_time[component.marker_right().id()];
                        let length = right - left;
                        let image = stream::iter(pair.components_dyn())
                            .map(assume_general_lifetime(|component: &ComponentInstance<T>| {
                                ctx.runtime
                                    .spawn({
                                        let component = component.clone();
                                        let ty = ty.clone();
                                        let ctx = ctx.clone();
                                        let timeline_time_inner = Arc::clone(&timeline_time_inner);
                                        async move {
                                            match render_inner(&component, internal_at, &ty, &ctx, &timeline_time_inner).await {
                                                Err(RenderError::NotProvided) => None,
                                                Err(RenderError::RenderTargetTimeOutOfRange { component: c, .. }) if &c == component.id() => None,
                                                other => Some((other, component)),
                                            }
                                        }
                                    })
                                    .auto_cancel()
                            }))
                            .buffered(10)
                            .map(|join_result| join_result.unwrap_or_else(|err| panic::resume_unwind(err.into_panic())))
                            .filter_map(future::ready)
                            .map(|(result, component)| result.map(|result| (result, component)))
                            .try_fold(ctx.audio_combiner_builder.new_combiner(length), |mut combiner, (result, component)| {
                                let Parameter::Audio((audio, audio_required_params)) = result else {
                                    return future::ready(Err(RenderError::OutputTypeMismatch {
                                        component: *component.id(),
                                        expect: ty.select(),
                                        actual: result.select(),
                                    }));
                                };
                                combiner.add(audio, audio_required_params);
                                future::ready(Ok(combiner))
                            })
                            .await?
                            .collect();
                        Ok(Parameter::Audio((image, time_map.clone())))
                    }
                    ParameterType::Binary(_) | ParameterType::String(_) | ParameterType::Integer(_) | ParameterType::RealNumber(_) | ParameterType::Boolean(_) | ParameterType::Dictionary(_) | ParameterType::Array(_) => stream::iter(pair.components_dyn().rev())
                        .map(assume_general_lifetime(|component: &ComponentInstance<T>| {
                            ctx.runtime
                                .spawn({
                                    let component = component.clone();
                                    let ty = ty.clone();
                                    let ctx = ctx.clone();
                                    let timeline_time_inner = Arc::clone(&timeline_time_inner);
                                    async move {
                                        match render_inner(&component, internal_at, &ty, &ctx, &timeline_time_inner).await {
                                            Err(RenderError::NotProvided) => None,
                                            Err(RenderError::RenderTargetTimeOutOfRange { component: c, .. }) if &c == component.id() => None,
                                            other => Some(other),
                                        }
                                    }
                                })
                                .auto_cancel()
                        }))
                        .buffered(10)
                        .map(|join_result| join_result.unwrap_or_else(|err| panic::resume_unwind(err.into_panic())))
                        .filter_map(future::ready)
                        .next()
                        .await
                        .unwrap_or(Err(RenderError::NotProvided)),
                    ParameterType::ComponentClass(_) => {
                        todo!()
                    }
                }
            }
            ComponentProcessorWrapper::GatherNative(_) => todo!(),
            ComponentProcessorWrapper::GatherComponent(_) => todo!(),
        }
    }
}

fn assume_general_lifetime<T, U, F>(f: F) -> F
where
    F: for<'a> FnMut(&'a T) -> U,
{
    f
}

#[derive(Debug, Clone)]
pub struct TimeMap {
    left: TimelineTime,
    right: TimelineTime,
    markers: Vec<(TimelineTime, MarkerTime)>,
}

impl TimeMap {
    pub fn new(left: &MarkerPin, markers: &[MarkerPin], right: &MarkerPin, timeline_time: &HashMap<MarkerPinId, TimelineTime>) -> RenderResult<TimeMap> {
        let markers = iter::once(left).chain(markers.iter()).chain(iter::once(right)).filter_map(|marker| Some((timeline_time[marker.id()], marker.locked_component_time()?))).collect::<Vec<_>>();
        let left = timeline_time[left.id()];
        let right = timeline_time[right.id()];
        Ok(TimeMap { left, right, markers })
    }

    pub fn left(&self) -> TimelineTime {
        self.left
    }

    pub fn right(&self) -> TimelineTime {
        self.right
    }

    pub fn map(&self, at: TimelineTime) -> Option<TimelineTime> {
        let TimeMap { left, right, ref markers } = *self;
        if at < left || right < at {
            return None;
        }
        match *markers.as_slice() {
            [] => Some(at - left),
            [(timeline_time, component_time)] => Some(at - timeline_time + component_time.into()),
            [(timeline_time1, component_time1), (timeline_time2, component_time2)] => {
                let p = (at - timeline_time1) / (timeline_time2 - timeline_time1);
                let time = component_time1.value() + (component_time2.value() - component_time1.value()) * p;
                Some(TimelineTime::new(time))
            }
            ref markers => {
                let i = markers.binary_search_by_key(&at, |&(time, _)| time).unwrap_or_else(|x| x);
                let [(timeline_time1, component_time1), (timeline_time2, component_time2)] = markers[i.saturating_sub(1).min(markers.len() - 2)..][..2] else {
                    unreachable!()
                };
                let p = (at - timeline_time1) / (timeline_time2 - timeline_time1);
                let time = component_time1.value() + (component_time2.value() - component_time1.value()) * p;
                Some(TimelineTime::new(time))
            }
        }
    }

    pub fn map_range_iter(&self, start: TimelineTime) -> impl Iterator<Item = TimeMapSegment> + '_ {
        let i = self.markers.binary_search_by_key(&start, |&(time, _)| time).map_or_else(convert::identity, |x| x + 1);
        let mut markers = &self.markers[i.saturating_sub(1).min(self.markers.len().saturating_sub(2))..];
        let (left, right) = match markers {
            [] => (TimeMapSegment::new(self.left..self.right, self.left..self.right, TimelineTime::ZERO..self.right - self.left), None),
            &[(source, target)] => {
                let target = TimelineTime::from(target);
                (TimeMapSegment::new(self.left..self.right, self.left..self.right, target - (source - self.left)..target + (self.right - source)), None)
            }
            &[(s1, t1), (s2, t2)] => {
                let t1 = TimelineTime::from(t1);
                let t2 = TimelineTime::from(t2);
                markers = &[];
                (TimeMapSegment::new(self.left..self.right, s1..s2, t1..t2), None)
            }
            &[(s1, t1), (s2, t2), ..] => {
                let &[(s3, t3), (s4, t4)] = markers.last_chunk().unwrap();
                let t1 = TimelineTime::from(t1);
                let t2 = TimelineTime::from(t2);
                let t3 = TimelineTime::from(t3);
                let t4 = TimelineTime::from(t4);
                markers = &markers[1..markers.len() - 1];
                (TimeMapSegment::new(self.left..s2, s1..s2, t1..t2), Some(TimeMapSegment::new(s3..self.right, s3..s4, t3..t4)))
            }
        };
        iter::once(left)
            .chain(
                markers
                    .windows(2)
                    .map(|markers| {
                        let &[(source1, target1), (source2, target2)] = markers else {
                            unreachable!();
                        };
                        let target1 = TimelineTime::from(target1);
                        let target2 = TimelineTime::from(target2);
                        TimeMapSegment::new(source1..source2, source1..source2, target1..target2)
                    })
                    .skip_while(move |segment| segment.time_range.end <= start),
            )
            .chain(right)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeMapSegment {
    time_range: Range<TimelineTime>,
    slope: MixedFraction,
    intercept: MixedFraction,
}

impl TimeMapSegment {
    fn new(time_range: Range<TimelineTime>, source_range: Range<TimelineTime>, target_range: Range<TimelineTime>) -> TimeMapSegment {
        let slope = (target_range.end.value() - target_range.start.value()) / (source_range.end.value() - source_range.start.value());
        let intercept = target_range.start.value() - slope * source_range.start.value();
        TimeMapSegment { time_range, slope, intercept }
    }

    pub fn start(&self) -> TimelineTime {
        self.time_range.start
    }

    pub fn end(&self) -> TimelineTime {
        self.time_range.end
    }

    pub fn map(&self, at: TimelineTime) -> TimelineTime {
        let time = self.slope * at.value() + self.intercept;
        TimelineTime::new(time)
    }

    pub fn map_inverse(&self, at: TimelineTime) -> TimelineTime {
        let time = (at.value() - self.intercept) / self.slope;
        TimelineTime::new(time)
    }

    pub fn scale(&self) -> MixedFraction {
        self.slope
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use mpdelta_core::component::marker_pin::MarkerPin;
    use mpdelta_core::mfrac;
    use mpdelta_core_test_util::TestIdGenerator;

    #[test]
    fn test_time_map() {
        let id = TestIdGenerator::new();
        fn time_map_for_test((markers, time_map): &(Vec<MarkerPin>, HashMap<MarkerPinId, TimelineTime>), at: TimelineTime) -> Option<TimelineTime> {
            assert!(markers.len() >= 2);
            let [left, markers @ .., right] = markers.as_slice() else { unreachable!() };
            TimeMap::new(left, markers, right, time_map).ok()?.map(at)
        }
        macro_rules! markers {
            ($($markers:expr),*$(,)?) => {
                {
                    let mut markers = Vec::new();
                    let mut time_map = HashMap::new();
                    macro_rules! marker {
                        ($t:expr) => {
                            let marker_pin_id = ::mpdelta_core::core::IdGenerator::generate_new(&id);
                            let marker_pin = MarkerPin::new_unlocked(marker_pin_id);
                            time_map.insert(*marker_pin.id(), TimelineTime::new($t));
                            markers.push(marker_pin);
                        };
                        ($t:expr, $m:expr) => {
                            let marker_pin_id = ::mpdelta_core::core::IdGenerator::generate_new(&id);
                            let marker_pin = MarkerPin::new(marker_pin_id, MarkerTime::new($m).unwrap());
                            time_map.insert(*marker_pin.id(), TimelineTime::new($t));
                            markers.push(marker_pin);
                        };
                    }
                    $($markers;)*
                    (markers, time_map)
                }
            }
        }

        let markers = markers![marker!(mfrac!(3)), marker!(mfrac!(4)), marker!(mfrac!(5)), marker!(mfrac!(6))];
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(3, 0, 10))), Some(v) if (v.value() - mfrac!(0, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(3, 5, 10))), Some(v) if (v.value() - mfrac!(0, 5, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(5, 0, 10))), Some(v) if (v.value() - mfrac!(2, 0, 10)) == MixedFraction::ZERO);
        let markers = markers![marker!(mfrac!(3)), marker!(mfrac!(4)), marker!(mfrac!(5), mfrac!(10)), marker!(mfrac!(6))];
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(3, 0, 10))), Some(v) if (v.value() - mfrac!(8, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(3, 5, 10))), Some(v) if (v.value() - mfrac!(8, 5, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(5, 0, 10))), Some(v) if (v.value() - mfrac!(10, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(5, 5, 10))), Some(v) if (v.value() - mfrac!(10, 5, 10)) == MixedFraction::ZERO);
        let markers = markers![marker!(mfrac!(3)), marker!(mfrac!(4), mfrac!(8)), marker!(mfrac!(5), mfrac!(10)), marker!(mfrac!(6))];
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(3, 0, 10))), Some(v) if (v.value() - mfrac!(6, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(3, 5, 10))), Some(v) if (v.value() - mfrac!(7, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(4, 5, 10))), Some(v) if (v.value() - mfrac!(9, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(5, 0, 10))), Some(v) if (v.value() - mfrac!(10, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(5, 5, 10))), Some(v) if (v.value() - mfrac!(11, 0, 10)) == MixedFraction::ZERO);
        let markers = markers![
            marker!(mfrac!(3)),
            marker!(mfrac!(4), mfrac!(8)),
            marker!(mfrac!(5), mfrac!(10)),
            marker!(mfrac!(6)),
            marker!(mfrac!(7), mfrac!(13)),
            marker!(mfrac!(8)),
            marker!(mfrac!(10), mfrac!(10)),
        ];
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(3, 0, 10))), Some(v) if (v.value() - mfrac!(6, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(3, 5, 10))), Some(v) if (v.value() - mfrac!(7, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(4, 5, 10))), Some(v) if (v.value() - mfrac!(9, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(5, 0, 10))), Some(v) if (v.value() - mfrac!(10, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(6, 0, 10))), Some(v) if (v.value() - mfrac!(11, 5, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(7, 0, 10))), Some(v) if (v.value() - mfrac!(13, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(8, 0, 10))), Some(v) if (v.value() - mfrac!(12, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, TimelineTime::new(mfrac!(10, 0, 10))), Some(v) if (v.value() - mfrac!(10, 0, 10)) == MixedFraction::ZERO);
    }

    #[test]
    fn test_time_map_range_iter() {
        let id = TestIdGenerator::new();
        fn time_map_for_test((markers, time_map): &(Vec<MarkerPin>, HashMap<MarkerPinId, TimelineTime>), at: TimelineTime) -> Vec<TimeMapSegment> {
            assert!(markers.len() >= 2);
            let [left, markers @ .., right] = markers.as_slice() else { unreachable!() };
            TimeMap::new(left, markers, right, time_map).unwrap().map_range_iter(at).collect()
        }
        macro_rules! markers {
            ($($markers:expr),*$(,)?) => {
                {
                    let mut markers = Vec::new();
                    let mut time_map = HashMap::new();
                    macro_rules! marker {
                        ($t:expr) => {
                            let marker_pin_id = ::mpdelta_core::core::IdGenerator::generate_new(&id);
                            let marker_pin = MarkerPin::new_unlocked(marker_pin_id);
                            time_map.insert(*marker_pin.id(), TimelineTime::new($t));
                            markers.push(marker_pin);
                        };
                        ($t:expr, $m:expr) => {
                            let marker_pin_id = ::mpdelta_core::core::IdGenerator::generate_new(&id);
                            let marker_pin = MarkerPin::new(marker_pin_id, MarkerTime::new($m).unwrap());
                            time_map.insert(*marker_pin.id(), TimelineTime::new($t));
                            markers.push(marker_pin);
                        };
                    }
                    $($markers;)*
                    (markers, time_map)
                }
            }
        }
        macro_rules! t {
            ($($t:tt)*) => {
                TimelineTime::new(mfrac!($($t)*))
            }
        }
        let markers = markers![marker!(mfrac!(3)), marker!(mfrac!(4)), marker!(mfrac!(5)), marker!(mfrac!(6))];
        assert_eq!(time_map_for_test(&markers, t!(3, 0, 10)), vec![TimeMapSegment::new(t!(3)..t!(6), t!(3)..t!(6), t!(0)..t!(3))]);
        assert_eq!(time_map_for_test(&markers, t!(3, 5, 10)), vec![TimeMapSegment::new(t!(3)..t!(6), t!(3)..t!(6), t!(0)..t!(3))]);
        assert_eq!(time_map_for_test(&markers, t!(5, 0, 10)), vec![TimeMapSegment::new(t!(3)..t!(6), t!(3)..t!(6), t!(0)..t!(3))]);
        let markers = markers![marker!(mfrac!(3)), marker!(mfrac!(4)), marker!(mfrac!(5), mfrac!(10)), marker!(mfrac!(6))];
        assert_eq!(time_map_for_test(&markers, t!(3, 0, 10)), vec![TimeMapSegment::new(t!(3)..t!(6), t!(3)..t!(6), t!(8)..t!(11))]);
        assert_eq!(time_map_for_test(&markers, t!(3, 5, 10)), vec![TimeMapSegment::new(t!(3)..t!(6), t!(3)..t!(6), t!(8)..t!(11))]);
        assert_eq!(time_map_for_test(&markers, t!(5, 0, 10)), vec![TimeMapSegment::new(t!(3)..t!(6), t!(3)..t!(6), t!(8)..t!(11))]);
        assert_eq!(time_map_for_test(&markers, t!(5, 5, 10)), vec![TimeMapSegment::new(t!(3)..t!(6), t!(3)..t!(6), t!(8)..t!(11))]);
        let markers = markers![marker!(mfrac!(3)), marker!(mfrac!(4), mfrac!(8)), marker!(mfrac!(5), mfrac!(10)), marker!(mfrac!(6))];
        assert_eq!(time_map_for_test(&markers, t!(3, 0, 10)), vec![TimeMapSegment::new(t!(3)..t!(6), t!(4)..t!(5), t!(8)..t!(10))]);
        assert_eq!(time_map_for_test(&markers, t!(3, 5, 10)), vec![TimeMapSegment::new(t!(3)..t!(6), t!(4)..t!(5), t!(8)..t!(10))]);
        assert_eq!(time_map_for_test(&markers, t!(4, 5, 10)), vec![TimeMapSegment::new(t!(3)..t!(6), t!(4)..t!(5), t!(8)..t!(10))]);
        assert_eq!(time_map_for_test(&markers, t!(5, 0, 10)), vec![TimeMapSegment::new(t!(3)..t!(6), t!(4)..t!(5), t!(8)..t!(10))]);
        assert_eq!(time_map_for_test(&markers, t!(5, 5, 10)), vec![TimeMapSegment::new(t!(3)..t!(6), t!(4)..t!(5), t!(8)..t!(10))]);
        let markers = markers![
            marker!(mfrac!(3)),
            marker!(mfrac!(4), mfrac!(8)),
            marker!(mfrac!(5), mfrac!(10)),
            marker!(mfrac!(6)),
            marker!(mfrac!(7), mfrac!(13)),
            marker!(mfrac!(8)),
            marker!(mfrac!(10), mfrac!(10)),
        ];
        assert_eq!(
            time_map_for_test(&markers, t!(3, 0, 10)),
            vec![
                TimeMapSegment::new(t!(3)..t!(5), t!(4)..t!(5), t!(8)..t!(10)),
                TimeMapSegment::new(t!(5)..t!(7), t!(5)..t!(7), t!(10)..t!(13)),
                TimeMapSegment::new(t!(7)..t!(10), t!(7)..t!(10), t!(13)..t!(10)),
            ]
        );
        assert_eq!(
            time_map_for_test(&markers, t!(3, 5, 10)),
            vec![
                TimeMapSegment::new(t!(3)..t!(5), t!(4)..t!(5), t!(8)..t!(10)),
                TimeMapSegment::new(t!(5)..t!(7), t!(5)..t!(7), t!(10)..t!(13)),
                TimeMapSegment::new(t!(7)..t!(10), t!(7)..t!(10), t!(13)..t!(10)),
            ]
        );
        assert_eq!(
            time_map_for_test(&markers, t!(4, 5, 10)),
            vec![
                TimeMapSegment::new(t!(3)..t!(5), t!(4)..t!(5), t!(8)..t!(10)),
                TimeMapSegment::new(t!(5)..t!(7), t!(5)..t!(7), t!(10)..t!(13)),
                TimeMapSegment::new(t!(7)..t!(10), t!(7)..t!(10), t!(13)..t!(10)),
            ]
        );
        assert_eq!(time_map_for_test(&markers, t!(5, 0, 10)), vec![TimeMapSegment::new(t!(3)..t!(7), t!(5)..t!(7), t!(10)..t!(13)), TimeMapSegment::new(t!(7)..t!(10), t!(7)..t!(10), t!(13)..t!(10))]);
        assert_eq!(time_map_for_test(&markers, t!(6, 0, 10)), vec![TimeMapSegment::new(t!(3)..t!(7), t!(5)..t!(7), t!(10)..t!(13)), TimeMapSegment::new(t!(7)..t!(10), t!(7)..t!(10), t!(13)..t!(10))]);
        assert_eq!(time_map_for_test(&markers, t!(7, 0, 10)), vec![TimeMapSegment::new(t!(3)..t!(10), t!(7)..t!(10), t!(13)..t!(10))]);
        assert_eq!(time_map_for_test(&markers, t!(8, 0, 10)), vec![TimeMapSegment::new(t!(3)..t!(10), t!(7)..t!(10), t!(13)..t!(10))]);
        assert_eq!(time_map_for_test(&markers, t!(10, 0, 10)), vec![TimeMapSegment::new(t!(3)..t!(10), t!(7)..t!(10), t!(13)..t!(10))]);
    }

    #[test]
    fn test_time_map_segment() {
        macro_rules! t {
            ($($t:tt)*) => {
                TimelineTime::new(mfrac!($($t)*))
            }
        }
        let segment = TimeMapSegment::new(t!(3)..t!(6), t!(3)..t!(6), t!(0)..t!(3));
        assert_eq!(segment.scale(), mfrac!(1));
        assert_eq!(segment.map(t!(3, 0, 10)), t!(0, 0, 10));
        assert_eq!(segment.map(t!(4, 5, 10)), t!(1, 5, 10));
        assert_eq!(segment.map(t!(6, 0, 10)), t!(3, 0, 10));
        let segment = TimeMapSegment::new(t!(3)..t!(6), t!(3)..t!(6), t!(8)..t!(11));
        assert_eq!(segment.scale(), mfrac!(1));
        assert_eq!(segment.map(t!(3, 0, 10)), t!(8, 0, 10));
        assert_eq!(segment.map(t!(4, 5, 10)), t!(9, 5, 10));
        assert_eq!(segment.map(t!(6, 0, 10)), t!(11, 0, 10));
        let segment = TimeMapSegment::new(t!(3)..t!(6), t!(4)..t!(5), t!(8)..t!(10));
        assert_eq!(segment.scale(), mfrac!(2));
        assert_eq!(segment.map(t!(4, 0, 10)), t!(8, 0, 10));
        assert_eq!(segment.map(t!(4, 5, 10)), t!(9, 0, 10));
        assert_eq!(segment.map(t!(5, 0, 10)), t!(10, 0, 10));
        let segment = TimeMapSegment::new(t!(3)..t!(5), t!(4)..t!(5), t!(8)..t!(10));
        assert_eq!(segment.scale(), mfrac!(2));
        assert_eq!(segment.map(t!(4, 0, 10)), t!(8, 0, 10));
        assert_eq!(segment.map(t!(4, 5, 10)), t!(9, 0, 10));
        assert_eq!(segment.map(t!(5, 0, 10)), t!(10, 0, 10));
        let segment = TimeMapSegment::new(t!(5)..t!(7), t!(5)..t!(7), t!(10)..t!(13));
        assert_eq!(segment.scale(), mfrac!(3, 2));
        assert_eq!(segment.map(t!(5, 0, 10)), t!(10, 0, 10));
        assert_eq!(segment.map(t!(6, 0, 10)), t!(11, 5, 10));
        assert_eq!(segment.map(t!(7, 0, 10)), t!(13, 0, 10));
        let segment = TimeMapSegment::new(t!(7)..t!(10), t!(7)..t!(10), t!(13)..t!(10));
        assert_eq!(segment.scale(), mfrac!(-1));
        assert_eq!(segment.map(t!(7, 0, 10)), t!(13, 0, 10));
        assert_eq!(segment.map(t!(8, 5, 10)), t!(11, 5, 10));
        assert_eq!(segment.map(t!(10, 0, 10)), t!(10, 0, 10));
    }
}
