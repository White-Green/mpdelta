use crate::component::class::ComponentClass;
use crate::component::marker_pin::MarkerPin;
use crate::component::parameter::{AudioRequiredParams, ImageRequiredParams, Parameter, ParameterNullableValue, ParameterTypedValue, Type, Value, ValueFixed, VariableParameterValue};
use crate::component::processor::ComponentProcessor;
use crate::ptr::{StaticPointer, StaticPointerOwned};
use std::sync::{Arc, RwLock};

type Cell<T> = RwLock<T>;

pub struct ComponentInstance<T> {
    component_class: StaticPointer<ComponentClass<T>>,
    marker_left: StaticPointerOwned<Cell<MarkerPin>>,
    marker_right: StaticPointerOwned<Cell<MarkerPin>>,
    markers: Vec<StaticPointerOwned<Cell<MarkerPin>>>,
    image_required_params: Option<ImageRequiredParams<T>>,
    audio_required_params: Option<AudioRequiredParams<T>>,
    fixed_parameters: Box<[(String, Parameter<'static, (Type, ValueFixed)>)]>,
    variable_parameters: Vec<(String, VariableParameterValue<T, ParameterTypedValue, ParameterNullableValue>)>,
    processor: Arc<dyn ComponentProcessor<T>>,
}

impl<T> ComponentInstance<T> {
    pub fn component_class(&self) -> &StaticPointer<ComponentClass<T>> {
        &self.component_class
    }
    pub fn marker_left(&self) -> &StaticPointerOwned<Cell<MarkerPin>> {
        &self.marker_left
    }
    pub fn marker_right(&self) -> &StaticPointerOwned<Cell<MarkerPin>> {
        &self.marker_right
    }
    pub fn markers(&self) -> &[StaticPointerOwned<Cell<MarkerPin>>] {
        &self.markers
    }
    pub fn image_required_params(&self) -> Option<&ImageRequiredParams<T>> {
        self.image_required_params.as_ref()
    }
    pub fn audio_required_params(&self) -> Option<&AudioRequiredParams<T>> {
        self.audio_required_params.as_ref()
    }
    pub fn fixed_parameters(&self) -> &[(String, Parameter<'static, (Type, ValueFixed)>)] {
        &self.fixed_parameters
    }
    pub fn variable_parameters(&self) -> &[(String, VariableParameterValue<T, ParameterTypedValue, ParameterNullableValue>)] {
        &self.variable_parameters
    }
    pub fn processor(&self) -> &Arc<dyn ComponentProcessor<T>> {
        &self.processor
    }
}
