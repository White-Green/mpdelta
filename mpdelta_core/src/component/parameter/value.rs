use cgmath::Quaternion;
use erased_serde::Error;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize, Serializer};
use std::any::Any;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::sync::Arc;
use std::{any, fmt, ptr};
use thiserror::Error;

#[derive(Debug, Clone, Copy)]
pub struct EasingInput(f64);

impl EasingInput {
    pub fn new(value: f64) -> EasingInput {
        #[cfg(debug_assertions)]
        if !(0.0..=1.0).contains(&value) {
            eprintln!("[{}:{}] {value} is not in [0.0, 1.0] // FIXME: このメッセージはdebug_assertもしくはまともなログ出力にしようと思ってるよ", file!(), line!());
        }
        EasingInput(if value.is_nan() { 0.0 } else { value.clamp(0.0, 1.0) })
    }

    pub fn value(&self) -> f64 {
        let value = self.0;
        assert!((0.0..=1.0).contains(&value));
        value
    }
}

impl PartialEq for EasingInput {
    fn eq(&self, other: &Self) -> bool {
        self.value() == other.value()
    }
}

impl Eq for EasingInput {}

impl PartialOrd for EasingInput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EasingInput {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value().partial_cmp(&other.value()).unwrap()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EasingIdentifier<'a> {
    #[serde(rename = "ns")]
    pub namespace: Cow<'a, str>,
    #[serde(rename = "n")]
    pub name: Cow<'a, str>,
}

#[cfg(any(feature = "proptest", test))]
const _: () = {
    use proptest::arbitrary::StrategyFor;
    use proptest::prelude::*;
    impl<'a> Arbitrary for EasingIdentifier<'a> {
        type Parameters = (<String as Arbitrary>::Parameters, <String as Arbitrary>::Parameters);
        type Strategy = proptest::strategy::Map<(StrategyFor<String>, StrategyFor<String>), fn((String, String)) -> EasingIdentifier<'a>>;
        fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
            let (namespace, name) = args;
            (String::arbitrary_with(namespace), String::arbitrary_with(name)).prop_map(|(namespace, name)| EasingIdentifier {
                namespace: Cow::Owned(namespace),
                name: Cow::Owned(name),
            })
        }
    }
};

impl<'a> EasingIdentifier<'a> {
    pub fn into_static(self) -> EasingIdentifier<'static> {
        EasingIdentifier {
            namespace: Cow::Owned(self.namespace.into_owned()),
            name: Cow::Owned(self.name.into_owned()),
        }
    }

    pub fn as_ref(&self) -> EasingIdentifier {
        EasingIdentifier {
            namespace: Cow::Borrowed(&self.namespace),
            name: Cow::Borrowed(&self.name),
        }
    }
}

pub trait Easing: Send + Sync {
    fn identifier(&self) -> EasingIdentifier;
    fn easing(&self, from: EasingInput) -> f64;
}

pub struct LinearEasing;

impl Easing for LinearEasing {
    fn identifier(&self) -> EasingIdentifier {
        EasingIdentifier {
            namespace: Cow::Borrowed("mpdelta"),
            name: Cow::Borrowed("Linear"),
        }
    }
    fn easing(&self, from: EasingInput) -> f64 {
        from.value()
    }
}

pub trait NamedAny: Any {
    fn type_name<'a, 'b: 'static>(&'a self) -> &'b str {
        any::type_name::<Self>()
    }
}

impl<T: Any> NamedAny for T {}

fn downcast_mut<T, U>(value: &mut T) -> Option<&mut U>
where
    T: Any + ?Sized,
    U: Any,
{
    if T::type_id(value) == any::TypeId::of::<U>() {
        Some(unsafe { ptr::from_mut(value).cast::<U>().as_mut().unwrap() })
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DynEditableSingleValueIdentifier<'a> {
    #[serde(rename = "ns")]
    pub namespace: Cow<'a, str>,
    #[serde(rename = "n")]
    pub name: Cow<'a, str>,
}

#[cfg(any(feature = "proptest", test))]
const _: () = {
    use proptest::arbitrary::StrategyFor;
    use proptest::prelude::*;
    impl<'a> Arbitrary for DynEditableSingleValueIdentifier<'a> {
        type Parameters = (<String as Arbitrary>::Parameters, <String as Arbitrary>::Parameters);
        type Strategy = proptest::strategy::Map<(StrategyFor<String>, StrategyFor<String>), fn((String, String)) -> DynEditableSingleValueIdentifier<'a>>;
        fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
            let (namespace, name) = args;
            (String::arbitrary_with(namespace), String::arbitrary_with(name)).prop_map(|(namespace, name)| DynEditableSingleValueIdentifier {
                namespace: Cow::Owned(namespace),
                name: Cow::Owned(name),
            })
        }
    }
};

impl<'a> DynEditableSingleValueIdentifier<'a> {
    pub fn into_static(self) -> DynEditableSingleValueIdentifier<'static> {
        DynEditableSingleValueIdentifier {
            namespace: Cow::Owned(self.namespace.into_owned()),
            name: Cow::Owned(self.name.into_owned()),
        }
    }

    pub fn as_ref(&self) -> DynEditableSingleValueIdentifier {
        DynEditableSingleValueIdentifier {
            namespace: Cow::Borrowed(&self.namespace),
            name: Cow::Borrowed(&self.name),
        }
    }
}

pub trait DynEditableSingleValueManager<T>: Send + Sync {
    fn identifier(&self) -> DynEditableSingleValueIdentifier;
    fn deserialize(&self, deserializer: &mut dyn erased_serde::Deserializer) -> Result<DynEditableSingleValue<T>, erased_serde::Error>;
}

pub trait DynEditableSingleValueMarker: NamedAny + erased_serde::Serialize {
    type Out;
    fn manager(&self) -> &dyn DynEditableSingleValueManager<Self::Out>;
    fn get_raw_value_mut(&mut self) -> &mut dyn NamedAny;
    fn get_value(&self) -> Self::Out;
}

#[derive(Debug)]
pub struct DynEditableSelfValueManager<T>(PhantomData<T>);

impl<T> Default for DynEditableSelfValueManager<T> {
    fn default() -> Self {
        DynEditableSelfValueManager(PhantomData)
    }
}

impl<T> DynEditableSingleValueManager<T> for DynEditableSelfValueManager<T>
where
    Self: Send + Sync,
    DynEditableSelfValue<T>: Send + Sync + DynEditableSingleValueMarkerCloneable<Out = T> + DeserializeOwned + 'static,
{
    fn identifier(&self) -> DynEditableSingleValueIdentifier {
        DynEditableSingleValueIdentifier {
            namespace: Cow::Borrowed("mpdelta"),
            name: Cow::Borrowed("SelfValue"),
        }
    }

    fn deserialize(&self, deserializer: &mut dyn erased_serde::Deserializer) -> Result<DynEditableSingleValue<T>, Error> {
        let value: DynEditableSelfValue<T> = erased_serde::deserialize(deserializer)?;
        Ok(DynEditableSingleValue::new(value))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynEditableSelfValue<T>(pub T);

impl<T> DynEditableSingleValueMarker for DynEditableSelfValue<T>
where
    T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
{
    type Out = T;
    fn manager(&self) -> &dyn DynEditableSingleValueManager<Self::Out> {
        &DynEditableSelfValueManager(PhantomData)
    }

    fn get_raw_value_mut(&mut self) -> &mut dyn NamedAny {
        &mut self.0
    }

    fn get_value(&self) -> Self::Out {
        self.0.clone()
    }
}

pub trait DynEditableSingleValueMarkerCloneable: DynEditableSingleValueMarker {
    fn clone_dyn(&self) -> DynEditableSingleValue<Self::Out>;
}

impl<T> DynEditableSingleValueMarkerCloneable for T
where
    T: DynEditableSingleValueMarker + Clone + Send + Sync + 'static,
{
    fn clone_dyn(&self) -> DynEditableSingleValue<T::Out> {
        DynEditableSingleValue(Box::new(self.clone()))
    }
}

pub struct DynEditableSingleValue<T>(Box<dyn DynEditableSingleValueMarkerCloneable<Out = T> + Send + Sync + 'static>);

impl<T: 'static> Debug for DynEditableSingleValue<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "DynEditableSingleValue({})", <dyn DynEditableSingleValueMarkerCloneable<Out = T> as NamedAny>::type_name(&*self.0))
    }
}

impl<T> DynEditableSingleValue<T> {
    pub fn new(value: impl DynEditableSingleValueMarkerCloneable<Out = T> + Send + Sync + 'static) -> DynEditableSingleValue<T> {
        DynEditableSingleValue(Box::new(value))
    }

    pub fn new_self(value: T) -> DynEditableSingleValue<T>
    where
        T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
    {
        DynEditableSingleValue(Box::new(DynEditableSelfValue(value)))
    }
}

impl<T> Serialize for DynEditableSingleValue<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        erased_serde::serialize(&*self.0, serializer)
    }
}

impl<T: 'static> DynEditableSingleValueMarker for DynEditableSingleValue<T> {
    type Out = T;
    fn manager(&self) -> &dyn DynEditableSingleValueManager<Self::Out> {
        self.0.manager()
    }

    fn get_raw_value_mut(&mut self) -> &mut dyn NamedAny {
        <dyn DynEditableSingleValueMarkerCloneable<Out = T>>::get_raw_value_mut(&mut *self.0)
    }

    fn get_value(&self) -> Self::Out {
        <dyn DynEditableSingleValueMarkerCloneable<Out = T>>::get_value(&*self.0)
    }
}

impl<T: 'static> Clone for DynEditableSingleValue<T> {
    fn clone(&self) -> Self {
        <dyn DynEditableSingleValueMarkerCloneable<Out = T>>::clone_dyn(&*self.0)
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[error("expected {expected}, but got {actual}")]
pub struct DowncastError {
    pub expected: &'static str,
    pub actual: &'static str,
}

pub trait SingleValueEdit {
    fn edit_value<T: 'static, R>(&mut self, f: impl FnOnce(&mut T) -> R) -> Result<R, DowncastError>;
}

impl<V> SingleValueEdit for V
where
    V: DynEditableSingleValueMarker + ?Sized,
{
    fn edit_value<T: 'static, R>(&mut self, f: impl FnOnce(&mut T) -> R) -> Result<R, DowncastError> {
        let raw_value = V::get_raw_value_mut(self);
        downcast_mut::<dyn NamedAny, T>(raw_value).map(f).ok_or_else(|| DowncastError {
            expected: any::type_name::<T>(),
            actual: <dyn NamedAny>::type_name(raw_value),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DynEditableEasingValueIdentifier<'a> {
    #[serde(rename = "ns")]
    pub namespace: Cow<'a, str>,
    #[serde(rename = "n")]
    pub name: Cow<'a, str>,
}

#[cfg(any(feature = "proptest", test))]
const _: () = {
    use proptest::arbitrary::StrategyFor;
    use proptest::prelude::*;
    impl<'a> Arbitrary for DynEditableEasingValueIdentifier<'a> {
        type Parameters = (<String as Arbitrary>::Parameters, <String as Arbitrary>::Parameters);
        type Strategy = proptest::strategy::Map<(StrategyFor<String>, StrategyFor<String>), fn((String, String)) -> DynEditableEasingValueIdentifier<'a>>;
        fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
            let (namespace, name) = args;
            (String::arbitrary_with(namespace), String::arbitrary_with(name)).prop_map(|(namespace, name)| DynEditableEasingValueIdentifier {
                namespace: Cow::Owned(namespace),
                name: Cow::Owned(name),
            })
        }
    }
};

impl<'a> DynEditableEasingValueIdentifier<'a> {
    pub fn into_static(self) -> DynEditableEasingValueIdentifier<'static> {
        DynEditableEasingValueIdentifier {
            namespace: Cow::Owned(self.namespace.into_owned()),
            name: Cow::Owned(self.name.into_owned()),
        }
    }

    pub fn as_ref(&self) -> DynEditableEasingValueIdentifier {
        DynEditableEasingValueIdentifier {
            namespace: Cow::Borrowed(&self.namespace),
            name: Cow::Borrowed(&self.name),
        }
    }
}

pub trait DynEditableEasingValueManager<T>: Send + Sync {
    fn identifier(&self) -> DynEditableEasingValueIdentifier;
    fn deserialize(&self, deserializer: &mut dyn erased_serde::Deserializer) -> Result<DynEditableEasingValue<T>, erased_serde::Error>;
}

pub trait DynEditableEasingValueMarker: NamedAny + erased_serde::Serialize {
    type Out: 'static;
    fn manager(&self) -> &dyn DynEditableEasingValueManager<Self::Out>;
    fn get_raw_value_mut(&mut self) -> &mut dyn NamedAny;
    fn get_value(&self, easing: f64) -> Self::Out;
}

impl<T> DynEditableEasingValueMarker for DynEditableSelfValue<T>
where
    T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
{
    type Out = T;

    fn manager(&self) -> &dyn DynEditableEasingValueManager<Self::Out> {
        &DynEditableSelfValueManager(PhantomData)
    }

    fn get_raw_value_mut(&mut self) -> &mut dyn NamedAny {
        &mut self.0
    }

    fn get_value(&self, _: f64) -> Self::Out {
        self.0.clone()
    }
}

impl<T> DynEditableEasingValueManager<T> for DynEditableSelfValueManager<T>
where
    Self: Send + Sync,
    DynEditableSelfValue<T>: Send + Sync + DynEditableEasingValueMarkerCloneable<Out = T> + DeserializeOwned + 'static,
{
    fn identifier(&self) -> DynEditableEasingValueIdentifier {
        DynEditableEasingValueIdentifier {
            namespace: Cow::Borrowed("mpdelta"),
            name: Cow::Borrowed("SingleEasingValue"),
        }
    }

    fn deserialize(&self, deserializer: &mut dyn erased_serde::Deserializer) -> Result<DynEditableEasingValue<T>, Error> {
        let value: DynEditableSelfValue<T> = erased_serde::deserialize(deserializer)?;
        Ok(DynEditableEasingValue::new(value))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynEditableLerpEasingValue<T>(pub (T, T));

pub trait Lerp {
    fn lerp(&self, other: &Self, easing: f64) -> Self;
}

impl Lerp for f64 {
    fn lerp(&self, other: &Self, easing: f64) -> Self {
        self + (other - self) * easing
    }
}

impl Lerp for Quaternion<f64> {
    fn lerp(&self, other: &Self, easing: f64) -> Self {
        self.slerp(*other, easing)
    }
}

#[derive(Debug)]
pub struct DynEditableLerpEasingValueManager<T>(PhantomData<T>);

impl<T> Default for DynEditableLerpEasingValueManager<T> {
    fn default() -> Self {
        DynEditableLerpEasingValueManager(PhantomData)
    }
}

impl<T> DynEditableEasingValueManager<T> for DynEditableLerpEasingValueManager<T>
where
    Self: Send + Sync,
    DynEditableLerpEasingValue<T>: Send + Sync + DynEditableEasingValueMarkerCloneable<Out = T> + DeserializeOwned + 'static,
{
    fn identifier(&self) -> DynEditableEasingValueIdentifier {
        DynEditableEasingValueIdentifier {
            namespace: Cow::Borrowed("mpdelta"),
            name: Cow::Borrowed("LerpEasingValue"),
        }
    }

    fn deserialize(&self, deserializer: &mut dyn erased_serde::Deserializer) -> Result<DynEditableEasingValue<T>, erased_serde::Error> {
        let value: DynEditableLerpEasingValue<T> = erased_serde::deserialize(deserializer)?;
        Ok(DynEditableEasingValue::new(value))
    }
}

impl<T> DynEditableEasingValueMarker for DynEditableLerpEasingValue<T>
where
    T: Clone + Send + Sync + Lerp + Serialize + DeserializeOwned + 'static,
{
    type Out = T;
    fn manager(&self) -> &dyn DynEditableEasingValueManager<Self::Out> {
        &DynEditableLerpEasingValueManager(PhantomData)
    }

    fn get_raw_value_mut(&mut self) -> &mut dyn NamedAny {
        &mut self.0
    }

    fn get_value(&self, easing: f64) -> Self::Out {
        let DynEditableLerpEasingValue((left, right)) = self;
        left.lerp(right, easing)
    }
}

pub trait DynEditableEasingValueMarkerCloneable: DynEditableEasingValueMarker {
    fn clone_dyn(&self) -> DynEditableEasingValue<Self::Out>;
}

impl<T> DynEditableEasingValueMarkerCloneable for T
where
    T: DynEditableEasingValueMarker + Clone + Send + Sync + 'static,
{
    fn clone_dyn(&self) -> DynEditableEasingValue<T::Out> {
        DynEditableEasingValue(Box::new(self.clone()))
    }
}

pub struct DynEditableEasingValue<T>(Box<dyn DynEditableEasingValueMarkerCloneable<Out = T> + Send + Sync + 'static>);

impl<T: 'static> Debug for DynEditableEasingValue<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "DynEditableEasingValue({})", <dyn DynEditableEasingValueMarkerCloneable<Out = T> as NamedAny>::type_name(&*self.0))
    }
}

impl<T> DynEditableEasingValue<T> {
    pub fn new(value: impl DynEditableEasingValueMarkerCloneable<Out = T> + Send + Sync + 'static) -> DynEditableEasingValue<T> {
        DynEditableEasingValue(Box::new(value))
    }
}

impl<T> Serialize for DynEditableEasingValue<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        erased_serde::serialize(&*self.0, serializer)
    }
}

impl<T: 'static> DynEditableEasingValueMarker for DynEditableEasingValue<T> {
    type Out = T;
    fn manager(&self) -> &dyn DynEditableEasingValueManager<Self::Out> {
        self.0.manager()
    }
    fn get_raw_value_mut(&mut self) -> &mut dyn NamedAny {
        <dyn DynEditableEasingValueMarkerCloneable<Out = T>>::get_raw_value_mut(&mut *self.0)
    }

    fn get_value(&self, easing: f64) -> Self::Out {
        <dyn DynEditableEasingValueMarkerCloneable<Out = T>>::get_value(&*self.0, easing)
    }
}

impl<T: 'static> Clone for DynEditableEasingValue<T> {
    fn clone(&self) -> Self {
        <dyn DynEditableEasingValueMarkerCloneable<Out = T>>::clone_dyn(&*self.0)
    }
}

pub trait EasingValueEdit {
    fn edit_value<T: 'static, R>(&mut self, f: impl FnOnce(&mut T) -> R) -> Result<R, DowncastError>;
}

impl<V> EasingValueEdit for V
where
    V: DynEditableEasingValueMarker + ?Sized,
{
    fn edit_value<T: 'static, R>(&mut self, f: impl FnOnce(&mut T) -> R) -> Result<R, DowncastError> {
        let raw = V::get_raw_value_mut(self);
        match downcast_mut::<dyn NamedAny, T>(raw) {
            Some(value) => Ok(f(value)),
            None => Err(DowncastError {
                expected: any::type_name::<T>(),
                actual: <dyn NamedAny>::type_name(raw),
            }),
        }
    }
}

pub struct EasingValue<Value> {
    pub value: DynEditableEasingValue<Value>,
    pub easing: Arc<dyn Easing>,
}

impl<T: Clone + 'static> Clone for EasingValue<T> {
    fn clone(&self) -> Self {
        let EasingValue { value, easing } = self;
        EasingValue { value: value.clone(), easing: easing.clone() }
    }
}

impl<T: 'static> EasingValue<T> {
    pub fn new(value: impl DynEditableEasingValueMarkerCloneable<Out = T> + Send + Sync + 'static, easing: Arc<dyn Easing>) -> EasingValue<T> {
        EasingValue { value: DynEditableEasingValue::new(value), easing }
    }

    pub fn get_value(&self, easing: f64) -> T {
        self.value.get_value(self.easing.easing(EasingInput::new(easing)))
    }
}

impl<Value: Debug> Debug for EasingValue<Value> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct(format!("EasingValue<{}>", any::type_name::<Value>()).as_str()).finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_named_any() {
        let value = 0;
        assert_eq!(value.type_name(), any::type_name::<i32>());
        let value = &value as &dyn NamedAny;
        assert_eq!(value.type_name(), any::type_name::<i32>());
        assert_eq!(<dyn NamedAny>::type_name(value), any::type_name::<i32>());
    }

    #[test]
    fn test_downcast_mut() {
        let mut value = 10;
        assert_eq!(downcast_mut::<i32, i32>(&mut value), Some(&mut 10));
        assert_eq!(downcast_mut::<i32, i64>(&mut value), None);
        let value = &mut value as &mut dyn NamedAny;
        assert_eq!(downcast_mut::<dyn NamedAny, i32>(value), Some(&mut 10));
        assert_eq!(downcast_mut::<dyn NamedAny, i64>(value), None);
    }

    #[test]
    fn test_single_value() {
        #[derive(Clone, Serialize)]
        struct Value(u32);
        impl DynEditableSingleValueMarker for Value {
            type Out = u64;
            fn manager(&self) -> &dyn DynEditableSingleValueManager<Self::Out> {
                todo!()
            }

            fn get_raw_value_mut(&mut self) -> &mut dyn NamedAny {
                &mut self.0
            }

            fn get_value(&self) -> u64 {
                <u64 as From<u32>>::from(self.0)
            }
        }

        let value = Value(10u32);
        assert_eq!(DynEditableSingleValueMarker::get_value(&value), 10u64);
        let mut value = DynEditableSingleValueMarkerCloneable::clone_dyn(&value);
        assert_eq!(DynEditableSingleValueMarker::get_value(&value), 10u64);
        assert_eq!(
            SingleValueEdit::edit_value::<u32, _>(&mut value, |value| {
                *value = 20;
                *value
            }),
            Ok(20)
        );
        assert_eq!(DynEditableSingleValueMarker::get_value(&value), 20u64);
        assert_eq!(
            SingleValueEdit::edit_value::<u64, _>(&mut value, |value| {
                *value = 30;
                *value
            }),
            Err(DowncastError {
                expected: any::type_name::<u64>(),
                actual: any::type_name::<u32>(),
            })
        );
        assert_eq!(DynEditableSingleValueMarker::get_value(&value), 20u64);
    }

    #[test]
    fn test_easing_value() {
        #[derive(Clone, Serialize)]
        struct Value1(u32, u32);
        impl DynEditableEasingValueMarker for Value1 {
            type Out = u64;
            fn manager(&self) -> &dyn DynEditableEasingValueManager<Self::Out> {
                todo!()
            }
            fn get_raw_value_mut(&mut self) -> &mut dyn NamedAny {
                self
            }

            fn get_value(&self, easing: f64) -> u64 {
                let Value1(left, right) = *self;
                (left as f64 * (1.0 - easing) + right as f64 * easing).round() as u64
            }
        }

        let value = Value1(10u32, 20u32);
        assert_eq!(DynEditableEasingValueMarker::get_value(&value, 0.0), 10);
        assert_eq!(DynEditableEasingValueMarker::get_value(&value, 0.5), 15);
        assert_eq!(DynEditableEasingValueMarker::get_value(&value, 1.0), 20);
        let mut value = DynEditableEasingValueMarkerCloneable::clone_dyn(&value);
        assert_eq!(DynEditableEasingValueMarker::get_value(&value, 0.0), 10);
        assert_eq!(DynEditableEasingValueMarker::get_value(&value, 0.5), 15);
        assert_eq!(DynEditableEasingValueMarker::get_value(&value, 1.0), 20);
        assert_eq!(
            EasingValueEdit::edit_value::<Value1, _>(&mut value, |Value1(value1, value2)| {
                *value1 = 20;
                *value2 = 30;
                (*value1, *value2)
            }),
            Ok((20, 30))
        );
        assert_eq!(DynEditableEasingValueMarker::get_value(&value, 0.0), 20);
        assert_eq!(DynEditableEasingValueMarker::get_value(&value, 0.5), 25);
        assert_eq!(DynEditableEasingValueMarker::get_value(&value, 1.0), 30);
        assert_eq!(
            EasingValueEdit::edit_value::<u64, _>(&mut value, |value1| {
                *value1 = 30;
                *value1
            }),
            Err(DowncastError {
                expected: any::type_name::<u64>(),
                actual: any::type_name::<Value1>(),
            })
        );
        assert_eq!(DynEditableEasingValueMarker::get_value(&value, 0.0), 20);
        assert_eq!(DynEditableEasingValueMarker::get_value(&value, 0.5), 25);
        assert_eq!(DynEditableEasingValueMarker::get_value(&value, 1.0), 30);
    }
}
