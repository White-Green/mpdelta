use crate::component::class::ComponentClass;
use crate::component::marker_pin::MarkerPin;
use crate::component::parameter::{AudioRequiredParams, ImageRequiredParams, Parameter, ParameterNullableValue, ParameterTypedValue, Type, ValueFixed, VariableParameterValue};
use crate::component::processor::ComponentProcessor;
use crate::ptr::{StaticPointer, StaticPointerOwned};
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use tokio::sync::RwLock;

type Cell<T> = RwLock<T>;

pub struct ComponentInstance<T> {
    component_class: StaticPointer<RwLock<dyn ComponentClass<T>>>,
    marker_left: StaticPointerOwned<Cell<MarkerPin>>,
    marker_right: StaticPointerOwned<Cell<MarkerPin>>,
    markers: Vec<StaticPointerOwned<Cell<MarkerPin>>>,
    image_required_params: Option<ImageRequiredParams<T>>,
    audio_required_params: Option<AudioRequiredParams<T>>,
    fixed_parameters: Box<[(String, Parameter<'static, (Type, ValueFixed)>)]>,
    variable_parameters: Vec<(String, VariableParameterValue<T, ParameterTypedValue, ParameterNullableValue>)>,
    processor: Arc<dyn ComponentProcessor<T>>,
}

impl<T> Debug for ComponentInstance<T>
where
    ImageRequiredParams<T>: Debug,
    AudioRequiredParams<T>: Debug,
    VariableParameterValue<T, ParameterTypedValue, ParameterNullableValue>: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentInstance")
            .field("component_class", &self.component_class)
            .field("marker_left", &self.marker_left)
            .field("marker_right", &self.marker_right)
            .field("markers", &self.markers)
            .field("image_required_params", &self.image_required_params)
            .field("audio_required_params", &self.audio_required_params)
            .field("fixed_parameters", &self.fixed_parameters)
            .field("variable_parameters", &self.variable_parameters)
            .finish_non_exhaustive()
    }
}

impl<T> ComponentInstance<T> {
    pub fn component_class(&self) -> &StaticPointer<RwLock<dyn ComponentClass<T>>> {
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