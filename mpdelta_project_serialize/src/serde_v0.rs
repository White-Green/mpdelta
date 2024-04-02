use crate::{DeserializeError, SerializeError};
use async_trait::async_trait;
use cgmath::{Quaternion, Vector3};
use futures::{stream, StreamExt, TryStreamExt};
use mpdelta_core::common::time_split_value::TimeSplitValue;
use mpdelta_core::component::class::{ComponentClass, ComponentClassIdentifier};
use mpdelta_core::component::instance::{ComponentInstance, ComponentInstanceHandle};
use mpdelta_core::component::link::MarkerLink;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerPinHandle, MarkerPinHandleOwned, MarkerTime};
use mpdelta_core::component::parameter::value::{DynEditableEasingValue, DynEditableEasingValueIdentifier, DynEditableEasingValueMarker, DynEditableSingleValue, DynEditableSingleValueIdentifier, DynEditableSingleValueMarker, EasingIdentifier, EasingValue};
use mpdelta_core::component::parameter::{
    AbstractFile, AudioRequiredParams, BlendMode, CompositeOperation, ImageRequiredParams, ImageRequiredParamsTransform, Never, Parameter, ParameterAllValues, ParameterNullableValue, ParameterValueFixed, ParameterValueRaw, ParameterValueType, ValueRaw, VariableParameterPriority,
    VariableParameterValue, Vector3Params,
};
use mpdelta_core::component::processor::ComponentProcessor;
use mpdelta_core::core::{ComponentClassLoader, EasingLoader, ValueManagerLoader};
use mpdelta_core::project::{Project, ProjectHandleOwned, RootComponentClass, RootComponentClassHandle, RootComponentClassHandleOwned};
use mpdelta_core::ptr::{StaticPointer, StaticPointerCow, StaticPointerOwned};
use mpdelta_core::time::TimelineTime;
use qcell::{TCell, TCellOwner};
use rayon::prelude::{IndexedParallelIterator, IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use serde::de::DeserializeOwned;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;
use std::{convert, future};
use tokio::runtime::Handle;
use tokio::sync::{OwnedRwLockReadGuard, RwLock};
use uuid::Uuid;

#[cfg(test)]
pub mod proptest;

pub const FORMAT_VERSION: u32 = 0;

pub trait TSerDe {
    type Ser: Serialize + Debug + Clone + PartialEq + Send + Sync + 'static;
    type De: DeserializeOwned + Debug + Clone + PartialEq + Send + Sync + 'static;
}

pub trait SerDeSelect: Debug + Clone + PartialEq + Send + Sync + 'static {
    type T<T: TSerDe>: Debug + Clone + PartialEq + Send + Sync + 'static;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ser;

#[derive(Debug, Clone, PartialEq)]
pub struct De;

impl SerDeSelect for Ser {
    type T<T: TSerDe> = T::Ser;
}

impl SerDeSelect for De {
    type T<T: TSerDe> = T::De;
}

#[derive(Debug, Clone)]
pub struct Wrapper<T>(pub T);

impl<T> PartialEq for Wrapper<T> {
    fn eq(&self, _other: &Self) -> bool {
        panic!("Wrapper should not be compared");
    }
}

impl<T: 'static> Serialize for Wrapper<DynEditableSingleValue<T>> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut serializer = serializer.serialize_map(Some(2))?;
        serializer.serialize_entry("t", &self.0.manager().identifier())?;
        serializer.serialize_entry("v", &self.0)?;
        serializer.end()
    }
}

impl<T: 'static> Serialize for Wrapper<DynEditableEasingValue<T>> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut serializer = serializer.serialize_map(Some(2))?;
        serializer.serialize_entry("t", &self.0.manager().identifier())?;
        serializer.serialize_entry("v", &self.0)?;
        serializer.end()
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UnDeserialized<Tag> {
    #[serde(rename = "t")]
    pub tag: Tag,
    #[serde(rename = "v")]
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum PinIndex {
    #[serde(rename = "l")]
    Left,
    #[serde(rename = "r")]
    Right,
    #[serde(rename = "m")]
    Marker(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct MarkerPinHandleForSerialize {
    #[serde(rename = "c")]
    pub component: Option<usize>,
    #[serde(rename = "i")]
    pub index: PinIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct ComponentInstanceHandleForSerialize {
    #[serde(rename = "c")]
    pub component: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct MarkerPinForSerialize(pub Option<MarkerTime>);

impl MarkerPinForSerialize {
    fn as_core<K>(&self) -> MarkerPinHandleOwned<K> {
        match self.0 {
            Some(locked) => StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::ZERO, locked))),
            None => StaticPointerOwned::new(TCell::new(MarkerPin::new_unlocked(TimelineTime::ZERO))),
        }
    }
}

pub type PinSplitValueForSerialize<T> = TimeSplitValue<MarkerPinHandleForSerialize, T>;
pub type Vector3ParamsForSerialize<S> = Vector3<VariableParameterValueForSerialize<PinSplitValueForSerialize<Option<EasingValueForSerialize<f64, S>>>>>;

fn serialize_vector3_params<K, T: ParameterValueType>(value: &Vector3Params<K, T>, pin_map: &HashMap<MarkerPinHandle<K>, MarkerPinHandleForSerialize>, component_map: &HashMap<ComponentInstanceHandle<K, T>, ComponentInstanceHandleForSerialize>) -> Vector3ParamsForSerialize<Ser> {
    let Vector3 { x, y, z } = value;
    Vector3 { x, y, z }.map(|value| {
        let &VariableParameterValue { ref params, ref components, priority } = value;
        VariableParameterValueForSerialize {
            params: params.map_time_value_ref(|pin| pin_map[pin], |value| value.as_ref().map(EasingValueForSerialize::from)),
            components: components.iter().map(|c| component_map[c]).collect(),
            priority,
        }
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct VariableParameterValueForSerialize<Nullable> {
    #[serde(rename = "v")]
    pub params: Nullable,
    #[serde(rename = "c")]
    pub components: Vec<ComponentInstanceHandleForSerialize>,
    #[serde(rename = "p")]
    pub priority: VariableParameterPriority,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "Vector3ParamsForSerialize<S>: Serialize, TimeSplitValue<MarkerPinHandleForSerialize, EasingValueForSerialize<Quaternion<f64>, S>>: Serialize",
    deserialize = "Vector3ParamsForSerialize<S>: Deserialize<'de>, TimeSplitValue<MarkerPinHandleForSerialize, EasingValueForSerialize<Quaternion<f64>, S>>: Deserialize<'de>"
))]
pub enum ImageRequiredParamsTransformForSerialize<S: SerDeSelect> {
    #[serde(rename = "p")]
    Params {
        #[serde(rename = "z")]
        size: Box<Vector3ParamsForSerialize<S>>,
        #[serde(rename = "s")]
        scale: Box<Vector3ParamsForSerialize<S>>,
        #[serde(rename = "t")]
        translate: Box<Vector3ParamsForSerialize<S>>,
        #[serde(rename = "r")]
        rotate: Box<TimeSplitValue<MarkerPinHandleForSerialize, EasingValueForSerialize<Quaternion<f64>, S>>>,
        #[serde(rename = "sc")]
        scale_center: Box<Vector3ParamsForSerialize<S>>,
        #[serde(rename = "rc")]
        rotate_center: Box<Vector3ParamsForSerialize<S>>,
    },
    #[serde(rename = "f")]
    Free {
        #[serde(rename = "lt")]
        left_top: Box<Vector3ParamsForSerialize<S>>,
        #[serde(rename = "rt")]
        right_top: Box<Vector3ParamsForSerialize<S>>,
        #[serde(rename = "lb")]
        left_bottom: Box<Vector3ParamsForSerialize<S>>,
        #[serde(rename = "rb")]
        right_bottom: Box<Vector3ParamsForSerialize<S>>,
    },
}

#[derive(Debug, PartialEq)]
pub struct NullableValueForSerialize<T, S>(PhantomData<(T, S)>);

impl<T: ParameterValueType, S: SerDeSelect> ParameterValueType for NullableValueForSerialize<T, S> {
    type Image = PinSplitValueForSerialize<Option<EasingValueForSerialize<T::Image, S>>>;
    type Audio = PinSplitValueForSerialize<Option<EasingValueForSerialize<T::Audio, S>>>;
    type Binary = PinSplitValueForSerialize<Option<EasingValueForSerialize<AbstractFile, S>>>;
    type String = PinSplitValueForSerialize<Option<EasingValueForSerialize<String, S>>>;
    type Integer = PinSplitValueForSerialize<Option<EasingValueForSerialize<i64, S>>>;
    type RealNumber = PinSplitValueForSerialize<Option<EasingValueForSerialize<f64, S>>>;
    type Boolean = PinSplitValueForSerialize<Option<EasingValueForSerialize<bool, S>>>;
    type Dictionary = Never;
    type Array = Never;
    type ComponentClass = Option<()>;
}

pub type ParameterNullableValueForSerialize<T, S> = Parameter<NullableValueForSerialize<T, S>>;

fn nullable_value_for_serialize<K: 'static, T: ParameterValueType>(value: &ParameterNullableValue<K, T>, pin_map: &HashMap<MarkerPinHandle<K>, MarkerPinHandleForSerialize>) -> ParameterNullableValueForSerialize<T, Ser> {
    match value {
        ParameterNullableValue::None => ParameterNullableValueForSerialize::None,
        ParameterNullableValue::Image(value) => ParameterNullableValueForSerialize::Image(value.map_time_value_ref(|pin| pin_map[pin], |value| value.as_ref().map(EasingValueForSerialize::from))),
        ParameterNullableValue::Audio(value) => ParameterNullableValueForSerialize::Audio(value.map_time_value_ref(|pin| pin_map[pin], |value| value.as_ref().map(EasingValueForSerialize::from))),
        ParameterNullableValue::Binary(value) => ParameterNullableValueForSerialize::Binary(value.map_time_value_ref(|pin| pin_map[pin], |value| value.as_ref().map(EasingValueForSerialize::from))),
        ParameterNullableValue::String(value) => ParameterNullableValueForSerialize::String(value.map_time_value_ref(|pin| pin_map[pin], |value| value.as_ref().map(EasingValueForSerialize::from))),
        ParameterNullableValue::Integer(value) => ParameterNullableValueForSerialize::Integer(value.map_time_value_ref(|pin| pin_map[pin], |value| value.as_ref().map(EasingValueForSerialize::from))),
        ParameterNullableValue::RealNumber(value) => ParameterNullableValueForSerialize::RealNumber(value.map_time_value_ref(|pin| pin_map[pin], |value| value.as_ref().map(EasingValueForSerialize::from))),
        ParameterNullableValue::Boolean(value) => ParameterNullableValueForSerialize::Boolean(value.map_time_value_ref(|pin| pin_map[pin], |value| value.as_ref().map(EasingValueForSerialize::from))),
        ParameterNullableValue::Dictionary(value) => {
            let _: &Never = value;
            unreachable!()
        }
        ParameterNullableValue::Array(value) => {
            let _: &Never = value;
            unreachable!()
        }
        &ParameterNullableValue::ComponentClass(value) => ParameterNullableValueForSerialize::ComponentClass(value),
    }
}

#[derive(Debug, PartialEq)]
pub struct ValueFixedForSerialize<Image, Audio, S>(PhantomData<(Image, Audio, S)>);

impl<T: 'static> TSerDe for DynEditableSingleValue<T> {
    type Ser = Wrapper<DynEditableSingleValue<T>>;
    type De = UnDeserialized<DynEditableSingleValueIdentifier<'static>>;
}

impl<Image, Audio, S> ParameterValueType for ValueFixedForSerialize<Image, Audio, S>
where
    Image: Send + Sync + Clone + 'static,
    Audio: Send + Sync + Clone + 'static,
    S: SerDeSelect,
{
    type Image = S::T<DynEditableSingleValue<Image>>;
    type Audio = S::T<DynEditableSingleValue<Audio>>;
    type Binary = S::T<DynEditableSingleValue<AbstractFile>>;
    type String = S::T<DynEditableSingleValue<String>>;
    type Integer = S::T<DynEditableSingleValue<i64>>;
    type RealNumber = S::T<DynEditableSingleValue<f64>>;
    type Boolean = S::T<DynEditableSingleValue<bool>>;
    type Dictionary = S::T<DynEditableSingleValue<HashMap<String, ParameterValueRaw<Image, Audio>>>>;
    type Array = S::T<DynEditableSingleValue<Vec<ParameterValueRaw<Image, Audio>>>>;
    type ComponentClass = ();
}

pub type ParameterValueFixedForSerialize<Image, Audio, S> = Parameter<ValueFixedForSerialize<Image, Audio, S>>;

fn value_fixed_for_serialize<Image, Audio>(value: &ParameterValueFixed<Image, Audio>) -> ParameterValueFixedForSerialize<Image, Audio, Ser>
where
    Image: Send + Sync + Clone + 'static,
    Audio: Send + Sync + Clone + 'static,
{
    match value {
        ParameterValueFixed::None => ParameterValueFixedForSerialize::None,
        ParameterValueFixed::Image(value) => ParameterValueFixedForSerialize::Image(Wrapper(value.clone())),
        ParameterValueFixed::Audio(value) => ParameterValueFixedForSerialize::Audio(Wrapper(value.clone())),
        ParameterValueFixed::Binary(value) => ParameterValueFixedForSerialize::Binary(Wrapper(value.clone())),
        ParameterValueFixed::String(value) => ParameterValueFixedForSerialize::String(Wrapper(value.clone())),
        ParameterValueFixed::Integer(value) => ParameterValueFixedForSerialize::Integer(Wrapper(value.clone())),
        ParameterValueFixed::RealNumber(value) => ParameterValueFixedForSerialize::RealNumber(Wrapper(value.clone())),
        ParameterValueFixed::Boolean(value) => ParameterValueFixedForSerialize::Boolean(Wrapper(value.clone())),
        ParameterValueFixed::Dictionary(value) => ParameterValueFixedForSerialize::Dictionary(Wrapper(value.clone())),
        ParameterValueFixed::Array(value) => ParameterValueFixedForSerialize::Array(Wrapper(value.clone())),
        ParameterValueFixed::ComponentClass(_) => ParameterValueFixedForSerialize::ComponentClass(()),
    }
}

impl<T: 'static> TSerDe for DynEditableEasingValue<T> {
    type Ser = Wrapper<DynEditableEasingValue<T>>;
    type De = UnDeserialized<DynEditableEasingValueIdentifier<'static>>;
}

#[derive(Serialize, Deserialize)]
#[serde(bound(serialize = "Value: 'static, S::T<DynEditableEasingValue<Value>>: Serialize", deserialize = "Value: 'static, S::T<DynEditableEasingValue<Value>>: Deserialize<'de>"))]
pub struct EasingValueForSerialize<Value: 'static, S: SerDeSelect> {
    #[serde(rename = "v")]
    pub value: S::T<DynEditableEasingValue<Value>>,
    #[serde(rename = "e")]
    pub easing: EasingIdentifier<'static>,
}

impl<Value: 'static, S: SerDeSelect> PartialEq for EasingValueForSerialize<Value, S> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value && self.easing == other.easing
    }
}

impl<Value, S> Debug for EasingValueForSerialize<Value, S>
where
    Value: 'static,
    S: SerDeSelect,
    S::T<DynEditableEasingValue<Value>>: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EasingValueForSerialize").field("value", &self.value).field("easing", &self.easing).finish()
    }
}

impl<Value, S: SerDeSelect> Clone for EasingValueForSerialize<Value, S> {
    fn clone(&self) -> Self {
        let EasingValueForSerialize { value, easing } = self;
        EasingValueForSerialize { value: value.clone(), easing: easing.clone() }
    }
}

impl<'a, Value> From<&'a EasingValue<Value>> for EasingValueForSerialize<Value, Ser> {
    fn from(value: &'a EasingValue<Value>) -> Self {
        EasingValueForSerialize {
            value: Wrapper(value.value.clone()),
            easing: value.easing.identifier().into_static(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "ImageRequiredParamsTransformForSerialize<S>: Serialize, PinSplitValueForSerialize<EasingValueForSerialize<f64, S>>: Serialize",
    deserialize = "ImageRequiredParamsTransformForSerialize<S>: Deserialize<'de>, PinSplitValueForSerialize<EasingValueForSerialize<f64, S>>: Deserialize<'de>"
))]
pub struct ImageRequiredParamsForSerialize<S: SerDeSelect> {
    #[serde(rename = "t")]
    pub transform: ImageRequiredParamsTransformForSerialize<S>,
    #[serde(rename = "bg")]
    pub background_color: [u8; 4],
    #[serde(rename = "o")]
    pub opacity: PinSplitValueForSerialize<EasingValueForSerialize<f64, S>>,
    #[serde(rename = "b")]
    pub blend_mode: PinSplitValueForSerialize<BlendMode>,
    #[serde(rename = "c")]
    pub composite_operation: PinSplitValueForSerialize<CompositeOperation>,
}

pub type SingleChannelVolumeForSerialize<S> = VariableParameterValueForSerialize<PinSplitValueForSerialize<Option<EasingValueForSerialize<f64, S>>>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(bound(serialize = "Vec<SingleChannelVolumeForSerialize<S>>: Serialize", deserialize = "Vec<SingleChannelVolumeForSerialize<S>>: Deserialize<'de>"))]
pub struct AudioRequiredParamsForSerialize<S: SerDeSelect> {
    #[serde(rename = "v")]
    pub volume: Vec<SingleChannelVolumeForSerialize<S>>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(bound(
    serialize = "Option<ImageRequiredParamsForSerialize<S>>: Serialize, Option<AudioRequiredParamsForSerialize<S>>: Serialize, Vec<ParameterValueFixedForSerialize<T::Image, T::Audio, S>>: Serialize, Vec<VariableParameterValueForSerialize<ParameterNullableValueForSerialize<T, S>>>: Serialize",
    deserialize = "Option<ImageRequiredParamsForSerialize<S>>: Deserialize<'de>, Option<AudioRequiredParamsForSerialize<S>>: Deserialize<'de>, Vec<ParameterValueFixedForSerialize<T::Image, T::Audio, S>>: Deserialize<'de>, Vec<VariableParameterValueForSerialize<ParameterNullableValueForSerialize<T, S>>>: Deserialize<'de>"
))]
pub struct ComponentInstanceForSerialize<T: ParameterValueType, S: SerDeSelect> {
    #[serde(rename = "l")]
    pub left: MarkerPinForSerialize,
    #[serde(rename = "r")]
    pub right: MarkerPinForSerialize,
    #[serde(rename = "m")]
    pub markers: Vec<MarkerPinForSerialize>,
    #[serde(rename = "i")]
    pub image_required_params: Option<ImageRequiredParamsForSerialize<S>>,
    #[serde(rename = "a")]
    pub audio_required_params: Option<AudioRequiredParamsForSerialize<S>>,
    #[serde(rename = "f")]
    pub fixed_parameters: Vec<ParameterValueFixedForSerialize<T::Image, T::Audio, S>>,
    #[serde(rename = "v")]
    pub variable_parameters: Vec<VariableParameterValueForSerialize<ParameterNullableValueForSerialize<T, S>>>,
    #[serde(rename = "c")]
    pub class: ComponentClassIdentifier<'static>,
}

impl<T: ParameterValueType, S: SerDeSelect> PartialEq for ComponentInstanceForSerialize<T, S> {
    fn eq(&self, other: &Self) -> bool {
        self.left == other.left
            && self.right == other.right
            && self.markers == other.markers
            && self.image_required_params == other.image_required_params
            && self.audio_required_params == other.audio_required_params
            && self.fixed_parameters == other.fixed_parameters
            && self.variable_parameters == other.variable_parameters
            && self.class == other.class
    }
}

impl<T: ParameterValueType, S: SerDeSelect + Debug> Debug for ComponentInstanceForSerialize<T, S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentInstanceForSerialize")
            .field("left", &self.left)
            .field("right", &self.right)
            .field("markers", &self.markers)
            .field("image_required_params", &self.image_required_params)
            .field("audio_required_params", &self.audio_required_params)
            .field("fixed_parameters", &self.fixed_parameters)
            .field("variable_parameters", &self.variable_parameters)
            .field("class", &self.class)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct MarkerLinkForSerialize {
    #[serde(rename = "f")]
    pub from: MarkerPinHandleForSerialize,
    #[serde(rename = "t")]
    pub to: MarkerPinHandleForSerialize,
    #[serde(rename = "l")]
    pub length: TimelineTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(bound(serialize = "Vec<ComponentInstanceForSerialize<T, S>>: Serialize", deserialize = "Vec<ComponentInstanceForSerialize<T, S>>: Deserialize<'de>"))]
pub struct RootComponentClassForSerialize<T: ParameterValueType, S: SerDeSelect> {
    pub id: Uuid,
    #[serde(rename = "c")]
    pub components: Vec<ComponentInstanceForSerialize<T, S>>,
    #[serde(rename = "lk")]
    pub links: Vec<MarkerLinkForSerialize>,
    #[serde(rename = "l")]
    pub length: MarkerTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(bound(serialize = "Vec<RootComponentClassForSerialize<T, S>>: Serialize", deserialize = "Vec<RootComponentClassForSerialize<T, S>>: Deserialize<'de>"))]
pub struct ProjectForSerialize<T: ParameterValueType, S: SerDeSelect> {
    pub id: Uuid,
    #[serde(rename = "c")]
    pub components: Vec<RootComponentClassForSerialize<T, S>>,
}

macro_rules! deserialize_easing_value {
    ($easing_value_loader:expr, $easing_loader:expr, $value:expr, $easing:expr) => {
        EasingValue {
            value: $easing_value_loader
                .easing_value_by_identifier($value.tag.as_ref())
                .await
                .ok_or(DeserializeError::UnknownEasingValue($value.tag))?
                .deserialize(&mut <dyn erased_serde::Deserializer>::erase($value.value))
                .map_err(DeserializeError::ValueDeserializationError)?,
            easing: $easing_loader.easing_by_identifier($easing.as_ref()).await.ok_or(DeserializeError::UnknownEasing($easing))?,
        }
    };
}

macro_rules! deserialize_pin_split_value {
    ($value:expr, $pins_map:expr, $easing_value_loader:expr, $easing_loader:expr) => {
        $value
            .try_map_time_value_async(
                |time| future::ready($pins_map.get(&time).cloned().ok_or(DeserializeError::UnknownPin(time))),
                |value| async move {
                    if let Some(EasingValueForSerialize { value, easing }) = value {
                        Ok(Some(deserialize_easing_value!($easing_value_loader, $easing_loader, value, easing)))
                    } else {
                        Ok(None)
                    }
                },
            )
            .await
    };
}

macro_rules! deserialize_fixed_value {
    ($value:expr, $easing_value_loader:expr) => {{
        $easing_value_loader
            .single_value_by_identifier($value.tag.as_ref())
            .await
            .ok_or(DeserializeError::UnknownSingleValue($value.tag))?
            .deserialize(&mut <dyn erased_serde::Deserializer>::erase($value.value))
    }};
}

impl<T: ParameterValueType> RootComponentClassForSerialize<T, Ser> {
    async fn from_core<K, D>(value: RootComponentClassHandle<K, T>, key: Arc<D>, runtime: Handle) -> Result<RootComponentClassForSerialize<T, Ser>, SerializeError<K, T>>
    where
        D: Deref<Target = TCellOwner<K>> + Send + Sync + 'static,
    {
        let Some(value) = value.upgrade() else {
            return Err(SerializeError::InvalidRootComponentClassHandle(value));
        };
        let value = value.read().await;
        let component_id = value.id();
        let value = value.get_owned().await;
        let class_identifiers = stream::iter(value.component().iter())
            .then(|component| async {
                let class = component.ro(&key).component_class();
                let Some(class) = class.upgrade() else {
                    return Err(SerializeError::InvalidComponentClassHandle(class.clone()));
                };
                let class = class.read().await;
                Ok(class.identifier().into_static())
            })
            .try_collect::<Vec<_>>()
            .await?;
        runtime
            .spawn_blocking(move || {
                let component_map = value
                    .component()
                    .par_iter()
                    .map(StaticPointerOwned::reference)
                    .cloned()
                    .enumerate()
                    .map(|(component, c)| (c, ComponentInstanceHandleForSerialize { component }))
                    .collect::<HashMap<_, _>>();
                let pin_map = value
                    .component()
                    .par_iter()
                    .enumerate()
                    .flat_map(|(component_index, c)| {
                        let c = c.ro(&key);
                        [(c.marker_left().clone(), PinIndex::Left), (c.marker_right().clone(), PinIndex::Right)]
                            .into_par_iter()
                            .chain(c.markers().par_iter().enumerate().map(|(i, m)| (StaticPointerOwned::reference(m).clone(), PinIndex::Marker(i))))
                            .map(move |(p, i)| (p, MarkerPinHandleForSerialize { component: Some(component_index), index: i }))
                    })
                    .chain([
                        (StaticPointerOwned::reference(value.left()).clone(), MarkerPinHandleForSerialize { component: None, index: PinIndex::Left }),
                        (StaticPointerOwned::reference(value.right()).clone(), MarkerPinHandleForSerialize { component: None, index: PinIndex::Right }),
                    ])
                    .collect::<HashMap<_, _>>();
                let (components, links) = rayon::join(
                    || {
                        value
                            .component()
                            .par_iter()
                            .zip(class_identifiers)
                            .map(|(component, class_identifier)| {
                                let component = component.ro(&key);
                                let left = component.marker_left();
                                let right = component.marker_right();
                                let Some(left) = left.upgrade() else {
                                    return Err(SerializeError::InvalidMarkerPinHandle(left.clone()));
                                };
                                let Some(right) = right.upgrade() else {
                                    return Err(SerializeError::InvalidMarkerPinHandle(right.clone()));
                                };
                                let mut markers = Vec::new();
                                component.markers().par_iter().map(|p| MarkerPinForSerialize(p.ro(&key).locked_component_time())).collect_into_vec(&mut markers);
                                let image_required_params = component.image_required_params().map(|image_required_params| {
                                    let &ImageRequiredParams {
                                        ref transform,
                                        background_color,
                                        ref opacity,
                                        ref blend_mode,
                                        ref composite_operation,
                                    } = image_required_params;
                                    let transform = match transform {
                                        ImageRequiredParamsTransform::Params {
                                            size,
                                            scale,
                                            translate,
                                            rotate,
                                            scale_center,
                                            rotate_center,
                                        } => ImageRequiredParamsTransformForSerialize::Params {
                                            size: Box::new(serialize_vector3_params(size, &pin_map, &component_map)),
                                            scale: Box::new(serialize_vector3_params(scale, &pin_map, &component_map)),
                                            translate: Box::new(serialize_vector3_params(translate, &pin_map, &component_map)),
                                            rotate: Box::new(rotate.map_time_value_ref(|pin| pin_map[pin], EasingValueForSerialize::from)),
                                            scale_center: Box::new(serialize_vector3_params(scale_center, &pin_map, &component_map)),
                                            rotate_center: Box::new(serialize_vector3_params(rotate_center, &pin_map, &component_map)),
                                        },
                                        ImageRequiredParamsTransform::Free { left_top, right_top, left_bottom, right_bottom } => ImageRequiredParamsTransformForSerialize::Free {
                                            left_top: Box::new(serialize_vector3_params(left_top, &pin_map, &component_map)),
                                            right_top: Box::new(serialize_vector3_params(right_top, &pin_map, &component_map)),
                                            left_bottom: Box::new(serialize_vector3_params(left_bottom, &pin_map, &component_map)),
                                            right_bottom: Box::new(serialize_vector3_params(right_bottom, &pin_map, &component_map)),
                                        },
                                    };

                                    ImageRequiredParamsForSerialize {
                                        transform,
                                        background_color,
                                        opacity: opacity.map_time_value_ref(|pin| pin_map[pin], EasingValueForSerialize::from),
                                        blend_mode: blend_mode.map_time_value_ref(|pin| pin_map[pin], BlendMode::clone),
                                        composite_operation: composite_operation.map_time_value_ref(|pin| pin_map[pin], CompositeOperation::clone),
                                    }
                                });
                                let audio_required_params = component.audio_required_params().map(|audio_required_params| {
                                    let AudioRequiredParams { volume } = audio_required_params;
                                    let volume = volume
                                        .iter()
                                        .map(|value| {
                                            let &VariableParameterValue { ref params, ref components, priority } = value;
                                            VariableParameterValueForSerialize {
                                                params: params.map_time_value_ref(|pin| pin_map[pin], |value| value.as_ref().map(EasingValueForSerialize::from)),
                                                components: components.iter().map(|c| component_map[c]).collect(),
                                                priority,
                                            }
                                        })
                                        .collect();
                                    AudioRequiredParamsForSerialize { volume }
                                });
                                let fixed_parameters = component.fixed_parameters().iter().map(value_fixed_for_serialize).collect::<Vec<_>>();
                                let variable_parameters = component
                                    .variable_parameters()
                                    .iter()
                                    .map(|value| {
                                        let &VariableParameterValue { ref params, ref components, priority } = value;
                                        VariableParameterValueForSerialize {
                                            params: nullable_value_for_serialize(params, &pin_map),
                                            components: components.iter().map(|c| component_map[c]).collect(),
                                            priority,
                                        }
                                    })
                                    .collect();
                                Ok(ComponentInstanceForSerialize {
                                    left: MarkerPinForSerialize(left.ro(&key).locked_component_time()),
                                    right: MarkerPinForSerialize(right.ro(&key).locked_component_time()),
                                    markers,
                                    image_required_params,
                                    audio_required_params,
                                    fixed_parameters,
                                    variable_parameters,
                                    class: class_identifier,
                                })
                            })
                            .collect::<Result<_, SerializeError<K, T>>>()
                    },
                    || {
                        let mut links = Vec::new();
                        value
                            .link()
                            .par_iter()
                            .map(|l| MarkerLinkForSerialize {
                                from: pin_map[&l.ro(&key).from],
                                to: pin_map[&l.ro(&key).to],
                                length: l.ro(&key).len,
                            })
                            .collect_into_vec(&mut links);
                        links
                    },
                );
                Ok(RootComponentClassForSerialize {
                    id: component_id,
                    components: components?,
                    links,
                    length: value.length(),
                })
            })
            .await
            .unwrap()
    }
}

impl<T: ParameterValueType> RootComponentClassForSerialize<T, De> {
    async fn into_core<K, C, P, Q, E>(self, class_loader: Arc<ComponentClassLoaderWrapper<K, T, C, P, Q, E>>, slot: OwnedRwLockReadGuard<RootComponentClass<K, T>>, runtime: Handle, key: Arc<RwLock<TCellOwner<K>>>) -> Result<(), DeserializeError<K>>
    where
        C: ComponentClassLoader<K, T> + 'static,
        P: ParameterValueType,
        P::Image: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Image>,
        P::Audio: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Audio>,
        P::Binary: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Binary>,
        P::String: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::String>,
        P::Integer: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Integer>,
        P::RealNumber: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::RealNumber>,
        P::Boolean: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Boolean>,
        P::Dictionary: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Dictionary>,
        P::Array: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Array>,
        P::ComponentClass: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::ComponentClass>,
        Q: ValueManagerLoader<Quaternion<f64>> + 'static,
        E: EasingLoader + 'static,
    {
        let mut slot = slot.get_mut().await;
        let RootComponentClassForSerialize { id: _, components, links, length } = self;
        slot.set_length(length);
        let (all_pins, pins_map) = components.iter().enumerate().fold(
            (
                Vec::with_capacity(components.len()),
                HashMap::from([
                    (MarkerPinHandleForSerialize { component: None, index: PinIndex::Left }, StaticPointerOwned::reference(slot.left()).clone()),
                    (MarkerPinHandleForSerialize { component: None, index: PinIndex::Right }, StaticPointerOwned::reference(slot.right()).clone()),
                ]),
            ),
            |(mut all_pins, mut pins_map), (i, ComponentInstanceForSerialize { left, right, markers, .. })| {
                let left = left.as_core();
                let right = right.as_core();
                let markers = markers.iter().map(MarkerPinForSerialize::as_core).collect::<Vec<_>>();
                let pin_map = [
                    (MarkerPinHandleForSerialize { component: Some(i), index: PinIndex::Left }, StaticPointerOwned::reference(&left).clone()),
                    (MarkerPinHandleForSerialize { component: Some(i), index: PinIndex::Right }, StaticPointerOwned::reference(&right).clone()),
                ]
                .into_iter()
                .chain(markers.iter().enumerate().map(|(j, pin)| (MarkerPinHandleForSerialize { component: Some(i), index: PinIndex::Marker(j) }, StaticPointerOwned::reference(pin).clone())));
                pins_map.extend(pin_map);
                all_pins.push((left, right, markers));
                (all_pins, pins_map)
            },
        );
        let pins_map = Arc::new(pins_map);
        let components_len = components.len();
        let (components, deserialize_remain_params) = stream::iter(components.into_iter().zip(all_pins))
            .map(|(component, (left, right, markers))| {
                let class_loader = Arc::clone(&class_loader);
                let pins_map = Arc::clone(&pins_map);
                runtime.spawn(async move {
                    let class_loader = &class_loader;
                    let pins_map = &pins_map;
                    let ComponentInstanceForSerialize {
                        left: _,
                        right: _,
                        markers: _,
                        image_required_params,
                        audio_required_params,
                        fixed_parameters,
                        variable_parameters,
                        class,
                    } = component;
                    let Some(class_ptr) = class_loader.component_class_by_identifier(class.as_ref()).await else {
                        return Err(DeserializeError::UnknownComponentClass(class));
                    };
                    let Some(class_ref) = class_ptr.upgrade() else {
                        return Err(DeserializeError::UnknownComponentClass(class));
                    };
                    let class_ref = class_ref.read().await;
                    let processor = class_ref.processor();
                    drop(class_ref);
                    let fixed_parameter_types = processor.fixed_parameter_types().await.to_vec();
                    let fixed_parameters = stream::iter(fixed_parameters)
                        .then(|value| async move {
                            let result = match value {
                                ParameterValueFixedForSerialize::None => Ok(ParameterValueFixed::None),
                                ParameterValueFixedForSerialize::Image(value) => deserialize_fixed_value!(value, class_loader.value_managers.image).map(ParameterValueFixed::Image),
                                ParameterValueFixedForSerialize::Audio(value) => deserialize_fixed_value!(value, class_loader.value_managers.audio).map(ParameterValueFixed::Audio),
                                ParameterValueFixedForSerialize::Binary(value) => deserialize_fixed_value!(value, class_loader.value_managers.binary).map(ParameterValueFixed::Binary),
                                ParameterValueFixedForSerialize::String(value) => deserialize_fixed_value!(value, class_loader.value_managers.string).map(ParameterValueFixed::String),
                                ParameterValueFixedForSerialize::Integer(value) => deserialize_fixed_value!(value, class_loader.value_managers.integer).map(ParameterValueFixed::Integer),
                                ParameterValueFixedForSerialize::RealNumber(value) => deserialize_fixed_value!(value, class_loader.value_managers.real_number).map(ParameterValueFixed::RealNumber),
                                ParameterValueFixedForSerialize::Boolean(value) => deserialize_fixed_value!(value, class_loader.value_managers.boolean).map(ParameterValueFixed::Boolean),
                                ParameterValueFixedForSerialize::Dictionary(value) => deserialize_fixed_value!(value, class_loader.value_managers.dictionary).map(ParameterValueFixed::Dictionary),
                                ParameterValueFixedForSerialize::Array(value) => deserialize_fixed_value!(value, class_loader.value_managers.array).map(ParameterValueFixed::Array),
                                ParameterValueFixedForSerialize::ComponentClass(()) => Ok(ParameterValueFixed::ComponentClass(())),
                            };
                            Ok::<_, DeserializeError<K>>(result?)
                        })
                        .try_collect::<Vec<_>>()
                        .await?;
                    let mut variable_parameter_types = Vec::new();
                    let raw_parameters = fixed_parameters
                        .iter()
                        .map(|value| match value {
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
                        })
                        .collect::<Vec<_>>();
                    processor.update_variable_parameter(&raw_parameters, &mut variable_parameter_types).await;
                    let variable_parameters = stream::iter(variable_parameters)
                        .then(|VariableParameterValueForSerialize { params, components, priority }| async move {
                            let params = match params {
                                ParameterNullableValueForSerialize::None => Ok(ParameterNullableValue::<K, T>::None),
                                ParameterNullableValueForSerialize::Image(value) => deserialize_pin_split_value!(value, pins_map, class_loader.value_managers.image, class_loader.easing_manager).map(ParameterNullableValue::Image),
                                ParameterNullableValueForSerialize::Audio(value) => deserialize_pin_split_value!(value, pins_map, class_loader.value_managers.audio, class_loader.easing_manager).map(ParameterNullableValue::Audio),
                                ParameterNullableValueForSerialize::Binary(value) => deserialize_pin_split_value!(value, pins_map, class_loader.value_managers.binary, class_loader.easing_manager).map(ParameterNullableValue::Binary),
                                ParameterNullableValueForSerialize::String(value) => deserialize_pin_split_value!(value, pins_map, class_loader.value_managers.string, class_loader.easing_manager).map(ParameterNullableValue::String),
                                ParameterNullableValueForSerialize::Integer(value) => deserialize_pin_split_value!(value, pins_map, class_loader.value_managers.integer, class_loader.easing_manager).map(ParameterNullableValue::Integer),
                                ParameterNullableValueForSerialize::RealNumber(value) => deserialize_pin_split_value!(value, pins_map, class_loader.value_managers.real_number, class_loader.easing_manager).map(ParameterNullableValue::RealNumber),
                                ParameterNullableValueForSerialize::Boolean(value) => deserialize_pin_split_value!(value, pins_map, class_loader.value_managers.boolean, class_loader.easing_manager).map(ParameterNullableValue::Boolean),
                                ParameterNullableValueForSerialize::Dictionary(value) => {
                                    let _: Never = value;
                                    unreachable!()
                                }
                                ParameterNullableValueForSerialize::Array(value) => {
                                    let _: Never = value;
                                    unreachable!()
                                }
                                ParameterNullableValueForSerialize::ComponentClass(value) => Ok(ParameterNullableValue::ComponentClass(value)),
                            };
                            Ok::<_, DeserializeError<K>>((params?, components, priority))
                        })
                        .try_collect::<Vec<_>>()
                        .await?;
                    let image_required_params_slot = image_required_params.as_ref().map(|_| ImageRequiredParams::new_default(StaticPointerOwned::reference(&left), StaticPointerOwned::reference(&right)));
                    let audio_required_params_slot = audio_required_params.as_ref().map(|_| AudioRequiredParams { volume: vec![] });
                    let mut instance = ComponentInstance::builder(class_ptr, StaticPointerCow::Owned(left), StaticPointerCow::Owned(right), markers, processor);
                    if let Some(image_required_params) = image_required_params_slot {
                        instance = instance.image_required_params(image_required_params);
                    }
                    if let Some(audio_required_params) = audio_required_params_slot {
                        instance = instance.audio_required_params(audio_required_params);
                    }
                    let instance = instance.fixed_parameters(fixed_parameter_types.into_boxed_slice(), fixed_parameters.into_boxed_slice()).variable_parameters(variable_parameter_types, Vec::new()).build();
                    let instance_slot = StaticPointerOwned::new(TCell::new(instance));
                    Ok::<_, DeserializeError<K>>((instance_slot, (variable_parameters, image_required_params, audio_required_params)))
                })
            })
            .buffered(16)
            .map(Result::unwrap)
            .try_fold((Vec::with_capacity(components_len), Vec::with_capacity(components_len)), |(mut slot_acc, mut params_acc), (slot, params)| {
                slot_acc.push(slot);
                params_acc.push(params);
                future::ready(Ok((slot_acc, params_acc)))
            })
            .await?;
        let component_instance_map = components
            .iter()
            .enumerate()
            .map(|(i, component)| (ComponentInstanceHandleForSerialize { component: i }, StaticPointerOwned::reference(component).clone()))
            .collect::<HashMap<_, _>>();
        let component_instance_map = Arc::new(component_instance_map);
        let stream = stream::iter(deserialize_remain_params)
            .map(|(variable_parameters, image_required_params, audio_required_params)| {
                let component_instance_map = Arc::clone(&component_instance_map);
                let pins_map = Arc::clone(&pins_map);
                let class_loader = Arc::clone(&class_loader);
                runtime.spawn(async move {
                    let component_instance_map = &component_instance_map;
                    let pins_map = &pins_map;
                    let class_loader = &class_loader;
                    let variable_parameters = variable_parameters
                        .into_iter()
                        .map(|(params, components, priority)| {
                            Ok::<_, DeserializeError<K>>(VariableParameterValue {
                                params,
                                components: components
                                    .into_iter()
                                    .map(|component| component_instance_map.get(&component).cloned().ok_or(DeserializeError::UnknownComponentInstanceHandle(component)))
                                    .collect::<Result<_, _>>()?,
                                priority,
                            })
                        })
                        .collect::<Result<_, _>>()?;
                    let image_required_params = if let Some(image_required_params) = image_required_params {
                        let ImageRequiredParamsForSerialize {
                            transform,
                            background_color,
                            opacity,
                            blend_mode,
                            composite_operation,
                        } = image_required_params;
                        let transform = match transform {
                            ImageRequiredParamsTransformForSerialize::Params {
                                size,
                                scale,
                                translate,
                                rotate,
                                scale_center,
                                rotate_center,
                            } => {
                                let (size, scale, translate, scale_center, rotate_center) = tokio::try_join!(
                                    deserialize_vector3_params(*size, class_loader, component_instance_map, pins_map),
                                    deserialize_vector3_params(*scale, class_loader, component_instance_map, pins_map),
                                    deserialize_vector3_params(*translate, class_loader, component_instance_map, pins_map),
                                    deserialize_vector3_params(*scale_center, class_loader, component_instance_map, pins_map),
                                    deserialize_vector3_params(*rotate_center, class_loader, component_instance_map, pins_map),
                                )?;
                                ImageRequiredParamsTransform::Params {
                                    size,
                                    scale,
                                    translate,
                                    rotate: rotate
                                        .try_map_time_value_async(
                                            |time| future::ready(pins_map.get(&time).cloned().ok_or(DeserializeError::UnknownPin(time))),
                                            |value| async move {
                                                let EasingValueForSerialize { value, easing } = value;
                                                Ok(EasingValue {
                                                    value: class_loader
                                                        .quaternion_manager
                                                        .easing_value_by_identifier(value.tag.as_ref())
                                                        .await
                                                        .ok_or(DeserializeError::UnknownEasingValue(value.tag))?
                                                        .deserialize(&mut <dyn erased_serde::Deserializer>::erase(value.value))
                                                        .map_err(DeserializeError::ValueDeserializationError)?,
                                                    easing: class_loader.easing_manager.easing_by_identifier(easing.as_ref()).await.ok_or(DeserializeError::UnknownEasing(easing))?,
                                                })
                                            },
                                        )
                                        .await?,
                                    scale_center,
                                    rotate_center,
                                }
                            }
                            ImageRequiredParamsTransformForSerialize::Free { left_top, right_top, left_bottom, right_bottom } => {
                                let (left_top, right_top, left_bottom, right_bottom) = tokio::try_join!(
                                    deserialize_vector3_params(*left_top, class_loader, component_instance_map, pins_map),
                                    deserialize_vector3_params(*right_top, class_loader, component_instance_map, pins_map),
                                    deserialize_vector3_params(*left_bottom, class_loader, component_instance_map, pins_map),
                                    deserialize_vector3_params(*right_bottom, class_loader, component_instance_map, pins_map),
                                )?;
                                ImageRequiredParamsTransform::Free { left_top, right_top, left_bottom, right_bottom }
                            }
                        };
                        let opacity = opacity
                            .try_map_time_value_async(
                                |time| future::ready(pins_map.get(&time).cloned().ok_or(DeserializeError::UnknownPin(time))),
                                |value| async move {
                                    let EasingValueForSerialize { value, easing } = value;
                                    Ok(deserialize_easing_value!(class_loader.value_managers.real_number, class_loader.easing_manager, value, easing))
                                },
                            )
                            .await?;
                        let blend_mode = blend_mode.try_map_time_value(|time| pins_map.get(&time).cloned().ok_or(DeserializeError::UnknownPin(time)), Ok)?;
                        let composite_operation = composite_operation.try_map_time_value(|time| pins_map.get(&time).cloned().ok_or(DeserializeError::UnknownPin(time)), Ok)?;
                        let image_required_params = ImageRequiredParams {
                            transform,
                            background_color,
                            opacity,
                            blend_mode,
                            composite_operation,
                        };
                        Some(image_required_params)
                    } else {
                        None
                    };
                    let audio_required_params = if let Some(audio_required_params) = audio_required_params {
                        let AudioRequiredParamsForSerialize { volume } = audio_required_params;
                        let volume = stream::iter(volume)
                            .then(|value| async move {
                                let VariableParameterValueForSerialize { params, components, priority } = value;
                                let params = params
                                    .try_map_time_value_async(
                                        |time| future::ready(pins_map.get(&time).cloned().ok_or(DeserializeError::UnknownPin(time))),
                                        |value| async move {
                                            if let Some(EasingValueForSerialize { value, easing }) = value {
                                                Ok(Some(deserialize_easing_value!(class_loader.value_managers.real_number, class_loader.easing_manager, value, easing)))
                                            } else {
                                                Ok(None)
                                            }
                                        },
                                    )
                                    .await?;
                                Ok::<_, DeserializeError<K>>(VariableParameterValue {
                                    params,
                                    components: components
                                        .into_iter()
                                        .map(|component| component_instance_map.get(&component).cloned().ok_or(DeserializeError::UnknownComponentInstanceHandle(component)))
                                        .collect::<Result<_, _>>()?,
                                    priority,
                                })
                            })
                            .try_collect()
                            .await?;
                        Some(AudioRequiredParams { volume })
                    } else {
                        None
                    };
                    Ok((variable_parameters, image_required_params, audio_required_params))
                })
            })
            .buffered(16)
            .map(Result::unwrap)
            .zip(stream::iter(components.iter()))
            .map(|(result, component)| result.map(|result| (result, component)));
        let mut key = key.write().await;
        stream
            .try_for_each(|((variable_parameters, image_required_params, audio_required_params), component)| {
                let component = component.rw(&mut key);
                *component.variable_parameters_mut() = variable_parameters;
                if let Some(params) = image_required_params {
                    component.set_image_required_params(params);
                }
                if let Some(params) = audio_required_params {
                    component.set_audio_required_params(params);
                }
                future::ready(Ok::<_, DeserializeError<K>>(()))
            })
            .await?;
        drop(key);
        let links = links
            .into_iter()
            .map(|link| {
                let MarkerLinkForSerialize { from, to, length } = link;
                let link = MarkerLink {
                    from: pins_map.get(&from).cloned().ok_or_else(|| DeserializeError::UnknownPin(from))?,
                    to: pins_map.get(&to).cloned().ok_or_else(|| DeserializeError::UnknownPin(to))?,
                    len: length,
                };
                Ok(StaticPointerOwned::new(TCell::new(link)))
            })
            .collect::<Result<_, DeserializeError<K>>>()?;
        *slot.component_mut() = components;
        *slot.link_mut() = links;
        Ok(())
    }
}

async fn deserialize_vector3_params<K, T, C, P, Q, E>(
    params: Vector3ParamsForSerialize<De>,
    class_loader: &ComponentClassLoaderWrapper<K, T, C, P, Q, E>,
    component_instance_map: &HashMap<ComponentInstanceHandleForSerialize, ComponentInstanceHandle<K, T>>,
    pins_map: &HashMap<MarkerPinHandleForSerialize, MarkerPinHandle<K>>,
) -> Result<Vector3Params<K, T>, DeserializeError<K>>
where
    T: ParameterValueType,
    C: ComponentClassLoader<K, T> + 'static,
    P: ParameterValueType,
    P::RealNumber: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::RealNumber>,
    E: EasingLoader + 'static,
{
    let transform = |value: VariableParameterValueForSerialize<PinSplitValueForSerialize<Option<EasingValueForSerialize<f64, De>>>>| async move {
        let VariableParameterValueForSerialize { params, components, priority } = value;
        let params = deserialize_pin_split_value!(params, pins_map, class_loader.value_managers.real_number, class_loader.easing_manager)?;
        let components = components.into_iter().map(|c| component_instance_map.get(&c).cloned().ok_or(DeserializeError::UnknownComponentInstanceHandle(c))).collect::<Result<_, _>>()?;
        Ok::<_, DeserializeError<K>>(VariableParameterValue { params, components, priority })
    };
    let Vector3ParamsForSerialize { x, y, z } = params;
    let (x, y, z) = tokio::try_join!(transform(x), transform(y), transform(z))?;
    Ok(Vector3Params { x, y, z })
}

impl<T: ParameterValueType> ProjectForSerialize<T, Ser> {
    pub async fn from_core<K, D>(project: &Project<K, T>, key: &Arc<D>, runtime: &Handle) -> Result<ProjectForSerialize<T, Ser>, SerializeError<K, T>>
    where
        D: Deref<Target = TCellOwner<K>> + Send + Sync + 'static,
    {
        let id = project.id();
        let components = stream::iter(project.children())
            // convert::identityを入れないとコンパイルが通らない(ライフタイムの推論に失敗してる?) 謎
            .map(convert::identity(|c: &RootComponentClassHandleOwned<K, T>| {
                runtime.spawn(RootComponentClassForSerialize::from_core(StaticPointerOwned::reference(c).clone(), Arc::clone(key), runtime.clone()))
            }))
            .buffered(16)
            .map(Result::unwrap)
            .try_collect()
            .await?;
        Ok(ProjectForSerialize { id, components })
    }
}

impl<T: ParameterValueType> ProjectForSerialize<T, De> {
    pub async fn into_core<K, C, P, Q, E>(self, class: C, runtime: &Handle, value_managers: ParameterAllValues<P>, quaternion_manager: Q, easing_manager: E, key: &Arc<RwLock<TCellOwner<K>>>) -> Result<ProjectHandleOwned<K, T>, DeserializeError<K>>
    where
        K: 'static,
        C: ComponentClassLoader<K, T> + 'static,
        P: ParameterValueType,
        P::Image: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Image>,
        P::Audio: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Audio>,
        P::Binary: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Binary>,
        P::String: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::String>,
        P::Integer: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Integer>,
        P::RealNumber: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::RealNumber>,
        P::Boolean: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Boolean>,
        P::Dictionary: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Dictionary>,
        P::Array: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::Array>,
        P::ComponentClass: ValueManagerLoader<<ValueRaw<T::Image, T::Audio> as ParameterValueType>::ComponentClass>,
        Q: ValueManagerLoader<Quaternion<f64>> + 'static,
        E: EasingLoader + 'static,
    {
        let ProjectForSerialize { id: project_id, components } = self;
        let project = Project::new_empty(project_id);
        let components_slot = components
            .iter()
            .map(|&RootComponentClassForSerialize { id, .. }| RootComponentClass::new_empty(id, StaticPointerOwned::reference(&project).clone(), project_id))
            .collect::<Vec<_>>();
        let mut project_write = project.write().await;
        project_write.add_children(StaticPointerOwned::reference(&project), components_slot).await;
        let components_slot = project_write.children();
        let components_slot_read = stream::iter(components_slot.iter()).then(StaticPointerOwned::read_owned).collect::<Vec<_>>().await;
        let class = ComponentClassLoaderWrapper::new(
            class,
            project_id,
            components_slot_read.iter().zip(components_slot.iter()).map(|(slot, handle_owned)| (&**slot, StaticPointerOwned::reference(handle_owned).clone())),
            value_managers,
            quaternion_manager,
            easing_manager,
        );
        let class = Arc::new(class);
        stream::iter(components.into_iter().zip(components_slot_read.into_iter()))
            .map(|(component, slot)| runtime.spawn(component.into_core(Arc::clone(&class), slot, runtime.clone(), Arc::clone(key))))
            .buffered(16)
            .map(Result::unwrap)
            .try_for_each(|_| future::ready(Ok(())))
            .await?;
        drop(project_write);
        Ok(project)
    }
}

struct ComponentClassLoaderWrapper<K, T, C, P: ParameterValueType, Q, E> {
    inner: C,
    project_id: Uuid,
    classes: HashMap<Uuid, StaticPointer<RwLock<dyn ComponentClass<K, T>>>>,
    value_managers: ParameterAllValues<P>,
    quaternion_manager: Q,
    easing_manager: E,
}

impl<K, T, C, P, Q, E> ComponentClassLoaderWrapper<K, T, C, P, Q, E>
where
    K: 'static,
    T: ParameterValueType,
    C: ComponentClassLoader<K, T>,
    P: ParameterValueType,
    Q: ValueManagerLoader<Quaternion<f64>>,
    E: EasingLoader,
{
    fn new<'a>(inner: C, project_id: Uuid, slot: impl IntoIterator<Item = (&'a RootComponentClass<K, T>, RootComponentClassHandle<K, T>)>, value_managers: ParameterAllValues<P>, quaternion_manager: Q, easing_manager: E) -> ComponentClassLoaderWrapper<K, T, C, P, Q, E> {
        let classes = slot.into_iter().map(|(class, handle)| (class.id(), handle.map(|weak| weak as _))).collect();
        ComponentClassLoaderWrapper {
            inner,
            project_id,
            classes,
            value_managers,
            quaternion_manager,
            easing_manager,
        }
    }
}

#[async_trait]
impl<K, T, C, P, Q, E> ComponentClassLoader<K, T> for ComponentClassLoaderWrapper<K, T, C, P, Q, E>
where
    K: 'static,
    T: ParameterValueType,
    C: ComponentClassLoader<K, T>,
    P: ParameterValueType,
    Self: Send + Sync,
{
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<K, T>>>]> {
        let inner = self.inner.get_available_component_classes().await;
        let mut classes = inner.into_owned();
        classes.extend(self.classes.values().cloned());
        Cow::Owned(classes)
    }

    async fn component_class_by_identifier(&self, identifier: ComponentClassIdentifier<'_>) -> Option<StaticPointer<RwLock<dyn ComponentClass<K, T>>>> {
        if identifier.namespace == "mpdelta" && identifier.name == "RootComponent" && identifier.inner_identifier[0] == self.project_id {
            self.classes.get(&identifier.inner_identifier[1]).cloned()
        } else {
            self.inner.component_class_by_identifier(identifier).await
        }
    }
}
