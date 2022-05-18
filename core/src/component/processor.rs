use crate::common::mapped_slice::MappedSliceMut;
use crate::component::instance::ComponentInstance;
use crate::component::parameter::{ParameterType, ParameterValue, ParameterValueFixed, ParameterValueViewForFix};
use std::time::Duration;

pub enum ComponentProcessorOutput {
    Native {
        processor: (), // TODO: Nativeのprocessorが未定なので無を置いておく
        parameter: Vec<ParameterValue>,
    },
    Component {
        components: Vec<ComponentInstance>,
        link: (), // TODO: MarkerPin同士のLink
    },
}

pub trait ComponentProcessor {
    fn update_variable_parameter(&self, fixed_params: &mut [ParameterValueFixed], variable_parameters: &mut Vec<(String, ParameterType)>);
    fn validate_parameter(&self, fixed_params: &[ParameterValueFixed], variable_params: &mut MappedSliceMut<ParameterValue, &ParameterValue, ParameterValueViewForFix>);
    fn natural_length(&self, fixed_params: &[ParameterValueFixed], variable_params: &[ParameterValue]) -> Duration;
    fn process(&self, fixed_params: &[ParameterValueFixed], variable_params: &[ParameterValue]) -> ComponentProcessorOutput;
}
