use crate::common::general_lifetime::AsGeneralLifetime;
use crate::common::time_split_value::{Immutable, Mutable, TimeSplitValue, TimeSplitValueView};
use crate::component::marker_pin::MarkerPin;
use crate::component::parameter::placeholder::{AudioPlaceholder, ImagePlaceholder};
use crate::component::parameter::value::EasingValue;
use crate::ptr::StaticPointer;
use cgmath::{Vector2, Vector3};
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::RwLock;

pub mod placeholder;
pub mod value;

pub trait ParameterValueType<'a> {
    type Image: 'a;
    type Audio: 'a;
    type Video: 'a;
    type File: 'a;
    type String: 'a;
    type Boolean: 'a;
    type Integer: 'a;
    type RealNumber: 'a;
    type Vec2: 'a;
    type Vec3: 'a;
    type Dictionary: 'a;
    type ComponentClass: 'a;
}

pub enum Parameter<'a, Type: ParameterValueType<'a>> {
    Image(Type::Image),
    Audio(Type::Audio),
    Video(Type::Video),
    File(Type::File),
    String(Type::String),
    Boolean(Type::Boolean),
    Integer(Type::Integer),
    RealNumber(Type::RealNumber),
    Vec2(Type::Vec2),
    Vec3(Type::Vec3),
    Dictionary(Type::Dictionary),
    ComponentClass(Type::ComponentClass),
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
    type Boolean = ();
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
    type Boolean = ();
    type Integer = Option<Range<i64>>;
    type RealNumber = Option<Range<f64>>;
    type Vec2 = Option<Range<Vector2<f64>>>;
    type Vec3 = Option<Range<Vector3<f64>>>;
    type Dictionary = HashMap<String, Parameter<'a, Type>>;
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
    type Boolean = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, bool>;
    type Integer = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, i64>;
    type RealNumber = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, EasingValue<f64>>;
    type Vec2 = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, EasingValue<Vector2<f64>>>;
    type Vec3 = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, EasingValue<Vector3<f64>>>;
    type Dictionary = TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, HashMap<String, ParameterValue>>;
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
    type Boolean = bool;
    type Integer = i64;
    type RealNumber = f64;
    type Vec2 = Vector2<f64>;
    type Vec3 = Vector3<f64>;
    type Dictionary = HashMap<String, ParameterValue>;
    type ComponentClass = ();
}

pub struct ValueFixedExceptComponentClass;
pub type ParameterValueFixedExceptComponentClass = Parameter<'static, ValueFixed>;

impl<'a> ParameterValueType<'a> for ValueFixedExceptComponentClass {
    type Image = ImagePlaceholder;
    type Audio = AudioPlaceholder;
    type Video = (ImagePlaceholder, AudioPlaceholder);
    type File = PathBuf;
    type String = String;
    type Boolean = bool;
    type Integer = i64;
    type RealNumber = f64;
    type Vec2 = Vector2<f64>;
    type Vec3 = Vector3<f64>;
    type Dictionary = HashMap<String, ParameterValue>;
    type ComponentClass = Never;
}

pub struct ValueViewForFix;
pub type ParameterValueViewForFix<'a> = Parameter<'a, ValueViewForFix>;

impl<'a> ParameterValueType<'a> for ValueViewForFix {
    type Image = ImagePlaceholder;
    type Audio = AudioPlaceholder;
    type Video = (ImagePlaceholder, AudioPlaceholder);
    type File = TimeSplitValueView<'a, StaticPointer<RwLock<MarkerPin>>, PathBuf, Immutable, Mutable>;
    type String = TimeSplitValueView<'a, StaticPointer<RwLock<MarkerPin>>, String, Immutable, Mutable>;
    type Boolean = TimeSplitValueView<'a, StaticPointer<RwLock<MarkerPin>>, bool, Immutable, Mutable>;
    type Integer = TimeSplitValueView<'a, StaticPointer<RwLock<MarkerPin>>, i64, Immutable, Mutable>;
    type RealNumber = TimeSplitValueView<'a, StaticPointer<RwLock<MarkerPin>>, EasingValue<f64>, Immutable, Mutable>;
    type Vec2 = TimeSplitValueView<'a, StaticPointer<RwLock<MarkerPin>>, EasingValue<Vector2<f64>>, Immutable, Mutable>;
    type Vec3 = TimeSplitValueView<'a, StaticPointer<RwLock<MarkerPin>>, EasingValue<Vector3<f64>>, Immutable, Mutable>;
    type Dictionary = TimeSplitValueView<'a, StaticPointer<RwLock<MarkerPin>>, HashMap<String, ParameterValue>, Immutable, Mutable>;
    type ComponentClass = ();
}

pub struct ImageRequiredParams {/* TODO */}

pub struct AudioRequiredParams {/* TODO */}

impl<'a> From<&'a mut ParameterValue> for ParameterValueViewForFix<'a> {
    fn from(value: &'a mut ParameterValue) -> Self {
        match value {
            ParameterValue::Image(a) => ParameterValueViewForFix::Image(*a),
            ParameterValue::Audio(a) => ParameterValueViewForFix::Audio(*a),
            ParameterValue::Video(a) => ParameterValueViewForFix::Video(*a),
            ParameterValue::File(a) => ParameterValueViewForFix::File(TimeSplitValueView::new(a)),
            ParameterValue::String(a) => ParameterValueViewForFix::String(TimeSplitValueView::new(a)),
            ParameterValue::Boolean(a) => ParameterValueViewForFix::Boolean(TimeSplitValueView::new(a)),
            ParameterValue::Integer(a) => ParameterValueViewForFix::Integer(TimeSplitValueView::new(a)),
            ParameterValue::RealNumber(a) => ParameterValueViewForFix::RealNumber(TimeSplitValueView::new(a)),
            ParameterValue::Vec2(a) => ParameterValueViewForFix::Vec2(TimeSplitValueView::new(a)),
            ParameterValue::Vec3(a) => ParameterValueViewForFix::Vec3(TimeSplitValueView::new(a)),
            ParameterValue::Dictionary(a) => ParameterValueViewForFix::Dictionary(TimeSplitValueView::new(a)),
            ParameterValue::ComponentClass(a) => ParameterValueViewForFix::ComponentClass(*a),
        }
    }
}

impl<'a: 'b, 'b> AsGeneralLifetime<'b> for ParameterValueViewForFix<'a> {
    type GeneralLifetimeType = ParameterValueViewForFix<'b>;
}
