use crate::common::mapped_slice::MappedSliceMut;
use crate::common::time_split_value::TimeSplitValue;
use crate::component::instance::ComponentInstance;
use crate::component::link::MarkerLink;
use crate::component::marker_pin::MarkerTime;
use crate::component::parameter::placeholder::{TimedAudioPlaceholder, TimedImagePlaceholder};
use crate::component::parameter::value::EasingValue;
use crate::component::parameter::{Never, Parameter, ParameterType, ParameterValue, ParameterValueFixed, ParameterValueType, ParameterValueViewForFix};
use crate::native::processor::NativeProcessor;
use cgmath::{Vector2, Vector3};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub struct NativeProcessorExecutable<T> {
    pub processor: Arc<dyn NativeProcessor<T>>,
    pub parameter: Vec<Parameter<'static, NativeProcessorInput>>,
}

pub enum ComponentProcessorOutput<T> {
    Native { processors: Vec<NativeProcessorExecutable<T>> },
    Component { components: Vec<ComponentInstance<T>>, link: Vec<MarkerLink> },
}

pub struct NativeProcessorInput;

impl<'a> ParameterValueType<'a> for NativeProcessorInput {
    type Image = TimedImagePlaceholder;
    type Audio = TimedAudioPlaceholder;
    type Video = (TimedImagePlaceholder, TimedAudioPlaceholder);
    type File = TimeSplitValue<MarkerTime, PathBuf>;
    type String = TimeSplitValue<MarkerTime, String>;
    type Boolean = TimeSplitValue<MarkerTime, bool>;
    type Integer = TimeSplitValue<MarkerTime, i64>;
    type RealNumber = TimeSplitValue<MarkerTime, EasingValue<f64>>;
    type Vec2 = TimeSplitValue<MarkerTime, Vector2<EasingValue<f64>>>;
    type Vec3 = TimeSplitValue<MarkerTime, Vector3<EasingValue<f64>>>;
    type Dictionary = TimeSplitValue<MarkerTime, HashMap<String, ParameterValue>>;
    type ComponentClass = Never;
}

pub trait ComponentProcessor<T> {
    fn update_variable_parameter(&self, fixed_params: &mut [ParameterValueFixed], variable_parameters: &mut Vec<(String, ParameterType)>);
    fn validate_parameter(&self, fixed_params: &[ParameterValueFixed], variable_params: &mut MappedSliceMut<ParameterValue, &ParameterValue, ParameterValueViewForFix>);
    fn natural_length(&self, fixed_params: &[ParameterValueFixed], variable_params: &[ParameterValue]) -> Duration;
    fn process(&self, fixed_params: &[ParameterValueFixed], variable_params: &[ParameterValue]) -> ComponentProcessorOutput<T>;
}
