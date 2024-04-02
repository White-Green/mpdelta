use crate::component::class::ComponentClass;
use crate::component::marker_pin::{MarkerPinHandle, MarkerPinHandleOwned};
use crate::component::parameter::{AudioRequiredParams, ImageRequiredParams, Parameter, ParameterNullableValue, ParameterValueFixed, ParameterValueType, Type, VariableParameterValue};
use crate::component::processor::ComponentProcessorWrapper;
use crate::ptr::{StaticPointer, StaticPointerCow, StaticPointerOwned};
use qcell::TCell;
use std::fmt::{Debug, Formatter};
use tokio::sync::RwLock;

pub struct ComponentInstanceBuilder<K: 'static, T: ParameterValueType> {
    component_class: StaticPointer<RwLock<dyn ComponentClass<K, T>>>,
    marker_left: MarkerPinHandleOwned<K>,
    marker_right: MarkerPinHandleOwned<K>,
    markers: Vec<MarkerPinHandleOwned<K>>,
    image_required_params: Option<ImageRequiredParams<K, T>>,
    audio_required_params: Option<AudioRequiredParams<K, T>>,
    fixed_parameters_type: Box<[(String, Parameter<Type>)]>,
    fixed_parameters: Box<[ParameterValueFixed<T::Image, T::Audio>]>,
    variable_parameters_type: Vec<(String, Parameter<Type>)>,
    variable_parameters: Vec<VariableParameterValue<K, T, ParameterNullableValue<K, T>>>,
    processor: ComponentProcessorWrapper<K, T>,
}

impl<K: 'static, T: ParameterValueType> ComponentInstanceBuilder<K, T> {
    pub fn new(component_class: StaticPointer<RwLock<dyn ComponentClass<K, T>>>, marker_left: MarkerPinHandleOwned<K>, marker_right: MarkerPinHandleOwned<K>, markers: Vec<MarkerPinHandleOwned<K>>, processor: impl Into<ComponentProcessorWrapper<K, T>>) -> ComponentInstanceBuilder<K, T> {
        ComponentInstanceBuilder {
            component_class,
            marker_left,
            marker_right,
            markers,
            image_required_params: None,
            audio_required_params: None,
            fixed_parameters_type: Box::new([]),
            fixed_parameters: Box::new([]),
            variable_parameters_type: Vec::new(),
            variable_parameters: Vec::new(),
            processor: processor.into(),
        }
    }

    pub fn image_required_params(mut self, params: ImageRequiredParams<K, T>) -> Self {
        self.image_required_params = Some(params);
        self
    }

    pub fn audio_required_params(mut self, params: AudioRequiredParams<K, T>) -> Self {
        self.audio_required_params = Some(params);
        self
    }

    pub fn fixed_parameters(mut self, types: Box<[(String, Parameter<Type>)]>, values: Box<[ParameterValueFixed<T::Image, T::Audio>]>) -> Self {
        self.fixed_parameters_type = types;
        self.fixed_parameters = values;
        self
    }

    pub fn variable_parameters(mut self, types: Vec<(String, Parameter<Type>)>, values: Vec<VariableParameterValue<K, T, ParameterNullableValue<K, T>>>) -> Self {
        self.variable_parameters_type = types;
        self.variable_parameters = values;
        self
    }

    pub fn build(self) -> ComponentInstance<K, T> {
        let ComponentInstanceBuilder {
            component_class,
            marker_left,
            marker_right,
            markers,
            image_required_params,
            audio_required_params,
            fixed_parameters_type,
            fixed_parameters,
            variable_parameters_type,
            variable_parameters,
            processor,
        } = self;
        ComponentInstance {
            component_class,
            marker_left,
            marker_right,
            markers,
            image_required_params,
            audio_required_params,
            fixed_parameters_type,
            fixed_parameters,
            variable_parameters_type,
            variable_parameters,
            processor,
        }
    }
}

pub struct ComponentInstance<K: 'static, T: ParameterValueType> {
    component_class: StaticPointer<RwLock<dyn ComponentClass<K, T>>>,
    marker_left: MarkerPinHandleOwned<K>,
    marker_right: MarkerPinHandleOwned<K>,
    markers: Vec<MarkerPinHandleOwned<K>>,
    image_required_params: Option<ImageRequiredParams<K, T>>,
    audio_required_params: Option<AudioRequiredParams<K, T>>,
    fixed_parameters_type: Box<[(String, Parameter<Type>)]>,
    fixed_parameters: Box<[ParameterValueFixed<T::Image, T::Audio>]>,
    variable_parameters_type: Vec<(String, Parameter<Type>)>,
    variable_parameters: Vec<VariableParameterValue<K, T, ParameterNullableValue<K, T>>>,
    processor: ComponentProcessorWrapper<K, T>,
}

pub type ComponentInstanceHandle<K, T> = StaticPointer<TCell<K, ComponentInstance<K, T>>>;
pub type ComponentInstanceHandleOwned<K, T> = StaticPointerOwned<TCell<K, ComponentInstance<K, T>>>;
pub type ComponentInstanceHandleCow<K, T> = StaticPointerCow<TCell<K, ComponentInstance<K, T>>>;

impl<K, T: ParameterValueType> Debug for ComponentInstance<K, T>
where
    ImageRequiredParams<K, T>: Debug,
    AudioRequiredParams<K, T>: Debug,
    VariableParameterValue<K, T, ParameterNullableValue<K, T>>: Debug,
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
            .field("marker_left", StaticPointerOwned::reference(&self.marker_left))
            .field("marker_right", StaticPointerOwned::reference(&self.marker_right))
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

impl<K, T: ParameterValueType> ComponentInstance<K, T> {
    pub fn builder(component_class: StaticPointer<RwLock<dyn ComponentClass<K, T>>>, marker_left: MarkerPinHandleOwned<K>, marker_right: MarkerPinHandleOwned<K>, markers: Vec<MarkerPinHandleOwned<K>>, processor: impl Into<ComponentProcessorWrapper<K, T>>) -> ComponentInstanceBuilder<K, T> {
        ComponentInstanceBuilder::new(component_class, marker_left, marker_right, markers, processor)
    }
    pub fn component_class(&self) -> &StaticPointer<RwLock<dyn ComponentClass<K, T>>> {
        &self.component_class
    }
    pub fn marker_left(&self) -> &MarkerPinHandle<K> {
        StaticPointerOwned::reference(&self.marker_left)
    }
    pub fn marker_right(&self) -> &MarkerPinHandle<K> {
        StaticPointerOwned::reference(&self.marker_right)
    }
    pub fn markers(&self) -> &[MarkerPinHandleOwned<K>] {
        &self.markers
    }
    pub fn markers_mut(&mut self) -> &mut Vec<MarkerPinHandleOwned<K>> {
        &mut self.markers
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
    pub fn set_audio_required_params(&mut self, params: AudioRequiredParams<K, T>) {
        if let Some(current_params) = self.audio_required_params.as_mut() {
            *current_params = params;
        }
    }
    pub fn fixed_parameters_type(&self) -> &[(String, Parameter<Type>)] {
        &self.fixed_parameters_type
    }
    pub fn fixed_parameters(&self) -> &[ParameterValueFixed<T::Image, T::Audio>] {
        &self.fixed_parameters
    }
    pub fn fixed_parameters_mut(&mut self) -> &mut [ParameterValueFixed<T::Image, T::Audio>] {
        &mut self.fixed_parameters
    }
    pub fn variable_parameters_type(&self) -> &[(String, Parameter<Type>)] {
        &self.variable_parameters_type
    }
    pub fn variable_parameters(&self) -> &[VariableParameterValue<K, T, ParameterNullableValue<K, T>>] {
        &self.variable_parameters
    }
    pub fn variable_parameters_mut(&mut self) -> &mut Vec<VariableParameterValue<K, T, ParameterNullableValue<K, T>>> {
        &mut self.variable_parameters
    }
    pub fn processor(&self) -> &ComponentProcessorWrapper<K, T> {
        &self.processor
    }
}
