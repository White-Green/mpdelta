use crate::component::parameter::{Never, Parameter, ParameterTypeExceptComponentClass, ParameterValueType};
use cgmath::{Vector2, Vector3};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::PathBuf;

pub trait NativeProcessor<T: ParameterValueType<'static>>: Send + Sync {
    fn parameter_type(&self) -> &[ParameterTypeExceptComponentClass];
    fn return_type(&self) -> &ParameterTypeExceptComponentClass;
    fn process(&self, params: &[ParameterNativeProcessorInputFixed<T::Image, T::Audio>]) -> ParameterNativeProcessorInputFixed<T::Image, T::Audio>;
}

pub struct NativeProcessorInputFixed<Image, Audio>(PhantomData<(Image, Audio)>);

pub type ParameterNativeProcessorInputFixed<Image, Audio> = Parameter<'static, NativeProcessorInputFixed<Image, Audio>>;

impl<'a, Image: Clone + Send + Sync + 'a, Audio: Clone + Send + Sync + 'a> ParameterValueType<'a> for NativeProcessorInputFixed<Image, Audio> {
    type Image = Image;
    type Audio = Audio;
    type Video = (Image, Audio);
    type File = PathBuf;
    type String = String;
    type Select = usize;
    type Boolean = bool;
    type Radio = bool;
    type Integer = i64;
    type RealNumber = f64;
    type Vec2 = Vector2<f64>;
    type Vec3 = Vector3<f64>;
    type Dictionary = HashMap<String, Parameter<'a, NativeProcessorInputFixed<Image, Audio>>>;
    type ComponentClass = Never;
}
