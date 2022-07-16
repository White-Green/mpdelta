use crate::component::instance::ComponentInstance;
use crate::component::parameter::ParameterType;

pub trait ComponentClass<T>: Send + Sync {
    fn generate_image(&self) -> bool;
    fn generate_audio(&self) -> bool;
    fn fixed_parameter_type(&self) -> &[(String, ParameterType)];
    fn default_variable_parameter_type(&self) -> &[(String, ParameterType)];
    fn instantiate(&self) -> ComponentInstance<T>;
}
