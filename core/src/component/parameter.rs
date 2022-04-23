use crate::component::parameter::placeholder::{AudioPlaceholder, ImagePlaceholder};
use crate::component::parameter::value::TimedValue;
use cgmath::{Vector2, Vector3};
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

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
    File(Arc<dyn TimedValue<PathBuf>>),
    String(Arc<dyn TimedValue<String>>),
    Boolean(Arc<dyn TimedValue<bool>>),
    Integer(Arc<dyn TimedValue<i64>>),
    RealNumber(Arc<dyn TimedValue<f64>>),
    Vec2(Arc<dyn TimedValue<Vector2<f64>>>),
    Vec3(Arc<dyn TimedValue<Vector3<f64>>>),
    Dictionary(Arc<dyn TimedValue<HashMap<String, ParameterValue>>>),
    ComponentClass(/* TODO */),
}

pub struct ImageRequiredParams {/* TODO */}

pub struct AudioRequiredParams {/* TODO */}
