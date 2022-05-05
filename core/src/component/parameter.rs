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

pub enum ParameterType {
    Image,
    Audio,
    Video,
    File { extension_filter: Option<Box<[String]>> },
    String { length_range: Option<Range<usize>> },
    Boolean,
    Integer { range: Option<Range<i64>> },
    RealNumber { range: Option<Range<f64>> },
    Vec2 { range: Option<Range<Vector2<f64>>> },
    Vec3 { range: Option<Range<Vector3<f64>>> },
    Dictionary(HashMap<String, ParameterType>),
    ComponentClass {/* TODO */},
}

pub enum ParameterValue {
    Image(ImagePlaceholder),
    Audio(AudioPlaceholder),
    File(TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, PathBuf>),
    String(TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, String>),
    Boolean(TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, bool>),
    Integer(TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, i64>),
    RealNumber(TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, EasingValue<f64>>),
    Vec2(TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, EasingValue<Vector2<f64>>>),
    Vec3(TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, EasingValue<Vector3<f64>>>),
    Dictionary(TimeSplitValue<StaticPointer<RwLock<MarkerPin>>, HashMap<String, ParameterValue>>),
    ComponentClass(/* TODO */),
}

pub enum ParameterValueViewForFix<'a> {
    Image(ImagePlaceholder),
    Audio(AudioPlaceholder),
    File(TimeSplitValueView<'a, StaticPointer<RwLock<MarkerPin>>, PathBuf, Immutable, Mutable>),
    String(TimeSplitValueView<'a, StaticPointer<RwLock<MarkerPin>>, String, Immutable, Mutable>),
    Boolean(TimeSplitValueView<'a, StaticPointer<RwLock<MarkerPin>>, bool, Immutable, Mutable>),
    Integer(TimeSplitValueView<'a, StaticPointer<RwLock<MarkerPin>>, i64, Immutable, Mutable>),
    RealNumber(TimeSplitValueView<'a, StaticPointer<RwLock<MarkerPin>>, EasingValue<f64>, Immutable, Mutable>),
    Vec2(TimeSplitValueView<'a, StaticPointer<RwLock<MarkerPin>>, EasingValue<Vector2<f64>>, Immutable, Mutable>),
    Vec3(TimeSplitValueView<'a, StaticPointer<RwLock<MarkerPin>>, EasingValue<Vector3<f64>>, Immutable, Mutable>),
    Dictionary(TimeSplitValueView<'a, StaticPointer<RwLock<MarkerPin>>, HashMap<String, ParameterValue>, Immutable, Mutable>),
    ComponentClass(/* TODO */),
}

pub struct ImageRequiredParams {/* TODO */}

pub struct AudioRequiredParams {/* TODO */}

impl<'a> From<&'a mut ParameterValue> for ParameterValueViewForFix<'a> {
    fn from(value: &'a mut ParameterValue) -> Self {
        match value {
            ParameterValue::Image(a) => ParameterValueViewForFix::Image(*a),
            ParameterValue::Audio(a) => ParameterValueViewForFix::Audio(*a),
            ParameterValue::File(a) => ParameterValueViewForFix::File(TimeSplitValueView::new(a)),
            ParameterValue::String(a) => ParameterValueViewForFix::String(TimeSplitValueView::new(a)),
            ParameterValue::Boolean(a) => ParameterValueViewForFix::Boolean(TimeSplitValueView::new(a)),
            ParameterValue::Integer(a) => ParameterValueViewForFix::Integer(TimeSplitValueView::new(a)),
            ParameterValue::RealNumber(a) => ParameterValueViewForFix::RealNumber(TimeSplitValueView::new(a)),
            ParameterValue::Vec2(a) => ParameterValueViewForFix::Vec2(TimeSplitValueView::new(a)),
            ParameterValue::Vec3(a) => ParameterValueViewForFix::Vec3(TimeSplitValueView::new(a)),
            ParameterValue::Dictionary(a) => ParameterValueViewForFix::Dictionary(TimeSplitValueView::new(a)),
            ParameterValue::ComponentClass() => ParameterValueViewForFix::ComponentClass(),
        }
    }
}

impl<'a: 'b, 'b> AsGeneralLifetime<'b> for ParameterValueViewForFix<'a> {
    type GeneralLifetimeType = ParameterValueViewForFix<'b>;

    fn into_general_lifetime(self) -> Self::GeneralLifetimeType {
        self
    }
}
