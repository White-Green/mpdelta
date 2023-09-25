use crate::component::class::ComponentClass;
use crate::component::marker_pin::{MarkerPinHandleCow, MarkerPinHandleOwned};
use crate::component::parameter::{AudioRequiredParams, ImageRequiredParams, Parameter, ParameterNullableValue, ParameterValue, ParameterValueFixed, Type, ValueFixed, VariableParameterValue};
use crate::component::processor::ComponentProcessor;
use crate::ptr::{StaticPointer, StaticPointerCow, StaticPointerOwned};
use qcell::TCell;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ComponentInstance<K: 'static, T> {
    component_class: StaticPointer<RwLock<dyn ComponentClass<K, T>>>,
    marker_left: MarkerPinHandleCow<K>,
    marker_right: MarkerPinHandleCow<K>,
    markers: Vec<MarkerPinHandleOwned<K>>,
    image_required_params: Option<ImageRequiredParams<K, T>>,
    audio_required_params: Option<AudioRequiredParams<K, T>>,
    fixed_parameters_type: Box<[(String, Parameter<Type>)]>,
    fixed_parameters: Box<[ParameterValueFixed]>,
    variable_parameters_type: Vec<(String, Parameter<Type>)>,
    variable_parameters: Vec<VariableParameterValue<K, T, ParameterValue<K>, ParameterNullableValue<K>>>,
    processor: Arc<dyn ComponentProcessor<K, T>>,
}

pub type ComponentInstanceHandle<K, T> = StaticPointer<TCell<K, ComponentInstance<K, T>>>;
pub type ComponentInstanceHandleOwned<K, T> = StaticPointerOwned<TCell<K, ComponentInstance<K, T>>>;
pub type ComponentInstanceHandleCow<K, T> = StaticPointerCow<TCell<K, ComponentInstance<K, T>>>;

impl<K, T> Debug for ComponentInstance<K, T>
where
    ImageRequiredParams<K, T>: Debug,
    AudioRequiredParams<K, T>: Debug,
    VariableParameterValue<K, T, ParameterValue<K>, ParameterNullableValue<K>>: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        struct DebugFn<F>(F);
        impl<F: for<'a> Fn(&mut Formatter<'a>) -> std::fmt::Result> Debug for DebugFn<F> {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                self.0(f)
            }
        }
        f.debug_struct("ComponentInstance")
            .field("component_class", &self.component_class)
            .field("marker_left", self.marker_left.ptr())
            .field("marker_right", self.marker_right.ptr())
            .field("markers", &DebugFn(|f: &mut Formatter<'_>| f.debug_list().entries(self.markers.iter().map(StaticPointerOwned::reference)).finish()))
            .field("image_required_params", &self.image_required_params)
            .field("audio_required_params", &self.audio_required_params)
            .field("fixed_parameters_type", &self.fixed_parameters_type)
            .field("fixed_parameters", &self.fixed_parameters)
            .field("variable_parameters_type", &self.variable_parameters_type)
            .field("variable_parameters", &self.variable_parameters)
            .finish_non_exhaustive()
    }
}

impl<K, T> ComponentInstance<K, T> {
    pub fn new_no_param(
        component_class: StaticPointer<RwLock<dyn ComponentClass<K, T>>>,
        marker_left: MarkerPinHandleCow<K>,
        marker_right: MarkerPinHandleCow<K>,
        image_required_params: Option<ImageRequiredParams<K, T>>,
        audio_required_params: Option<AudioRequiredParams<K, T>>,
        processor: Arc<dyn ComponentProcessor<K, T>>,
    ) -> ComponentInstance<K, T> {
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
    pub fn component_class(&self) -> &StaticPointer<RwLock<dyn ComponentClass<K, T>>> {
        &self.component_class
    }
    pub fn marker_left(&self) -> &MarkerPinHandleCow<K> {
        &self.marker_left
    }
    pub fn marker_right(&self) -> &MarkerPinHandleCow<K> {
        &self.marker_right
    }
    pub fn markers(&self) -> &[MarkerPinHandleOwned<K>] {
        &self.markers
    }
    pub fn image_required_params(&self) -> Option<&ImageRequiredParams<K, T>> {
        self.image_required_params.as_ref()
    }
    pub fn set_image_required_params(&mut self, params: ImageRequiredParams<K, T>) {
        if let Some(current_params) = self.image_required_params.as_mut() {
            *current_params = params;
        }
    }
    pub fn audio_required_params(&self) -> Option<&AudioRequiredParams<K, T>> {
        self.audio_required_params.as_ref()
    }
    pub fn fixed_parameters_type(&self) -> &[(String, Parameter<Type>)] {
        &self.fixed_parameters_type
    }
    pub fn fixed_parameters(&self) -> &[Parameter<ValueFixed>] {
        &self.fixed_parameters
    }
    pub fn variable_parameters_type(&self) -> &[(String, Parameter<Type>)] {
        &self.variable_parameters_type
    }
    pub fn variable_parameters(&self) -> &[VariableParameterValue<K, T, ParameterValue<K>, ParameterNullableValue<K>>] {
        &self.variable_parameters
    }
    pub fn processor(&self) -> &Arc<dyn ComponentProcessor<K, T>> {
        &self.processor
    }
}
