use crate::common::time_split_value::TimeSplitValue;
use crate::component::instance::ComponentInstance;
use crate::component::marker_pin::MarkerPin;
use crate::component::parameter::placeholder::{Placeholder, TagAudio, TagImage};
use crate::component::parameter::value::{DefaultEasing, EasingValue, FrameVariableValue};
use crate::ptr::StaticPointer;
use crate::time::TimelineTime;
use cgmath::{One, Quaternion, Vector2, Vector3};
use either::Either;
use qcell::TCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::mem;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

pub mod placeholder;
pub mod value;

pub trait ParameterValueType: 'static + Send + Sync {
    type Image: 'static + Clone + Send + Sync;
    type Audio: 'static + Clone + Send + Sync;
    type Video: 'static + Clone + Send + Sync;
    type File: 'static + Clone + Send + Sync;
    type String: 'static + Clone + Send + Sync;
    type Select: 'static + Clone + Send + Sync;
    type Boolean: 'static + Clone + Send + Sync;
    type Radio: 'static + Clone + Send + Sync;
    type Integer: 'static + Clone + Send + Sync;
    type RealNumber: 'static + Clone + Send + Sync;
    type Vec2: 'static + Clone + Send + Sync;
    type Vec3: 'static + Clone + Send + Sync;
    type Dictionary: 'static + Clone + Send + Sync;
    type ComponentClass: 'static + Clone + Send + Sync;
}

impl<A, B> ParameterValueType for (A, B)
where
    A: ParameterValueType,
    B: ParameterValueType,
{
    type Image = (A::Image, B::Image);
    type Audio = (A::Audio, B::Audio);
    type Video = (A::Video, B::Video);
    type File = (A::File, B::File);
    type String = (A::String, B::String);
    type Select = (A::Select, B::Select);
    type Boolean = (A::Boolean, B::Boolean);
    type Radio = (A::Radio, B::Radio);
    type Integer = (A::Integer, B::Integer);
    type RealNumber = (A::RealNumber, B::RealNumber);
    type Vec2 = (A::Vec2, B::Vec2);
    type Vec3 = (A::Vec3, B::Vec3);
    type Dictionary = (A::Dictionary, B::Dictionary);
    type ComponentClass = (A::ComponentClass, B::ComponentClass);
}

impl<T: ParameterValueType> ParameterValueType for Option<T> {
    type Image = Option<T::Image>;
    type Audio = Option<T::Audio>;
    type Video = Option<T::Video>;
    type File = Option<T::File>;
    type String = Option<T::String>;
    type Select = Option<T::Select>;
    type Boolean = Option<T::Boolean>;
    type Radio = Option<T::Radio>;
    type Integer = Option<T::Integer>;
    type RealNumber = Option<T::RealNumber>;
    type Vec2 = Option<T::Vec2>;
    type Vec3 = Option<T::Vec3>;
    type Dictionary = Option<T::Dictionary>;
    type ComponentClass = Option<T::ComponentClass>;
}

pub enum Parameter<Type: ParameterValueType> {
    None,
    Image(Type::Image),
    Audio(Type::Audio),
    Video(Type::Video),
    File(Type::File),
    String(Type::String),
    Select(Type::Select),
    Boolean(Type::Boolean),
    Radio(Type::Radio),
    Integer(Type::Integer),
    RealNumber(Type::RealNumber),
    Vec2(Type::Vec2),
    Vec3(Type::Vec3),
    Dictionary(Type::Dictionary),
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
    pub fn into_video(self) -> Result<Type::Video, Self> {
        if let Parameter::Video(value) = self {
            Ok(value)
        } else {
            Err(self)
        }
    }
    pub fn into_file(self) -> Result<Type::File, Self> {
        if let Parameter::File(value) = self {
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
    pub fn into_select(self) -> Result<Type::Select, Self> {
        if let Parameter::Select(value) = self {
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
    pub fn into_radio(self) -> Result<Type::Radio, Self> {
        if let Parameter::Radio(value) = self {
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
    pub fn into_vec2(self) -> Result<Type::Vec2, Self> {
        if let Parameter::Vec2(value) = self {
            Ok(value)
        } else {
            Err(self)
        }
    }
    pub fn into_vec3(self) -> Result<Type::Vec3, Self> {
        if let Parameter::Vec3(value) = self {
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
    pub fn as_video(&self) -> Option<&Type::Video> {
        if let Parameter::Video(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_file(&self) -> Option<&Type::File> {
        if let Parameter::File(value) = self {
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
    pub fn as_select(&self) -> Option<&Type::Select> {
        if let Parameter::Select(value) = self {
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
    pub fn as_radio(&self) -> Option<&Type::Radio> {
        if let Parameter::Radio(value) = self {
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
    pub fn as_vec2(&self) -> Option<&Type::Vec2> {
        if let Parameter::Vec2(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn as_vec3(&self) -> Option<&Type::Vec3> {
        if let Parameter::Vec3(value) = self {
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
                | (Self::Video(_), Parameter::Video(_))
                | (Self::File(_), Parameter::File(_))
                | (Self::String(_), Parameter::String(_))
                | (Self::Select(_), Parameter::Select(_))
                | (Self::Boolean(_), Parameter::Boolean(_))
                | (Self::Radio(_), Parameter::Radio(_))
                | (Self::Integer(_), Parameter::Integer(_))
                | (Self::RealNumber(_), Parameter::RealNumber(_))
                | (Self::Vec2(_), Parameter::Vec2(_))
                | (Self::Vec3(_), Parameter::Vec3(_))
                | (Self::Dictionary(_), Parameter::Dictionary(_))
                | (Self::ComponentClass(_), Parameter::ComponentClass(_))
        )
    }
    pub fn select(&self) -> Parameter<ParameterSelect> {
        match self {
            Self::None => Parameter::<ParameterSelect>::None,
            Self::Image(_) => Parameter::<ParameterSelect>::Image(()),
            Self::Audio(_) => Parameter::<ParameterSelect>::Audio(()),
            Self::Video(_) => Parameter::<ParameterSelect>::Video(()),
            Self::File(_) => Parameter::<ParameterSelect>::File(()),
            Self::String(_) => Parameter::<ParameterSelect>::String(()),
            Self::Select(_) => Parameter::<ParameterSelect>::Select(()),
            Self::Boolean(_) => Parameter::<ParameterSelect>::Boolean(()),
            Self::Radio(_) => Parameter::<ParameterSelect>::Radio(()),
            Self::Integer(_) => Parameter::<ParameterSelect>::Integer(()),
            Self::RealNumber(_) => Parameter::<ParameterSelect>::RealNumber(()),
            Self::Vec2(_) => Parameter::<ParameterSelect>::Vec2(()),
            Self::Vec3(_) => Parameter::<ParameterSelect>::Vec3(()),
            Self::Dictionary(_) => Parameter::<ParameterSelect>::Dictionary(()),
            Self::ComponentClass(_) => Parameter::<ParameterSelect>::ComponentClass(()),
        }
    }
}

impl<Type: ParameterValueType> Clone for Parameter<Type> {
    fn clone(&self) -> Self {
        match self {
            Self::None => Self::None,
            Self::Image(value) => Self::Image(value.clone()),
            Self::Audio(value) => Self::Audio(value.clone()),
            Self::Video(value) => Self::Video(value.clone()),
            Self::File(value) => Self::File(value.clone()),
            Self::String(value) => Self::String(value.clone()),
            Self::Select(value) => Self::Select(value.clone()),
            Self::Boolean(value) => Self::Boolean(value.clone()),
            Self::Radio(value) => Self::Radio(value.clone()),
            Self::Integer(value) => Self::Integer(value.clone()),
            Self::RealNumber(value) => Self::RealNumber(value.clone()),
            Self::Vec2(value) => Self::Vec2(value.clone()),
            Self::Vec3(value) => Self::Vec3(value.clone()),
            Self::Dictionary(value) => Self::Dictionary(value.clone()),
            Self::ComponentClass(value) => Self::ComponentClass(value.clone()),
        }
    }
}

impl<Type: ParameterValueType> Copy for Parameter<Type>
where
    Type::Image: Copy,
    Type::Audio: Copy,
    Type::Video: Copy,
    Type::File: Copy,
    Type::String: Copy,
    Type::Select: Copy,
    Type::Boolean: Copy,
    Type::Radio: Copy,
    Type::Integer: Copy,
    Type::RealNumber: Copy,
    Type::Vec2: Copy,
    Type::Vec3: Copy,
    Type::Dictionary: Copy,
    Type::ComponentClass: Copy,
{
}

impl<Type: ParameterValueType> PartialEq for Parameter<Type>
where
    Type::Image: PartialEq,
    Type::Audio: PartialEq,
    Type::Video: PartialEq,
    Type::File: PartialEq,
    Type::String: PartialEq,
    Type::Select: PartialEq,
    Type::Boolean: PartialEq,
    Type::Radio: PartialEq,
    Type::Integer: PartialEq,
    Type::RealNumber: PartialEq,
    Type::Vec2: PartialEq,
    Type::Vec3: PartialEq,
    Type::Dictionary: PartialEq,
    Type::ComponentClass: PartialEq,
{
    fn eq(&self, rhs: &Self) -> bool {
        match (self, rhs) {
            (Self::None, Self::None) => true,
            (Self::Image(value0), Self::Image(value1)) => value0 == value1,
            (Self::Audio(value0), Self::Audio(value1)) => value0 == value1,
            (Self::Video(value0), Self::Video(value1)) => value0 == value1,
            (Self::File(value0), Self::File(value1)) => value0 == value1,
            (Self::String(value0), Self::String(value1)) => value0 == value1,
            (Self::Select(value0), Self::Select(value1)) => value0 == value1,
            (Self::Boolean(value0), Self::Boolean(value1)) => value0 == value1,
            (Self::Radio(value0), Self::Radio(value1)) => value0 == value1,
            (Self::Integer(value0), Self::Integer(value1)) => value0 == value1,
            (Self::RealNumber(value0), Self::RealNumber(value1)) => value0 == value1,
            (Self::Vec2(value0), Self::Vec2(value1)) => value0 == value1,
            (Self::Vec3(value0), Self::Vec3(value1)) => value0 == value1,
            (Self::Dictionary(value0), Self::Dictionary(value1)) => value0 == value1,
            (Self::ComponentClass(value0), Self::ComponentClass(value1)) => value0 == value1,
            _ => false,
        }
    }
}

impl<Type: ParameterValueType> Eq for Parameter<Type>
where
    Type::Image: Eq,
    Type::Audio: Eq,
    Type::Video: Eq,
    Type::File: Eq,
    Type::String: Eq,
    Type::Select: Eq,
    Type::Boolean: Eq,
    Type::Radio: Eq,
    Type::Integer: Eq,
    Type::RealNumber: Eq,
    Type::Vec2: Eq,
    Type::Vec3: Eq,
    Type::Dictionary: Eq,
    Type::ComponentClass: Eq,
{
}

impl<Type: ParameterValueType> Hash for Parameter<Type>
where
    Type::Image: Hash,
    Type::Audio: Hash,
    Type::Video: Hash,
    Type::File: Hash,
    Type::String: Hash,
    Type::Select: Hash,
    Type::Boolean: Hash,
    Type::Radio: Hash,
    Type::Integer: Hash,
    Type::RealNumber: Hash,
    Type::Vec2: Hash,
    Type::Vec3: Hash,
    Type::Dictionary: Hash,
    Type::ComponentClass: Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        let tag = mem::discriminant(self);
        tag.hash(state);
        match self {
            Self::None => {}
            Self::Image(value) => value.hash(state),
            Self::Audio(value) => value.hash(state),
            Self::Video(value) => value.hash(state),
            Self::File(value) => value.hash(state),
            Self::String(value) => value.hash(state),
            Self::Select(value) => value.hash(state),
            Self::Boolean(value) => value.hash(state),
            Self::Radio(value) => value.hash(state),
            Self::Integer(value) => value.hash(state),
            Self::RealNumber(value) => value.hash(state),
            Self::Vec2(value) => value.hash(state),
            Self::Vec3(value) => value.hash(state),
            Self::Dictionary(value) => value.hash(state),
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
            Parameter::Video(_) => f.write_str("Video"),
            Parameter::File(_) => f.write_str("File"),
            Parameter::String(_) => f.write_str("String"),
            Parameter::Select(_) => f.write_str("Select"),
            Parameter::Boolean(_) => f.write_str("Boolean"),
            Parameter::Radio(_) => f.write_str("Radio"),
            Parameter::Integer(_) => f.write_str("Integer"),
            Parameter::RealNumber(_) => f.write_str("RealNumber"),
            Parameter::Vec2(_) => f.write_str("Vec2"),
            Parameter::Vec3(_) => f.write_str("Vec3"),
            Parameter::Dictionary(_) => f.write_str("Dictionary"),
            Parameter::ComponentClass(_) => f.write_str("ComponentClass"),
        }
    }
}

pub struct ParameterAllValues<Type: ParameterValueType> {
    pub image: Type::Image,
    pub audio: Type::Audio,
    pub video: Type::Video,
    pub file: Type::File,
    pub string: Type::String,
    pub select: Type::Select,
    pub boolean: Type::Boolean,
    pub radio: Type::Radio,
    pub integer: Type::Integer,
    pub real_number: Type::RealNumber,
    pub vec2: Type::Vec2,
    pub vec3: Type::Vec3,
    pub dictionary: Type::Dictionary,
    pub component_class: Type::ComponentClass,
}

impl<Type: ParameterValueType> Default for ParameterAllValues<Type>
where
    Type::Image: Default,
    Type::Audio: Default,
    Type::Video: Default,
    Type::File: Default,
    Type::String: Default,
    Type::Select: Default,
    Type::Boolean: Default,
    Type::Radio: Default,
    Type::Integer: Default,
    Type::RealNumber: Default,
    Type::Vec2: Default,
    Type::Vec3: Default,
    Type::Dictionary: Default,
    Type::ComponentClass: Default,
{
    fn default() -> Self {
        ParameterAllValues {
            image: Default::default(),
            audio: Default::default(),
            video: Default::default(),
            file: Default::default(),
            string: Default::default(),
            select: Default::default(),
            boolean: Default::default(),
            radio: Default::default(),
            integer: Default::default(),
            real_number: Default::default(),
            vec2: Default::default(),
            vec3: Default::default(),
            dictionary: Default::default(),
            component_class: Default::default(),
        }
    }
}

impl<Type: ParameterValueType> Clone for ParameterAllValues<Type> {
    fn clone(&self) -> Self {
        ParameterAllValues {
            image: self.image.clone(),
            audio: self.audio.clone(),
            video: self.video.clone(),
            file: self.file.clone(),
            string: self.string.clone(),
            select: self.select.clone(),
            boolean: self.boolean.clone(),
            radio: self.radio.clone(),
            integer: self.integer.clone(),
            real_number: self.real_number.clone(),
            vec2: self.vec2.clone(),
            vec3: self.vec3.clone(),
            dictionary: self.dictionary.clone(),
            component_class: self.component_class.clone(),
        }
    }
}

impl<Type: ParameterValueType> PartialEq for ParameterAllValues<Type>
where
    Type::Image: PartialEq,
    Type::Audio: PartialEq,
    Type::Video: PartialEq,
    Type::File: PartialEq,
    Type::String: PartialEq,
    Type::Select: PartialEq,
    Type::Boolean: PartialEq,
    Type::Radio: PartialEq,
    Type::Integer: PartialEq,
    Type::RealNumber: PartialEq,
    Type::Vec2: PartialEq,
    Type::Vec3: PartialEq,
    Type::Dictionary: PartialEq,
    Type::ComponentClass: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.image == other.image
            && self.audio == other.audio
            && self.video == other.video
            && self.file == other.file
            && self.string == other.string
            && self.select == other.select
            && self.boolean == other.boolean
            && self.radio == other.radio
            && self.integer == other.integer
            && self.real_number == other.real_number
            && self.vec2 == other.vec2
            && self.vec3 == other.vec3
            && self.dictionary == other.dictionary
            && self.component_class == other.component_class
    }
}

impl<Type: ParameterValueType> Eq for ParameterAllValues<Type>
where
    Type::Image: Eq,
    Type::Audio: Eq,
    Type::Video: Eq,
    Type::File: Eq,
    Type::String: Eq,
    Type::Select: Eq,
    Type::Boolean: Eq,
    Type::Radio: Eq,
    Type::Integer: Eq,
    Type::RealNumber: Eq,
    Type::Vec2: Eq,
    Type::Vec3: Eq,
    Type::Dictionary: Eq,
    Type::ComponentClass: Eq,
{
}

impl<Type: ParameterValueType> Hash for ParameterAllValues<Type>
where
    Type::Image: Hash,
    Type::Audio: Hash,
    Type::Video: Hash,
    Type::File: Hash,
    Type::String: Hash,
    Type::Select: Hash,
    Type::Boolean: Hash,
    Type::Radio: Hash,
    Type::Integer: Hash,
    Type::RealNumber: Hash,
    Type::Vec2: Hash,
    Type::Vec3: Hash,
    Type::Dictionary: Hash,
    Type::ComponentClass: Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.image.hash(state);
        self.audio.hash(state);
        self.video.hash(state);
        self.file.hash(state);
        self.string.hash(state);
        self.select.hash(state);
        self.boolean.hash(state);
        self.radio.hash(state);
        self.integer.hash(state);
        self.real_number.hash(state);
        self.vec2.hash(state);
        self.vec3.hash(state);
        self.dictionary.hash(state);
        self.component_class.hash(state);
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Never {}

pub struct Type;

pub type ParameterType = Parameter<Type>;

impl ParameterValueType for Type {
    type Image = ();
    type Audio = ();
    type Video = ();
    type File = Option<Box<[String]>>;
    type String = Option<Range<usize>>;
    type Select = Box<[String]>;
    type Boolean = ();
    type Radio = usize;
    type Integer = Option<Range<i64>>;
    type RealNumber = Option<Range<f64>>;
    type Vec2 = Option<Range<Vector2<f64>>>;
    type Vec3 = Option<Range<Vector3<f64>>>;
    type Dictionary = HashMap<String, Parameter<Type>>;
    type ComponentClass = ();
}

pub struct TypeExceptComponentClass;

pub type ParameterTypeExceptComponentClass = Parameter<TypeExceptComponentClass>;

impl ParameterValueType for TypeExceptComponentClass {
    type Image = ();
    type Audio = ();
    type Video = ();
    type File = Option<Box<[String]>>;
    type String = Option<Range<usize>>;
    type Select = Box<[String]>;
    type Boolean = ();
    type Radio = usize;
    type Integer = Option<Range<i64>>;
    type RealNumber = Option<Range<f64>>;
    type Vec2 = Option<Range<Vector2<f64>>>;
    type Vec3 = Option<Range<Vector3<f64>>>;
    type Dictionary = HashMap<String, Parameter<TypeExceptComponentClass>>;
    type ComponentClass = Never;
}

pub struct Value<K>(PhantomData<K>);

unsafe impl<K> Send for Value<K> {}

unsafe impl<K> Sync for Value<K> {}

pub type ParameterValue<K> = Parameter<Value<K>>;

impl<K: 'static> ParameterValueType for Value<K> {
    type Image = Placeholder<TagImage>;
    type Audio = Placeholder<TagAudio>;
    type Video = (Placeholder<TagImage>, Placeholder<TagAudio>);
    type File = TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, PathBuf>;
    type String = TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, String>;
    type Select = TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, usize>;
    type Boolean = TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, bool>;
    type Radio = TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, bool>;
    type Integer = TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, i64>;
    type RealNumber = TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, EasingValue<f64>>;
    type Vec2 = Vector2<TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, EasingValue<f64>>>;
    type Vec3 = Vector3<TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, EasingValue<f64>>>;
    type Dictionary = Never;
    type ComponentClass = ();
}

#[derive(Clone)]
pub struct ComponentProcessorInput;

pub type ComponentProcessorInputValue = Parameter<ComponentProcessorInput>;

impl ParameterValueType for ComponentProcessorInput {
    type Image = Placeholder<TagImage>;
    type Audio = Placeholder<TagAudio>;
    type Video = (Placeholder<TagImage>, Placeholder<TagAudio>);
    type File = TimeSplitValue<TimelineTime, Option<Either<PathBuf, FrameVariableValue<PathBuf>>>>;
    type String = TimeSplitValue<TimelineTime, Option<Either<String, FrameVariableValue<String>>>>;
    type Select = TimeSplitValue<TimelineTime, Option<Either<usize, FrameVariableValue<usize>>>>;
    type Boolean = TimeSplitValue<TimelineTime, Option<Either<bool, FrameVariableValue<bool>>>>;
    type Radio = TimeSplitValue<TimelineTime, Option<Either<bool, FrameVariableValue<bool>>>>;
    type Integer = TimeSplitValue<TimelineTime, Option<Either<i64, FrameVariableValue<i64>>>>;
    type RealNumber = TimeSplitValue<TimelineTime, Option<Either<EasingValue<f64>, FrameVariableValue<f64>>>>;
    type Vec2 = Vector2<TimeSplitValue<TimelineTime, Option<Either<EasingValue<f64>, FrameVariableValue<f64>>>>>;
    type Vec3 = Vector3<TimeSplitValue<TimelineTime, Option<Either<EasingValue<f64>, FrameVariableValue<f64>>>>>;
    type Dictionary = Never;
    type ComponentClass = ();
}

pub struct FrameVariable;

pub type ParameterFrameVariableValue = Parameter<FrameVariable>;

impl ParameterValueType for FrameVariable {
    type Image = Placeholder<TagImage>;
    type Audio = Placeholder<TagAudio>;
    type Video = (Placeholder<TagImage>, Placeholder<TagAudio>);
    type File = FrameVariableValue<PathBuf>;
    type String = FrameVariableValue<String>;
    type Select = FrameVariableValue<usize>;
    type Boolean = FrameVariableValue<bool>;
    type Radio = FrameVariableValue<bool>;
    type Integer = FrameVariableValue<i64>;
    type RealNumber = FrameVariableValue<f64>;
    type Vec2 = FrameVariableValue<Vector2<f64>>;
    type Vec3 = FrameVariableValue<Vector3<f64>>;
    type Dictionary = Never;
    type ComponentClass = Never;
}

pub struct NullableValue<K>(PhantomData<K>);

unsafe impl<K> Send for NullableValue<K> {}

unsafe impl<K> Sync for NullableValue<K> {}

pub type ParameterNullableValue<K> = Parameter<NullableValue<K>>;

impl<K: 'static> ParameterValueType for NullableValue<K> {
    type Image = Option<Placeholder<TagImage>>;
    type Audio = Option<Placeholder<TagAudio>>;
    type Video = Option<(Placeholder<TagImage>, Placeholder<TagAudio>)>;
    type File = TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, Option<PathBuf>>;
    type String = TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, Option<String>>;
    type Select = TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, Option<usize>>;
    type Boolean = TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, Option<bool>>;
    type Radio = TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, Option<bool>>;
    type Integer = TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, Option<i64>>;
    type RealNumber = TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, Option<EasingValue<f64>>>;
    type Vec2 = Vector2<TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, Option<EasingValue<f64>>>>;
    type Vec3 = Vector3<TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, Option<EasingValue<f64>>>>;
    type Dictionary = Never;
    type ComponentClass = Option<()>;
}

pub struct TypedValue<K>(PhantomData<K>);

unsafe impl<K> Send for TypedValue<K> {}

unsafe impl<K> Sync for TypedValue<K> {}

pub type ParameterTypedValue<K> = Parameter<TypedValue<K>>;

impl<K: 'static> ParameterValueType for TypedValue<K> {
    type Image = Placeholder<TagImage>;
    type Audio = Placeholder<TagAudio>;
    type Video = (Placeholder<TagImage>, Placeholder<TagAudio>);
    type File = (Option<Box<[String]>>, TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, PathBuf>);
    type String = (Option<Range<usize>>, TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, String>);
    type Select = (Box<[String]>, TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, usize>);
    type Boolean = TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, bool>;
    type Radio = (usize, TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, bool>);
    type Integer = (Option<Range<i64>>, TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, i64>);
    type RealNumber = (Option<Range<f64>>, TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, EasingValue<f64>>);
    type Vec2 = (Option<Range<Vector2<f64>>>, Vector2<TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, EasingValue<f64>>>);
    type Vec3 = (Option<Range<Vector3<f64>>>, Vector3<TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, EasingValue<f64>>>);
    type Dictionary = (HashMap<String, Parameter<Type>>, TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, HashMap<String, ParameterValue<K>>>);
    type ComponentClass = ();
}

pub struct ValueFixed;

pub type ParameterValueFixed = Parameter<ValueFixed>;

impl ParameterValueType for ValueFixed {
    type Image = Placeholder<TagImage>;
    type Audio = Placeholder<TagAudio>;
    type Video = (Placeholder<TagImage>, Placeholder<TagAudio>);
    type File = PathBuf;
    type String = String;
    type Select = usize;
    type Boolean = bool;
    type Radio = bool;
    type Integer = i64;
    type RealNumber = f64;
    type Vec2 = Vector2<f64>;
    type Vec3 = Vector3<f64>;
    type Dictionary = HashMap<String, ParameterValueFixed>;
    type ComponentClass = ();
}

pub struct ValueFixedExceptComponentClass;

pub type ParameterValueFixedExceptComponentClass = Parameter<ValueFixedExceptComponentClass>;

impl ParameterValueType for ValueFixedExceptComponentClass {
    type Image = Placeholder<TagImage>;
    type Audio = Placeholder<TagAudio>;
    type Video = (Placeholder<TagImage>, Placeholder<TagAudio>);
    type File = PathBuf;
    type String = String;
    type Select = usize;
    type Boolean = bool;
    type Radio = bool;
    type Integer = i64;
    type RealNumber = f64;
    type Vec2 = Vector2<f64>;
    type Vec3 = Vector3<f64>;
    type Dictionary = HashMap<String, ParameterValueFixedExceptComponentClass>;
    type ComponentClass = Never;
}

pub struct ParameterSelect;

impl ParameterValueType for ParameterSelect {
    type Image = ();
    type Audio = ();
    type Video = ();
    type File = ();
    type String = ();
    type Select = ();
    type Boolean = ();
    type Radio = ();
    type Integer = ();
    type RealNumber = ();
    type Vec2 = ();
    type Vec3 = ();
    type Dictionary = ();
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
        self.0.partial_cmp(&other.0)
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
#[derive(Debug, Clone, Copy, Default)]
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
#[derive(Debug, Clone, Copy, Default)]
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

#[derive(Debug, Clone)]
pub enum VariableParameterPriority {
    PrioritizeManually,
    PrioritizeComponent,
}

type PinSplitValue<K, T> = TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, T>;

pub enum VariableParameterValue<K: 'static, T, Manually, Nullable> {
    Manually(Manually),
    MayComponent {
        params: Nullable,
        components: Vec<StaticPointer<TCell<K, ComponentInstance<K, T>>>>,
        priority: VariableParameterPriority,
    },
}

impl<K, T, Manually: Debug, Nullable: Debug> Debug for VariableParameterValue<K, T, Manually, Nullable> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VariableParameterValue::Manually(manually) => f.debug_tuple("Manually").field(manually).finish(),
            VariableParameterValue::MayComponent { params, components, priority } => f.debug_struct("MayComponent").field("params", params).field("components", components).field("priority", priority).finish(),
        }
    }
}

impl<K, T, Manually: Clone, Nullable: Clone> Clone for VariableParameterValue<K, T, Manually, Nullable> {
    fn clone(&self) -> Self {
        match self {
            VariableParameterValue::Manually(value) => VariableParameterValue::Manually(value.clone()),
            VariableParameterValue::MayComponent { params, components, priority } => VariableParameterValue::MayComponent {
                params: params.clone(),
                components: components.clone(),
                priority: priority.clone(),
            },
        }
    }
}

#[derive(Debug)]
pub struct ImageRequiredParams<K: 'static, T> {
    pub aspect_ratio: (u32, u32),
    pub transform: ImageRequiredParamsTransform<K, T>,
    pub background_color: [u8; 4],
    pub opacity: PinSplitValue<K, EasingValue<f64>>,
    pub blend_mode: PinSplitValue<K, BlendMode>,
    pub composite_operation: PinSplitValue<K, CompositeOperation>,
}

impl<K, T> ImageRequiredParams<K, T> {
    pub fn new_default(marker_left: &StaticPointer<TCell<K, MarkerPin>>, marker_right: &StaticPointer<TCell<K, MarkerPin>>) -> ImageRequiredParams<K, T> {
        let one = TimeSplitValue::new(marker_left.clone(), EasingValue { from: 1., to: 1., easing: Arc::new(DefaultEasing) }, marker_right.clone());
        let one_value = VariableParameterValue::Manually(one);
        let zero = VariableParameterValue::Manually(TimeSplitValue::new(marker_left.clone(), EasingValue { from: 0., to: 0., easing: Arc::new(DefaultEasing) }, marker_right.clone()));
        ImageRequiredParams {
            aspect_ratio: (1, 1),
            transform: ImageRequiredParamsTransform::Params {
                scale: Vector3 {
                    x: one_value.clone(),
                    y: one_value.clone(),
                    z: one_value,
                },
                translate: Vector3 { x: zero.clone(), y: zero.clone(), z: zero.clone() },
                rotate: TimeSplitValue::new(
                    marker_left.clone(),
                    EasingValue {
                        from: Quaternion::one(),
                        to: Quaternion::one(),
                        easing: Arc::new(DefaultEasing),
                    },
                    marker_right.clone(),
                ),
                scale_center: Vector3 { x: zero.clone(), y: zero.clone(), z: zero.clone() },
                rotate_center: Vector3 { x: zero.clone(), y: zero.clone(), z: zero },
            },
            background_color: [0; 4],
            opacity: TimeSplitValue::new(marker_left.clone(), EasingValue { from: 1., to: 1., easing: Arc::new(DefaultEasing) }, marker_right.clone()),
            blend_mode: TimeSplitValue::new(marker_left.clone(), Default::default(), marker_right.clone()),
            composite_operation: TimeSplitValue::new(marker_left.clone(), Default::default(), marker_right.clone()),
        }
    }
}

impl<K, T> Clone for ImageRequiredParams<K, T> {
    fn clone(&self) -> Self {
        let ImageRequiredParams {
            aspect_ratio,
            transform,
            background_color,
            opacity,
            blend_mode,
            composite_operation,
        } = self;
        ImageRequiredParams {
            aspect_ratio: *aspect_ratio,
            transform: transform.clone(),
            background_color: *background_color,
            opacity: opacity.clone(),
            blend_mode: blend_mode.clone(),
            composite_operation: composite_operation.clone(),
        }
    }
}

pub type Vector3Params<K, T> = Vector3<VariableParameterValue<K, T, PinSplitValue<K, EasingValue<f64>>, PinSplitValue<K, Option<EasingValue<f64>>>>>;

#[derive(Debug)]
pub enum ImageRequiredParamsTransform<K: 'static, T> {
    Params {
        scale: Vector3Params<K, T>,
        translate: Vector3Params<K, T>,
        rotate: TimeSplitValue<StaticPointer<TCell<K, MarkerPin>>, EasingValue<Quaternion<f64>>>,
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

impl<K, T> Clone for ImageRequiredParamsTransform<K, T> {
    fn clone(&self) -> Self {
        match self {
            ImageRequiredParamsTransform::Params { scale, translate, rotate, scale_center, rotate_center } => ImageRequiredParamsTransform::Params {
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
pub struct ImageRequiredParamsFrameVariable {
    pub aspect_ratio: (u32, u32),
    pub transform: ImageRequiredParamsTransformFrameVariable,
    pub background_color: [u8; 4],
    pub opacity: FrameVariableValue<Opacity>,
    pub blend_mode: FrameVariableValue<BlendMode>,
    pub composite_operation: FrameVariableValue<CompositeOperation>,
}

impl ImageRequiredParamsFrameVariable {
    pub fn get(&self, at: TimelineTime) -> ImageRequiredParamsFixed {
        let ImageRequiredParamsFrameVariable {
            aspect_ratio,
            transform,
            background_color,
            opacity,
            blend_mode,
            composite_operation,
        } = self;
        ImageRequiredParamsFixed {
            aspect_ratio: *aspect_ratio,
            transform: transform.get(at),
            background_color: *background_color,
            opacity: *opacity.get(at).unwrap(),
            blend_mode: *blend_mode.get(at).unwrap(),
            composite_operation: *composite_operation.get(at).unwrap(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ImageRequiredParamsTransformFrameVariable {
    Params {
        scale: FrameVariableValue<Vector3<f64>>,
        translate: FrameVariableValue<Vector3<f64>>,
        rotate: FrameVariableValue<Quaternion<f64>>,
        scale_center: FrameVariableValue<Vector3<f64>>,
        rotate_center: FrameVariableValue<Vector3<f64>>,
    },
    Free {
        left_top: FrameVariableValue<Vector3<f64>>,
        right_top: FrameVariableValue<Vector3<f64>>,
        left_bottom: FrameVariableValue<Vector3<f64>>,
        right_bottom: FrameVariableValue<Vector3<f64>>,
    },
}

impl ImageRequiredParamsTransformFrameVariable {
    pub fn get(&self, at: TimelineTime) -> ImageRequiredParamsTransformFixed {
        match self {
            ImageRequiredParamsTransformFrameVariable::Params { scale, translate, rotate, scale_center, rotate_center } => ImageRequiredParamsTransformFixed::Params {
                scale: *scale.get(at).unwrap(),
                translate: *translate.get(at).unwrap(),
                rotate: *rotate.get(at).unwrap(),
                scale_center: *scale_center.get(at).unwrap(),
                rotate_center: *rotate_center.get(at).unwrap(),
            },
            ImageRequiredParamsTransformFrameVariable::Free { left_top, right_top, left_bottom, right_bottom } => ImageRequiredParamsTransformFixed::Free {
                left_top: *left_top.get(at).unwrap(),
                right_top: *right_top.get(at).unwrap(),
                left_bottom: *left_bottom.get(at).unwrap(),
                right_bottom: *right_bottom.get(at).unwrap(),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageRequiredParamsFixed {
    pub aspect_ratio: (u32, u32),
    pub transform: ImageRequiredParamsTransformFixed,
    pub background_color: [u8; 4],
    pub opacity: Opacity,
    pub blend_mode: BlendMode,
    pub composite_operation: CompositeOperation,
}

#[derive(Debug, Clone)]
pub enum ImageRequiredParamsTransformFixed {
    Params {
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

#[derive(Debug)]
pub struct AudioRequiredParams<K: 'static, T> {
    pub volume: Vec<VariableParameterValue<K, T, PinSplitValue<K, EasingValue<f64>>, PinSplitValue<K, Option<EasingValue<f64>>>>>,
}

impl<K, T> Clone for AudioRequiredParams<K, T> {
    fn clone(&self) -> Self {
        let AudioRequiredParams { volume } = self;
        AudioRequiredParams { volume: volume.clone() }
    }
}

#[derive(Debug, Clone)]
pub struct AudioRequiredParamsFrameVariable {
    pub volume: Vec<FrameVariableValue<f64>>,
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
