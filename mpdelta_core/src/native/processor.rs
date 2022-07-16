use crate::component::parameter::{Parameter, ParameterTypeExceptComponentClass, ParameterValueFixedExceptComponentClass, ParameterValueType};

pub trait NativeProcessor<T: ParameterValueType<'static>> {
    fn parameter_type(&self) -> &[ParameterTypeExceptComponentClass];
    fn return_type(&self) -> &ParameterTypeExceptComponentClass;
    fn process(&self, params: &[ParameterValueFixedExceptComponentClass]) -> Parameter<'static, T>;
}
