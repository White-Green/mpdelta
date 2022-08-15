use crate::component::instance::ComponentInstance;
use crate::component::link::MarkerLink;
use crate::component::parameter::placeholder::{Placeholder, TagAudio, TagImage};
use crate::component::parameter::value::FrameVariableValue;
use crate::component::parameter::{ComponentProcessorInputValue, Never, Parameter, ParameterFrameVariableValue, ParameterType, ParameterValueFixed, ParameterValueFixedExceptComponentClass, ParameterValueType};
use crate::native::processor::NativeProcessor;
use crate::ptr::StaticPointerCow;
use cgmath::{Vector2, Vector3};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

pub struct NativeProcessorExecutable<T> {
    pub processor: Arc<dyn NativeProcessor<T>>,
    pub parameter: Arc<[Parameter<'static, NativeProcessorInput>]>,
}

impl<T> Clone for NativeProcessorExecutable<T> {
    fn clone(&self) -> Self {
        NativeProcessorExecutable {
            processor: Arc::clone(&self.processor),
            parameter: Arc::clone(&self.parameter),
        }
    }
}

pub enum ComponentProcessorBody<'a, T> {
    Native(Cow<'a, [Arc<dyn Fn(&[ParameterValueFixed], &[ParameterFrameVariableValue]) -> NativeProcessorExecutable<T> + Send + Sync + 'a>]>),
    Component(Arc<dyn Fn(&[ParameterValueFixed], &[ComponentProcessorInputValue]) -> (Vec<StaticPointerCow<RwLock<ComponentInstance<T>>>>, Vec<StaticPointerCow<RwLock<MarkerLink>>>) + Send + Sync + 'a>),
}

pub struct NativeProcessorInput;

impl<'a> ParameterValueType<'a> for NativeProcessorInput {
    type Image = Placeholder<TagImage>;
    type Audio = Placeholder<TagAudio>;
    type Video = (Placeholder<TagImage>, Placeholder<TagAudio>);
    type File = FrameVariableValue<PathBuf>;
    type String = FrameVariableValue<String>;
    type Select = FrameVariableValue<usize>;
    type Boolean = FrameVariableValue<bool>;
    type Radio = FrameVariableValue<bool>;
    type Integer = FrameVariableValue<i64>;
    type RealNumber = FrameVariableValue<f64>;
    type Vec2 = FrameVariableValue<Vector2<f64>>;
    type Vec3 = FrameVariableValue<Vector3<f64>>;
    type Dictionary = FrameVariableValue<HashMap<String, ParameterValueFixedExceptComponentClass>>;
    type ComponentClass = Never;
}

pub trait ComponentProcessor<T>: Send + Sync {
    fn update_variable_parameter(&self, fixed_params: &mut [ParameterValueFixed], variable_parameters: &mut Vec<(String, ParameterType)>);
    fn natural_length(&self, fixed_params: &[ParameterValueFixed]) -> Duration;
    fn get_processor(&self) -> ComponentProcessorBody<'_, T>;
}
