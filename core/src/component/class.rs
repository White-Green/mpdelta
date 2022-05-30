use crate::component::instance::ComponentInstance;
use crate::component::parameter::ParameterType;
use std::collections::HashMap;
use std::sync::Arc;
use crate::component::processor::ComponentProcessor;

pub struct ComponentClass {
    generate_image: bool,
    generate_audio: bool,
    parameter_type: HashMap<String, ParameterType>,
    processor: Arc<dyn ComponentProcessor>,
}

impl ComponentClass {
    pub fn new() -> ComponentClass {
        todo!()
    }
    pub fn generate_image(&self) -> bool {
        self.generate_image
    }
    pub fn generate_audio(&self) -> bool {
        self.generate_audio
    }
    pub fn parameter_type(&self) -> &HashMap<String, ParameterType> {
        &self.parameter_type
    }
    pub fn processor(&self) -> &Arc<dyn ComponentProcessor> {
        &self.processor
    }
    pub fn instantiate(&self) -> ComponentInstance {
        todo!()
    }
}
