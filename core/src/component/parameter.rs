use crate::component::marker_pin::MarkerPin;
use crate::component::parameter::placeholder::{AudioPlaceholder, ImagePlaceholder};
use crate::component::parameter::value::{EasingValue, TimeSplitValue};
use crate::ptr::StaticPointer;
use cgmath::{Vector2, Vector3};
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;

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
    File(TimeSplitValue<StaticPointer<MarkerPin>, PathBuf>),
    String(TimeSplitValue<StaticPointer<MarkerPin>, String>),
    Boolean(TimeSplitValue<StaticPointer<MarkerPin>, bool>),
    Integer(TimeSplitValue<StaticPointer<MarkerPin>, i64>),
    RealNumber(TimeSplitValue<StaticPointer<MarkerPin>, EasingValue<f64>>),
    Vec2(TimeSplitValue<StaticPointer<MarkerPin>, EasingValue<Vector2<f64>>>),
    Vec3(TimeSplitValue<StaticPointer<MarkerPin>, EasingValue<Vector3<f64>>>),
    Dictionary(TimeSplitValue<StaticPointer<MarkerPin>, HashMap<String, ParameterValue>>),
    ComponentClass(/* TODO */),
}

pub struct ImageRequiredParams {/* TODO */}

pub struct AudioRequiredParams {/* TODO */}
