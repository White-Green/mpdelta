use crate::thread_cancel::{AutoCancellable, CancellationGuard};
use crate::{Combiner, CombinerBuilder, ImageSizeRequest, RenderError};
use cgmath::Vector3;
use futures::{stream, StreamExt, TryStreamExt};
use mpdelta_core::component::instance::ComponentInstanceHandle;
use mpdelta_core::component::marker_pin::{MarkerPinHandle, MarkerPinHandleOwned};
use mpdelta_core::component::parameter::AudioRequiredParams;
use mpdelta_core::component::parameter::{AbstractFile, AudioRequiredParamsFixed, ImageRequiredParams, ImageRequiredParamsFixed, ImageRequiredParamsTransform, ImageRequiredParamsTransformFixed, Opacity, Parameter, ParameterType, ParameterValueFixed, ParameterValueType, VariableParameterValue};
use mpdelta_core::component::processor::{ComponentProcessorWrapper, ComponentsLinksPair};
use mpdelta_core::ptr::StaticPointerOwned;
use mpdelta_core::time::TimelineTime;
use qcell::TCellOwner;
use rayon::iter::IndexedParallelIterator;
use rayon::iter::IntoParallelRefIterator;
use rayon::iter::ParallelIterator;
use std::collections::HashMap;
use std::future;
use std::future::Future;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;
use std::{iter, mem, panic};
use tokio::runtime::Handle;

mod evaluate_parameter;

pub(crate) struct Renderer<K: 'static, T: ParameterValueType, ImageCombinerBuilder, AudioCombinerBuilder> {
    runtime: Handle,
    component: ComponentInstanceHandle<K, T>,
    image_combiner_builder: Arc<ImageCombinerBuilder>,
    audio_combiner_builder: Arc<AudioCombinerBuilder>,
}

impl<K, T, ImageCombinerBuilder, AudioCombinerBuilder> Renderer<K, T, ImageCombinerBuilder, AudioCombinerBuilder>
where
    K: 'static,
    T: ParameterValueType,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageSizeRequest, Param = ImageRequiredParamsFixed> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = (), Param = AudioRequiredParamsFixed> + 'static,
{
    pub(crate) fn new(runtime: Handle, component: ComponentInstanceHandle<K, T>, image_combiner_builder: Arc<ImageCombinerBuilder>, audio_combiner_builder: Arc<AudioCombinerBuilder>) -> Self {
        Renderer {
            runtime,
            component,
            image_combiner_builder,
            audio_combiner_builder,
        }
    }

    pub(crate) fn render(&self, at: usize, ty: ParameterType, key: Arc<impl Deref<Target = TCellOwner<K>> + Send + Sync + 'static>) -> impl Future<Output = Result<ParameterValueFixed<T::Image, T::Audio>, RenderError<K, T>>> + Send + 'static {
        let ctx = RenderContext {
            runtime: self.runtime.clone(),
            key,
            image_combiner_builder: Arc::clone(&self.image_combiner_builder),
            audio_combiner_builder: Arc::clone(&self.audio_combiner_builder),
            _phantom: PhantomData,
        };
        let component = self.component.clone();
        async move {
            render_inner(&component, TimelineTime::new(at as f64 / 60.).unwrap() /* TODO: */, &ty, &ctx).await.map(into_parameter_value_fixed)
        }
    }
}

struct RenderContext<Key, T, ImageCombinerBuilder, AudioCombinerBuilder> {
    runtime: Handle,
    key: Arc<Key>,
    image_combiner_builder: Arc<ImageCombinerBuilder>,
    audio_combiner_builder: Arc<AudioCombinerBuilder>,
    _phantom: PhantomData<T>,
}

impl<Key, T, ImageCombinerBuilder, AudioCombinerBuilder> Clone for RenderContext<Key, T, ImageCombinerBuilder, AudioCombinerBuilder> {
    fn clone(&self) -> Self {
        let RenderContext {
            runtime,
            key,
            image_combiner_builder,
            audio_combiner_builder,
            _phantom,
        } = self;
        RenderContext {
            runtime: runtime.clone(),
            key: Arc::clone(key),
            image_combiner_builder: Arc::clone(image_combiner_builder),
            audio_combiner_builder: Arc::clone(audio_combiner_builder),
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
    type Image = (Image, ImageRequiredParamsFixed);
    type Audio = (Audio, AudioRequiredParamsFixed);
    type Binary = AbstractFile;
    type String = String;
    type Integer = i64;
    type RealNumber = f64;
    type Boolean = bool;
    type Dictionary = HashMap<String, ParameterValueFixed<Image, Audio>>;
    type Array = Vec<ParameterValueFixed<Image, Audio>>;
    type ComponentClass = ();
}

fn into_parameter_value_fixed<Image, Audio>(value: Parameter<RenderOutput<Image, Audio>>) -> ParameterValueFixed<Image, Audio>
where
    Image: Send + Sync + Clone + 'static,
    Audio: Send + Sync + Clone + 'static,
{
    match value {
        Parameter::None => ParameterValueFixed::None,
        Parameter::Image((value, _)) => ParameterValueFixed::Image(value),
        Parameter::Audio((value, _)) => ParameterValueFixed::Audio(value),
        Parameter::Binary(value) => ParameterValueFixed::Binary(value),
        Parameter::String(value) => ParameterValueFixed::String(value),
        Parameter::Integer(value) => ParameterValueFixed::Integer(value),
        Parameter::RealNumber(value) => ParameterValueFixed::RealNumber(value),
        Parameter::Boolean(value) => ParameterValueFixed::Boolean(value),
        Parameter::Dictionary(value) => ParameterValueFixed::Dictionary(value),
        Parameter::Array(value) => ParameterValueFixed::Array(value),
        Parameter::ComponentClass(value) => ParameterValueFixed::ComponentClass(value),
    }
}

fn from_parameter_value_fixed<Image, Audio>(value: ParameterValueFixed<Image, Audio>) -> Parameter<RenderOutput<Image, Audio>>
where
    Image: Send + Sync + Clone + 'static,
    Audio: Send + Sync + Clone + 'static,
{
    match value {
        ParameterValueFixed::None => Parameter::None,
        ParameterValueFixed::Image(_) => unreachable!(),
        ParameterValueFixed::Audio(_) => unreachable!(),
        ParameterValueFixed::Binary(value) => Parameter::Binary(value),
        ParameterValueFixed::String(value) => Parameter::String(value),
        ParameterValueFixed::Integer(value) => Parameter::Integer(value),
        ParameterValueFixed::RealNumber(value) => Parameter::RealNumber(value),
        ParameterValueFixed::Boolean(value) => Parameter::Boolean(value),
        ParameterValueFixed::Dictionary(value) => Parameter::Dictionary(value),
        ParameterValueFixed::Array(value) => Parameter::Array(value),
        ParameterValueFixed::ComponentClass(value) => Parameter::ComponentClass(value),
    }
}

#[allow(clippy::manual_async_fn)]
fn render_inner<'a, K, T, Key, ImageCombinerBuilder, AudioCombinerBuilder>(
    component_handle: &'a ComponentInstanceHandle<K, T>,
    at: TimelineTime,
    ty: &'a ParameterType,
    ctx: &'a RenderContext<Key, T, ImageCombinerBuilder, AudioCombinerBuilder>,
) -> impl Future<Output = Result<Parameter<RenderOutput<T::Image, T::Audio>>, RenderError<K, T>>> + Send + 'a
where
    K: 'static,
    T: ParameterValueType,
    Key: Deref<Target = TCellOwner<K>> + Send + Sync + 'static,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageSizeRequest, Param = ImageRequiredParamsFixed> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = (), Param = AudioRequiredParamsFixed> + 'static,
{
    async move {
        let Some(component) = component_handle.upgrade() else {
            return Err(RenderError::InvalidComponent(component_handle.clone()));
        };
        let component = component.ro(&ctx.key);
        let fixed_parameters = component.fixed_parameters();
        let variable_parameters = async {
            let cancellation_guard = CancellationGuard::new();
            let variable_parameters = ctx
                .runtime
                .spawn_blocking({
                    let component_handle = component_handle.clone();
                    let ctx = ctx.clone();
                    let cancellation_token = cancellation_guard.token();
                    move || {
                        let Some(component) = component_handle.upgrade() else {
                            return Err(RenderError::InvalidComponent(component_handle.clone()));
                        };
                        let component = component.ro(&ctx.key);
                        let variable_parameters = component.variable_parameters();
                        let variable_parameters_type = component.variable_parameters_type();
                        if variable_parameters.len() != variable_parameters_type.len() {
                            return Err(RenderError::InvalidVariableParameter {
                                component: component_handle.clone(),
                                index: variable_parameters.len().min(variable_parameters_type.len()),
                            });
                        }
                        variable_parameters
                            .par_iter()
                            .zip(variable_parameters_type)
                            .enumerate()
                            .map(|(i, (param, (_, ty)))| evaluate_parameter::evaluate_parameter(param, ty, at, &ctx, &cancellation_token).unwrap_or_else(|| Err(RenderError::InvalidVariableParameter { component: component_handle.clone(), index: i })))
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
                aspect_ratio,
                ref transform,
                background_color,
                ref opacity,
                ref blend_mode,
                ref composite_operation,
            } = component.image_required_params().unwrap();
            let cancellation_guard = CancellationGuard::new();
            let cancellation_token = cancellation_guard.token();
            let eval_at = |value: &VariableParameterValue<_, _, _>| evaluate_parameter::evaluate_parameter_f64(value, at, ctx, &cancellation_token).unwrap().map(|result| result.into_real_number().unwrap());
            let transform = match transform {
                ImageRequiredParamsTransform::Params {
                    scale: Vector3 { x: scale_x, y: scale_y, z: scale_z },
                    translate: Vector3 { x: translate_x, y: translate_y, z: translate_z },
                    rotate,
                    scale_center: Vector3 {
                        x: scale_center_x,
                        y: scale_center_y,
                        z: scale_center_z,
                    },
                    rotate_center: Vector3 {
                        x: rotate_center_x,
                        y: rotate_center_y,
                        z: rotate_center_z,
                    },
                } => ImageRequiredParamsTransformFixed::Params {
                    scale: Vector3 {
                        x: eval_at(scale_x)?,
                        y: eval_at(scale_y)?,
                        z: eval_at(scale_z)?,
                    },
                    translate: Vector3 {
                        x: eval_at(translate_x)?,
                        y: eval_at(translate_y)?,
                        z: eval_at(translate_z)?,
                    },
                    rotate: evaluate_parameter::evaluate_time_split_value_at(rotate, at, &ctx.key).unwrap()?,
                    scale_center: Vector3 {
                        x: eval_at(scale_center_x)?,
                        y: eval_at(scale_center_y)?,
                        z: eval_at(scale_center_z)?,
                    },
                    rotate_center: Vector3 {
                        x: eval_at(rotate_center_x)?,
                        y: eval_at(rotate_center_y)?,
                        z: eval_at(rotate_center_z)?,
                    },
                },
                ImageRequiredParamsTransform::Free {
                    left_top: Vector3 { x: left_top_x, y: left_top_y, z: left_top_z },
                    right_top: Vector3 { x: right_top_x, y: right_top_y, z: right_top_z },
                    left_bottom: Vector3 { x: left_bottom_x, y: left_bottom_y, z: left_bottom_z },
                    right_bottom: Vector3 {
                        x: right_bottom_x,
                        y: right_bottom_y,
                        z: right_bottom_z,
                    },
                } => ImageRequiredParamsTransformFixed::Free {
                    left_top: Vector3 {
                        x: eval_at(left_top_x)?,
                        y: eval_at(left_top_y)?,
                        z: eval_at(left_top_z)?,
                    },
                    right_top: Vector3 {
                        x: eval_at(right_top_x)?,
                        y: eval_at(right_top_y)?,
                        z: eval_at(right_top_z)?,
                    },
                    left_bottom: Vector3 {
                        x: eval_at(left_bottom_x)?,
                        y: eval_at(left_bottom_y)?,
                        z: eval_at(left_bottom_z)?,
                    },
                    right_bottom: Vector3 {
                        x: eval_at(right_bottom_x)?,
                        y: eval_at(right_bottom_y)?,
                        z: eval_at(right_bottom_z)?,
                    },
                },
            };
            Ok::<_, RenderError<K, T>>(ImageRequiredParamsFixed {
                aspect_ratio,
                transform,
                background_color,
                opacity: Opacity::new(evaluate_parameter::evaluate_time_split_value_at(opacity, at, &ctx.key).unwrap()?).unwrap_or(Opacity::OPAQUE),
                blend_mode: evaluate_parameter::evaluate_time_split_value_at(blend_mode, at, &ctx.key).unwrap()?,
                composite_operation: evaluate_parameter::evaluate_time_split_value_at(composite_operation, at, &ctx.key).unwrap()?,
            })
        };
        let audio_required_params = || {
            let AudioRequiredParams { volume } = component.audio_required_params().unwrap();
            let cancellation_guard = CancellationGuard::new();
            let cancellation_token = cancellation_guard.token();
            Ok::<_, RenderError<K, T>>(AudioRequiredParamsFixed {
                volume: volume
                    .iter()
                    .map(|volume| evaluate_parameter::evaluate_parameter_f64(volume, at, ctx, &cancellation_token).unwrap().map(|result| result.into_real_number().unwrap()))
                    .collect::<Result<Vec<_>, _>>()?,
            })
        };
        let internal_at = match time_map::<K, T>(component.marker_left(), component.markers(), component.marker_right(), &ctx.key, at) {
            Ok(internal_at) => internal_at,
            Err(RenderError::RenderTargetTimeOutOfRange { range, at, .. }) => return Err(RenderError::RenderTargetTimeOutOfRange { component: component_handle.clone(), range, at }),
            Err(err) => return Err(err),
        };
        match component.processor() {
            ComponentProcessorWrapper::Native(processor) => {
                if !processor.supports_output_type(ty.select()) {
                    return Err(RenderError::NotProvided);
                }
                match processor.process(fixed_parameters, &variable_parameters.await?, component.variable_parameters_type(), internal_at, ty.select()).await {
                    Parameter::Image(image) => Ok(Parameter::Image((image, image_required_params()?))),
                    Parameter::Audio(audio) => Ok(Parameter::Audio((audio, audio_required_params()?))),
                    other => Ok(from_parameter_value_fixed(other)),
                }
            }
            ComponentProcessorWrapper::Component(processor) => {
                let ComponentsLinksPair(components, links) = processor.process(fixed_parameters, &[], &[/* TODO */], component.variable_parameters_type()).await;
                mpdelta_differential::collect_cached_time(&components, &links, component.marker_left(), component.marker_right(), &ctx.key)?;
                match ty {
                    ParameterType::None => Ok(Parameter::None),
                    ParameterType::Image(_) => {
                        let image = stream::iter(components)
                            .map(|component| {
                                ctx.runtime
                                    .spawn({
                                        let ty = ty.clone();
                                        let ctx: RenderContext<_, _, _, _> = ctx.clone();
                                        async move {
                                            match render_inner(&component, internal_at, &ty, &ctx).await {
                                                Err(RenderError::NotProvided) => None,
                                                Err(RenderError::RenderTargetTimeOutOfRange { component: c, .. }) if &c == component.ptr() => None,
                                                other => Some((other, component)),
                                            }
                                        }
                                    })
                                    .auto_cancel()
                            })
                            .buffered(10)
                            .map(|join_result| join_result.unwrap_or_else(|err| panic::resume_unwind(err.into_panic())))
                            .filter_map(future::ready)
                            .map(|(result, component)| result.map(|result| (result, component)))
                            .try_fold(ctx.image_combiner_builder.new_combiner(ImageSizeRequest { width: 1920., height: 1080. }), |mut combiner, (result, component)| {
                                let Parameter::Image((image, image_required_params)) = result else {
                                    return future::ready(Err(RenderError::OutputTypeMismatch {
                                        component: component.reference(),
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
                        let image = stream::iter(components)
                            .map(|component| {
                                ctx.runtime
                                    .spawn({
                                        let ty = ty.clone();
                                        let ctx = ctx.clone();
                                        async move {
                                            match render_inner(&component, internal_at, &ty, &ctx).await {
                                                Err(RenderError::NotProvided) => None,
                                                Err(RenderError::RenderTargetTimeOutOfRange { component: c, .. }) if &c == component.ptr() => None,
                                                other => Some((other, component)),
                                            }
                                        }
                                    })
                                    .auto_cancel()
                            })
                            .buffered(10)
                            .map(|join_result| join_result.unwrap_or_else(|err| panic::resume_unwind(err.into_panic())))
                            .filter_map(future::ready)
                            .map(|(result, component)| result.map(|result| (result, component)))
                            .try_fold(ctx.audio_combiner_builder.new_combiner(()), |mut combiner, (result, component)| {
                                let Parameter::Audio((audio, audio_required_params)) = result else {
                                    return future::ready(Err(RenderError::OutputTypeMismatch {
                                        component: component.reference(),
                                        expect: ty.select(),
                                        actual: result.select(),
                                    }));
                                };
                                combiner.add(audio, audio_required_params);
                                future::ready(Ok(combiner))
                            })
                            .await?
                            .collect();
                        Ok(Parameter::Audio((image, audio_required_params()?)))
                    }
                    ParameterType::Binary(_) | ParameterType::String(_) | ParameterType::Integer(_) | ParameterType::RealNumber(_) | ParameterType::Boolean(_) | ParameterType::Dictionary(_) | ParameterType::Array(_) => stream::iter(components.into_iter().rev())
                        .map(|component| {
                            ctx.runtime
                                .spawn({
                                    let ty = ty.clone();
                                    let ctx = ctx.clone();
                                    async move {
                                        match render_inner(&component, internal_at, &ty, &ctx).await {
                                            Err(RenderError::NotProvided) => None,
                                            Err(RenderError::RenderTargetTimeOutOfRange { component: c, .. }) if &c == component.ptr() => None,
                                            other => Some(other),
                                        }
                                    }
                                })
                                .auto_cancel()
                        })
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

fn time_map<K, T: ParameterValueType>(left: &MarkerPinHandle<K>, markers: &[MarkerPinHandleOwned<K>], right: &MarkerPinHandle<K>, key: &TCellOwner<K>, at: TimelineTime) -> Result<TimelineTime, RenderError<K, T>> {
    let left_ref = left.upgrade().ok_or_else(|| RenderError::InvalidMarker(left.clone()))?;
    let right_ref = right.upgrade().ok_or_else(|| RenderError::InvalidMarker(right.clone()))?;
    if at < left_ref.ro(key).cached_timeline_time() || right_ref.ro(key).cached_timeline_time() < at {
        return Err(RenderError::RenderTargetTimeOutOfRange {
            component: Default::default(),
            range: left_ref.ro(key).cached_timeline_time()..right_ref.ro(key).cached_timeline_time(),
            at,
        });
    }
    let markers = iter::once(left)
        .chain(markers.iter().map(StaticPointerOwned::reference))
        .chain(iter::once(right))
        .map(|marker| marker.upgrade().ok_or_else(|| RenderError::InvalidMarker(marker.clone())))
        .filter(|marker| match marker {
            Ok(marker) => marker.ro(key).locked_component_time().is_some(),
            Err(_) => true,
        })
        .collect::<Result<Vec<_>, _>>()?;
    match markers.as_slice() {
        [] => Ok(at - left_ref.ro(key).cached_timeline_time()),
        [marker] => {
            let marker = marker.ro(key);
            Ok(at - marker.cached_timeline_time() + marker.locked_component_time().unwrap().into())
        }
        [marker1, marker2] => {
            let marker1 = marker1.ro(key);
            let marker2 = marker2.ro(key);
            let time1 = marker1.cached_timeline_time();
            let time2 = marker2.cached_timeline_time();
            let p = (at - time1) / (time2 - time1);
            let time = marker1.locked_component_time().unwrap().value() + (marker2.locked_component_time().unwrap().value() - marker1.locked_component_time().unwrap().value()) * p;
            Ok(TimelineTime::new(time).unwrap())
        }
        markers => {
            let i = markers.binary_search_by_key(&at, |marker| marker.ro(key).cached_timeline_time()).unwrap_or_else(|x| x);
            let [marker1, marker2] = &markers[i.saturating_sub(1).min(markers.len() - 2)..][..2] else { unreachable!() };
            let marker1 = marker1.ro(key);
            let marker2 = marker2.ro(key);
            let time1 = marker1.cached_timeline_time();
            let time2 = marker2.cached_timeline_time();
            let p = (at - time1) / (time2 - time1);
            let time = marker1.locked_component_time().unwrap().value() + (marker2.locked_component_time().unwrap().value() - marker1.locked_component_time().unwrap().value()) * p;
            Ok(TimelineTime::new(time).unwrap())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use mpdelta_core::component::marker_pin::{MarkerPin, MarkerTime};
    use qcell::TCell;

    struct TestParameterValueType;

    impl ParameterValueType for TestParameterValueType {
        type Image = ();
        type Audio = ();
        type Binary = ();
        type String = ();
        type Integer = ();
        type RealNumber = ();
        type Boolean = ();
        type Dictionary = ();
        type Array = ();
        type ComponentClass = ();
    }

    #[test]
    fn test_time_map() {
        struct K;
        let key = TCellOwner::new();
        fn time_map_for_test(markers: &[MarkerPinHandleOwned<K>], key: &TCellOwner<K>, at: TimelineTime) -> Result<TimelineTime, RenderError<K, TestParameterValueType>> {
            assert!(markers.len() >= 2);
            let [left, markers @ .., right] = markers else { unreachable!() };
            time_map(StaticPointerOwned::reference(left), markers, StaticPointerOwned::reference(right), key, at)
        }
        macro_rules! marker {
            ($t:expr$(,)?) => {
                StaticPointerOwned::new(TCell::new(MarkerPin::new_unlocked(TimelineTime::new($t).unwrap())))
            };
            ($t:expr, $m:expr$(,)?) => {
                StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::new($t).unwrap(), MarkerTime::new($m).unwrap())))
            };
        }
        let markers = [marker!(3.), marker!(4.), marker!(5.), marker!(6.)];
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(3.0).unwrap()), Ok(v) if (v.value() - 0.0).abs() < f64::EPSILON);
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(3.5).unwrap()), Ok(v) if (v.value() - 0.5).abs() < f64::EPSILON);
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(5.0).unwrap()), Ok(v) if (v.value() - 2.0).abs() < f64::EPSILON);
        let markers = [marker!(3.), marker!(4.), marker!(5., 10.), marker!(6.)];
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(3.0).unwrap()), Ok(v) if (v.value() - 8.0).abs() < f64::EPSILON);
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(3.5).unwrap()), Ok(v) if (v.value() - 8.5).abs() < f64::EPSILON);
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(5.0).unwrap()), Ok(v) if (v.value() - 10.0).abs() < f64::EPSILON);
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(5.5).unwrap()), Ok(v) if (v.value() - 10.5).abs() < f64::EPSILON);
        let markers = [marker!(3.), marker!(4., 8.), marker!(5., 10.), marker!(6.)];
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(3.0).unwrap()), Ok(v) if (v.value() - 6.0).abs() < f64::EPSILON);
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(3.5).unwrap()), Ok(v) if (v.value() - 7.0).abs() < f64::EPSILON);
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(4.5).unwrap()), Ok(v) if (v.value() - 9.0).abs() < f64::EPSILON);
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(5.0).unwrap()), Ok(v) if (v.value() - 10.0).abs() < f64::EPSILON);
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(5.5).unwrap()), Ok(v) if (v.value() - 11.0).abs() < f64::EPSILON);
        let markers = [marker!(3.), marker!(4., 8.), marker!(5., 10.), marker!(6.), marker!(7., 13.), marker!(8.), marker!(10., 10.)];
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(3.0).unwrap()), Ok(v) if (v.value() - 6.0).abs() < f64::EPSILON);
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(3.5).unwrap()), Ok(v) if (v.value() - 7.0).abs() < f64::EPSILON);
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(4.5).unwrap()), Ok(v) if (v.value() - 9.0).abs() < f64::EPSILON);
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(5.0).unwrap()), Ok(v) if (v.value() - 10.0).abs() < f64::EPSILON);
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(6.0).unwrap()), Ok(v) if (v.value() - 11.5).abs() < f64::EPSILON);
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(7.0).unwrap()), Ok(v) if (v.value() - 13.0).abs() < f64::EPSILON);
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(8.0).unwrap()), Ok(v) if (v.value() - 12.0).abs() < f64::EPSILON);
        assert_matches!(time_map_for_test(&markers, &key, TimelineTime::new(10.0).unwrap()), Ok(v) if (v.value() - 10.0).abs() < f64::EPSILON);
    }
}