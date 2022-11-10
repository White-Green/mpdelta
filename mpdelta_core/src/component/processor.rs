use crate::component::instance::ComponentInstance;
use crate::component::link::MarkerLink;
use crate::component::marker_pin::MarkerTime;
use crate::component::parameter::placeholder::{Placeholder, TagAudio, TagImage};
use crate::component::parameter::value::FrameVariableValue;
use crate::component::parameter::{ComponentProcessorInputValue, Never, Parameter, ParameterFrameVariableValue, ParameterSelect, ParameterType, ParameterValueFixed, ParameterValueType};
use crate::native::processor::NativeProcessor;
use crate::ptr::StaticPointerCow;
use crate::time::TimelineTime;
use async_trait::async_trait;
use cgmath::{Vector2, Vector3};
use qcell::TCell;
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub struct NativeProcessorExecutable<T> {
    pub processor: Arc<dyn NativeProcessor<T>>,
    pub parameter: Arc<[Parameter<NativeProcessorInput>]>,
}

impl<T> Clone for NativeProcessorExecutable<T> {
    fn clone(&self) -> Self {
        NativeProcessorExecutable {
            processor: Arc::clone(&self.processor),
            parameter: Arc::clone(&self.parameter),
        }
    }
}

pub trait NativeProcessorBuilder<T> {
    fn output_type(&self) -> Parameter<ParameterSelect>;
    fn build(&self, fixed_parameters: &[ParameterValueFixed], variable_parameters: &[ParameterFrameVariableValue], variable_parameter_type: &[(String, ParameterType)], frames: &mut dyn Iterator<Item = TimelineTime>, map_time: &dyn Fn(TimelineTime) -> MarkerTime) -> NativeProcessorExecutable<T>;
}

pub trait ProcessorComponentBuilder<K, T> {
    fn build(
        &self,
        fixed_parameters: &[ParameterValueFixed],
        variable_parameters: &[ComponentProcessorInputValue],
        variable_parameter_type: &[(String, ParameterType)],
        frames: &mut dyn Iterator<Item = TimelineTime>,
        map_time: &dyn Fn(TimelineTime) -> MarkerTime,
    ) -> (Vec<StaticPointerCow<TCell<K, ComponentInstance<K, T>>>>, Vec<StaticPointerCow<TCell<K, MarkerLink<K>>>>);
}

pub enum ComponentProcessorBody<'a, K, T> {
    Native(Cow<'a, [Arc<dyn NativeProcessorBuilder<T> + Send + Sync + 'a>]>),
    Component(Arc<dyn ProcessorComponentBuilder<K, T> + Send + Sync + 'a>),
}

pub struct NativeProcessorInput;

impl ParameterValueType for NativeProcessorInput {
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
    type Dictionary = Never;
    type ComponentClass = Never;
}

#[async_trait]
pub trait ComponentProcessor<K, T>: Send + Sync {
    async fn update_variable_parameter(&self, fixed_params: &mut [ParameterValueFixed], variable_parameters: &mut Vec<(String, ParameterType)>);
    async fn natural_length(&self, fixed_params: &[ParameterValueFixed]) -> Duration;
    async fn get_processor(&self) -> ComponentProcessorBody<'_, K, T>;
}
