use crate::component::instance::ComponentInstance;
use crate::component::parameter::ParameterType;
use std::collections::HashMap;

pub struct ComponentClass {
    generate_image: bool,
    generate_audio: bool,
    parameter_type: HashMap<String, ParameterType>,
    processor: (), // TODO:処理系を詰めないとどういう構成にするか決まらないのでとりあえず無を置いておく
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
    pub fn instantiate(&self) -> ComponentInstance<(), ()> {
        todo!()
    }
}
