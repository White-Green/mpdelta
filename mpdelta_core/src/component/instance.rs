use crate::component::class::ComponentClass;
use crate::component::marker_pin::MarkerPin;
use crate::component::parameter::{AudioRequiredParams, ImageRequiredParams, Parameter, ParameterNullableValue, ParameterValue, ParameterValueFixed, Type, ValueFixed, VariableParameterValue};
use crate::component::processor::ComponentProcessor;
use crate::ptr::{StaticPointer, StaticPointerCow, StaticPointerOwned};

use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use tokio::sync::RwLock;

type Cell<T> = RwLock<T>;

pub struct ComponentInstance<T> {
    component_class: StaticPointer<RwLock<dyn ComponentClass<T>>>,
    marker_left: StaticPointerCow<Cell<MarkerPin>>,
    marker_right: StaticPointerCow<Cell<MarkerPin>>,
    markers: Vec<StaticPointerOwned<Cell<MarkerPin>>>,
    image_required_params: Option<ImageRequiredParams<T>>,
    audio_required_params: Option<AudioRequiredParams<T>>,
    fixed_parameters_type: Box<[(String, Parameter<'static, Type>)]>,
    fixed_parameters: Box<[ParameterValueFixed]>,
    variable_parameters_type: Vec<(String, Parameter<'static, Type>)>,
    variable_parameters: Vec<VariableParameterValue<T, ParameterValue, ParameterNullableValue>>,
    processor: Arc<dyn ComponentProcessor<T>>,
}

impl<T> Debug for ComponentInstance<T>
where
    ImageRequiredParams<T>: Debug,
    AudioRequiredParams<T>: Debug,
    VariableParameterValue<T, ParameterValue, ParameterNullableValue>: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentInstance")
            .field("component_class", &self.component_class)
            .field("marker_left", &self.marker_left)
            .field("marker_right", &self.marker_right)
            .field("markers", &self.markers)
            .field("image_required_params", &self.image_required_params)
            .field("audio_required_params", &self.audio_required_params)
            .field("fixed_parameters_type", &self.fixed_parameters_type)
            .field("fixed_parameters", &self.fixed_parameters)
            .field("variable_parameters_type", &self.variable_parameters_type)
            .field("variable_parameters", &self.variable_parameters)
            .finish_non_exhaustive()
    }
}

impl<T> ComponentInstance<T> {
    pub fn new_no_param(
        component_class: StaticPointer<RwLock<dyn ComponentClass<T>>>,
        marker_left: StaticPointerCow<Cell<MarkerPin>>,
        marker_right: StaticPointerCow<Cell<MarkerPin>>,
        image_required_params: Option<ImageRequiredParams<T>>,
        audio_required_params: Option<AudioRequiredParams<T>>,
        processor: Arc<dyn ComponentProcessor<T>>,
    ) -> ComponentInstance<T> {
        ComponentInstance {
            component_class,
            marker_left,
            marker_right,
            markers: Vec::new(),
            image_required_params,
            audio_required_params,
            fixed_parameters_type: Vec::new().into_boxed_slice(),
            fixed_parameters: Vec::new().into_boxed_slice(),
            variable_parameters_type: Vec::new(),
            variable_parameters: Vec::new(),
            processor,
        }
    }
    pub fn component_class(&self) -> &StaticPointer<RwLock<dyn ComponentClass<T>>> {
        &self.component_class
    }
    pub fn marker_left(&self) -> &StaticPointerCow<Cell<MarkerPin>> {
        &self.marker_left
    }
    pub fn marker_right(&self) -> &StaticPointerCow<Cell<MarkerPin>> {
        &self.marker_right
    }
    pub fn markers(&self) -> &[StaticPointerOwned<Cell<MarkerPin>>] {
        &self.markers
    }
    pub fn image_required_params(&self) -> Option<&ImageRequiredParams<T>> {
        self.image_required_params.as_ref()
    }
    pub fn set_image_required_params(&mut self, params: ImageRequiredParams<T>) {
        if let Some(current_params) = self.image_required_params.as_mut() {
            *current_params = params;
        }
    }
    pub fn audio_required_params(&self) -> Option<&AudioRequiredParams<T>> {
        self.audio_required_params.as_ref()
    }
    pub fn fixed_parameters_type(&self) -> &[(String, Parameter<'static, Type>)] {
        &self.fixed_parameters_type
    }
    pub fn fixed_parameters(&self) -> &[Parameter<'static, ValueFixed>] {
        &self.fixed_parameters
    }
    pub fn variable_parameters_type(&self) -> &[(String, Parameter<'static, Type>)] {
        &self.variable_parameters_type
    }
    pub fn variable_parameters(&self) -> &[VariableParameterValue<T, ParameterValue, ParameterNullableValue>] {
        &self.variable_parameters
    }
    pub fn processor(&self) -> &Arc<dyn ComponentProcessor<T>> {
        &self.processor
    }
}
