use crate::common::time_split_value::TimeSplitValue;
use crate::component::instance::ComponentInstance;
use crate::component::marker_pin::MarkerPin;
use crate::component::parameter::placeholder::{Placeholder, TagAudio, TagImage};
use crate::component::parameter::value::{EasingValue, FrameVariableValue};
use crate::ptr::StaticPointer;
use crate::time::TimelineTime;
use cgmath::{Quaternion, Vector2, Vector3};
use either::Either;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::PathBuf;
use tokio::sync::RwLock;

pub mod placeholder;
pub mod value;

pub trait ParameterValueType<'a>: 'a {
    type Image: 'a + Clone + Send + Sync;
    type Audio: 'a + Clone + Send + Sync;
    type Video: 'a + Clone + Send + Sync;
    type File: 'a + Clone + Send + Sync;
    type String: 'a + Clone + Send + Sync;
    type Select: 'a + Clone + Send + Sync;
    type Boolean: 'a + Clone + Send + Sync;
    type Radio: 'a + Clone + Send + Sync;
    type Integer: 'a + Clone + Send + Sync;
    type RealNumber: 'a + Clone + Send + Sync;
    type Vec2: 'a + Clone + Send + Sync;
    type Vec3: 'a + Clone + Send + Sync;
    type Dictionary: 'a + Clone + Send + Sync;
    type ComponentClass: 'a + Clone + Send + Sync;
}

impl<'a, A, B> ParameterValueType<'a> for (A, B)
where
    A: ParameterValueType<'a>,
    B: ParameterValueType<'a>,
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

pub enum Parameter<'a, Type: ParameterValueType<'a>> {
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

impl<'a, Type: ParameterValueType<'a>> Parameter<'a, Type> {
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
    pub fn equals_type<'b, Type2: ParameterValueType<'b>>(&self, other: &Parameter<'b, Type2>) -> bool {
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
}

impl<'a, Type: ParameterValueType<'a>> Clone for Parameter<'a, Type> {
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

impl<'a, Type: ParameterValueType<'a>> Debug for Parameter<'a, Type> {
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

#[derive(Debug, Copy, Clone)]
pub enum Never {}

pub struct Type;

pub type ParameterType = Parameter<'static, Type>;

impl<'a> ParameterValueType<'a> for Type {
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
    type Dictionary = HashMap<String, Parameter<'a, Type>>;
    type ComponentClass = ();
}

pub struct TypeExceptComponentClass;

pub type ParameterTypeExceptComponentClass = Parameter<'static, TypeExceptComponentClass>;

impl<'a> ParameterValueType<'a> for TypeExceptComponentClass {
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
    type Dictionary = HashMap<String, Parameter<'a, TypeExceptComponentClass>>;
    type ComponentClass = Never;
}

pub struct Value;

pub type ParameterValue = Parameter<'static, Value>;

impl<'a> ParameterValueType<'a> for Value {
    type Image = Placeholder<TagImage>;
    type Audio = Placeholder<TagAudio>;
    type Video = (Placeholder<TagImage>, Placeholder<TagAudio>);
    type File = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, PathBuf>;
    type String = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, String>;
    type Select = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, usize>;
    type Boolean = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, bool>;
    type Radio = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, bool>;
    type Integer = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, i64>;
    type RealNumber = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, EasingValue<f64>>;
    type Vec2 = Vector2<TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, EasingValue<f64>>>;
    type Vec3 = Vector3<TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, EasingValue<f64>>>;
    type Dictionary = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, HashMap<String, ParameterValue>>;
    type ComponentClass = ();
}

#[derive(Clone)]
pub struct ComponentProcessorInput;

pub type ComponentProcessorInputValue = Parameter<'static, ComponentProcessorInput>;

impl<'a> ParameterValueType<'a> for ComponentProcessorInput {
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
    type Dictionary = TimeSplitValue<TimelineTime, Option<Either<HashMap<String, ComponentProcessorInputValue>, FrameVariableValue<HashMap<String, ComponentProcessorInputValue>>>>>;
    type ComponentClass = ();
}

pub struct FrameVariable;

pub type ParameterFrameVariableValue = Parameter<'static, FrameVariable>;

impl<'a> ParameterValueType<'a> for FrameVariable {
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
    type Dictionary = FrameVariableValue<HashMap<String, ParameterFrameVariableValue>>;
    type ComponentClass = Never;
}

pub struct NullableValue;

pub type ParameterNullableValue = Parameter<'static, NullableValue>;

impl<'a> ParameterValueType<'a> for NullableValue {
    type Image = Option<Placeholder<TagImage>>;
    type Audio = Option<Placeholder<TagAudio>>;
    type Video = Option<(Placeholder<TagImage>, Placeholder<TagAudio>)>;
    type File = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<PathBuf>>;
    type String = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<String>>;
    type Select = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<usize>>;
    type Boolean = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<bool>>;
    type Radio = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<bool>>;
    type Integer = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<i64>>;
    type RealNumber = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<EasingValue<f64>>>;
    type Vec2 = Vector2<TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<EasingValue<f64>>>>;
    type Vec3 = Vector3<TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<EasingValue<f64>>>>;
    type Dictionary = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<HashMap<String, ParameterNullableValue>>>;
    type ComponentClass = Option<()>;
}

pub struct TypedValue;

pub type ParameterTypedValue = Parameter<'static, TypedValue>;

impl<'a> ParameterValueType<'a> for TypedValue {
    type Image = Placeholder<TagImage>;
    type Audio = Placeholder<TagAudio>;
    type Video = (Placeholder<TagImage>, Placeholder<TagAudio>);
    type File = (Option<Box<[String]>>, TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, PathBuf>);
    type String = (Option<Range<usize>>, TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, String>);
    type Select = (Box<[String]>, TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, usize>);
    type Boolean = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, bool>;
    type Radio = (usize, TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, bool>);
    type Integer = (Option<Range<i64>>, TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, i64>);
    type RealNumber = (Option<Range<f64>>, TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, EasingValue<f64>>);
    type Vec2 = (Option<Range<Vector2<f64>>>, Vector2<TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, EasingValue<f64>>>);
    type Vec3 = (Option<Range<Vector3<f64>>>, Vector3<TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, EasingValue<f64>>>);
    type Dictionary = (HashMap<String, Parameter<'a, Type>>, TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, HashMap<String, ParameterValue>>);
    type ComponentClass = ();
}

pub struct ValueFixed;

pub type ParameterValueFixed = Parameter<'static, ValueFixed>;

impl<'a> ParameterValueType<'a> for ValueFixed {
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

pub type ParameterValueFixedExceptComponentClass = Parameter<'static, ValueFixedExceptComponentClass>;

impl<'a> ParameterValueType<'a> for ValueFixedExceptComponentClass {
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

impl<'a> ParameterValueType<'a> for ParameterSelect {
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

type PinSplitValue<T> = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, T>;

#[derive(Debug)]
pub enum VariableParameterValue<T, Manually, Nullable> {
    Manually(Manually),
    MayComponent {
        params: Nullable,
        components: Vec<StaticPointer<RwLock<ComponentInstance<T>>>>,
        priority: VariableParameterPriority,
    },
}

impl<T, Manually: Clone, Nullable: Clone> Clone for VariableParameterValue<T, Manually, Nullable> {
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
pub struct ImageRequiredParams<T> {
    pub aspect_ratio: (u32, u32),
    pub transform: ImageRequiredParamsTransform<T>,
    pub opacity: PinSplitValue<EasingValue<Opacity>>,
    pub blend_mode: PinSplitValue<BlendMode>,
    pub composite_operation: PinSplitValue<CompositeOperation>,
}

impl<T> Clone for ImageRequiredParams<T> {
    fn clone(&self) -> Self {
        let ImageRequiredParams {
            aspect_ratio,
            transform,
            opacity,
            blend_mode,
            composite_operation,
        } = self;
        ImageRequiredParams {
            aspect_ratio: *aspect_ratio,
            transform: transform.clone(),
            opacity: opacity.clone(),
            blend_mode: blend_mode.clone(),
            composite_operation: composite_operation.clone(),
        }
    }
}

#[derive(Debug)]
pub enum ImageRequiredParamsTransform<T> {
    Params {
        scale: Vector3<VariableParameterValue<T, PinSplitValue<EasingValue<f64>>, PinSplitValue<Option<EasingValue<f64>>>>>,
        translate: Vector3<VariableParameterValue<T, PinSplitValue<EasingValue<f64>>, PinSplitValue<Option<EasingValue<f64>>>>>,
        rotate: TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, EasingValue<Quaternion<f64>>>,
        scale_center: Vector3<VariableParameterValue<T, PinSplitValue<EasingValue<f64>>, PinSplitValue<Option<EasingValue<f64>>>>>,
        rotate_center: Vector3<VariableParameterValue<T, PinSplitValue<EasingValue<f64>>, PinSplitValue<Option<EasingValue<f64>>>>>,
    },
    Free {
        left_top: Vector3<VariableParameterValue<T, PinSplitValue<EasingValue<f64>>, PinSplitValue<Option<EasingValue<f64>>>>>,
        right_top: Vector3<VariableParameterValue<T, PinSplitValue<EasingValue<f64>>, PinSplitValue<Option<EasingValue<f64>>>>>,
        left_bottom: Vector3<VariableParameterValue<T, PinSplitValue<EasingValue<f64>>, PinSplitValue<Option<EasingValue<f64>>>>>,
        right_bottom: Vector3<VariableParameterValue<T, PinSplitValue<EasingValue<f64>>, PinSplitValue<Option<EasingValue<f64>>>>>,
    },
}

impl<T> Clone for ImageRequiredParamsTransform<T> {
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
    pub opacity: FrameVariableValue<Opacity>,
    pub blend_mode: FrameVariableValue<BlendMode>,
    pub composite_operation: FrameVariableValue<CompositeOperation>,
}

impl ImageRequiredParamsFrameVariable {
    pub fn get(&self, at: TimelineTime) -> ImageRequiredParamsFixed {
        let ImageRequiredParamsFrameVariable {
            aspect_ratio,
            transform,
            opacity,
            blend_mode,
            composite_operation,
        } = self;
        ImageRequiredParamsFixed {
            aspect_ratio: *aspect_ratio,
            transform: transform.get(at),
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

#[derive(Debug)]
pub struct ImageRequiredParamsFixed {
    pub aspect_ratio: (u32, u32),
    pub transform: ImageRequiredParamsTransformFixed,
    pub opacity: Opacity,
    pub blend_mode: BlendMode,
    pub composite_operation: CompositeOperation,
}

#[derive(Debug)]
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
pub struct AudioRequiredParams<T> {
    pub volume: Vec<VariableParameterValue<T, PinSplitValue<EasingValue<f64>>, PinSplitValue<Option<EasingValue<f64>>>>>,
}

impl<T> Clone for AudioRequiredParams<T> {
    fn clone(&self) -> Self {
        let AudioRequiredParams { volume } = self;
        AudioRequiredParams { volume: volume.clone() }
    }
}

#[derive(Debug)]
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
