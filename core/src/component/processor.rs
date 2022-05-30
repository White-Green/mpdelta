use crate::common::mapped_slice::MappedSliceMut;
use crate::common::time_split_value::TimeSplitValue;
use crate::component::instance::ComponentInstance;
use crate::component::marker_pin::MarkerTime;
use crate::component::parameter::placeholder::{TimedAudioPlaceholder, TimedImagePlaceholder};
use crate::component::parameter::value::EasingValue;
use crate::component::parameter::{Parameter, ParameterType, ParameterValue, ParameterValueFixed, ParameterValueType, ParameterValueViewForFix};
use cgmath::{Vector2, Vector3};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

pub enum ComponentProcessorOutput {
    Native {
        processor: (),
        // TODO: Nativeのprocessorが未定なので無を置いておく
        parameter: Vec<Parameter<'static, ComponentProcessorOutput>>,
    },
    Component {
        components: Vec<ComponentInstance>,
        link: (), // TODO: MarkerPin同士のLink
    },
}

impl<'a> ParameterValueType<'a> for ComponentProcessorOutput {
    type Image = TimedImagePlaceholder;
    type Audio = TimedAudioPlaceholder;
    type Video = (TimedImagePlaceholder, TimedAudioPlaceholder);
    type File = TimeSplitValue<MarkerTime, PathBuf>;
    type String = TimeSplitValue<MarkerTime, String>;
    type Boolean = TimeSplitValue<MarkerTime, bool>;
    type Integer = TimeSplitValue<MarkerTime, i64>;
    type RealNumber = TimeSplitValue<MarkerTime, EasingValue<f64>>;
    type Vec2 = TimeSplitValue<MarkerTime, EasingValue<Vector2<f64>>>;
    type Vec3 = TimeSplitValue<MarkerTime, EasingValue<Vector3<f64>>>;
    type Dictionary = TimeSplitValue<MarkerTime, HashMap<String, ParameterValue>>;
    type ComponentClass = ();
}

pub trait ComponentProcessor {
    fn update_variable_parameter(&self, fixed_params: &mut [ParameterValueFixed], variable_parameters: &mut Vec<(String, ParameterType)>);
    fn validate_parameter(&self, fixed_params: &[ParameterValueFixed], variable_params: &mut MappedSliceMut<ParameterValue, &ParameterValue, ParameterValueViewForFix>);
    fn natural_length(&self, fixed_params: &[ParameterValueFixed], variable_params: &[ParameterValue]) -> Duration;
    fn process(&self, fixed_params: &[ParameterValueFixed], variable_params: &[ParameterValue]) -> ComponentProcessorOutput;
}
