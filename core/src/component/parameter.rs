use crate::common::time_split_value::TimeSplitValue;
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

pub struct ImageRequiredParams {/* TODO */}

pub struct AudioRequiredParams {/* TODO */}
