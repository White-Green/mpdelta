use crate::component::instance::ComponentInstance;
use crate::component::parameter::ParameterType;
use crate::component::processor::ComponentProcessor;
use std::collections::HashMap;
use std::sync::Arc;

pub struct ComponentClass<T> {
    generate_image: bool,
    generate_audio: bool,
    parameter_type: HashMap<String, ParameterType>,
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
    pub fn parameter_type(&self) -> &HashMap<String, ParameterType> {
        &self.parameter_type
    }
    pub fn processor(&self) -> &Arc<dyn ComponentProcessor<T>> {
        &self.processor
    }
    pub fn instantiate(&self) -> ComponentInstance<T> {
        todo!()
    }
}
