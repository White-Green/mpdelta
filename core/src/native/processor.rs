use crate::component::parameter::{ParameterTypeExceptComponentClass, ParameterValueFixedExceptComponentClass};

pub trait NativeProcessor<T> {
    fn parameter_type(&self) -> &[ParameterTypeExceptComponentClass];
    fn return_type(&self) -> &ParameterTypeExceptComponentClass;
    fn process(&self, params: &[ParameterValueFixedExceptComponentClass]) -> T;
}
