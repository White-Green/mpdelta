use crate::common::time_split_value::TimeSplitValue;
use crate::component::instance::ComponentInstanceHandle;
use crate::component::marker_pin::MarkerPinHandle;
use crate::component::parameter::placeholder::{Placeholder, TagAudio, TagImage};
use crate::component::parameter::value::{DynEditableLerpEasingValue, DynEditableSingleValue, EasingValue, LinearEasing};
use cgmath::{One, Quaternion, Vector3};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::io::{IoSliceMut, Read, Seek, SeekFrom};
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::Arc;
use std::{io, mem};
use uuid::Uuid;

pub mod placeholder;
pub mod value;

pub trait ParameterValueType: 'static + Send + Sync {
    type Image: 'static + Clone + Send + Sync;
    type Audio: 'static + Clone + Send + Sync;
    type Binary: 'static + Clone + Send + Sync;
    type String: 'static + Clone + Send + Sync;
    type Integer: 'static + Clone + Send + Sync;
    type RealNumber: 'static + Clone + Send + Sync;
    type Boolean: 'static + Clone + Send + Sync;
    type Dictionary: 'static + Clone + Send + Sync;
    type Array: 'static + Clone + Send + Sync;
    type ComponentClass: 'static + Clone + Send + Sync;
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(any(feature = "proptest", test), derive(proptest_derive::Arbitrary))]
pub enum Parameter<Type: ParameterValueType> {
    #[serde(rename = "n")]
    None,
    #[serde(rename = "img")]
    Image(Type::Image),
    #[serde(rename = "aud")]
    Audio(Type::Audio),
    #[serde(rename = "bin")]
    Binary(Type::Binary),
    #[serde(rename = "str")]
    String(Type::String),
    #[serde(rename = "int")]
    Integer(Type::Integer),
    #[serde(rename = "real")]
    RealNumber(Type::RealNumber),
    #[serde(rename = "bool")]
    Boolean(Type::Boolean),
    #[serde(rename = "dict")]
    Dictionary(Type::Dictionary),
    #[serde(rename = "arr")]
    Array(Type::Array),
    #[serde(rename = "cc")]
    ComponentClass(Type::ComponentClass),
}

impl<Type: ParameterValueType> Parameter<Type> {
    pub fn into_none(self) -> Result<(), Self> {
        if let Parameter::None = self {
            Ok(())
        } else {
            Err(self)
        }
    }
    pub fn into_image(self) -> Result<Type::Image, Self> {
        if let Parameter::Image(value) = self {
            Ok(value)
        } else {
            Err(self)
        }
    }
    pub fn into_audio(self) -> Result<Type::Audio, Self> {
        if let Parameter::Audio(value) = self {
            Ok(value)
        } else {
            Err(self)
        }
    }
    pub fn into_binary(self) -> Result<Type::Binary, Self> {
        if let Parameter::Binary(value) = self {
            Ok(value)
        } else {
            Err(self)
        }
    }
    pub fn into_string(self) -> Result<Type::String, Self> {
        if let Parameter::String(value) = self {
            Ok(value)
        } else {
            Err(self)
        }
    }
    pub fn into_integer(self) -> Result<Type::Integer, Self> {
        if let Parameter::Integer(value) = self {
            Ok(value)
        } else {
            Err(self)
        }
    }
    pub fn into_real_number(self) -> Result<Type::RealNumber, Self> {
        if let Parameter::RealNumber(value) = self {
            Ok(value)
        } else {
            Err(self)
        }
    }
    pub fn into_boolean(self) -> Result<Type::Boolean, Self> {
        if let Parameter::Boolean(value) = self {
            Ok(value)
        } else {
            Err(self)
        }
    }
    pub fn into_dictionary(self) -> Result<Type::Dictionary, Self> {
        if let Parameter::Dictionary(value) = self {
            Ok(value)
        } else {
            Err(self)
        }
    }
    pub fn into_array(self) -> Result<Type::Array, Self> {
        if let Parameter::Array(value) = self {
            Ok(value)
        } else {
            Err(self)
        }
    }
    pub fn into_component_class(self) -> Result<Type::ComponentClass, Self> {
        if let Parameter::ComponentClass(value) = self {
            Ok(value)
        } else {
            Err(self)
        }
    }
    pub fn as_none(&self) -> Option<()> {
        if let Parameter::None = self {
            Some(())
        } else {
            None
        }
    }
    pub fn as_image(&self) -> Option<&Type::Image> {
        if let Parameter::Image(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_audio(&self) -> Option<&Type::Audio> {
        if let Parameter::Audio(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_binary(&self) -> Option<&Type::Binary> {
        if let Parameter::Binary(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_string(&self) -> Option<&Type::String> {
        if let Parameter::String(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_integer(&self) -> Option<&Type::Integer> {
        if let Parameter::Integer(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_real_number(&self) -> Option<&Type::RealNumber> {
        if let Parameter::RealNumber(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_boolean(&self) -> Option<&Type::Boolean> {
        if let Parameter::Boolean(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_dictionary(&self) -> Option<&Type::Dictionary> {
        if let Parameter::Dictionary(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_array(&self) -> Option<&Type::Array> {
        if let Parameter::Array(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_component_class(&self) -> Option<&Type::ComponentClass> {
        if let Parameter::ComponentClass(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn equals_type<Type2: ParameterValueType>(&self, other: &Parameter<Type2>) -> bool {
        matches!(
            (self, other),
            (Self::None, Parameter::None)
                | (Self::Image(_), Parameter::Image(_))
                | (Self::Audio(_), Parameter::Audio(_))
                | (Self::Binary(_), Parameter::Binary(_))
                | (Self::String(_), Parameter::String(_))
                | (Self::Integer(_), Parameter::Integer(_))
                | (Self::RealNumber(_), Parameter::RealNumber(_))
                | (Self::Boolean(_), Parameter::Boolean(_))
                | (Self::Dictionary(_), Parameter::Dictionary(_))
                | (Self::Array(_), Parameter::Array(_))
                | (Self::ComponentClass(_), Parameter::ComponentClass(_))
        )
    }
    pub fn select(&self) -> Parameter<ParameterSelect> {
        match self {
            Self::None => Parameter::<ParameterSelect>::None,
            Self::Image(_) => Parameter::<ParameterSelect>::Image(()),
            Self::Audio(_) => Parameter::<ParameterSelect>::Audio(()),
            Self::Binary(_) => Parameter::<ParameterSelect>::Binary(()),
            Self::String(_) => Parameter::<ParameterSelect>::String(()),
            Self::Integer(_) => Parameter::<ParameterSelect>::Integer(()),
            Self::RealNumber(_) => Parameter::<ParameterSelect>::RealNumber(()),
            Self::Boolean(_) => Parameter::<ParameterSelect>::Boolean(()),
            Self::Dictionary(_) => Parameter::<ParameterSelect>::Dictionary(()),
            Self::Array(_) => Parameter::<ParameterSelect>::Array(()),
            Self::ComponentClass(_) => Parameter::<ParameterSelect>::ComponentClass(()),
        }
    }
}

// SAFETY: ParameterValueTypeのassociated typesには全部Send境界を付けているので安全
unsafe impl<Type: ParameterValueType> Send for Parameter<Type> {}

// SAFETY: ParameterValueTypeのassociated typesには全部Sync境界を付けているので安全
unsafe impl<Type: ParameterValueType> Sync for Parameter<Type> {}

impl<Type: ParameterValueType> Clone for Parameter<Type> {
    fn clone(&self) -> Self {
        match self {
            Self::None => Self::None,
            Self::Image(value) => Self::Image(value.clone()),
            Self::Audio(value) => Self::Audio(value.clone()),
            Self::Binary(value) => Self::Binary(value.clone()),
            Self::String(value) => Self::String(value.clone()),
            Self::Integer(value) => Self::Integer(value.clone()),
            Self::RealNumber(value) => Self::RealNumber(value.clone()),
            Self::Boolean(value) => Self::Boolean(value.clone()),
            Self::Dictionary(value) => Self::Dictionary(value.clone()),
            Self::Array(value) => Self::Array(value.clone()),
            Self::ComponentClass(value) => Self::ComponentClass(value.clone()),
        }
    }
}

impl<Type: ParameterValueType> Copy for Parameter<Type>
where
    Type::Image: Copy,
    Type::Audio: Copy,
    Type::Binary: Copy,
    Type::String: Copy,
    Type::Integer: Copy,
    Type::RealNumber: Copy,
    Type::Boolean: Copy,
    Type::Dictionary: Copy,
    Type::Array: Copy,
    Type::ComponentClass: Copy,
{
}

impl<Type: ParameterValueType> PartialEq for Parameter<Type>
where
    Type::Image: PartialEq,
    Type::Audio: PartialEq,
    Type::Binary: PartialEq,
    Type::String: PartialEq,
    Type::Integer: PartialEq,
    Type::RealNumber: PartialEq,
    Type::Boolean: PartialEq,
    Type::Dictionary: PartialEq,
    Type::Array: PartialEq,
    Type::ComponentClass: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        if mem::discriminant(self) != mem::discriminant(other) {
            return false;
        }
        match (self, other) {
            (Self::None, Self::None) => true,
            (Self::Image(value0), Self::Image(value1)) => value0 == value1,
            (Self::Audio(value0), Self::Audio(value1)) => value0 == value1,
            (Self::Binary(value0), Self::Binary(value1)) => value0 == value1,
            (Self::String(value0), Self::String(value1)) => value0 == value1,
            (Self::Integer(value0), Self::Integer(value1)) => value0 == value1,
            (Self::RealNumber(value0), Self::RealNumber(value1)) => value0 == value1,
            (Self::Boolean(value0), Self::Boolean(value1)) => value0 == value1,
            (Self::Dictionary(value0), Self::Dictionary(value1)) => value0 == value1,
            (Self::Array(value0), Self::Array(value1)) => value0 == value1,
            (Self::ComponentClass(value0), Self::ComponentClass(value1)) => value0 == value1,
            _ => unreachable!(),
        }
    }
}

impl<Type: ParameterValueType> Eq for Parameter<Type>
where
    Type::Image: Eq,
    Type::Audio: Eq,
    Type::Binary: Eq,
    Type::String: Eq,
    Type::Integer: Eq,
    Type::RealNumber: Eq,
    Type::Boolean: Eq,
    Type::Dictionary: Eq,
    Type::Array: Eq,
    Type::ComponentClass: Eq,
{
}

impl<Type: ParameterValueType> Hash for Parameter<Type>
where
    Type::Image: Hash,
    Type::Audio: Hash,
    Type::Binary: Hash,
    Type::String: Hash,
    Type::Integer: Hash,
    Type::RealNumber: Hash,
    Type::Boolean: Hash,
    Type::Dictionary: Hash,
    Type::Array: Hash,
    Type::ComponentClass: Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        mem::discriminant(self).hash(state);
        match self {
            Self::None => {}
            Self::Image(value) => value.hash(state),
            Self::Audio(value) => value.hash(state),
            Self::Binary(value) => value.hash(state),
            Self::String(value) => value.hash(state),
            Self::Integer(value) => value.hash(state),
            Self::RealNumber(value) => value.hash(state),
            Self::Boolean(value) => value.hash(state),
            Self::Dictionary(value) => value.hash(state),
            Self::Array(value) => value.hash(state),
            Self::ComponentClass(value) => value.hash(state),
        }
    }
}

impl<Type: ParameterValueType> Debug for Parameter<Type> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Parameter::None => f.write_str("None"),
            Parameter::Image(_) => f.write_str("Image"),
            Parameter::Audio(_) => f.write_str("Audio"),
            Parameter::Binary(_) => f.write_str("Binary"),
            Parameter::String(_) => f.write_str("String"),
            Parameter::Integer(_) => f.write_str("Integer"),
            Parameter::RealNumber(_) => f.write_str("RealNumber"),
            Parameter::Boolean(_) => f.write_str("Boolean"),
            Parameter::Dictionary(_) => f.write_str("Dictionary"),
            Parameter::Array(_) => f.write_str("Array"),
            Parameter::ComponentClass(_) => f.write_str("ComponentClass"),
        }
    }
}

pub struct ParameterAllValues<Type: ParameterValueType> {
    pub image: Type::Image,
    pub audio: Type::Audio,
    pub binary: Type::Binary,
    pub string: Type::String,
    pub integer: Type::Integer,
    pub real_number: Type::RealNumber,
    pub boolean: Type::Boolean,
    pub dictionary: Type::Dictionary,
    pub array: Type::Array,
    pub component_class: Type::ComponentClass,
}

impl<Type: ParameterValueType> Default for ParameterAllValues<Type>
where
    Type::Image: Default,
    Type::Audio: Default,
    Type::Binary: Default,
    Type::String: Default,
    Type::Integer: Default,
    Type::RealNumber: Default,
    Type::Boolean: Default,
    Type::Dictionary: Default,
    Type::Array: Default,
    Type::ComponentClass: Default,
{
    fn default() -> Self {
        ParameterAllValues {
            image: Default::default(),
            audio: Default::default(),
            binary: Default::default(),
            string: Default::default(),
            integer: Default::default(),
            real_number: Default::default(),
            boolean: Default::default(),
            dictionary: Default::default(),
            array: Default::default(),
            component_class: Default::default(),
        }
    }
}

impl<Type: ParameterValueType> Clone for ParameterAllValues<Type> {
    fn clone(&self) -> Self {
        ParameterAllValues {
            image: self.image.clone(),
            audio: self.audio.clone(),
            binary: self.binary.clone(),
            string: self.string.clone(),
            integer: self.integer.clone(),
            real_number: self.real_number.clone(),
            boolean: self.boolean.clone(),
            dictionary: self.dictionary.clone(),
            array: self.array.clone(),
            component_class: self.component_class.clone(),
        }
    }
}

impl<Type: ParameterValueType> PartialEq for ParameterAllValues<Type>
where
    Type::Image: PartialEq,
    Type::Audio: PartialEq,
    Type::Binary: PartialEq,
    Type::String: PartialEq,
    Type::Integer: PartialEq,
    Type::RealNumber: PartialEq,
    Type::Boolean: PartialEq,
    Type::Dictionary: PartialEq,
    Type::Array: PartialEq,
    Type::ComponentClass: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.image == other.image
            && self.audio == other.audio
            && self.binary == other.binary
            && self.string == other.string
            && self.integer == other.integer
            && self.real_number == other.real_number
            && self.boolean == other.boolean
            && self.dictionary == other.dictionary
            && self.array == other.array
            && self.component_class == other.component_class
    }
}

impl<Type: ParameterValueType> Eq for ParameterAllValues<Type>
where
    Type::Image: Eq,
    Type::Audio: Eq,
    Type::Binary: Eq,
    Type::String: Eq,
    Type::Integer: Eq,
    Type::RealNumber: Eq,
    Type::Boolean: Eq,
    Type::Dictionary: Eq,
    Type::Array: Eq,
    Type::ComponentClass: Eq,
{
}

impl<Type: ParameterValueType> Hash for ParameterAllValues<Type>
where
    Type::Image: Hash,
    Type::Audio: Hash,
    Type::Binary: Hash,
    Type::String: Hash,
    Type::Integer: Hash,
    Type::RealNumber: Hash,
    Type::Boolean: Hash,
    Type::Dictionary: Hash,
    Type::Array: Hash,
    Type::ComponentClass: Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.image.hash(state);
        self.audio.hash(state);
        self.binary.hash(state);
        self.string.hash(state);
        self.integer.hash(state);
        self.real_number.hash(state);
        self.boolean.hash(state);
        self.dictionary.hash(state);
        self.array.hash(state);
        self.component_class.hash(state);
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Never {}

pub struct Type;

pub type ParameterType = Parameter<Type>;

impl ParameterValueType for Type {
    type Image = ();
    type Audio = ();
    type Binary = ();
    type String = ();
    type Integer = ();
    type RealNumber = ();
    type Boolean = ();
    type Dictionary = Vec<(String, Parameter<Type>)>;
    type Array = Box<Parameter<Type>>;
    type ComponentClass = ();
}

pub struct TypeExceptComponentClass;

pub type ParameterTypeExceptComponentClass = Parameter<TypeExceptComponentClass>;

impl ParameterValueType for TypeExceptComponentClass {
    type Image = ();
    type Audio = ();
    type Binary = Option<Box<[String]>>;
    type String = Option<Range<usize>>;
    type Integer = Option<Range<i64>>;
    type RealNumber = Option<Range<f64>>;
    type Boolean = ();
    type Dictionary = Vec<(String, Parameter<TypeExceptComponentClass>)>;
    type Array = Box<Parameter<TypeExceptComponentClass>>;
    type ComponentClass = Never;
}

pub trait FileAbstraction: Read + Seek + Send + Sync {
    fn identifier(&self) -> Uuid;
    fn duplicate(&self) -> Box<dyn FileAbstraction>;
}

pub struct AbstractFile(Box<dyn FileAbstraction>);

impl AbstractFile {
    pub fn new(file: impl FileAbstraction + 'static) -> AbstractFile {
        AbstractFile(Box::new(file))
    }
}

impl Read for AbstractFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        self.0.read_vectored(bufs)
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.0.read_to_end(buf)
    }

    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        self.0.read_to_string(buf)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.0.read_exact(buf)
    }
}

impl Seek for AbstractFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.0.seek(pos)
    }

    fn rewind(&mut self) -> io::Result<()> {
        self.0.rewind()
    }

    fn stream_position(&mut self) -> io::Result<u64> {
        self.0.stream_position()
    }
}

impl FileAbstraction for AbstractFile {
    fn identifier(&self) -> Uuid {
        self.0.identifier()
    }
    fn duplicate(&self) -> Box<dyn FileAbstraction> {
        self.0.duplicate()
    }
}

impl Clone for AbstractFile {
    fn clone(&self) -> Self {
        AbstractFile(self.0.duplicate())
    }
}

impl Default for AbstractFile {
    fn default() -> Self {
        struct AbstractEmptyFile;
        impl Read for AbstractEmptyFile {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Ok(0)
            }
        }

        impl Seek for AbstractEmptyFile {
            fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
                Ok(0)
            }
        }

        impl FileAbstraction for AbstractEmptyFile {
            fn identifier(&self) -> Uuid {
                Uuid::nil()
            }
            fn duplicate(&self) -> Box<dyn FileAbstraction> {
                Box::new(AbstractEmptyFile)
            }
        }

        AbstractFile(Box::new(AbstractEmptyFile))
    }
}

impl Hash for AbstractFile {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.identifier().hash(state);
    }
}

pub struct Value<K>(PhantomData<K>);

unsafe impl<K> Send for Value<K> {}

unsafe impl<K> Sync for Value<K> {}

pub type ParameterValue<K> = Parameter<Value<K>>;

impl<K: 'static> ParameterValueType for Value<K> {
    type Image = Placeholder<TagImage>;
    type Audio = Placeholder<TagAudio>;
    type Binary = TimeSplitValue<MarkerPinHandle<K>, EasingValue<AbstractFile>>;
    type String = TimeSplitValue<MarkerPinHandle<K>, EasingValue<String>>;
    type Integer = TimeSplitValue<MarkerPinHandle<K>, EasingValue<i64>>;
    type RealNumber = TimeSplitValue<MarkerPinHandle<K>, EasingValue<f64>>;
    type Boolean = TimeSplitValue<MarkerPinHandle<K>, EasingValue<bool>>;
    type Dictionary = Never;
    type Array = Never;
    type ComponentClass = ();
}

pub struct NullableValue<K, T>(PhantomData<(K, T)>);

unsafe impl<K, T> Send for NullableValue<K, T> {}

unsafe impl<K, T> Sync for NullableValue<K, T> {}

pub type ParameterNullableValue<K, T> = Parameter<NullableValue<K, T>>;

impl<K: 'static, T: ParameterValueType> ParameterValueType for NullableValue<K, T> {
    type Image = TimeSplitValue<MarkerPinHandle<K>, Option<EasingValue<T::Image>>>;
    type Audio = TimeSplitValue<MarkerPinHandle<K>, Option<EasingValue<T::Audio>>>;
    type Binary = TimeSplitValue<MarkerPinHandle<K>, Option<EasingValue<AbstractFile>>>;
    type String = TimeSplitValue<MarkerPinHandle<K>, Option<EasingValue<String>>>;
    type Integer = TimeSplitValue<MarkerPinHandle<K>, Option<EasingValue<i64>>>;
    type RealNumber = TimeSplitValue<MarkerPinHandle<K>, Option<EasingValue<f64>>>;
    type Boolean = TimeSplitValue<MarkerPinHandle<K>, Option<EasingValue<bool>>>;
    type Dictionary = Never;
    type Array = Never;
    type ComponentClass = Option<()>;
}

pub struct TypedValue<K>(PhantomData<K>);

unsafe impl<K> Send for TypedValue<K> {}

unsafe impl<K> Sync for TypedValue<K> {}

pub type ParameterTypedValue<K> = Parameter<TypedValue<K>>;

impl<K: 'static> ParameterValueType for TypedValue<K> {
    type Image = Placeholder<TagImage>;
    type Audio = Placeholder<TagAudio>;
    type Binary = (Option<Box<[String]>>, TimeSplitValue<MarkerPinHandle<K>, EasingValue<AbstractFile>>);
    type String = (Option<Range<usize>>, TimeSplitValue<MarkerPinHandle<K>, EasingValue<String>>);
    type Integer = (Option<Range<i64>>, TimeSplitValue<MarkerPinHandle<K>, EasingValue<i64>>);
    type RealNumber = (Option<Range<f64>>, TimeSplitValue<MarkerPinHandle<K>, EasingValue<f64>>);
    type Boolean = TimeSplitValue<MarkerPinHandle<K>, EasingValue<bool>>;
    type Dictionary = (Vec<(String, Parameter<Type>)>, TimeSplitValue<MarkerPinHandle<K>, HashMap<String, ParameterValue<K>>>);
    type Array = (Vec<Parameter<Type>>, TimeSplitValue<MarkerPinHandle<K>, Vec<ParameterValue<K>>>);
    type ComponentClass = ();
}

pub struct ValueRaw<Image, Audio>(PhantomData<(Image, Audio)>);

unsafe impl<Image, Audio> Send for ValueRaw<Image, Audio> {}

unsafe impl<Image, Audio> Sync for ValueRaw<Image, Audio> {}

pub type ParameterValueRaw<Image, Audio> = Parameter<ValueRaw<Image, Audio>>;

impl<Image: Send + Sync + Clone + 'static, Audio: Send + Sync + Clone + 'static> ParameterValueType for ValueRaw<Image, Audio> {
    type Image = Image;
    type Audio = Audio;
    type Binary = AbstractFile;
    type String = String;
    type Integer = i64;
    type RealNumber = f64;
    type Boolean = bool;
    type Dictionary = HashMap<String, ParameterValueRaw<Image, Audio>>;
    type Array = Vec<ParameterValueRaw<Image, Audio>>;
    type ComponentClass = ();
}

pub struct ValueFixed<Image, Audio>(PhantomData<(Image, Audio)>);

unsafe impl<Image, Audio> Send for ValueFixed<Image, Audio> {}

unsafe impl<Image, Audio> Sync for ValueFixed<Image, Audio> {}

pub type ParameterValueFixed<Image, Audio> = Parameter<ValueFixed<Image, Audio>>;

impl<Image: Send + Sync + Clone + 'static, Audio: Send + Sync + Clone + 'static> ParameterValueType for ValueFixed<Image, Audio> {
    type Image = DynEditableSingleValue<Image>;
    type Audio = DynEditableSingleValue<Audio>;
    type Binary = DynEditableSingleValue<AbstractFile>;
    type String = DynEditableSingleValue<String>;
    type Integer = DynEditableSingleValue<i64>;
    type RealNumber = DynEditableSingleValue<f64>;
    type Boolean = DynEditableSingleValue<bool>;
    type Dictionary = DynEditableSingleValue<HashMap<String, ParameterValueRaw<Image, Audio>>>;
    type Array = DynEditableSingleValue<Vec<ParameterValueRaw<Image, Audio>>>;
    type ComponentClass = ();
}

pub struct ValueFixedExceptComponentClass;

pub type ParameterValueFixedExceptComponentClass = Parameter<ValueFixedExceptComponentClass>;

impl ParameterValueType for ValueFixedExceptComponentClass {
    type Image = Placeholder<TagImage>;
    type Audio = Placeholder<TagAudio>;
    type Binary = DynEditableSingleValue<AbstractFile>;
    type String = DynEditableSingleValue<String>;
    type Integer = DynEditableSingleValue<i64>;
    type RealNumber = DynEditableSingleValue<f64>;
    type Boolean = DynEditableSingleValue<bool>;
    type Dictionary = HashMap<String, ParameterValueFixedExceptComponentClass>;
    type Array = Vec<ParameterValueFixedExceptComponentClass>;
    type ComponentClass = Never;
}

pub struct ParameterSelect;

impl ParameterValueType for ParameterSelect {
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

#[derive(Debug, Clone, Copy)]
pub struct Opacity(f64);

impl Opacity {
    pub const OPAQUE: Opacity = Opacity(1.0);
    pub const TRANSPARENT: Opacity = Opacity(0.0);

    pub fn new(value: f64) -> Option<Opacity> {
        if (0.0..=1.).contains(&value) {
            Some(Opacity(if value == -0. { 0. } else { value }))
        } else {
            None
        }
    }

    pub fn saturating_new(value: f64) -> Opacity {
        if value.is_nan() || value <= -0. {
            Opacity(0.)
        } else if value > 1. {
            Opacity(1.)
        } else {
            Opacity(value)
        }
    }

    pub fn value(self) -> f64 {
        self.0
    }
}

impl PartialEq for Opacity {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Opacity {}

impl PartialOrd for Opacity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Opacity {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.partial_cmp(&other.0).unwrap()
    }
}

impl Hash for Opacity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_be_bytes().hash(state);
    }
}

impl Default for Opacity {
    fn default() -> Self {
        Opacity::new(1.0).unwrap()
    }
}

// ref: https://www.w3.org/TR/compositing-1/
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize_repr, Deserialize_repr)]
#[cfg_attr(any(feature = "proptest", test), derive(proptest_derive::Arbitrary))]
#[repr(u8)]
pub enum BlendMode {
    #[default]
    Normal = 0,
    Multiply = 1,
    Screen = 2,
    Overlay = 3,
    Darken = 4,
    Lighten = 5,
    ColorDodge = 6,
    ColorBurn = 7,
    HardLight = 8,
    SoftLight = 9,
    Difference = 10,
    Exclusion = 11,
    Hue = 12,
    Saturation = 13,
    Color = 14,
    Luminosity = 15,
}

// ref: https://www.w3.org/TR/compositing-1/
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize_repr, Deserialize_repr)]
#[cfg_attr(any(feature = "proptest", test), derive(proptest_derive::Arbitrary))]
#[repr(u8)]
pub enum CompositeOperation {
    Clear = 0,
    Copy = 1,
    Destination = 2,
    #[default]
    SourceOver = 3,
    DestinationOver = 4,
    SourceIn = 5,
    DestinationIn = 6,
    SourceOut = 7,
    DestinationOut = 8,
    SourceAtop = 9,
    DestinationAtop = 10,
    XOR = 11,
    Lighter = 12,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(any(feature = "proptest", test), derive(proptest_derive::Arbitrary))]
pub enum VariableParameterPriority {
    #[serde(rename = "m")]
    PrioritizeManually,
    #[serde(rename = "c")]
    PrioritizeComponent,
}

pub type PinSplitValue<K, T> = TimeSplitValue<MarkerPinHandle<K>, T>;

#[derive(Debug)]
pub struct VariableParameterValue<K: 'static, T: ParameterValueType, Nullable> {
    pub params: Nullable,
    pub components: Vec<ComponentInstanceHandle<K, T>>,
    pub priority: VariableParameterPriority,
}

impl<K, T: ParameterValueType, Nullable: Clone> Clone for VariableParameterValue<K, T, Nullable> {
    fn clone(&self) -> Self {
        let VariableParameterValue { params, components, priority } = self;
        VariableParameterValue {
            params: params.clone(),
            components: components.clone(),
            priority: *priority,
        }
    }
}

impl<K: 'static, T: ParameterValueType, Nullable> VariableParameterValue<K, T, Nullable> {
    pub fn new(value: Nullable) -> VariableParameterValue<K, T, Nullable> {
        VariableParameterValue {
            params: value,
            components: Vec::new(),
            priority: VariableParameterPriority::PrioritizeManually,
        }
    }
}

#[derive(Debug)]
pub struct ImageRequiredParams<K: 'static, T: ParameterValueType> {
    pub transform: ImageRequiredParamsTransform<K, T>,
    pub background_color: [u8; 4],
    pub opacity: PinSplitValue<K, EasingValue<f64>>,
    pub blend_mode: PinSplitValue<K, BlendMode>,
    pub composite_operation: PinSplitValue<K, CompositeOperation>,
}

impl<K, T: ParameterValueType> ImageRequiredParams<K, T> {
    pub fn new_default(marker_left: &MarkerPinHandle<K>, marker_right: &MarkerPinHandle<K>) -> ImageRequiredParams<K, T> {
        let one = TimeSplitValue::new(marker_left.clone(), Some(EasingValue::new(DynEditableLerpEasingValue((1., 1.)), Arc::new(LinearEasing))), marker_right.clone());
        let one_value = VariableParameterValue::new(one);
        let zero = VariableParameterValue::new(TimeSplitValue::new(marker_left.clone(), Some(EasingValue::new(DynEditableLerpEasingValue((0., 0.)), Arc::new(LinearEasing))), marker_right.clone()));
        ImageRequiredParams {
            transform: ImageRequiredParamsTransform::Params {
                size: Vector3 {
                    x: one_value.clone(),
                    y: one_value.clone(),
                    z: one_value.clone(),
                },
                scale: Vector3 {
                    x: one_value.clone(),
                    y: one_value.clone(),
                    z: one_value,
                },
                translate: Vector3 { x: zero.clone(), y: zero.clone(), z: zero.clone() },
                rotate: TimeSplitValue::new(marker_left.clone(), EasingValue::new(DynEditableLerpEasingValue((Quaternion::one(), Quaternion::one())), Arc::new(LinearEasing)), marker_right.clone()),
                scale_center: Vector3 { x: zero.clone(), y: zero.clone(), z: zero.clone() },
                rotate_center: Vector3 { x: zero.clone(), y: zero.clone(), z: zero },
            },
            background_color: [0; 4],
            opacity: TimeSplitValue::new(marker_left.clone(), EasingValue::new(DynEditableLerpEasingValue((1., 1.)), Arc::new(LinearEasing)), marker_right.clone()),
            blend_mode: TimeSplitValue::new(marker_left.clone(), Default::default(), marker_right.clone()),
            composite_operation: TimeSplitValue::new(marker_left.clone(), Default::default(), marker_right.clone()),
        }
    }
}

impl<K, T: ParameterValueType> Clone for ImageRequiredParams<K, T> {
    fn clone(&self) -> Self {
        let ImageRequiredParams {
            transform,
            background_color,
            opacity,
            blend_mode,
            composite_operation,
        } = self;
        ImageRequiredParams {
            transform: transform.clone(),
            background_color: *background_color,
            opacity: opacity.clone(),
            blend_mode: blend_mode.clone(),
            composite_operation: composite_operation.clone(),
        }
    }
}

pub type Vector3Params<K, T> = Vector3<VariableParameterValue<K, T, PinSplitValue<K, Option<EasingValue<f64>>>>>;

#[derive(Debug)]
pub enum ImageRequiredParamsTransform<K: 'static, T: ParameterValueType> {
    Params {
        size: Vector3Params<K, T>,
        scale: Vector3Params<K, T>,
        translate: Vector3Params<K, T>,
        rotate: TimeSplitValue<MarkerPinHandle<K>, EasingValue<Quaternion<f64>>>,
        scale_center: Vector3Params<K, T>,
        rotate_center: Vector3Params<K, T>,
    },
    Free {
        left_top: Vector3Params<K, T>,
        right_top: Vector3Params<K, T>,
        left_bottom: Vector3Params<K, T>,
        right_bottom: Vector3Params<K, T>,
    },
}

impl<K, T: ParameterValueType> Clone for ImageRequiredParamsTransform<K, T> {
    fn clone(&self) -> Self {
        match self {
            ImageRequiredParamsTransform::Params {
                size,
                scale,
                translate,
                rotate,
                scale_center,
                rotate_center,
            } => ImageRequiredParamsTransform::Params {
                size: size.clone(),
                scale: scale.clone(),
                translate: translate.clone(),
                rotate: rotate.clone(),
                scale_center: scale_center.clone(),
                rotate_center: rotate_center.clone(),
            },
            ImageRequiredParamsTransform::Free { left_top, right_top, left_bottom, right_bottom } => ImageRequiredParamsTransform::Free {
                left_top: left_top.clone(),
                right_top: right_top.clone(),
                left_bottom: left_bottom.clone(),
                right_bottom: right_bottom.clone(),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageRequiredParamsFixed {
    pub transform: ImageRequiredParamsTransformFixed,
    pub background_color: [u8; 4],
    pub opacity: Opacity,
    pub blend_mode: BlendMode,
    pub composite_operation: CompositeOperation,
}

#[derive(Debug, Clone)]
pub enum ImageRequiredParamsTransformFixed {
    Params {
        size: Vector3<f64>,
        scale: Vector3<f64>,
        translate: Vector3<f64>,
        rotate: Quaternion<f64>,
        scale_center: Vector3<f64>,
        rotate_center: Vector3<f64>,
    },
    Free {
        left_top: Vector3<f64>,
        right_top: Vector3<f64>,
        left_bottom: Vector3<f64>,
        right_bottom: Vector3<f64>,
    },
}

pub type SingleChannelVolume<K, T> = VariableParameterValue<K, T, PinSplitValue<K, Option<EasingValue<f64>>>>;

#[derive(Debug)]
pub struct AudioRequiredParams<K: 'static, T: ParameterValueType> {
    pub volume: Vec<SingleChannelVolume<K, T>>,
}

impl<K: 'static, T: ParameterValueType> AudioRequiredParams<K, T> {
    pub fn new_default(left: &MarkerPinHandle<K>, right: &MarkerPinHandle<K>, channels: usize) -> AudioRequiredParams<K, T> {
        let one = TimeSplitValue::new(left.clone(), Some(EasingValue::new(DynEditableLerpEasingValue((1., 1.)), Arc::new(LinearEasing))), right.clone());
        let one_value = VariableParameterValue::new(one);
        AudioRequiredParams { volume: vec![one_value; channels] }
    }
}

impl<K, T: ParameterValueType> Clone for AudioRequiredParams<K, T> {
    fn clone(&self) -> Self {
        let AudioRequiredParams { volume } = self;
        AudioRequiredParams { volume: volume.clone() }
    }
}
#[derive(Debug, Clone)]
pub struct AudioRequiredParamsFixed {
    pub volume: Vec<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;

    #[test]
    fn test_opacity() {
        assert_eq!(Opacity::new(-f64::EPSILON), None);
        assert_eq!(Opacity::new(-0.0), Some(Opacity(0.0)));
        assert_eq!(Opacity::new(0.0), Some(Opacity(0.0)));
        assert_eq!(Opacity::new(0.5), Some(Opacity(0.5)));
        assert_eq!(Opacity::new(1.0), Some(Opacity(1.0)));
        assert_eq!(Opacity::new(1.0 + f64::EPSILON), None);
        assert_eq!(
            {
                let mut hasher = DefaultHasher::new();
                Opacity::new(-0.0).unwrap().hash(&mut hasher);
                hasher.finish()
            },
            {
                let mut hasher = DefaultHasher::new();
                Opacity::new(0.0).unwrap().hash(&mut hasher);
                hasher.finish()
            }
        );

        assert_eq!(Opacity::saturating_new(f64::NEG_INFINITY), Opacity(0.0));
        assert_eq!(Opacity::saturating_new(-f64::EPSILON), Opacity(0.0));
        assert_eq!(Opacity::saturating_new(-0.0), Opacity(0.0));
        assert_eq!(Opacity::saturating_new(0.0), Opacity(0.0));
        assert_eq!(Opacity::saturating_new(0.5), Opacity(0.5));
        assert_eq!(Opacity::saturating_new(1.0), Opacity(1.0));
        assert_eq!(Opacity::saturating_new(1.0 + f64::EPSILON), Opacity(1.0));
        assert_eq!(Opacity::saturating_new(f64::INFINITY), Opacity(1.0));
        assert_eq!(Opacity::saturating_new(f64::NAN), Opacity(0.0));
        assert_eq!(Opacity::saturating_new(-f64::NAN), Opacity(0.0));
    }
}
