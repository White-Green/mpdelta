use self::component_validity::{collect_invalidate_range, ComponentInvalidateRange, ImageRequiredParamsInvalidateRange, ImageRequiredParamsInvalidateRangeTransform};
use crate::time_stretch::{GlobalTime, LocalTime, TimeStretch};
use crate::{AudioCombinerParam, AudioCombinerRequest, Combiner, CombinerBuilder, ImageCombinerParam, ImageCombinerRequest, ImageSizeRequest, InvalidateRange, RenderError, RenderResult};
use arc_swap::{ArcSwap, Guard};
use async_trait::async_trait;
use cgmath::Vector3;
use futures::future::OptionFuture;
use futures::{stream, FutureExt, StreamExt, TryFutureExt, TryStreamExt};
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::common::time_split_value_persistent::TimeSplitValuePersistent;
use mpdelta_core::component::class::{ComponentClass, ComponentClassIdentifier};
use mpdelta_core::component::instance::{ComponentInstance, ComponentInstanceId};
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerPinId, MarkerTime};
use mpdelta_core::component::parameter::value::{DynEditableEasingValueMarker, DynEditableSingleValueMarker, EasingInput, EasingValue};
use mpdelta_core::component::parameter::{
    AbstractFile, AudioRequiredParams, ImageRequiredParams, ImageRequiredParamsFixed, ImageRequiredParamsTransform, ImageRequiredParamsTransformFixed, Never, Opacity, Parameter, ParameterNullableValue, ParameterSelect, ParameterType, ParameterValueFixed, ParameterValueRaw, ParameterValueType,
    VariableParameterPriority, VariableParameterValue,
};
use mpdelta_core::component::processor::{
    CacheKey, ComponentProcessor, ComponentProcessorGatherNativeDyn, ComponentProcessorNative, ComponentProcessorNativeDyn, ComponentProcessorWrapper, DynError, DynGatherNativeParameter, GatherNativeParameter, ImageSize, NativeGatherProcessorInput, NativeProcessorInput, NativeProcessorRequest,
    ParameterGatherNativeProcessorParam, ProcessorCache,
};
use mpdelta_core::core::IdGenerator;
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use mpdelta_core::time::TimelineTime;
use rpds::VectorSync;
use std::any::Any;
use std::borrow::Cow;
use std::collections::HashMap;
use std::error::Error;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::sync::Arc;
use std::{future, iter};
use tokio::runtime::Handle;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;

mod component_validity;

macro_rules! assert_impl {
    (<$(($($type_arg:tt)+)),*$(,)?> => ($($tr:tt)+): $e:expr) => {
        {
            fn f<__Type, $($($type_arg)+),*>(v: __Type) -> __Type
            where __Type: $($tr)+ { v }
            f($e)
        }
    };
    (($($tr:tt)+): $e:expr) => {
        assert_impl!(<> => ($($tr)+): $e)
    };
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

struct RenderingContext<ImageCombinerBuilder, AudioCombinerBuilder, Cache> {
    runtime: Handle,
    image_combiner_builder: ImageCombinerBuilder,
    audio_combiner_builder: AudioCombinerBuilder,
    cache: Cache,
}

struct EvaluationContext<T: ParameterValueType, ImageCombinerBuilder, AudioCombinerBuilder, Cache> {
    render_ctx: Arc<RenderingContext<ImageCombinerBuilder, AudioCombinerBuilder, Cache>>,
    components: HashMap<ComponentInstanceId, Arc<ComponentRenderer<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache>>>,
    image_size: ImageSize,
    time_map: Arc<HashMap<MarkerPinId, TimelineTime>>,
    #[allow(unused)] // Dropされないためのguardとしてのフィールド
    fixed_parameters_placeholder_owned: Box<[StaticPointerOwned<RwLock<dyn ComponentClass<T>>>]>,
    #[allow(unused)] // Dropされないためのguardとしてのフィールド
    variable_parameters_placeholder_owned: Box<[StaticPointerOwned<RwLock<dyn ComponentClass<T>>>]>,
    fixed_parameter_component_map: HashMap<ParameterComponentMapKey<T>, ParameterValueRaw<T::Image, T::Audio>>,
    variable_parameter_component_map: HashMap<ParameterComponentMapKey<T>, ParameterGatherNativeProcessorParam<T::Image, T::Audio>>,
}

struct ParameterComponentMapKey<T>(*const dyn ComponentProcessorNativeDyn<T>);

// SAFETY: これはアドレス値の比較のためにのみ用いるため安全
unsafe impl<T> Send for ParameterComponentMapKey<T> {}

unsafe impl<T> Sync for ParameterComponentMapKey<T> {}

impl<T> ParameterComponentMapKey<T>
where
    T: ParameterValueType,
{
    fn new(processor: &Arc<dyn ComponentProcessorNativeDyn<T>>) -> Self {
        ParameterComponentMapKey(Arc::as_ptr(processor))
    }
}

impl<T> PartialEq for ParameterComponentMapKey<T> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::addr_eq(self.0, other.0)
    }
}

impl<T> Eq for ParameterComponentMapKey<T> {}

impl<T> Hash for ParameterComponentMapKey<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

struct PlaceholderComponentProcessor {
    ty: Parameter<ParameterSelect>,
}

struct ParameterComponentClass {
    inner: Arc<PlaceholderComponentProcessor>,
}

#[async_trait]
impl<T> ComponentProcessor<T> for PlaceholderComponentProcessor
where
    T: ParameterValueType,
{
    async fn fixed_parameter_types(&self) -> &[(String, ParameterType)] {
        &[]
    }

    async fn update_variable_parameter(&self, _: &[ParameterValueRaw<T::Image, T::Audio>], variable_parameters: &mut Vec<(String, ParameterType)>) {
        variable_parameters.clear();
    }

    async fn num_interprocess_pins(&self, _: &[ParameterValueRaw<T::Image, T::Audio>]) -> usize {
        0
    }
}

#[async_trait]
impl<T> ComponentProcessorNative<T> for PlaceholderComponentProcessor
where
    T: ParameterValueType,
{
    type WholeComponentCacheKey = ();
    type WholeComponentCacheValue = ();
    type FramedCacheKey = ();
    type FramedCacheValue = ();

    fn whole_component_cache_key(&self, _: &[ParameterValueRaw<T::Image, T::Audio>], _: &[TimelineTime]) -> Option<Self::WholeComponentCacheKey> {
        None
    }

    fn framed_cache_key(&self, _: NativeProcessorInput<'_, T>, _: TimelineTime, _: Parameter<ParameterSelect>) -> Option<Self::FramedCacheKey> {
        None
    }

    async fn natural_length(&self, _: &[ParameterValueRaw<T::Image, T::Audio>], _: &mut Option<Arc<Self::WholeComponentCacheValue>>) -> Option<MarkerTime> {
        None
    }

    async fn supports_output_type(&self, _: &[ParameterValueRaw<T::Image, T::Audio>], out: Parameter<ParameterSelect>, _: &mut Option<Arc<Self::WholeComponentCacheValue>>) -> bool {
        self.ty == out
    }

    async fn process(&self, _: NativeProcessorInput<'_, T>, _: TimelineTime, _: Parameter<NativeProcessorRequest>, _: &mut Option<Arc<Self::WholeComponentCacheValue>>, _: &mut Option<Arc<Self::FramedCacheValue>>) -> ParameterValueRaw<T::Image, T::Audio> {
        unimplemented!("Renderer内部でのみ使われるので、この経路で呼ばれることはないはず")
    }
}

#[async_trait]
impl<T> ComponentClass<T> for ParameterComponentClass
where
    T: ParameterValueType,
{
    fn human_readable_identifier(&self) -> &str {
        "Parameter Placeholder"
    }

    fn identifier(&self) -> ComponentClassIdentifier {
        ComponentClassIdentifier {
            namespace: Cow::Borrowed("mpdelta-internal"),
            name: Cow::Borrowed("ParameterPlaceholder"),
            inner_identifier: Default::default(),
        }
    }

    fn processor(&self) -> ComponentProcessorWrapper<T> {
        ComponentProcessorWrapper::Native(Arc::clone(&self.inner) as Arc<_>)
    }

    async fn instantiate(&self, this: &StaticPointer<RwLock<dyn ComponentClass<T>>>, id: &dyn IdGenerator) -> ComponentInstance<T> {
        let left = MarkerPin::new_unlocked(id.generate_new());
        let right = MarkerPin::new_unlocked(id.generate_new());
        let left_id = *left.id();
        let right_id = *right.id();
        let builder = ComponentInstance::builder(this.clone(), left, right, Vec::new(), ComponentProcessorWrapper::Native(Arc::clone(&self.inner) as Arc<_>));
        let builder = match self.inner.ty {
            Parameter::Image(_) => builder.image_required_params(ImageRequiredParams::new_default(&left_id, &right_id)),
            Parameter::Audio(_) => builder.audio_required_params(AudioRequiredParams::new_default(&left_id, &right_id, 2)),
            _ => builder,
        };
        builder.build(id)
    }
}

macro_rules! make_map {
    ($f:ident) => {
        |result: RenderResult<Parameter<RenderOutput<_, _>>>| match result {
            Ok(value) => Some(Ok(value.$f().ok().unwrap())),
            Err(RenderError::RenderTargetTimeOutOfRange { .. } | RenderError::NotProvided) => None,
            Err(e) => Some(Err(e)),
        }
    };
}

struct ParameterForComponent<T> {
    component_class_owned: StaticPointerOwned<RwLock<dyn ComponentClass<T>>>,
    parameter: Arc<dyn ComponentProcessorNativeDyn<T>>,
}

impl<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache> EvaluationContext<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache>
where
    T: ParameterValueType,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
    Cache: ProcessorCache + 'static,
{
    #[allow(clippy::type_complexity)]
    fn make_render_task_generator(self: &Arc<Self>, ty: ParameterType, at: GlobalTime) -> impl for<'a> Fn((&'a ComponentInstanceId, &'a Arc<ComponentInvalidateRange>)) -> JoinHandle<RenderResult<Parameter<RenderOutput<T::Image, T::Audio>>>> + '_ {
        move |(component, invalidate_range)| self.render_ctx.runtime.spawn(Arc::clone(&self.components[component]).render(Arc::clone(self), Arc::clone(invalidate_range), at, ty.clone()))
    }

    fn get_nullable_value<V: 'static>(&self, value: &TimeSplitValuePersistent<MarkerPinId, Option<EasingValue<V>>>, at: GlobalTime) -> Option<V> {
        let value_index = value.binary_search_by(|pin| GlobalTime::new(self.time_map[pin]).cmp(&at)).unwrap_or_else(|x| x - 1);
        let (left, easing_value, right) = value.get_value(value_index).unwrap();
        easing_value.as_ref().map(|value| {
            let left = self.time_map[left];
            let right = self.time_map[right];
            let p = (at.time() - left) / (right - left);
            let p = value.easing.easing(EasingInput::new(p.into_f64()));
            value.value.get_value(p)
        })
    }

    fn combine<'a, V, C, F>(
        self: &'a Arc<Self>,
        ty: ParameterType,
        combiner: C,
        components: &'a VectorSync<ComponentInstanceId>,
        invalidate_ranges: &'a [Arc<ComponentInvalidateRange>],
        at: GlobalTime,
        map: F,
    ) -> impl Future<Output = RenderResult<V>> + Send + use<'a, T, ImageCombinerBuilder, AudioCombinerBuilder, Cache, V, C, F>
    where
        V: Send + 'a,
        C: Combiner<V> + 'a,
        C::Param: Send,
        F: Fn(RenderResult<Parameter<RenderOutput<T::Image, T::Audio>>>) -> Option<RenderResult<(V, C::Param)>> + Send,
    {
        stream::iter(components.iter().zip(invalidate_ranges))
            .map(self.make_render_task_generator(ty, at))
            .buffered(16)
            .map(Result::unwrap)
            .filter_map(move |result| future::ready(map(result)))
            .try_fold(combiner, |mut combiner, (value, param)| {
                combiner.add(value, param);
                future::ready(Ok(combiner))
            })
            .and_then(|combiner| combiner.collect().map(Ok))
    }

    #[allow(clippy::too_many_arguments)]
    async fn combine_by_replace<V>(
        self: &Arc<Self>,
        ty: ParameterType,
        value: &TimeSplitValuePersistent<MarkerPinId, Option<EasingValue<V>>>,
        components: &VectorSync<ComponentInstanceId>,
        invalidate_ranges: &[Arc<ComponentInvalidateRange>],
        priority: VariableParameterPriority,
        at: GlobalTime,
        map: impl Fn(RenderResult<Parameter<RenderOutput<T::Image, T::Audio>>>) -> Option<RenderResult<V>> + Sync,
        default: impl FnOnce() -> V,
    ) -> RenderResult<V>
    where
        V: Send + Sync + 'static,
    {
        assert_eq!(components.len(), invalidate_ranges.len());
        macro_rules! value_by_components {
            () => {
                stream::iter(components.iter().zip(invalidate_ranges).rev().map(self.make_render_task_generator(ty, at)))
                    .buffered(16)
                    .map(Result::unwrap)
                    .filter_map(|result| future::ready(map(result)))
                    .boxed()
            };
        }
        match priority {
            VariableParameterPriority::PrioritizeManually => {
                if let Some(value) = self.get_nullable_value(value, at) {
                    return Ok(value);
                }
                if let Some(value) = value_by_components!().next().await {
                    return value;
                }
                Ok(default())
            }
            VariableParameterPriority::PrioritizeComponent => {
                if let Some(value) = value_by_components!().next().await {
                    return value;
                }
                if let Some(value) = self.get_nullable_value(value, at) {
                    return Ok(value);
                }
                Ok(default())
            }
        }
    }

    fn variable_parameter_for_component(&self, ty: Parameter<ParameterSelect>) -> ParameterForComponent<T> {
        let inner = Arc::new(PlaceholderComponentProcessor { ty });
        ParameterForComponent {
            component_class_owned: StaticPointerOwned::new(RwLock::new(ParameterComponentClass { inner: Arc::clone(&inner) })).map(|v| v, |v| v),
            parameter: inner,
        }
    }

    async fn variable_parameter_for_native(
        self: &Arc<Self>,
        component_image_size: &ImageSizeRequest,
        transform: Option<&ImageRequiredParamsTransformFixed>,
        param: &VariableParameterValue<ParameterNullableValue<T>>,
        invalidate_ranges: &[Arc<ComponentInvalidateRange>],
        at: GlobalTime,
    ) -> RenderResult<ParameterValueRaw<T::Image, Never>> {
        let &VariableParameterValue { ref params, ref components, priority } = param;
        match params {
            ParameterNullableValue::None => Ok(Parameter::None),
            ParameterNullableValue::Image(value) => {
                let request = ImageCombinerRequest {
                    size_request: *component_image_size,
                    transform: transform.cloned(),
                };
                let mut combiner = self.render_ctx.image_combiner_builder.new_combiner(request);
                if let Some(image) = self.get_nullable_value(value, at) {
                    combiner.add(image, ImageRequiredParamsFixed::default());
                }
                Ok(Parameter::Image(self.combine::<T::Image, _, _>(ParameterType::Image(()), combiner, components, invalidate_ranges, at, make_map!(into_image)).await?))
            }
            ParameterNullableValue::Audio(_) => Err(RenderError::UnsupportedParameterType),
            ParameterNullableValue::Binary(value) => Ok(Parameter::Binary(self.combine_by_replace(ParameterType::Binary(()), value, components, invalidate_ranges, priority, at, make_map!(into_binary), Default::default).await?)),
            ParameterNullableValue::String(value) => Ok(Parameter::String(self.combine_by_replace(ParameterType::String(()), value, components, invalidate_ranges, priority, at, make_map!(into_string), Default::default).await?)),
            ParameterNullableValue::Integer(value) => Ok(Parameter::Integer(self.combine_by_replace(ParameterType::Integer(()), value, components, invalidate_ranges, priority, at, make_map!(into_integer), Default::default).await?)),
            ParameterNullableValue::RealNumber(value) => Ok(Parameter::RealNumber(
                self.combine_by_replace(ParameterType::RealNumber(()), value, components, invalidate_ranges, priority, at, make_map!(into_real_number), Default::default).await?,
            )),
            ParameterNullableValue::Boolean(value) => Ok(Parameter::Boolean(self.combine_by_replace(ParameterType::Boolean(()), value, components, invalidate_ranges, priority, at, make_map!(into_boolean), Default::default).await?)),
            ParameterNullableValue::Dictionary(_) => unimplemented!(),
            ParameterNullableValue::Array(_) => unimplemented!(),
            ParameterNullableValue::ComponentClass(_) => unimplemented!(),
        }
    }

    async fn variable_parameter_for_gather_native(
        self: &Arc<Self>,
        transform: Option<(&ImageRequiredParamsTransform, &Arc<ImageRequiredParamsInvalidateRangeTransform>)>,
        component_length: TimelineTime,
        time_map: &Arc<TimeStretch<GlobalTime, LocalTime>>,
        invert_time_map: &Arc<TimeStretch<LocalTime, GlobalTime>>,
        param: &VariableParameterValue<ParameterNullableValue<T>>,
        invalidate_ranges: &Arc<[Arc<ComponentInvalidateRange>]>,
    ) -> RenderResult<ParameterGatherNativeProcessorParam<T::Image, T::Audio>> {
        let &VariableParameterValue { ref params, ref components, priority } = param;
        macro_rules! make_param {
            ($parameter_type:ident, $value:expr, $into:ident) => {
                Ok(ParameterGatherNativeProcessorParam::<T::Image, T::Audio>::$parameter_type(DynGatherNativeParameter::new(CombineByReplaceParam {
                    eval_ctx: Arc::clone(self),
                    time_map: Arc::clone(invert_time_map),
                    parameter_type: ParameterType::$parameter_type(()),
                    value: $value.clone(),
                    components: components.clone(),
                    invalidate_ranges: Arc::clone(invalidate_ranges),
                    priority,
                    unwrap: make_map!($into),
                    default: Default::default,
                })))
            };
        }
        match params {
            ParameterNullableValue::None => Ok(Parameter::None),
            ParameterNullableValue::Image(value) => Ok(Parameter::Image(DynGatherNativeParameter::new(ImageCombineParam {
                eval_ctx: Arc::clone(self),
                time_map: Arc::clone(invert_time_map),
                transform: transform.map(|(v, i)| (v.clone(), i.clone())),
                value: value.clone(),
                components: components.clone(),
                invalidate_ranges: Arc::clone(invalidate_ranges),
            }))),
            ParameterNullableValue::Audio(value) => {
                let request = AudioCombinerRequest {
                    length: component_length,
                    invert_time_map: Some(Arc::clone(invert_time_map)),
                };
                let mut combiner = self.render_ctx.audio_combiner_builder.new_combiner(request);
                if let Some(audio) = self.get_nullable_value(value, GlobalTime::ZERO) {
                    combiner.add(audio, AudioCombinerParam::new(Arc::new([]), Arc::clone(time_map), InvalidateRange::new()));
                }
                self.combine::<T::Audio, _, _>(ParameterType::Audio(()), combiner, components, invalidate_ranges, GlobalTime::ZERO, make_map!(into_audio)).await.map(Parameter::Audio)
            }
            ParameterNullableValue::Binary(value) => make_param!(Binary, value, into_binary),
            ParameterNullableValue::String(value) => make_param!(String, value, into_string),
            ParameterNullableValue::Integer(value) => make_param!(Integer, value, into_integer),
            ParameterNullableValue::RealNumber(value) => make_param!(RealNumber, value, into_real_number),
            ParameterNullableValue::Boolean(value) => make_param!(Boolean, value, into_boolean),
            ParameterNullableValue::Dictionary(_) => unimplemented!(),
            ParameterNullableValue::Array(_) => unimplemented!(),
            ParameterNullableValue::ComponentClass(_) => unimplemented!(),
        }
    }

    async fn eval_image_required_params(self: &Arc<Self>, params: &ImageRequiredParams, invalidate_range: &ImageRequiredParamsInvalidateRange, at: GlobalTime) -> RenderResult<ImageRequiredParamsFixed> {
        let &ImageRequiredParams {
            ref transform,
            background_color,
            ref opacity,
            ref blend_mode,
            ref composite_operation,
        } = params;
        macro_rules! select_pin_split_value {
            ($value:expr) => {{
                let value_index = $value.binary_search_by(|pin| GlobalTime::new(self.time_map[pin]).cmp(&at)).unwrap_or_else(|x| x - 1);
                $value.get_value(value_index).unwrap()
            }};
        }
        let (left, opacity, right) = select_pin_split_value!(opacity);
        let left = self.time_map[left];
        let right = self.time_map[right];
        let p = (at.time() - left) / (right - left);
        let p = opacity.easing.easing(EasingInput::new(p.into_f64()));
        let opacity = Opacity::saturating_new(opacity.value.get_value(p));
        let (_, &blend_mode, _) = select_pin_split_value!(blend_mode);
        let (_, &composite_operation, _) = select_pin_split_value!(composite_operation);
        Ok(ImageRequiredParamsFixed {
            transform: self.eval_image_required_params_transform(transform, &invalidate_range.transform, at).await?,
            background_color,
            opacity,
            blend_mode,
            composite_operation,
        })
    }

    async fn eval_image_required_params_transform(self: &Arc<Self>, params: &ImageRequiredParamsTransform, invalidate_range: &ImageRequiredParamsInvalidateRangeTransform, at: GlobalTime) -> RenderResult<ImageRequiredParamsTransformFixed> {
        macro_rules! vec3 {
            ($vector3:expr, $invalidate_ranges:expr) => {
                Vector3::from(tokio::try_join!(
                    self.combine_by_replace(ParameterType::RealNumber(()), &$vector3.x.params, &$vector3.x.components, &$invalidate_ranges.x, $vector3.x.priority, at, make_map!(into_real_number), Default::default),
                    self.combine_by_replace(ParameterType::RealNumber(()), &$vector3.y.params, &$vector3.y.components, &$invalidate_ranges.y, $vector3.y.priority, at, make_map!(into_real_number), Default::default),
                    self.combine_by_replace(ParameterType::RealNumber(()), &$vector3.z.params, &$vector3.z.components, &$invalidate_ranges.z, $vector3.z.priority, at, make_map!(into_real_number), Default::default),
                )?)
            };
        }
        match (params, invalidate_range) {
            (
                ImageRequiredParamsTransform::Params {
                    size,
                    scale,
                    translate,
                    rotate,
                    scale_center,
                    rotate_center,
                },
                ImageRequiredParamsInvalidateRangeTransform::Params {
                    size: size_invalidate_range,
                    scale: scale_invalidate_range,
                    translate: translate_invalidate_range,
                    scale_center: scale_center_invalidate_range,
                    rotate_center: rotate_center_invalidate_range,
                },
            ) => Ok(ImageRequiredParamsTransformFixed::Params {
                size: vec3!(size, size_invalidate_range),
                scale: vec3!(scale, scale_invalidate_range),
                translate: vec3!(translate, translate_invalidate_range),
                rotate: {
                    let value_index = rotate.binary_search_by(|pin| GlobalTime::new(self.time_map[pin]).cmp(&at)).unwrap_or_else(|x| x - 1);
                    let (left, rotate, right) = rotate.get_value(value_index).unwrap();
                    let left = self.time_map[left];
                    let right = self.time_map[right];
                    let p = (at.time() - left) / (right - left);
                    let p = rotate.easing.easing(EasingInput::new(p.into_f64()));
                    rotate.value.get_value(p)
                },
                scale_center: vec3!(scale_center, scale_center_invalidate_range),
                rotate_center: vec3!(rotate_center, rotate_center_invalidate_range),
            }),
            (
                ImageRequiredParamsTransform::Free { left_top, right_top, left_bottom, right_bottom },
                ImageRequiredParamsInvalidateRangeTransform::Free {
                    left_top: left_top_invalidate_range,
                    right_top: right_top_invalidate_range,
                    left_bottom: left_bottom_invalidate_range,
                    right_bottom: right_bottom_invalidate_range,
                },
            ) => Ok(ImageRequiredParamsTransformFixed::Free {
                left_top: vec3!(left_top, left_top_invalidate_range),
                right_top: vec3!(right_top, right_top_invalidate_range),
                left_bottom: vec3!(left_bottom, left_bottom_invalidate_range),
                right_bottom: vec3!(right_bottom, right_bottom_invalidate_range),
            }),
            _ => unreachable!(),
        }
    }

    fn eval_audio_required_params(self: &Arc<Self>, params: &AudioRequiredParams, invalidate_range: &[Arc<[Arc<ComponentInvalidateRange>]>], time_map: &Arc<TimeStretch<LocalTime, GlobalTime>>) -> Arc<[DynGatherNativeParameter<f64>]> {
        assert_eq!(params.volume.len(), invalidate_range.len());
        params
            .volume
            .iter()
            .zip(invalidate_range)
            .map(|(volume, invalidate_range)| {
                DynGatherNativeParameter::new(CombineByReplaceParam {
                    eval_ctx: Arc::clone(self),
                    time_map: Arc::clone(time_map),
                    parameter_type: ParameterType::RealNumber(()),
                    value: volume.params.clone(),
                    components: volume.components.clone(),
                    invalidate_ranges: invalidate_range.clone(),
                    priority: volume.priority,
                    unwrap: make_map!(into_real_number),
                    default: || 1.,
                })
            })
            .collect()
    }
}

struct CombineByReplaceParam<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache, V, F, D>
where
    T: ParameterValueType,
{
    eval_ctx: Arc<EvaluationContext<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache>>,
    time_map: Arc<TimeStretch<LocalTime, GlobalTime>>,
    parameter_type: ParameterType,
    value: TimeSplitValuePersistent<MarkerPinId, Option<EasingValue<V>>>,
    components: VectorSync<ComponentInstanceId>,
    invalidate_ranges: Arc<[Arc<ComponentInvalidateRange>]>,
    priority: VariableParameterPriority,
    unwrap: F,
    default: D,
}

impl<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache, V, F, D> Clone for CombineByReplaceParam<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache, V, F, D>
where
    T: ParameterValueType,
    F: Clone,
    D: Clone,
{
    fn clone(&self) -> Self {
        let CombineByReplaceParam {
            eval_ctx,
            time_map,
            parameter_type,
            value,
            components,
            invalidate_ranges,
            priority,
            unwrap,
            default,
        } = self;
        CombineByReplaceParam {
            eval_ctx: eval_ctx.clone(),
            time_map: time_map.clone(),
            parameter_type: parameter_type.clone(),
            value: value.clone(),
            components: components.clone(),
            invalidate_ranges: invalidate_ranges.clone(),
            priority: *priority,
            unwrap: unwrap.clone(),
            default: default.clone(),
        }
    }
}

impl<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache, V, F, D> GatherNativeParameter<V> for CombineByReplaceParam<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache, V, F, D>
where
    T: ParameterValueType,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
    Cache: ProcessorCache + 'static,
    V: Send + Sync + 'static,
    F: Fn(RenderResult<Parameter<RenderOutput<T::Image, T::Audio>>>) -> Option<RenderResult<V>> + Sync,
    D: Fn() -> V + Sync,
{
    type Err = RenderError;
    async fn get_param(&self, at: TimelineTime) -> Result<V, Self::Err> {
        let at = LocalTime::new(at);
        let Some(at) = self.time_map.map(at) else { return Ok((self.default)()) };
        self.eval_ctx.combine_by_replace(self.parameter_type.clone(), &self.value, &self.components, &self.invalidate_ranges, self.priority, at, &self.unwrap, &self.default).await
    }
}

struct ImageCombineParam<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache>
where
    T: ParameterValueType,
{
    eval_ctx: Arc<EvaluationContext<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache>>,
    time_map: Arc<TimeStretch<LocalTime, GlobalTime>>,
    transform: Option<(ImageRequiredParamsTransform, Arc<ImageRequiredParamsInvalidateRangeTransform>)>,
    value: TimeSplitValuePersistent<MarkerPinId, Option<EasingValue<T::Image>>>,
    components: VectorSync<ComponentInstanceId>,
    invalidate_ranges: Arc<[Arc<ComponentInvalidateRange>]>,
}

impl<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache> Clone for ImageCombineParam<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache>
where
    T: ParameterValueType,
{
    fn clone(&self) -> Self {
        let ImageCombineParam {
            eval_ctx,
            time_map,
            transform,
            value,
            components,
            invalidate_ranges,
        } = self;
        ImageCombineParam {
            eval_ctx: eval_ctx.clone(),
            time_map: time_map.clone(),
            transform: transform.clone(),
            value: value.clone(),
            components: components.clone(),
            invalidate_ranges: invalidate_ranges.clone(),
        }
    }
}

impl<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache> GatherNativeParameter<T::Image> for ImageCombineParam<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache>
where
    T: ParameterValueType,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
    Cache: ProcessorCache + 'static,
{
    type Err = RenderError;
    fn get_param(&self, at: TimelineTime) -> impl Future<Output = Result<T::Image, Self::Err>> + Send + '_ {
        let at = LocalTime::new(at);
        // https://github.com/rust-lang/rust/issues/100013 に引っかかるのでこんな変な書き方になっている
        let size_request = ImageSizeRequest {
            width: self.eval_ctx.image_size.width as f32,
            height: self.eval_ctx.image_size.height as f32,
        };
        let Some(at) = self.time_map.map(at) else {
            return self.eval_ctx.render_ctx.image_combiner_builder.new_combiner(ImageCombinerRequest::from(size_request)).collect().map(Ok).boxed();
        };
        async move {
            let transform = if let Some((transform, invalidate_range)) = &self.transform {
                self.eval_ctx.eval_image_required_params_transform(transform, invalidate_range, at).await?
            } else {
                ImageRequiredParamsTransformFixed::default()
            };
            let request = ImageCombinerRequest { size_request, transform: Some(transform) };
            let mut combiner = self.eval_ctx.render_ctx.image_combiner_builder.new_combiner(request);
            if let Some(image) = self.eval_ctx.get_nullable_value(&self.value, at) {
                combiner.add(image, ImageRequiredParamsFixed::default());
            }
            self.eval_ctx.combine::<T::Image, _, _>(ParameterType::Image(()), combiner, &self.components, &self.invalidate_ranges, at, make_map!(into_image)).await
        }
        .boxed()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CachePointer(*const (dyn Any + Send + Sync + 'static));

// SAFETY: これはアドレス値の比較のためにのみ用いるため安全
unsafe impl Send for CachePointer {}

unsafe impl Sync for CachePointer {}

struct ComponentRenderer<T: ParameterValueType, ImageCombinerBuilder, AudioCombinerBuilder, Cache> {
    component: Arc<ComponentInstance<T>>,
    state_lock: Mutex<()>,
    state: ArcSwap<ComponentRendererState<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache>>,
}

enum ComponentRendererState<T: ParameterValueType, ImageCombinerBuilder, AudioCombinerBuilder, Cache> {
    New(Arc<ComponentInstance<T>>),
    Components {
        components: Arc<[(ComponentInstanceId, Arc<ComponentInvalidateRange>)]>,
        image_size: ImageSize,
        time_map: Arc<TimeStretch<GlobalTime, LocalTime>>,
        invert_time_map: Arc<TimeStretch<LocalTime, GlobalTime>>,
        inner_evaluation_context: Arc<EvaluationContext<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache>>,
    },
    Native {
        processor: Arc<dyn ComponentProcessorNativeDyn<T>>,
        time_map: Arc<TimeStretch<GlobalTime, LocalTime>>,
        invert_time_map: Arc<TimeStretch<LocalTime, GlobalTime>>,
        fixed_parameters: Box<[ParameterValueRaw<T::Image, T::Audio>]>,
        interprocess_pins: Arc<[TimelineTime]>,
        whole_component_cache_key: Option<Arc<dyn CacheKey>>,
    },
    GatherNative {
        processor: Arc<dyn ComponentProcessorGatherNativeDyn<T>>,
        time_map: Arc<TimeStretch<GlobalTime, LocalTime>>,
        invert_time_map: Arc<TimeStretch<LocalTime, GlobalTime>>,
        fixed_parameters: Box<[ParameterValueRaw<T::Image, T::Audio>]>,
        interprocess_pins: Arc<[TimelineTime]>,
        variable_parameters: Arc<[ParameterGatherNativeProcessorParam<T::Image, T::Audio>]>,
        whole_component_cache_key: Option<Arc<dyn CacheKey>>,
    },
    FixedParameter {
        time_map: Arc<TimeStretch<GlobalTime, LocalTime>>,
        invert_time_map: Arc<TimeStretch<LocalTime, GlobalTime>>,
        fixed_param: ParameterValueRaw<T::Image, T::Audio>,
    },
    VariableParameter {
        time_map: Arc<TimeStretch<GlobalTime, LocalTime>>,
        invert_time_map: Arc<TimeStretch<LocalTime, GlobalTime>>,
        variable_param: ParameterGatherNativeProcessorParam<<T as ParameterValueType>::Image, <T as ParameterValueType>::Audio>,
    },
}

impl<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache> ComponentRenderer<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache>
where
    T: ParameterValueType,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
    Cache: ProcessorCache + 'static,
{
    fn new(component: Arc<ComponentInstance<T>>) -> ComponentRenderer<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache> {
        ComponentRenderer {
            component: Arc::clone(&component),
            state_lock: Mutex::new(()),
            state: ArcSwap::new(Arc::new(ComponentRendererState::New(component))),
        }
    }

    // 'staticであることを明示したい
    #[allow(clippy::manual_async_fn)]
    fn render(
        self: Arc<Self>,
        eval_ctx: Arc<EvaluationContext<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache>>,
        invalidate_range: Arc<ComponentInvalidateRange>,
        at: GlobalTime,
        ty: ParameterType,
    ) -> impl Future<Output = RenderResult<Parameter<RenderOutput<T::Image, T::Audio>>>> + Send + 'static {
        async move {
            let component_range = GlobalTime::new(eval_ctx.time_map[self.component.marker_left().id()])..GlobalTime::new(eval_ctx.time_map[self.component.marker_right().id()]);
            let ignore_local_time = matches!(ty, Parameter::Audio(_));
            let valid_local_time = component_range.contains(&at);
            if !ignore_local_time && !valid_local_time {
                return Err(RenderError::RenderTargetTimeOutOfRange {
                    component: *self.component.id(),
                    range: component_range.start.time()..component_range.end.time(),
                    at: at.into(),
                });
            }
            macro_rules! image_required_params {
                () => {
                    eval_ctx.eval_image_required_params(self.component.image_required_params().unwrap(), invalidate_range.image_required_params.as_ref().unwrap(), at).await?
                };
            }
            macro_rules! audio_required_params {
                ($time_map:expr) => {
                    eval_ctx.eval_audio_required_params(self.component.audio_required_params().unwrap(), &invalidate_range.audio_required_params.as_ref().unwrap(), &$time_map)
                };
            }
            let state: Guard<Arc<ComponentRendererState<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache>>> = loop {
                let state = self.state.load();
                if let ComponentRendererState::New(_) = &**state {
                    let write_guard = self.state_lock.lock().await;
                    let ComponentRendererState::New(component) = &**self.state.load() else {
                        continue;
                    };
                    let time_map = Arc::new(TimeStretch::new(component.marker_left(), component.markers(), component.marker_right(), &eval_ctx.time_map));
                    let invert_time_map = Arc::new(time_map.invert().unwrap());

                    let new_state: ComponentRendererState<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache> = match component.processor() {
                        ComponentProcessorWrapper::Component(processor) => {
                            let interprocess_pins = component.interprocess_pins();
                            let interprocess_pins = iter::once(component.marker_left())
                                .chain(component.markers())
                                .chain(iter::once(component.marker_right()))
                                .filter_map(|p| interprocess_pins.contains(p.id()).then_some(*p.id()))
                                .collect::<Vec<_>>();
                            let transform = component.image_required_params().map(|params| (&*params.transform, &invalidate_range.image_required_params.as_ref().unwrap().transform));
                            let fixed_parameters = eval_fixed_parameters(component.fixed_parameters()).collect::<Box<[_]>>();
                            let component_length = processor.natural_length(&fixed_parameters, &interprocess_pins).await.into();
                            let variable_parameters = stream::iter(component.variable_parameters().iter().zip(&invalidate_range.variable_parameters))
                                .then(|(p, invalidate_ranges)| eval_ctx.variable_parameter_for_gather_native(transform, component_length, &time_map, &invert_time_map, p, invalidate_ranges))
                                .try_collect::<Vec<_>>()
                                .await?;
                            let (fixed_parameters_placeholder_owned, fixed_parameter_processors): (Vec<_>, Vec<_>) = component
                                .fixed_parameters_type()
                                .iter()
                                .map(|(_, ty)| eval_ctx.variable_parameter_for_component(ty.select()))
                                .map(|ParameterForComponent { component_class_owned, parameter }| (component_class_owned, parameter))
                                .unzip();
                            let (variable_parameters_placeholder_owned, variable_parameter_processors): (Vec<_>, Vec<_>) = component
                                .variable_parameters_type()
                                .iter()
                                .map(|(_, ty)| eval_ctx.variable_parameter_for_component(ty.select()))
                                .map(|ParameterForComponent { component_class_owned, parameter }| (component_class_owned, parameter))
                                .unzip();
                            let fixed_parameters_placeholder = fixed_parameters_placeholder_owned.iter().map(StaticPointerOwned::reference).cloned().collect::<Vec<_>>();
                            let variable_parameters_placeholder = variable_parameters_placeholder_owned.iter().map(StaticPointerOwned::reference).cloned().collect::<Vec<_>>();

                            let variable_parameter_component_map = variable_parameter_processors.into_iter().zip(variable_parameters).map(|(p, param)| (ParameterComponentMapKey::new(&p), param)).collect();
                            let fixed_parameter_component_map = fixed_parameter_processors.into_iter().zip(&fixed_parameters).map(|(p, param)| (ParameterComponentMapKey::new(&p), param.clone())).collect();

                            let result = processor.process(&fixed_parameters, &fixed_parameters_placeholder, &interprocess_pins, &variable_parameters_placeholder, component.variable_parameters_type()).await;
                            let inner_time_map = mpdelta_differential::collect_cached_time(&*result)?;
                            let (component_ids, component_map): (Vec<_>, HashMap<_, _>) = result.components_dyn().map(|component| (*component.id(), (*component.id(), Arc::new(ComponentRenderer::new(Arc::clone(component)))))).unzip();
                            let component_invalidate_range = collect_invalidate_range(&component_ids, &component_map, &inner_time_map);
                            let inner_eval_ctx = EvaluationContext {
                                render_ctx: Arc::clone(&eval_ctx.render_ctx),
                                components: component_map,
                                image_size: result.default_image_size(),
                                time_map: Arc::new(inner_time_map),
                                fixed_parameters_placeholder_owned: fixed_parameters_placeholder_owned.into_boxed_slice(),
                                variable_parameters_placeholder_owned: variable_parameters_placeholder_owned.into_boxed_slice(),
                                fixed_parameter_component_map,
                                variable_parameter_component_map,
                            };
                            ComponentRendererState::Components {
                                components: component_ids.into_iter().zip(component_invalidate_range).collect(),
                                image_size: result.default_image_size(),
                                time_map,
                                invert_time_map,
                                inner_evaluation_context: Arc::new(inner_eval_ctx),
                            }
                        }
                        ComponentProcessorWrapper::Native(processor) => {
                            if let Some(fixed_param) = eval_ctx.fixed_parameter_component_map.get(&ParameterComponentMapKey::new(processor)) {
                                ComponentRendererState::FixedParameter {
                                    time_map,
                                    invert_time_map,
                                    fixed_param: fixed_param.clone(),
                                }
                            } else if let Some(variable_param) = eval_ctx.variable_parameter_component_map.get(&ParameterComponentMapKey::new(processor)) {
                                ComponentRendererState::VariableParameter {
                                    time_map,
                                    invert_time_map,
                                    variable_param: variable_param.clone(),
                                }
                            } else {
                                let interprocess_pins = component.interprocess_pins();
                                let interprocess_pins = iter::once(component.marker_left())
                                    .chain(component.markers())
                                    .chain(iter::once(component.marker_right()))
                                    .filter(|p| interprocess_pins.contains(p.id()))
                                    .map(|pin| time_map.map(GlobalTime::new(eval_ctx.time_map[pin.id()])).unwrap())
                                    .map(LocalTime::time)
                                    .collect::<Vec<_>>();
                                let fixed_parameters = eval_fixed_parameters(component.fixed_parameters()).collect::<Box<[_]>>();
                                let whole_component_cache_key = processor.whole_component_cache_key(&fixed_parameters, &interprocess_pins);
                                ComponentRendererState::Native {
                                    processor: Arc::clone(processor),
                                    time_map,
                                    invert_time_map,
                                    fixed_parameters,
                                    interprocess_pins: interprocess_pins.into(),
                                    whole_component_cache_key,
                                }
                            }
                        }
                        ComponentProcessorWrapper::GatherNative(processor) => {
                            let interprocess_pins = component.interprocess_pins();
                            let interprocess_pins = iter::once(component.marker_left())
                                .chain(component.markers())
                                .chain(iter::once(component.marker_right()))
                                .filter(|p| interprocess_pins.contains(p.id()))
                                .map(|pin| time_map.map(GlobalTime::new(eval_ctx.time_map[pin.id()])).unwrap())
                                .map(LocalTime::time)
                                .collect::<Vec<_>>();
                            let fixed_parameters = eval_fixed_parameters(component.fixed_parameters()).collect::<Box<[_]>>();
                            let whole_component_cache_key = processor.whole_component_cache_key(&fixed_parameters, &interprocess_pins);
                            let mut whole_component_cache = OptionFuture::from(whole_component_cache_key.as_ref().map(|key| eval_ctx.render_ctx.cache.get(key))).await.flatten();
                            let whole_component_cache_ptr = whole_component_cache.as_ref().map(Arc::as_ptr).map(CachePointer);
                            let component_length = processor.natural_length(&fixed_parameters, &mut whole_component_cache).await.unwrap_or(MarkerTime::new(MixedFraction::MAX).unwrap());
                            if let (Some(whole_component_cache_key), Some(whole_component_cache)) = (&whole_component_cache_key, whole_component_cache) {
                                if whole_component_cache_ptr.is_none_or(|p| p != CachePointer(Arc::as_ptr(&whole_component_cache))) {
                                    eval_ctx.render_ctx.cache.insert(Arc::clone(whole_component_cache_key), whole_component_cache).await;
                                }
                            }
                            let transform = component.image_required_params().map(|params| (&*params.transform, &invalidate_range.image_required_params.as_ref().unwrap().transform));
                            let variable_parameters = stream::iter(component.variable_parameters().iter().zip(&invalidate_range.variable_parameters))
                                .then(|(p, invalidate_ranges)| eval_ctx.variable_parameter_for_gather_native(transform, component_length.into(), &time_map, &invert_time_map, p, invalidate_ranges))
                                .try_collect::<Vec<_>>()
                                .await?;
                            ComponentRendererState::GatherNative {
                                processor: Arc::clone(processor),
                                time_map,
                                invert_time_map,
                                fixed_parameters,
                                interprocess_pins: interprocess_pins.into(),
                                variable_parameters: variable_parameters.into(),
                                whole_component_cache_key,
                            }
                        }
                    };
                    self.state.store(Arc::new(new_state));
                    drop(write_guard);
                } else {
                    break state;
                };
            };
            match &**state {
                ComponentRendererState::New(_) => {
                    unreachable!()
                }
                ComponentRendererState::Components {
                    components,
                    image_size,
                    time_map,
                    invert_time_map,
                    inner_evaluation_context,
                } => {
                    let at = if ignore_local_time { LocalTime::ZERO } else { time_map.map(at).unwrap() };
                    let at = GlobalTime::new(at.time());
                    macro_rules! iter {
                        () => {
                            components
                                .iter()
                                .map(assert_impl!((for<'a> Fn(&'a (ComponentInstanceId, Arc<ComponentInvalidateRange>)) -> (&'a ComponentInstanceId, &'a Arc<ComponentInvalidateRange>)):
                                    |(c, i)| (c, i))).map(inner_evaluation_context.make_render_task_generator(ty.clone(), at))
                        }
                    }
                    macro_rules! as_stream {
                        ($iter:expr) => {
                            stream::iter($iter).buffered(16).map(Result::unwrap).filter_map(|result| {
                                future::ready(match result {
                                    Err(RenderError::NotProvided | RenderError::RenderTargetTimeOutOfRange { .. }) => None,
                                    other => Some(other),
                                })
                            })
                        };
                    }
                    macro_rules! unwrap_iter {
                        ($ty:ident) => {
                            match as_stream!(iter!().rev()).next().await {
                                Some(Ok(Parameter::<RenderOutput<T::Image, T::Audio>>::$ty(value))) => Ok(Parameter::$ty(value)),
                                Some(Ok(_)) => unreachable!(),
                                None => Ok(Parameter::<RenderOutput<T::Image, T::Audio>>::$ty(Default::default())),
                                Some(Err(e)) => Err(e),
                            }
                        };
                    }
                    match ty {
                        ParameterType::None => Ok(Parameter::None),
                        ParameterType::Image(_) => {
                            let request = ImageCombinerRequest {
                                size_request: ImageSizeRequest {
                                    width: image_size.width as f32,
                                    height: image_size.height as f32,
                                },
                                transform: None,
                            };
                            let combiner = inner_evaluation_context.render_ctx.image_combiner_builder.new_combiner(request);
                            let combiner = as_stream!(iter!())
                                .try_fold(combiner, |mut acc, result| async {
                                    let (image, param) = result.into_image().ok().unwrap();
                                    acc.add(image, param);
                                    Ok(acc)
                                })
                                .await?;
                            Ok(Parameter::<RenderOutput<T::Image, T::Audio>>::Image((combiner.collect().boxed().await, image_required_params!())))
                        }
                        ParameterType::Audio(_) => {
                            let request = AudioCombinerRequest {
                                length: eval_ctx.time_map[self.component.marker_right().id()] - eval_ctx.time_map[self.component.marker_left().id()],
                                invert_time_map: None,
                            };
                            let combiner = inner_evaluation_context.render_ctx.audio_combiner_builder.new_combiner(request);
                            let combiner = as_stream!(iter!())
                                .try_fold(combiner, |mut acc, result| async {
                                    let (audio, param) = result.into_audio().ok().unwrap();
                                    acc.add(audio, param);
                                    Ok(acc)
                                })
                                .await?;
                            Ok(Parameter::<RenderOutput<T::Image, T::Audio>>::Audio((
                                combiner.collect().boxed().await,
                                AudioCombinerParam::new(audio_required_params!(invert_time_map), Arc::clone(time_map), invalidate_range.invalidate_range.clone()),
                            )))
                        }
                        ParameterType::Binary(_) => unwrap_iter!(Binary),
                        ParameterType::String(_) => unwrap_iter!(String),
                        ParameterType::Integer(_) => unwrap_iter!(Integer),
                        ParameterType::RealNumber(_) => unwrap_iter!(RealNumber),
                        ParameterType::Boolean(_) => unwrap_iter!(Boolean),
                        ParameterType::Dictionary(_) => unimplemented!(),
                        ParameterType::Array(_) => unimplemented!(),
                        ParameterType::ComponentClass(_) => unimplemented!(),
                    }
                }
                ComponentRendererState::Native {
                    processor,
                    time_map,
                    invert_time_map,
                    fixed_parameters,
                    interprocess_pins,
                    whole_component_cache_key,
                } => {
                    let mut whole_component_cache = OptionFuture::from(whole_component_cache_key.as_ref().map(|key| eval_ctx.render_ctx.cache.get(key))).await.flatten();
                    let old_whole_component_cache_ptr = whole_component_cache.as_ref().map(Arc::as_ptr).map(CachePointer);
                    if !processor.supports_output_type(fixed_parameters, ty.select(), &mut whole_component_cache).await {
                        if let (Some(key), Some(value)) = (whole_component_cache_key, whole_component_cache) {
                            if old_whole_component_cache_ptr.is_none_or(|p| p != CachePointer(Arc::as_ptr(&value))) {
                                eval_ctx.render_ctx.cache.insert(Arc::clone(key), value).await;
                            }
                        }
                        return Err(RenderError::NotProvided);
                    }
                    let transform = if let Some(image_required_params) = self.component.image_required_params().filter(|_| valid_local_time) {
                        Some(eval_ctx.eval_image_required_params_transform(&image_required_params.transform, &invalidate_range.image_required_params.as_ref().unwrap().transform, at).await?)
                    } else {
                        None
                    };
                    let image_size = ImageSizeRequest {
                        width: eval_ctx.image_size.width as f32,
                        height: eval_ctx.image_size.height as f32,
                    };
                    let variable_parameters = stream::iter(self.component.variable_parameters().iter().zip(&invalidate_range.variable_parameters))
                        .then(|(param, invalidate_ranges)| eval_ctx.variable_parameter_for_native(&image_size, transform.as_ref(), param, invalidate_ranges, at))
                        .try_collect::<Vec<_>>()
                        .await?;
                    let at = if ignore_local_time { LocalTime::ZERO } else { time_map.map(at).unwrap() };
                    let parameters = NativeProcessorInput {
                        fixed_parameters,
                        interprocess_pins,
                        variable_parameters: &variable_parameters,
                        variable_parameter_type: self.component.variable_parameters_type(),
                    };
                    let framed_cache_key = processor.framed_cache_key(parameters, at.time(), ty.select());
                    let mut framed_cache = OptionFuture::from(framed_cache_key.as_ref().map(|key| eval_ctx.render_ctx.cache.get(key))).await.flatten();
                    let old_framed_cache_ptr = framed_cache.as_ref().map(Arc::as_ptr).map(CachePointer);
                    let ty = match ty {
                        ParameterType::None => Parameter::<NativeProcessorRequest>::None,
                        ParameterType::Image(_) => {
                            let size = match transform {
                                Some(ImageRequiredParamsTransformFixed::Params { size: Vector3 { x, y, .. }, .. }) => ((eval_ctx.image_size.width as f64 * x).round() as u32, (eval_ctx.image_size.height as f64 * y).round() as u32),
                                _ => (eval_ctx.image_size.width, eval_ctx.image_size.height),
                            };
                            Parameter::<NativeProcessorRequest>::Image(size)
                        }
                        ParameterType::Audio(_) => Parameter::<NativeProcessorRequest>::Audio(()),
                        ParameterType::Binary(_) => Parameter::<NativeProcessorRequest>::Binary(()),
                        ParameterType::String(_) => Parameter::<NativeProcessorRequest>::String(()),
                        ParameterType::Integer(_) => Parameter::<NativeProcessorRequest>::Integer(()),
                        ParameterType::RealNumber(_) => Parameter::<NativeProcessorRequest>::RealNumber(()),
                        ParameterType::Boolean(_) => Parameter::<NativeProcessorRequest>::Boolean(()),
                        ParameterType::Dictionary(_) => Parameter::<NativeProcessorRequest>::Dictionary(()),
                        ParameterType::Array(_) => Parameter::<NativeProcessorRequest>::Array(()),
                        ParameterType::ComponentClass(_) => Parameter::<NativeProcessorRequest>::ComponentClass(()),
                    };
                    let result = processor.process(parameters, at.time(), ty, &mut whole_component_cache, &mut framed_cache).await;
                    tokio::join!(
                        async {
                            if let (Some(key), Some(value)) = (whole_component_cache_key, whole_component_cache) {
                                if old_whole_component_cache_ptr.is_none_or(|p| p != CachePointer(Arc::as_ptr(&value))) {
                                    eval_ctx.render_ctx.cache.insert(Arc::clone(key), value).await;
                                }
                            }
                        },
                        async {
                            if let (Some(key), Some(value)) = (framed_cache_key, framed_cache) {
                                if old_framed_cache_ptr.is_none_or(|p| p != CachePointer(Arc::as_ptr(&value))) {
                                    eval_ctx.render_ctx.cache.insert(key, value).await;
                                }
                            }
                        }
                    );
                    if ty.select() != result.select() {
                        return Err(RenderError::OutputTypeMismatch {
                            component: *self.component.id(),
                            expect: ty.select(),
                            actual: result.select(),
                        });
                    }
                    let result = match result {
                        Parameter::None => Parameter::<RenderOutput<T::Image, T::Audio>>::None,
                        Parameter::Image(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Image((value, image_required_params!())),
                        Parameter::Audio(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Audio((value, AudioCombinerParam::new(audio_required_params!(invert_time_map), Arc::clone(time_map), invalidate_range.invalidate_range.clone()))),
                        Parameter::Binary(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Binary(value),
                        Parameter::String(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::String(value),
                        Parameter::Integer(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Integer(value),
                        Parameter::RealNumber(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::RealNumber(value),
                        Parameter::Boolean(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Boolean(value),
                        Parameter::Dictionary(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Dictionary(value),
                        Parameter::Array(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Array(value),
                        Parameter::ComponentClass(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::ComponentClass(value),
                    };
                    Ok(result)
                }
                ComponentRendererState::GatherNative {
                    processor,
                    time_map,
                    invert_time_map,
                    fixed_parameters,
                    interprocess_pins,
                    variable_parameters,
                    whole_component_cache_key,
                } => {
                    let parameters = NativeGatherProcessorInput {
                        fixed_parameters,
                        interprocess_pins,
                        variable_parameters,
                        variable_parameter_type: self.component.variable_parameters_type(),
                    };
                    let at = if ignore_local_time { LocalTime::ZERO } else { time_map.map(at).unwrap() };
                    let ty = match ty {
                        ParameterType::None => Parameter::<NativeProcessorRequest>::None,
                        ParameterType::Image(_) => Parameter::<NativeProcessorRequest>::Image((eval_ctx.image_size.width, eval_ctx.image_size.height)),
                        ParameterType::Audio(_) => Parameter::<NativeProcessorRequest>::Audio(()),
                        ParameterType::Binary(_) => Parameter::<NativeProcessorRequest>::Binary(()),
                        ParameterType::String(_) => Parameter::<NativeProcessorRequest>::String(()),
                        ParameterType::Integer(_) => Parameter::<NativeProcessorRequest>::Integer(()),
                        ParameterType::RealNumber(_) => Parameter::<NativeProcessorRequest>::RealNumber(()),
                        ParameterType::Boolean(_) => Parameter::<NativeProcessorRequest>::Boolean(()),
                        ParameterType::Dictionary(_) => Parameter::<NativeProcessorRequest>::Dictionary(()),
                        ParameterType::Array(_) => Parameter::<NativeProcessorRequest>::Array(()),
                        ParameterType::ComponentClass(_) => Parameter::<NativeProcessorRequest>::ComponentClass(()),
                    };
                    let mut cache = OptionFuture::from(whole_component_cache_key.as_ref().map(|key| eval_ctx.render_ctx.cache.get(key))).await.flatten();
                    let old_cache_ptr = cache.as_ref().map(Arc::as_ptr).map(CachePointer);
                    if !processor.supports_output_type(fixed_parameters, ty.select(), &mut cache).await {
                        if let (Some(key), Some(value)) = (whole_component_cache_key, cache) {
                            if old_cache_ptr.is_none_or(|p| p != CachePointer(Arc::as_ptr(&value))) {
                                eval_ctx.render_ctx.cache.insert(Arc::clone(key), value).await;
                            }
                        }
                        return Err(RenderError::NotProvided);
                    }
                    let result = processor.process(parameters, at.time(), ty, &mut cache).await;
                    if let (Some(key), Some(value)) = (whole_component_cache_key, cache) {
                        if old_cache_ptr.is_none_or(|p| p != CachePointer(Arc::as_ptr(&value))) {
                            eval_ctx.render_ctx.cache.insert(Arc::clone(key), value).await;
                        }
                    }
                    if ty.select() != result.select() {
                        return Err(RenderError::OutputTypeMismatch {
                            component: *self.component.id(),
                            expect: ty.select(),
                            actual: result.select(),
                        });
                    }
                    let result = match result {
                        Parameter::None => Parameter::<RenderOutput<T::Image, T::Audio>>::None,
                        Parameter::Image(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Image((value, image_required_params!())),
                        Parameter::Audio(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Audio((value, AudioCombinerParam::new(audio_required_params!(invert_time_map), Arc::clone(time_map), invalidate_range.invalidate_range.clone()))),
                        Parameter::Binary(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Binary(value),
                        Parameter::String(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::String(value),
                        Parameter::Integer(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Integer(value),
                        Parameter::RealNumber(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::RealNumber(value),
                        Parameter::Boolean(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Boolean(value),
                        Parameter::Dictionary(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Dictionary(value),
                        Parameter::Array(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Array(value),
                        Parameter::ComponentClass(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::ComponentClass(value),
                    };
                    Ok(result)
                }
                ComponentRendererState::FixedParameter { time_map, invert_time_map, fixed_param } => Ok(match fixed_param.clone() {
                    Parameter::None => Parameter::<RenderOutput<T::Image, T::Audio>>::None,
                    Parameter::Image(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Image((value, image_required_params!())),
                    Parameter::Audio(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Audio((value, AudioCombinerParam::new(audio_required_params!(invert_time_map), Arc::clone(time_map), invalidate_range.invalidate_range.clone()))),
                    Parameter::Binary(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Binary(value),
                    Parameter::String(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::String(value),
                    Parameter::Integer(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Integer(value),
                    Parameter::RealNumber(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::RealNumber(value),
                    Parameter::Boolean(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Boolean(value),
                    Parameter::Dictionary(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Dictionary(value),
                    Parameter::Array(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::Array(value),
                    Parameter::ComponentClass(value) => Parameter::<RenderOutput<T::Image, T::Audio>>::ComponentClass(value),
                }),
                ComponentRendererState::VariableParameter { time_map, invert_time_map, variable_param } => {
                    let map_err = |dyn_error: DynError| match <dyn Error + Send + Sync>::downcast(dyn_error.0) {
                        Ok(error) => *error,
                        Err(dyn_error) => RenderError::UnknownError(Arc::from(dyn_error)),
                    };
                    match variable_param {
                        Parameter::None => Ok(Parameter::None),
                        Parameter::Image(value) => Ok(Parameter::<RenderOutput<T::Image, T::Audio>>::Image((value.get_param(at.time()).await.map_err(map_err)?, image_required_params!()))),
                        Parameter::Audio(value) => Ok(Parameter::<RenderOutput<T::Image, T::Audio>>::Audio((
                            value.clone(),
                            AudioCombinerParam::new(audio_required_params!(invert_time_map), Arc::clone(time_map), invalidate_range.invalidate_range.clone()),
                        ))),
                        Parameter::Binary(value) => value.get_param(at.time()).await.map(Parameter::<RenderOutput<T::Image, T::Audio>>::Binary).map_err(map_err),
                        Parameter::String(value) => value.get_param(at.time()).await.map(Parameter::<RenderOutput<T::Image, T::Audio>>::String).map_err(map_err),
                        Parameter::Integer(value) => value.get_param(at.time()).await.map(Parameter::<RenderOutput<T::Image, T::Audio>>::Integer).map_err(map_err),
                        Parameter::RealNumber(value) => value.get_param(at.time()).await.map(Parameter::<RenderOutput<T::Image, T::Audio>>::RealNumber).map_err(map_err),
                        Parameter::Boolean(value) => value.get_param(at.time()).await.map(Parameter::<RenderOutput<T::Image, T::Audio>>::Boolean).map_err(map_err),
                        Parameter::Dictionary(value) => value.get_param(at.time()).await.map(Parameter::<RenderOutput<T::Image, T::Audio>>::Dictionary).map_err(map_err),
                        Parameter::Array(value) => value.get_param(at.time()).await.map(Parameter::<RenderOutput<T::Image, T::Audio>>::Array).map_err(map_err),
                        Parameter::ComponentClass(value) => value.get_param(at.time()).await.map(Parameter::<RenderOutput<T::Image, T::Audio>>::ComponentClass).map_err(map_err),
                    }
                }
            }
        }
    }
}

fn eval_fixed_parameter<Image, Audio>(fixed_parameter: &ParameterValueFixed<Image, Audio>) -> ParameterValueRaw<Image, Audio>
where
    Image: Send + Sync + Clone + 'static,
    Audio: Send + Sync + Clone + 'static,
{
    match fixed_parameter {
        ParameterValueFixed::None => ParameterValueRaw::None,
        ParameterValueFixed::Image(value) => ParameterValueRaw::Image(value.get_value()),
        ParameterValueFixed::Audio(value) => ParameterValueRaw::Audio(value.get_value()),
        ParameterValueFixed::Binary(value) => ParameterValueRaw::Binary(value.get_value()),
        ParameterValueFixed::String(value) => ParameterValueRaw::String(value.get_value()),
        ParameterValueFixed::Integer(value) => ParameterValueRaw::Integer(value.get_value()),
        ParameterValueFixed::RealNumber(value) => ParameterValueRaw::RealNumber(value.get_value()),
        ParameterValueFixed::Boolean(value) => ParameterValueRaw::Boolean(value.get_value()),
        ParameterValueFixed::Dictionary(value) => ParameterValueRaw::Dictionary(value.get_value()),
        ParameterValueFixed::Array(value) => ParameterValueRaw::Array(value.get_value()),
        ParameterValueFixed::ComponentClass(()) => ParameterValueRaw::ComponentClass(()),
    }
}

fn eval_fixed_parameters<Image, Audio>(fixed_parameters: &[ParameterValueFixed<Image, Audio>]) -> impl Iterator<Item = ParameterValueRaw<Image, Audio>> + '_
where
    Image: Send + Sync + Clone + 'static,
    Audio: Send + Sync + Clone + 'static,
{
    fixed_parameters.iter().map(eval_fixed_parameter)
}

pub struct Renderer<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache>
where
    T: ParameterValueType,
{
    eval_ctx: Arc<EvaluationContext<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache>>,
    renderer: Arc<ComponentRenderer<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache>>,
    length: MarkerTime,
    invalidate_range: Arc<ComponentInvalidateRange>,
}

impl<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache> Renderer<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache>
where
    T: ParameterValueType,
    ImageCombinerBuilder: CombinerBuilder<T::Image, Request = ImageCombinerRequest, Param = ImageCombinerParam> + 'static,
    AudioCombinerBuilder: CombinerBuilder<T::Audio, Request = AudioCombinerRequest, Param = AudioCombinerParam> + 'static,
    Cache: ProcessorCache + 'static,
{
    pub fn new(component: Arc<ComponentInstance<T>>, runtime: Handle, image_combiner_builder: ImageCombinerBuilder, audio_combiner_builder: AudioCombinerBuilder, cache: Cache) -> Renderer<T, ImageCombinerBuilder, AudioCombinerBuilder, Cache> {
        let renderer = Arc::new(ComponentRenderer::new(component));
        let invalidate_range = Arc::new(ComponentInvalidateRange::new_default(&renderer.component));
        let marker_left_time = renderer.component.marker_left().locked_component_time().unwrap().into();
        let marker_right_time = renderer.component.marker_right().locked_component_time().unwrap().into();
        let eval_ctx = Arc::new(EvaluationContext {
            render_ctx: Arc::new(RenderingContext {
                runtime,
                image_combiner_builder,
                audio_combiner_builder,
                cache,
            }),
            components: HashMap::from([(*renderer.component.id(), Arc::clone(&renderer))]),
            image_size: ImageSize { width: 1920, height: 1080 },
            time_map: Arc::new(HashMap::from([(*renderer.component.marker_left().id(), marker_left_time), (*renderer.component.marker_right().id(), marker_right_time)])),
            fixed_parameters_placeholder_owned: Box::new([]),
            variable_parameters_placeholder_owned: Box::new([]),
            fixed_parameter_component_map: Default::default(),
            variable_parameter_component_map: Default::default(),
        });
        Renderer {
            eval_ctx,
            renderer,
            length: MarkerTime::new((marker_right_time - marker_left_time).value()).unwrap(),
            invalidate_range,
        }
    }

    pub fn component_length(&self) -> MarkerTime {
        self.length
    }

    pub fn render(&self, at: TimelineTime, ty: ParameterType) -> impl Future<Output = RenderResult<ParameterValueRaw<T::Image, T::Audio>>> + Send + 'static {
        Arc::clone(&self.renderer).render(Arc::clone(&self.eval_ctx), Arc::clone(&self.invalidate_range), GlobalTime::new(at), ty).map_ok(|result| match result {
            Parameter::None => Parameter::None,
            Parameter::Image((value, _)) => Parameter::Image(value),
            Parameter::Audio((value, _)) => Parameter::Audio(value),
            Parameter::Binary(value) => Parameter::Binary(value),
            Parameter::String(value) => Parameter::String(value),
            Parameter::Integer(value) => Parameter::Integer(value),
            Parameter::RealNumber(value) => Parameter::RealNumber(value),
            Parameter::Boolean(value) => Parameter::Boolean(value),
            Parameter::Dictionary(value) => Parameter::Dictionary(value),
            Parameter::Array(value) => Parameter::Array(value),
            Parameter::ComponentClass(value) => Parameter::ComponentClass(value),
        })
    }
}
