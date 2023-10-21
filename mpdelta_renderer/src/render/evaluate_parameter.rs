use crate::render::{RenderContext, RenderOutput};
use crate::{render, Combiner, CombinerBuilder, ImageSizeRequest, RenderError};
use futures::pin_mut;
use mpdelta_core::common::time_split_value::TimeSplitValue;
use mpdelta_core::component::instance::ComponentInstanceHandle;
use mpdelta_core::component::marker_pin::MarkerPinHandle;
use mpdelta_core::component::parameter::value::{DynEditableEasingValueMarker, EasingInput, EasingValue};
use mpdelta_core::component::parameter::{AbstractFile, AudioRequiredParamsFixed, ImageRequiredParamsFixed, Never, Parameter, ParameterNullableValue, ParameterType, ParameterValueFixed, ParameterValueType, VariableParameterPriority, VariableParameterValue};
use mpdelta_core::time::TimelineTime;
use qcell::TCellOwner;
use std::future::Future;
use std::ops::Deref;
use std::task::{Context, Poll};

pub(super) fn evaluate_parameter_f64<'a, K, T, Key, ImageCombinerBuilder, AudioCombinerBuilder>(
    param: &'a VariableParameterValue<K, T, TimeSplitValue<MarkerPinHandle<K>, Option<EasingValue<f64>>>>,
    at: TimelineTime,
    ctx: &'a RenderContext<Key, T, ImageCombinerBuilder, AudioCombinerBuilder>,
) -> Option<Result<ParameterValueFixed<T::Image, T::Audio>, RenderError<K, T>>>
where
    K: 'static,
    T: ParameterValueType,
    Key: Deref<Target = TCellOwner<K>> + Send + Sync + 'static,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageSizeRequest, Param = ImageRequiredParamsFixed> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = (), Param = AudioRequiredParamsFixed> + 'static,
{
    let VariableParameterValue { params, components, priority } = param;
    let get_manually_param = || evaluate_time_split_value_at(params, at, &ctx.key).map(|result| result.map(ParameterValueFixed::RealNumber));
    let ty = &ParameterType::RealNumber(());
    let component_param = ComponentParamCalculator { components, ty, at, ctx };
    evaluate_parameter_inner(get_manually_param, component_param, priority, ty, ctx)
}

pub(super) fn evaluate_parameter<'a, K, T, Key, ImageCombinerBuilder, AudioCombinerBuilder>(
    param: &'a VariableParameterValue<K, T, ParameterNullableValue<K, T>>,
    ty: &ParameterType,
    at: TimelineTime,
    ctx: &'a RenderContext<Key, T, ImageCombinerBuilder, AudioCombinerBuilder>,
) -> Option<Result<ParameterValueFixed<T::Image, T::Audio>, RenderError<K, T>>>
where
    K: 'static,
    T: ParameterValueType,
    Key: Deref<Target = TCellOwner<K>> + Send + Sync + 'static,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageSizeRequest, Param = ImageRequiredParamsFixed> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = (), Param = AudioRequiredParamsFixed> + 'static,
{
    let VariableParameterValue { params, components, priority } = param;
    if !params.equals_type(ty) {
        return None;
    }
    let get_manually_param = || match params {
        Parameter::None => Some(Ok(ParameterValueFixed::None)),
        Parameter::Image(value) => evaluate_time_split_value_at(value, at, &ctx.key).map(|result| result.map(ParameterValueFixed::Image)),
        Parameter::Audio(value) => evaluate_time_split_value_at(value, at, &ctx.key).map(|result| result.map(ParameterValueFixed::Audio)),
        Parameter::Binary(value) => evaluate_time_split_value_at(value, at, &ctx.key).map(|result| result.map(ParameterValueFixed::Binary)),
        Parameter::String(value) => evaluate_time_split_value_at(value, at, &ctx.key).map(|result| result.map(ParameterValueFixed::String)),
        Parameter::Integer(value) => evaluate_time_split_value_at(value, at, &ctx.key).map(|result| result.map(ParameterValueFixed::Integer)),
        Parameter::RealNumber(value) => evaluate_time_split_value_at(value, at, &ctx.key).map(|result| result.map(ParameterValueFixed::RealNumber)),
        Parameter::Boolean(value) => evaluate_time_split_value_at(value, at, &ctx.key).map(|result| result.map(ParameterValueFixed::Boolean)),
        Parameter::Dictionary(value) => {
            let _: &Never = value;
            unreachable!()
        }
        Parameter::Array(value) => {
            let _: &Never = value;
            unreachable!()
        }
        Parameter::ComponentClass(_) => {
            todo!()
        }
    };
    let component_param = ComponentParamCalculator { components, ty, at, ctx };
    evaluate_parameter_inner(get_manually_param, component_param, priority, ty, ctx)
}

struct ComponentParamCalculator<'a, K: 'static, T: ParameterValueType, Key, ImageCombinerBuilder, AudioCombinerBuilder> {
    components: &'a [ComponentInstanceHandle<K, T>],
    ty: &'a ParameterType,
    at: TimelineTime,
    ctx: &'a RenderContext<Key, T, ImageCombinerBuilder, AudioCombinerBuilder>,
}

impl<'a, K, T, Key, ImageCombinerBuilder, AudioCombinerBuilder> ComponentParamCalculator<'a, K, T, Key, ImageCombinerBuilder, AudioCombinerBuilder>
where
    K: 'static,
    T: ParameterValueType,
    Key: Deref<Target = TCellOwner<K>> + Send + Sync + 'static,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageSizeRequest, Param = ImageRequiredParamsFixed> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = (), Param = AudioRequiredParamsFixed> + 'static,
{
    fn calc(self) -> Option<Result<impl Future<Output = Result<Parameter<RenderOutput<T::Image, T::Audio>>, RenderError<K, T>>> + 'a, RenderError<K, T>>> {
        let ComponentParamCalculator { components, ty, at, ctx } = self;
        let render_target_component = components.iter().rev().find_map(|component| {
            let (left, right) = {
                let Some(component) = component.upgrade() else {
                    return Some(Err(RenderError::InvalidComponent(component.clone())));
                };
                let component = component.ro(&ctx.key);
                let Some(left) = component.marker_left().upgrade() else {
                    return Some(Err(RenderError::InvalidMarker(component.marker_left().clone())));
                };
                let left = left.ro(&ctx.key).cached_timeline_time();
                let Some(right) = component.marker_right().upgrade() else {
                    return Some(Err(RenderError::InvalidMarker(component.marker_right().clone())));
                };
                let right = right.ro(&ctx.key).cached_timeline_time();
                (left, right)
            };
            if !(left <= at && at <= right) {
                return None;
            }
            Some(Ok(component))
        });
        match render_target_component {
            None => None,
            Some(Err(err)) => Some(Err(err)),
            Some(Ok(component)) => Some(Ok(render::render_inner(component, at, ty, ctx))),
        }
    }
}

fn evaluate_parameter_inner<K, T, Key, ImageCombinerBuilder, AudioCombinerBuilder>(
    get_manually_param: impl FnOnce() -> Option<Result<ParameterValueFixed<T::Image, T::Audio>, RenderError<K, T>>>,
    component_param: ComponentParamCalculator<'_, K, T, Key, ImageCombinerBuilder, AudioCombinerBuilder>,
    priority: &VariableParameterPriority,
    ty: &ParameterType,
    ctx: &RenderContext<Key, T, ImageCombinerBuilder, AudioCombinerBuilder>,
) -> Option<Result<ParameterValueFixed<T::Image, T::Audio>, RenderError<K, T>>>
where
    K: 'static,
    T: ParameterValueType,
    Key: Deref<Target = TCellOwner<K>> + Send + Sync + 'static,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageSizeRequest, Param = ImageRequiredParamsFixed> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = (), Param = AudioRequiredParamsFixed> + 'static,
{
    match priority {
        VariableParameterPriority::PrioritizeManually => {
            if let Some(value) = get_manually_param() {
                return Some(value);
            }
            match component_param.calc() {
                None => Some(Ok(default_value(ty, ctx))),
                Some(Err(err)) => Some(Err(err)),
                Some(Ok(value)) => Some(await_future_in_rayon_context(value).map(render::into_parameter_value_fixed)),
            }
        }
        VariableParameterPriority::PrioritizeComponent => match component_param.calc() {
            Some(Err(err)) => Some(Err(err)),
            Some(Ok(value)) => Some(await_future_in_rayon_context(value).map(render::into_parameter_value_fixed)),
            None => Some(get_manually_param().unwrap_or_else(|| Ok(default_value(ty, ctx)))),
        },
    }
}

fn await_future_in_rayon_context<F: Future>(fut: F) -> F::Output {
    pin_mut!(fut);
    let mut context = Context::from_waker(futures::task::noop_waker_ref());
    loop {
        match fut.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => {
                rayon::yield_now();
            }
        }
    }
}

fn default_value<Key, T: ParameterValueType, ImageCombinerBuilder, AudioCombinerBuilder>(ty: &ParameterType, ctx: &RenderContext<Key, T, ImageCombinerBuilder, AudioCombinerBuilder>) -> ParameterValueFixed<T::Image, T::Audio>
where
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageSizeRequest, Param = ImageRequiredParamsFixed> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = (), Param = AudioRequiredParamsFixed> + 'static,
{
    match ty {
        ParameterType::None => ParameterValueFixed::None,
        ParameterType::Image(_) => ParameterValueFixed::Image(ctx.image_combiner_builder.new_combiner(ImageSizeRequest { width: 0., height: 0. }).collect()),
        ParameterType::Audio(_) => ParameterValueFixed::Audio(ctx.audio_combiner_builder.new_combiner(()).collect()),
        ParameterType::Binary(_) => ParameterValueFixed::Binary(AbstractFile::default()),
        ParameterType::String(_) => ParameterValueFixed::String(String::default()),
        ParameterType::Integer(_) => ParameterValueFixed::Integer(0),
        ParameterType::RealNumber(_) => ParameterValueFixed::RealNumber(0.),
        ParameterType::Boolean(_) => ParameterValueFixed::Boolean(false),
        ParameterType::Dictionary(ty) => ParameterValueFixed::Dictionary(ty.iter().map(|(k, v)| (k.clone(), default_value(v, ctx))).collect()),
        ParameterType::Array(_) => ParameterValueFixed::Array(Vec::new()),
        ParameterType::ComponentClass(_) => ParameterValueFixed::ComponentClass(()),
    }
}

pub(super) trait MayBeEasingValue<T> {
    fn get_value_easing(&self, p: impl FnOnce() -> EasingInput) -> Option<T>;
}

impl<T: 'static> MayBeEasingValue<T> for Option<EasingValue<T>> {
    fn get_value_easing(&self, p: impl FnOnce() -> EasingInput) -> Option<T> {
        let value = self.as_ref()?;
        Some(value.value.get_value(value.easing.easing(p())))
    }
}

impl<T: 'static> MayBeEasingValue<T> for EasingValue<T> {
    fn get_value_easing(&self, p: impl FnOnce() -> EasingInput) -> Option<T> {
        Some(self.value.get_value(self.easing.easing(p())))
    }
}

impl<T: Clone> MayBeEasingValue<T> for T {
    fn get_value_easing(&self, _: impl FnOnce() -> EasingInput) -> Option<T> {
        Some(self.clone())
    }
}

pub(super) fn evaluate_time_split_value_at<K, T: ParameterValueType, V: 'static>(value: &TimeSplitValue<MarkerPinHandle<K>, impl MayBeEasingValue<V>>, at: TimelineTime, key: &TCellOwner<K>) -> Option<Result<V, RenderError<K, T>>> {
    let mut left = 0;
    let mut right = value.len_value();
    while left < right {
        let mid = left + (right - left) / 2;
        let (time_left, value, time_right) = value.get_value(mid).unwrap();
        let Some(time_left) = time_left.upgrade() else {
            return Some(Err(RenderError::InvalidMarker(time_left.clone())));
        };
        let time_left = time_left.ro(key).cached_timeline_time();
        let Some(time_right) = time_right.upgrade() else {
            return Some(Err(RenderError::InvalidMarker(time_right.clone())));
        };
        let time_right = time_right.ro(key).cached_timeline_time();
        if time_left <= at && at <= time_right {
            return value.get_value_easing(|| EasingInput::new((at - time_left) / (time_right - time_left))).map(Ok);
        } else if at < time_left {
            right = mid;
        } else {
            left = mid + 1;
        }
    }
    None
}
