use crate::component::instance::ComponentInstance;
use crate::component::parameter::ParameterType;
use crate::component::processor::ComponentProcessor;
use std::sync::Arc;

pub struct ComponentClass<T> {
    generate_image: bool,
    generate_audio: bool,
    fixed_parameter_type: Vec<(String, ParameterType)>,
    default_variable_parameter_type: Vec<(String, ParameterType)>,
    processor: Arc<dyn ComponentProcessor<T>>,
}

impl<T> ComponentClass<T> {
    pub fn new() -> ComponentClass<T> {
        todo!()
    }
    pub fn generate_image(&self) -> bool {
        self.generate_image
    }
    pub fn generate_audio(&self) -> bool {
        self.generate_audio
    }
    pub fn fixed_parameter_type(&self) -> &[(String, ParameterType)] {
        &self.fixed_parameter_type
    }
    pub fn default_variable_parameter_type(&self) -> &[(String, ParameterType)] {
        &self.default_variable_parameter_type
    }
    pub fn processor(&self) -> &Arc<dyn ComponentProcessor<T>> {
        &self.processor
    }
    pub fn instantiate(&self) -> ComponentInstance<T> {
        todo!()
    }
}