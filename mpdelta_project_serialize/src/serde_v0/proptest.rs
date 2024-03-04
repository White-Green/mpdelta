use crate::serde_v0::{
    AudioRequiredParamsForSerialize, ComponentInstanceForSerialize, ComponentInstanceHandleForSerialize, EasingValueForSerialize, ImageRequiredParamsForSerialize, ImageRequiredParamsTransformForSerialize, MarkerLinkForSerialize, MarkerPinForSerialize, MarkerPinHandleForSerialize,
    ParameterNullableValueForSerialize, ParameterValueFixedForSerialize, ProjectForSerialize, RootComponentClassForSerialize, Ser, VariableParameterValueForSerialize, Vector3ParamsForSerialize, Wrapper,
};
use cgmath::Vector3;
use erased_serde::{Deserializer, Error};
use mpdelta_core::common::time_split_value::TimeSplitValue;
use mpdelta_core::component::class::ComponentClassIdentifier;
use mpdelta_core::component::marker_pin::MarkerTime;
use mpdelta_core::component::parameter::value::{
    DynEditableEasingValue, DynEditableEasingValueIdentifier, DynEditableEasingValueManager, DynEditableEasingValueMarker, DynEditableSingleValue, DynEditableSingleValueIdentifier, DynEditableSingleValueManager, DynEditableSingleValueMarker, EasingIdentifier, NamedAny,
};
use mpdelta_core::component::parameter::{BlendMode, CompositeOperation, ParameterValueType, VariableParameterPriority};
use proptest::array::{uniform3, uniform4};
use proptest::collection::vec;
use proptest::option::of;
use proptest::prelude::{any, Just, Strategy};
use proptest::strategy::TupleUnion;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;
use uuid::Uuid;

pub fn vector3_param() -> impl Strategy<Value = Vector3ParamsForSerialize<Ser>> {
    uniform3((
        TimeSplitValue::strategy_from(any::<MarkerPinHandleForSerialize>(), of(easing_value()), 1..10),
        vec(any::<ComponentInstanceHandleForSerialize>(), 0..10),
        any::<VariableParameterPriority>(),
    ))
    .prop_map(|array| Vector3::from(array.map(|(params, components, priority)| VariableParameterValueForSerialize { params, components, priority })))
}

pub fn image_required_params_transform() -> impl Strategy<Value = ImageRequiredParamsTransformForSerialize<Ser>> {
    TupleUnion::new((
        (
            1,
            Arc::new(
                (vector3_param(), vector3_param(), TimeSplitValue::strategy_from(any::<MarkerPinHandleForSerialize>(), easing_value(), 1..10), vector3_param(), vector3_param()).prop_map(|(scale, translate, rotate, scale_center, rotate_center)| ImageRequiredParamsTransformForSerialize::Params {
                    scale,
                    translate,
                    rotate,
                    scale_center,
                    rotate_center,
                }),
            ),
        ),
        (
            1,
            Arc::new((vector3_param(), vector3_param(), vector3_param(), vector3_param()).prop_map(|(left_top, right_top, left_bottom, right_bottom)| ImageRequiredParamsTransformForSerialize::Free { left_top, right_top, left_bottom, right_bottom })),
        ),
    ))
}

pub fn image_required_params() -> impl Strategy<Value = ImageRequiredParamsForSerialize<Ser>> {
    (
        (1u32.., 1u32..),
        image_required_params_transform(),
        uniform4(any::<u8>()),
        TimeSplitValue::strategy_from(any::<MarkerPinHandleForSerialize>(), easing_value(), 1..10),
        TimeSplitValue::strategy_from(any::<MarkerPinHandleForSerialize>(), any::<BlendMode>(), 1..10),
        TimeSplitValue::strategy_from(any::<MarkerPinHandleForSerialize>(), any::<CompositeOperation>(), 1..10),
    )
        .prop_map(|(aspect_ratio, transform, background_color, opacity, blend_mode, composite_operation)| ImageRequiredParamsForSerialize {
            aspect_ratio,
            transform,
            background_color,
            opacity,
            blend_mode,
            composite_operation,
        })
}

pub fn audio_required_params() -> impl Strategy<Value = AudioRequiredParamsForSerialize<Ser>> {
    vec(
        (
            TimeSplitValue::strategy_from(any::<MarkerPinHandleForSerialize>(), of(easing_value()), 1..10),
            vec(any::<ComponentInstanceHandleForSerialize>(), 0..10),
            any::<VariableParameterPriority>(),
        )
            .prop_map(|(params, components, priority)| VariableParameterValueForSerialize { params, components, priority }),
        1..5,
    )
    .prop_map(|volume| AudioRequiredParamsForSerialize { volume })
}

pub fn single_value<V: Send + Sync + Clone + 'static>() -> impl Strategy<Value = Wrapper<DynEditableSingleValue<V>>> {
    TupleUnion::new((
        (1, Arc::new((any::<String>(), any::<String>()).prop_map(|(a, b)| DynEditableSingleValue::new(EasingValue1(a, b, PhantomData))).prop_map(Wrapper))),
        (1, Arc::new((any::<String>(), any::<String>()).prop_map(|(from, to)| DynEditableSingleValue::new(EasingValue2 { from, to, _phantom: PhantomData })).prop_map(Wrapper))),
        (
            1,
            Arc::new(
                (any::<String>(), any::<String>(), any::<String>())
                    .prop_map(|(from, center, to)| DynEditableSingleValue::new(EasingValue3 { from, center, to, _phantom: PhantomData }))
                    .prop_map(Wrapper),
            ),
        ),
        (
            1,
            Arc::new(
                (any::<String>(), vec(any::<String>(), 0..10), any::<String>())
                    .prop_map(|(from, center, to)| DynEditableSingleValue::new(EasingValue4 { from, center, to, _phantom: PhantomData }))
                    .prop_map(Wrapper),
            ),
        ),
    ))
}

pub fn fixed_parameters<T: ParameterValueType>() -> impl Strategy<Value = ParameterValueFixedForSerialize<T::Image, T::Audio, Ser>> {
    TupleUnion::new((
        (1, Arc::new(Just(ParameterValueFixedForSerialize::None))),
        (1, Arc::new(single_value().prop_map(ParameterValueFixedForSerialize::Image))),
        (1, Arc::new(single_value().prop_map(ParameterValueFixedForSerialize::Audio))),
        (1, Arc::new(single_value().prop_map(ParameterValueFixedForSerialize::Binary))),
        (1, Arc::new(single_value().prop_map(ParameterValueFixedForSerialize::String))),
        (1, Arc::new(single_value().prop_map(ParameterValueFixedForSerialize::Integer))),
        (1, Arc::new(single_value().prop_map(ParameterValueFixedForSerialize::RealNumber))),
        (1, Arc::new(single_value().prop_map(ParameterValueFixedForSerialize::Boolean))),
        (1, Arc::new(single_value().prop_map(ParameterValueFixedForSerialize::Dictionary))),
        (1, Arc::new(single_value().prop_map(ParameterValueFixedForSerialize::Array))),
    ))
}

macro_rules! impl_easing_value {
    ($value:ident, $manager:ident) => {
        impl<T: Clone + Send + Sync + 'static> DynEditableEasingValueMarker for $value<T> {
            type Out = T;

            fn manager(&self) -> &dyn DynEditableEasingValueManager<Self::Out> {
                &$manager(PhantomData)
            }

            fn get_raw_values_mut(&mut self) -> (&mut dyn NamedAny, &mut dyn NamedAny) {
                unimplemented!()
            }

            fn get_value(&self, _easing: f64) -> Self::Out {
                unimplemented!()
            }
        }

        impl<T: Clone + Send + Sync + 'static> DynEditableSingleValueMarker for $value<T> {
            type Out = T;

            fn manager(&self) -> &dyn DynEditableSingleValueManager<Self::Out> {
                &$manager(PhantomData)
            }

            fn get_raw_value_mut(&mut self) -> &mut dyn NamedAny {
                unimplemented!()
            }

            fn get_value(&self) -> Self::Out {
                unimplemented!()
            }
        }

        impl<T: Clone + Send + Sync + 'static> DynEditableEasingValueManager<T> for $manager<T> {
            fn identifier(&self) -> DynEditableEasingValueIdentifier {
                DynEditableEasingValueIdentifier {
                    namespace: Cow::Borrowed("mpdelta::test"),
                    name: Cow::Borrowed(concat!(stringify!($value), "::Easing")),
                }
            }

            fn deserialize(&self, deserializer: &mut dyn Deserializer) -> Result<DynEditableEasingValue<T>, Error> {
                let value: $value<T> = erased_serde::deserialize(deserializer)?;
                Ok(DynEditableEasingValue::new(value))
            }
        }

        impl<T: Clone + Send + Sync + 'static> DynEditableSingleValueManager<T> for $manager<T> {
            fn identifier(&self) -> DynEditableSingleValueIdentifier {
                DynEditableSingleValueIdentifier {
                    namespace: Cow::Borrowed("mpdelta::test"),
                    name: Cow::Borrowed(concat!(stringify!($value), "::Single")),
                }
            }

            fn deserialize(&self, deserializer: &mut dyn Deserializer) -> Result<DynEditableSingleValue<T>, Error> {
                let value: $value<T> = erased_serde::deserialize(deserializer)?;
                Ok(DynEditableSingleValue::new(value))
            }
        }
    };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EasingValue1<T>(String, String, PhantomData<T>);

struct EasingValue1Manager<T>(PhantomData<T>);

impl_easing_value!(EasingValue1, EasingValue1Manager);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EasingValue2<T> {
    from: String,
    to: String,
    _phantom: PhantomData<T>,
}

struct EasingValue2Manager<T>(PhantomData<T>);

impl_easing_value!(EasingValue2, EasingValue2Manager);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EasingValue3<T> {
    from: String,
    center: String,
    to: String,
    _phantom: PhantomData<T>,
}

struct EasingValue3Manager<T>(PhantomData<T>);

impl_easing_value!(EasingValue3, EasingValue3Manager);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EasingValue4<T> {
    from: String,
    center: Vec<String>,
    to: String,
    _phantom: PhantomData<T>,
}

struct EasingValue4Manager<T>(PhantomData<T>);

impl_easing_value!(EasingValue4, EasingValue4Manager);

pub fn easing_value<V: Send + Sync + Clone + 'static>() -> impl Strategy<Value = EasingValueForSerialize<V, Ser>> {
    let value = TupleUnion::new((
        (1, Arc::new((any::<String>(), any::<String>()).prop_map(|(a, b)| DynEditableEasingValue::new(EasingValue1(a, b, PhantomData))).prop_map(Wrapper))),
        (1, Arc::new((any::<String>(), any::<String>()).prop_map(|(from, to)| DynEditableEasingValue::new(EasingValue2 { from, to, _phantom: PhantomData })).prop_map(Wrapper))),
        (
            1,
            Arc::new(
                (any::<String>(), any::<String>(), any::<String>())
                    .prop_map(|(from, center, to)| DynEditableEasingValue::new(EasingValue3 { from, center, to, _phantom: PhantomData }))
                    .prop_map(Wrapper),
            ),
        ),
        (
            1,
            Arc::new(
                (any::<String>(), vec(any::<String>(), 0..10), any::<String>())
                    .prop_map(|(from, center, to)| DynEditableEasingValue::new(EasingValue4 { from, center, to, _phantom: PhantomData }))
                    .prop_map(Wrapper),
            ),
        ),
    ));
    (value, any::<EasingIdentifier>()).prop_map(|(value, easing)| EasingValueForSerialize { value, easing })
}

pub fn parameter_nullable_values<T: ParameterValueType>() -> impl Strategy<Value = ParameterNullableValueForSerialize<T, Ser>> {
    TupleUnion::new((
        (1, Arc::new(Just(ParameterNullableValueForSerialize::None))),
        (1, Arc::new(TimeSplitValue::strategy_from(any::<MarkerPinHandleForSerialize>(), of(easing_value()), 1..10).prop_map(ParameterNullableValueForSerialize::Image))),
        (1, Arc::new(TimeSplitValue::strategy_from(any::<MarkerPinHandleForSerialize>(), of(easing_value()), 1..10).prop_map(ParameterNullableValueForSerialize::Audio))),
        (1, Arc::new(TimeSplitValue::strategy_from(any::<MarkerPinHandleForSerialize>(), of(easing_value()), 1..10).prop_map(ParameterNullableValueForSerialize::Binary))),
        (1, Arc::new(TimeSplitValue::strategy_from(any::<MarkerPinHandleForSerialize>(), of(easing_value()), 1..10).prop_map(ParameterNullableValueForSerialize::String))),
        (1, Arc::new(TimeSplitValue::strategy_from(any::<MarkerPinHandleForSerialize>(), of(easing_value()), 1..10).prop_map(ParameterNullableValueForSerialize::Integer))),
        (1, Arc::new(TimeSplitValue::strategy_from(any::<MarkerPinHandleForSerialize>(), of(easing_value()), 1..10).prop_map(ParameterNullableValueForSerialize::RealNumber))),
        (1, Arc::new(TimeSplitValue::strategy_from(any::<MarkerPinHandleForSerialize>(), of(easing_value()), 1..10).prop_map(ParameterNullableValueForSerialize::Boolean))),
    ))
}

pub fn variable_parameters<T: ParameterValueType>() -> impl Strategy<Value = VariableParameterValueForSerialize<ParameterNullableValueForSerialize<T, Ser>>> {
    (parameter_nullable_values(), vec(any::<ComponentInstanceHandleForSerialize>(), 0..10), any::<VariableParameterPriority>()).prop_map(|(params, components, priority)| VariableParameterValueForSerialize { params, components, priority })
}

pub fn component_instance<T: ParameterValueType>() -> impl Strategy<Value = ComponentInstanceForSerialize<T, Ser>> {
    (
        any::<MarkerPinForSerialize>(),
        any::<MarkerPinForSerialize>(),
        vec(any::<MarkerPinForSerialize>(), 0..10),
        of(image_required_params()),
        of(audio_required_params()),
        vec(fixed_parameters::<T>(), 0..10),
        vec(variable_parameters(), 0..10),
        any::<ComponentClassIdentifier>(),
    )
        .prop_map(|(left, right, markers, image_required_params, audio_required_params, fixed_parameters, variable_parameters, class)| ComponentInstanceForSerialize {
            left,
            right,
            markers,
            image_required_params,
            audio_required_params,
            fixed_parameters,
            variable_parameters,
            class,
        })
}

pub fn root_component_class<T: Debug + ParameterValueType>() -> impl Strategy<Value = RootComponentClassForSerialize<T, Ser>> {
    (any::<u128>(), vec(component_instance(), 0..10), vec(any::<MarkerLinkForSerialize>(), 0..10), any::<MarkerTime>()).prop_map(|(id, components, links, length)| RootComponentClassForSerialize { id: Uuid::from_u128(id), components, links, length })
}

pub fn project<T: Debug + ParameterValueType>() -> impl Strategy<Value = ProjectForSerialize<T, Ser>> {
    (any::<u128>(), vec(root_component_class(), 0..10)).prop_map(|(id, components)| ProjectForSerialize { id: Uuid::from_u128(id), components })
}
