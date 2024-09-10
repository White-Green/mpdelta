use crate::render::{RenderContext, RenderOutput};
use crate::thread_cancel::CancellationToken;
use crate::{render, AudioCombinerParam, AudioCombinerRequest, Combiner, CombinerBuilder, ImageCombinerParam, ImageCombinerRequest, ImageSizeRequest, RenderResult};
use futures::pin_mut;
use mpdelta_core::common::time_split_value_persistent::TimeSplitValuePersistent;
use mpdelta_core::component::instance::ComponentInstanceId;
use mpdelta_core::component::marker_pin::MarkerPinId;
use mpdelta_core::component::parameter::value::{DynEditableEasingValueMarker, EasingInput, EasingValue};
use mpdelta_core::component::parameter::{AbstractFile, Never, Parameter, ParameterNullableValue, ParameterType, ParameterValueRaw, ParameterValueType, VariableParameterPriority, VariableParameterValue};
use mpdelta_core::time::TimelineTime;
use rpds::VectorSync;
use std::collections::HashMap;
use std::future::Future;
use std::task::{Context, Poll};

type F64Params = VariableParameterValue<TimeSplitValuePersistent<MarkerPinId, Option<EasingValue<f64>>>>;

#[allow(clippy::type_complexity)]
pub(super) fn evaluate_parameter_f64<'a, T, ImageCombinerBuilder, AudioCombinerBuilder>(
    param: &'a F64Params,
    at: TimelineTime,
    ctx: &'a RenderContext<T, ImageCombinerBuilder, AudioCombinerBuilder>,
    time_map: &'a HashMap<MarkerPinId, TimelineTime>,
    cancellation_token: &CancellationToken,
) -> Option<RenderResult<ParameterValueRaw<T::Image, T::Audio>>>
where
    T: ParameterValueType,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
{
    let VariableParameterValue { params, components, priority } = param;
    let get_manually_param = || evaluate_time_split_value_at(params, at, time_map).map(|result| result.map(ParameterValueRaw::RealNumber));
    let ty = &ParameterType::RealNumber(());
    let component_param = ComponentParamCalculator { components, ty, at, ctx };
    evaluate_parameter_inner(get_manually_param, component_param, priority, ty, ctx, cancellation_token)
}

#[allow(clippy::type_complexity)]
pub(super) fn evaluate_parameter<'a, T, ImageCombinerBuilder, AudioCombinerBuilder>(
    param: &'a VariableParameterValue<ParameterNullableValue<T>>,
    ty: &ParameterType,
    at: TimelineTime,
    ctx: &'a RenderContext<T, ImageCombinerBuilder, AudioCombinerBuilder>,
    time_map: &'a HashMap<MarkerPinId, TimelineTime>,
    cancellation_token: &CancellationToken,
) -> Option<RenderResult<ParameterValueRaw<T::Image, T::Audio>>>
where
    T: ParameterValueType,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
{
    let VariableParameterValue { params, components, priority } = param;
    if !params.equals_type(ty) {
        return None;
    }
    let get_manually_param = || match params {
        Parameter::None => Some(Ok(ParameterValueRaw::None)),
        Parameter::Image(value) => evaluate_time_split_value_at(value, at, time_map).map(|result| result.map(ParameterValueRaw::Image)),
        Parameter::Audio(value) => evaluate_time_split_value_at(value, at, time_map).map(|result| result.map(ParameterValueRaw::Audio)),
        Parameter::Binary(value) => evaluate_time_split_value_at(value, at, time_map).map(|result| result.map(ParameterValueRaw::Binary)),
        Parameter::String(value) => evaluate_time_split_value_at(value, at, time_map).map(|result| result.map(ParameterValueRaw::String)),
        Parameter::Integer(value) => evaluate_time_split_value_at(value, at, time_map).map(|result| result.map(ParameterValueRaw::Integer)),
        Parameter::RealNumber(value) => evaluate_time_split_value_at(value, at, time_map).map(|result| result.map(ParameterValueRaw::RealNumber)),
        Parameter::Boolean(value) => evaluate_time_split_value_at(value, at, time_map).map(|result| result.map(ParameterValueRaw::Boolean)),
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
    evaluate_parameter_inner(get_manually_param, component_param, priority, ty, ctx, cancellation_token)
}

#[allow(dead_code)]
struct ComponentParamCalculator<'a, T: ParameterValueType, ImageCombinerBuilder, AudioCombinerBuilder> {
    components: &'a VectorSync<ComponentInstanceId>,
    ty: &'a ParameterType,
    at: TimelineTime,
    ctx: &'a RenderContext<T, ImageCombinerBuilder, AudioCombinerBuilder>,
}

impl<'a, T, ImageCombinerBuilder, AudioCombinerBuilder> ComponentParamCalculator<'a, T, ImageCombinerBuilder, AudioCombinerBuilder>
where
    T: ParameterValueType,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
{
    #[allow(clippy::type_complexity)]
    fn calc(self) -> Option<RenderResult<std::future::Ready<RenderResult<Parameter<RenderOutput<T::Image, T::Audio>>>>>> {
        todo!()
    }
}

#[allow(clippy::type_complexity)]
fn evaluate_parameter_inner<T, ImageCombinerBuilder, AudioCombinerBuilder>(
    get_manually_param: impl FnOnce() -> Option<RenderResult<ParameterValueRaw<T::Image, T::Audio>>>,
    component_param: ComponentParamCalculator<'_, T, ImageCombinerBuilder, AudioCombinerBuilder>,
    priority: &VariableParameterPriority,
    ty: &ParameterType,
    ctx: &RenderContext<T, ImageCombinerBuilder, AudioCombinerBuilder>,
    cancellation_token: &CancellationToken,
) -> Option<RenderResult<ParameterValueRaw<T::Image, T::Audio>>>
where
    T: ParameterValueType,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
{
    match priority {
        VariableParameterPriority::PrioritizeManually => {
            if let Some(value) = get_manually_param() {
                return Some(value);
            }
            match component_param.calc() {
                None => Some(Ok(default_value(ty, ctx))),
                Some(Err(err)) => Some(Err(err)),
                Some(Ok(value)) => Some(await_future_in_rayon_context(value, cancellation_token).map(render::into_parameter_value_fixed)),
            }
        }
        VariableParameterPriority::PrioritizeComponent => match component_param.calc() {
            Some(Err(err)) => Some(Err(err)),
            Some(Ok(value)) => Some(await_future_in_rayon_context(value, cancellation_token).map(render::into_parameter_value_fixed)),
            None => Some(get_manually_param().unwrap_or_else(|| Ok(default_value(ty, ctx)))),
        },
    }
}

fn await_future_in_rayon_context<F: Future>(fut: F, cancellation_token: &CancellationToken) -> F::Output {
    pin_mut!(fut);
    let mut context = Context::from_waker(futures::task::noop_waker_ref());
    loop {
        cancellation_token.assert_not_canceled();
        match fut.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => {
                rayon::yield_now();
            }
        }
    }
}

fn default_value<T: ParameterValueType, ImageCombinerBuilder, AudioCombinerBuilder>(ty: &ParameterType, ctx: &RenderContext<T, ImageCombinerBuilder, AudioCombinerBuilder>) -> ParameterValueRaw<T::Image, T::Audio>
where
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
{
    match ty {
        ParameterType::None => ParameterValueRaw::None,
        ParameterType::Image(_) => ParameterValueRaw::Image(ctx.image_combiner_builder.new_combiner(ImageSizeRequest { width: 0., height: 0. }).collect()),
        ParameterType::Audio(_) => ParameterValueRaw::Audio(ctx.audio_combiner_builder.new_combiner(TimelineTime::ZERO).collect()),
        ParameterType::Binary(_) => ParameterValueRaw::Binary(AbstractFile::default()),
        ParameterType::String(_) => ParameterValueRaw::String(String::default()),
        ParameterType::Integer(_) => ParameterValueRaw::Integer(0),
        ParameterType::RealNumber(_) => ParameterValueRaw::RealNumber(0.),
        ParameterType::Boolean(_) => ParameterValueRaw::Boolean(false),
        ParameterType::Dictionary(ty) => ParameterValueRaw::Dictionary(ty.iter().map(|(k, v)| (k.clone(), default_value(v, ctx))).collect()),
        ParameterType::Array(_) => ParameterValueRaw::Array(Vec::new()),
        ParameterType::ComponentClass(_) => ParameterValueRaw::ComponentClass(()),
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

pub(super) fn evaluate_time_split_value_at<V: 'static>(value: &TimeSplitValuePersistent<MarkerPinId, impl MayBeEasingValue<V>>, at: TimelineTime, time_map: &HashMap<MarkerPinId, TimelineTime>) -> Option<RenderResult<V>> {
    let mut left = 0;
    let mut right = value.len_value();
    while left < right {
        let mid = left + (right - left) / 2;
        let (time_left, value, time_right) = value.get_value(mid).unwrap();
        let time_left = time_map[time_left];
        let time_right = time_map[time_right];
        if time_left <= at && at <= time_right {
            return value.get_value_easing(|| EasingInput::new(((at - time_left) / (time_right - time_left)).into_f64())).map(Ok);
        } else if at < time_left {
            right = mid;
        } else {
            left = mid + 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use mpdelta_core::component::marker_pin::MarkerPin;
    use mpdelta_core::component::parameter::value::{DynEditableEasingValueManager, Easing, EasingIdentifier, NamedAny};
    use mpdelta_core::core::IdGenerator;
    use mpdelta_core::{mfrac, time_split_value_persistent};
    use mpdelta_core_test_util::TestIdGenerator;
    use serde::Serialize;
    use std::sync::Arc;

    #[test]
    fn test_evaluate_time_split_value_at() {
        let id = TestIdGenerator::new();
        #[derive(Clone, Serialize)]
        struct SimpleEasingValue(f64, f64);
        impl DynEditableEasingValueMarker for SimpleEasingValue {
            type Out = f64;
            fn manager(&self) -> &dyn DynEditableEasingValueManager<Self::Out> {
                todo!()
            }

            fn get_raw_value_mut(&mut self) -> &mut dyn NamedAny {
                unimplemented!()
            }
            fn get_value(&self, p: f64) -> f64 {
                self.0 + (self.1 - self.0) * p
            }
        }
        struct FunctionEasing<F>(F);
        impl<F: Send + Sync + Fn(f64) -> f64> Easing for FunctionEasing<F> {
            fn identifier(&self) -> EasingIdentifier {
                todo!()
            }
            fn easing(&self, from: EasingInput) -> f64 {
                (self.0)(from.value())
            }
        }
        let markers = [MarkerPin::new_unlocked(id.generate_new()), MarkerPin::new_unlocked(id.generate_new()), MarkerPin::new_unlocked(id.generate_new()), MarkerPin::new_unlocked(id.generate_new())];
        let time_map = HashMap::from([
            (*markers[0].id(), TimelineTime::new(mfrac!(0))),
            (*markers[1].id(), TimelineTime::new(mfrac!(1))),
            (*markers[2].id(), TimelineTime::new(mfrac!(2))),
            (*markers[3].id(), TimelineTime::new(mfrac!(3))),
        ]);
        let value = time_split_value_persistent![
            *markers[0].id(),
            EasingValue::new(SimpleEasingValue(0.0, 1.0), Arc::new(FunctionEasing(|p: f64| p))),
            *markers[1].id(),
            EasingValue::new(SimpleEasingValue(1.0, 2.0), Arc::new(FunctionEasing(|p: f64| p * p))),
            *markers[2].id(),
            EasingValue::new(SimpleEasingValue(2.0, 0.0), Arc::new(FunctionEasing(|p: f64| p.sqrt()))),
            *markers[3].id(),
        ];
        assert_matches!(evaluate_time_split_value_at::<f64>(&value, TimelineTime::new(-mfrac!(0, 25, 100)), &time_map), None);
        assert_matches!(evaluate_time_split_value_at::<f64>(&value, TimelineTime::new(mfrac!(0, 00, 100)), &time_map), Some(Ok(v)) if (v - 0.0000).abs() < f64::EPSILON);
        assert_matches!(evaluate_time_split_value_at::<f64>(&value, TimelineTime::new(mfrac!(0, 25, 100)), &time_map), Some(Ok(v)) if (v - 0.2500).abs() < f64::EPSILON);
        assert_matches!(evaluate_time_split_value_at::<f64>(&value, TimelineTime::new(mfrac!(0, 50, 100)), &time_map), Some(Ok(v)) if (v - 0.5000).abs() < f64::EPSILON);
        assert_matches!(evaluate_time_split_value_at::<f64>(&value, TimelineTime::new(mfrac!(0, 75, 100)), &time_map), Some(Ok(v)) if (v - 0.7500).abs() < f64::EPSILON);
        assert_matches!(evaluate_time_split_value_at::<f64>(&value, TimelineTime::new(mfrac!(1, 00, 100)), &time_map), Some(Ok(v)) if (v - 1.0000).abs() < f64::EPSILON);
        assert_matches!(evaluate_time_split_value_at::<f64>(&value, TimelineTime::new(mfrac!(1, 25, 100)), &time_map), Some(Ok(v)) if (v - 1.0625).abs() < f64::EPSILON);
        assert_matches!(evaluate_time_split_value_at::<f64>(&value, TimelineTime::new(mfrac!(1, 50, 100)), &time_map), Some(Ok(v)) if (v - 1.2500).abs() < f64::EPSILON);
        assert_matches!(evaluate_time_split_value_at::<f64>(&value, TimelineTime::new(mfrac!(1, 75, 100)), &time_map), Some(Ok(v)) if (v - 1.5625).abs() < f64::EPSILON);
        assert_matches!(evaluate_time_split_value_at::<f64>(&value, TimelineTime::new(mfrac!(2, 00, 100)), &time_map), Some(Ok(v)) if (v - 2.0000).abs() < f64::EPSILON);
        assert_matches!(evaluate_time_split_value_at::<f64>(&value, TimelineTime::new(mfrac!(2, 25, 100)), &time_map), Some(Ok(v)) if (v - 1.0000).abs() < f64::EPSILON);
        assert_matches!(evaluate_time_split_value_at::<f64>(&value, TimelineTime::new(mfrac!(2, 50, 100)), &time_map), Some(Ok(v)) if (v - (2.0 - f64::sqrt(2.))).abs() < f64::EPSILON);
        assert_matches!(evaluate_time_split_value_at::<f64>(&value, TimelineTime::new(mfrac!(2, 75, 100)), &time_map), Some(Ok(v)) if (v - (2.0 - f64::sqrt(3.))).abs() < f64::EPSILON);
        assert_matches!(evaluate_time_split_value_at::<f64>(&value, TimelineTime::new(mfrac!(3, 00, 100)), &time_map), Some(Ok(v)) if (v - 0.0000).abs() < f64::EPSILON);
        assert_matches!(evaluate_time_split_value_at::<f64>(&value, TimelineTime::new(mfrac!(3, 25, 100)), &time_map), None);
    }
}
