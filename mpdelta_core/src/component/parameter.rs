use crate::common::time_split_value::TimeSplitValue;
use crate::component::instance::ComponentInstance;
use crate::component::marker_pin::MarkerPin;
use crate::component::parameter::placeholder::{AudioPlaceholder, ImagePlaceholder};
use crate::component::parameter::value::EasingValue;
use crate::ptr::StaticPointer;
use cgmath::{Quaternion, Vector2, Vector3};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::PathBuf;
use tokio::sync::RwLock;

pub mod placeholder;
pub mod value;

pub trait ParameterValueType<'a> {
    type Image: 'a;
    type Audio: 'a;
    type Video: 'a;
    type File: 'a;
    type String: 'a;
    type Select: 'a;
    type Boolean: 'a;
    type Radio: 'a;
    type Integer: 'a;
    type RealNumber: 'a;
    type Vec2: 'a;
    type Vec3: 'a;
    type Dictionary: 'a;
    type ComponentClass: 'a;
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
    type Image = ImagePlaceholder;
    type Audio = AudioPlaceholder;
    type Video = (ImagePlaceholder, AudioPlaceholder);
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

pub struct NullableValue;

pub type ParameterNullableValue = Parameter<'static, NullableValue>;

impl<'a> ParameterValueType<'a> for NullableValue {
    type Image = Option<ImagePlaceholder>;
    type Audio = Option<AudioPlaceholder>;
    type Video = Option<(ImagePlaceholder, AudioPlaceholder)>;
    type File = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<PathBuf>>;
    type String = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<String>>;
    type Select = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<usize>>;
    type Boolean = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<bool>>;
    type Radio = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<bool>>;
    type Integer = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<i64>>;
    type RealNumber = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<EasingValue<f64>>>;
    type Vec2 = Vector2<TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<EasingValue<f64>>>>;
    type Vec3 = Vector3<TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<EasingValue<f64>>>>;
    type Dictionary = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, Option<HashMap<String, ParameterValue>>>;
    type ComponentClass = Option<()>;
}

pub struct TypedValue;

pub type ParameterTypedValue = Parameter<'static, TypedValue>;

impl<'a> ParameterValueType<'a> for TypedValue {
    type Image = ImagePlaceholder;
    type Audio = AudioPlaceholder;
    type Video = (ImagePlaceholder, AudioPlaceholder);
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
    type Image = ImagePlaceholder;
    type Audio = AudioPlaceholder;
    type Video = (ImagePlaceholder, AudioPlaceholder);
    type File = PathBuf;
    type String = String;
    type Select = usize;
    type Boolean = bool;
    type Radio = bool;
    type Integer = i64;
    type RealNumber = f64;
    type Vec2 = Vector2<f64>;
    type Vec3 = Vector3<f64>;
    type Dictionary = HashMap<String, ParameterValue>;
    type ComponentClass = ();
}

pub struct ValueFixedExceptComponentClass;

pub type ParameterValueFixedExceptComponentClass = Parameter<'static, ValueFixedExceptComponentClass>;

impl<'a> ParameterValueType<'a> for ValueFixedExceptComponentClass {
    type Image = ImagePlaceholder;
    type Audio = AudioPlaceholder;
    type Video = (ImagePlaceholder, AudioPlaceholder);
    type File = PathBuf;
    type String = String;
    type Select = usize;
    type Boolean = bool;
    type Radio = bool;
    type Integer = i64;
    type RealNumber = f64;
    type Vec2 = Vector2<f64>;
    type Vec3 = Vector3<f64>;
    type Dictionary = HashMap<String, ParameterValue>;
    type ComponentClass = Never;
}

#[derive(Debug, Clone, Copy)]
pub struct Opacity(f64);

impl Opacity {
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

// ref: https://www.w3.org/TR/compositing-1/
#[derive(Debug)]
pub enum BlendMode {
    Clear,
    Copy,
    Destination,
    SourceOver,
    DestinationOver,
    SourceIn,
    DestinationIn,
    SourceOut,
    DestinationOut,
    SourceAtop,
    DestinationAtop,
    XOR,
    Lighter,
}

// ref: https://www.w3.org/TR/compositing-1/
#[derive(Debug)]
pub enum CompositeOperation {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

#[derive(Debug)]
pub enum VariableParameterPriority {
    PrioritizeManually,
    PrioritizeComponent,
}

type PinSplitValue<T> = TimeSplitValue<StaticPointer<MarkerPin>, T>;

#[derive(Debug)]
pub enum VariableParameterValue<T, Manually, Nullable> {
    Manually(Manually),
    MayComponent {
        params: Nullable,
        components: Vec<StaticPointer<ComponentInstance<T>>>,
        priority: VariableParameterPriority,
    },
}

#[derive(Debug)]
pub struct ImageRequiredParams<T> {
    transform: ImageRequiredParamsTransform<T>,
    opacity: VariableParameterValue<T, PinSplitValue<EasingValue<Opacity>>, PinSplitValue<Option<EasingValue<Opacity>>>>,
    blend_mode: VariableParameterValue<T, PinSplitValue<BlendMode>, PinSplitValue<Option<BlendMode>>>,
    composite_operation: VariableParameterValue<T, PinSplitValue<CompositeOperation>, PinSplitValue<Option<CompositeOperation>>>,
}

#[derive(Debug)]
pub enum ImageRequiredParamsTransform<T> {
    Params {
        scale: VariableParameterValue<T, Vector3<PinSplitValue<EasingValue<f64>>>, Vector3<PinSplitValue<Option<EasingValue<f64>>>>>,
        translate: VariableParameterValue<T, Vector3<PinSplitValue<EasingValue<f64>>>, Vector3<PinSplitValue<Option<EasingValue<f64>>>>>,
        rotate: VariableParameterValue<T, PinSplitValue<EasingValue<Quaternion<f64>>>, PinSplitValue<Option<EasingValue<Quaternion<f64>>>>>,
        scale_center: VariableParameterValue<T, Vector3<PinSplitValue<EasingValue<f64>>>, Vector3<PinSplitValue<Option<EasingValue<f64>>>>>,
        rotate_center: VariableParameterValue<T, Vector3<PinSplitValue<EasingValue<f64>>>, Vector3<PinSplitValue<Option<EasingValue<f64>>>>>,
    },
    Free {
        left_top: VariableParameterValue<T, Vector3<PinSplitValue<EasingValue<f64>>>, Vector3<PinSplitValue<Option<EasingValue<f64>>>>>,
        right_top: VariableParameterValue<T, Vector3<PinSplitValue<EasingValue<f64>>>, Vector3<PinSplitValue<Option<EasingValue<f64>>>>>,
        left_bottom: VariableParameterValue<T, Vector3<PinSplitValue<EasingValue<f64>>>, Vector3<PinSplitValue<Option<EasingValue<f64>>>>>,
        right_bottom: VariableParameterValue<T, Vector3<PinSplitValue<EasingValue<f64>>>, Vector3<PinSplitValue<Option<EasingValue<f64>>>>>,
    },
}

#[derive(Debug)]
pub struct ImageRequiredParamsFixed {
    transform: ImageRequiredParamsTransformFixed,
    opacity: Opacity,
    blend_mode: BlendMode,
    composite_operation: CompositeOperation,
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
