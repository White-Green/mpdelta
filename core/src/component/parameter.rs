use cgmath::{Vector2, Vector3};
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;

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

pub enum ParameterValue<Image, Audio> {
    Image(Image),
    Audio(Audio),
    File(PathBuf),
    String(String),
    Boolean(bool),
    Integer(i64),
    RealNumber(f64),
    Vec2(Vector2<f64>),
    Vec3(Vector3<f64>),
    Dictionary(HashMap<String, ParameterValue<Image, Audio>>),
    ComponentClass(/* TODO */),
}

pub struct ImageRequiredParams {/* TODO */}

pub struct AudioRequiredParams {/* TODO */}
