use crate::common::time_split_value::TimeSplitValue;
use crate::component::parameter::placeholder::{AudioPlaceholder, ImagePlaceholder};
use crate::component::parameter::value::EasingValue;
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

pub enum ParameterValue<SplitBy> {
    Image(ImagePlaceholder),
    Audio(AudioPlaceholder),
    File(TimeSplitValue<SplitBy, PathBuf>),
    String(TimeSplitValue<SplitBy, String>),
    Boolean(TimeSplitValue<SplitBy, bool>),
    Integer(TimeSplitValue<SplitBy, i64>),
    RealNumber(TimeSplitValue<SplitBy, EasingValue<f64>>),
    Vec2(TimeSplitValue<SplitBy, EasingValue<Vector2<f64>>>),
    Vec3(TimeSplitValue<SplitBy, EasingValue<Vector3<f64>>>),
    Dictionary(TimeSplitValue<SplitBy, HashMap<String, ParameterValue<SplitBy>>>),
    ComponentClass(/* TODO */),
}

pub struct ImageRequiredParams {/* TODO */}

pub struct AudioRequiredParams {/* TODO */}
