use crate::component::parameter::{AbstractFile, Never, Parameter, ParameterTypeExceptComponentClass, ParameterValueType};
use crate::time::TimelineTime;

use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::PathBuf;

pub trait NativeProcessor<T: ParameterValueType>: Send + Sync {
    fn parameter_type(&self) -> &[ParameterTypeExceptComponentClass];
    fn return_type(&self) -> &ParameterTypeExceptComponentClass;
    fn has_same_output(&self, time1: TimelineTime, time2: TimelineTime, params: &[ParameterNativeProcessorInputFixed<T::Image, T::Audio>]) -> bool;
    fn process(&self, time: TimelineTime, params: &[ParameterNativeProcessorInputFixed<T::Image, T::Audio>]) -> ParameterNativeProcessorInputFixed<T::Image, T::Audio>;
}

pub struct NativeProcessorInputFixed<Image, Audio>(PhantomData<(Image, Audio)>);

pub type ParameterNativeProcessorInputFixed<Image, Audio> = Parameter<NativeProcessorInputFixed<Image, Audio>>;

impl<Image: Clone + Send + Sync + 'static, Audio: Clone + Send + Sync + 'static> ParameterValueType for NativeProcessorInputFixed<Image, Audio> {
    type Image = Image;
    type Audio = Audio;
    type Binary = AbstractFile;
    type String = String;
    type Integer = i64;
    type RealNumber = f64;
    type Boolean = bool;
    type Dictionary = Never;
    type Array = Never;
    type ComponentClass = Never;
}

pub struct NativeProcessorOutput<Image, Audio>(PhantomData<(Image, Audio)>);

pub type ParameterNativeProcessorOutput<Image, Audio> = Parameter<NativeProcessorOutput<Image, Audio>>;

impl<Image: Clone + Send + Sync + 'static, Audio: Clone + Send + Sync + 'static> ParameterValueType for NativeProcessorOutput<Image, Audio> {
    type Image = Image;
    type Audio = Audio;
    type Binary = PathBuf;
    type String = String;
    type Integer = i64;
    type RealNumber = f64;
    type Boolean = bool;
    type Dictionary = HashMap<String, Parameter<NativeProcessorOutput<Image, Audio>>>;
    type Array = Vec<Parameter<NativeProcessorOutput<Image, Audio>>>;
    type ComponentClass = Never;
}
