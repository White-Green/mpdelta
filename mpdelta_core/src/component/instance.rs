use crate::component::class::ComponentClass;
use crate::component::marker_pin::{MarkerPin, MarkerPinId};
use crate::component::parameter::{AudioRequiredParams, ImageRequiredParams, Parameter, ParameterNullableValue, ParameterValueFixed, ParameterValueType, Type, VariableParameterValue};
use crate::component::processor::ComponentProcessorWrapper;
use crate::core::IdGenerator;
use crate::ptr::StaticPointer;
use rpds::{HashTrieSet, HashTrieSetSync, Vector, VectorSync};
use std::borrow::Borrow;
use std::fmt::{Debug, Formatter};
use std::iter;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct ComponentInstanceBuilder<T: ParameterValueType> {
    component_class: StaticPointer<RwLock<dyn ComponentClass<T>>>,
    marker_left: MarkerPin,
    marker_right: MarkerPin,
    markers: Arc<Vec<MarkerPin>>,
    interprocess_pins: HashTrieSetSync<MarkerPinId>,
    image_required_params: Option<Arc<ImageRequiredParams>>,
    audio_required_params: Option<Arc<AudioRequiredParams>>,
    fixed_parameters_type: Arc<[(String, Parameter<Type>)]>,
    fixed_parameters: Arc<[ParameterValueFixed<T::Image, T::Audio>]>,
    variable_parameters_type: Arc<Vec<(String, Parameter<Type>)>>,
    variable_parameters: VectorSync<VariableParameterValue<ParameterNullableValue<T>>>,
    processor: ComponentProcessorWrapper<T>,
}

impl<T: ParameterValueType> ComponentInstanceBuilder<T> {
    pub fn new(component_class: StaticPointer<RwLock<dyn ComponentClass<T>>>, marker_left: MarkerPin, marker_right: MarkerPin, markers: impl Into<Arc<Vec<MarkerPin>>>, processor: impl Into<ComponentProcessorWrapper<T>>) -> ComponentInstanceBuilder<T> {
        ComponentInstanceBuilder {
            component_class,
            marker_left,
            marker_right,
            markers: markers.into(),
            interprocess_pins: HashTrieSet::new_sync(),
            image_required_params: None,
            audio_required_params: None,
            fixed_parameters_type: Arc::new([]),
            fixed_parameters: Arc::new([]),
            variable_parameters_type: Arc::new(Vec::new()),
            variable_parameters: Vector::new_sync(),
            processor: processor.into(),
        }
    }

    pub fn image_required_params(mut self, params: impl Into<Arc<ImageRequiredParams>>) -> Self {
        self.image_required_params = Some(params.into());
        self
    }

    pub fn audio_required_params(mut self, params: impl Into<Arc<AudioRequiredParams>>) -> Self {
        self.audio_required_params = Some(params.into());
        self
    }

    pub fn fixed_parameters(mut self, types: Arc<[(String, Parameter<Type>)]>, values: Arc<[ParameterValueFixed<T::Image, T::Audio>]>) -> Self {
        self.fixed_parameters_type = types;
        self.fixed_parameters = values;
        self
    }

    pub fn variable_parameters(mut self, types: impl Into<Arc<Vec<(String, Parameter<Type>)>>>, values: VectorSync<VariableParameterValue<ParameterNullableValue<T>>>) -> Self {
        self.variable_parameters_type = types.into();
        self.variable_parameters = values;
        self
    }

    pub fn build(self, id_generator: &(impl IdGenerator + ?Sized)) -> ComponentInstance<T> {
        let ComponentInstanceBuilder {
            component_class,
            marker_left,
            marker_right,
            markers,
            interprocess_pins,
            image_required_params,
            audio_required_params,
            fixed_parameters_type,
            fixed_parameters,
            variable_parameters_type,
            variable_parameters,
            processor,
        } = self;
        ComponentInstance {
            id: ComponentInstanceId::new(id_generator.generate_new()),
            component_class,
            marker_left,
            marker_right,
            markers,
            interprocess_pins,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentInstanceId {
    id: Uuid,
}

impl ComponentInstanceId {
    fn new(id: Uuid) -> ComponentInstanceId {
        ComponentInstanceId { id }
    }

    pub fn raw_id(&self) -> Uuid {
        self.id
    }
}

pub struct ComponentInstance<T: ParameterValueType> {
    id: ComponentInstanceId,
    component_class: StaticPointer<RwLock<dyn ComponentClass<T>>>,
    marker_left: MarkerPin,
    marker_right: MarkerPin,
    markers: Arc<Vec<MarkerPin>>,
    interprocess_pins: HashTrieSetSync<MarkerPinId>,
    image_required_params: Option<Arc<ImageRequiredParams>>,
    audio_required_params: Option<Arc<AudioRequiredParams>>,
    fixed_parameters_type: Arc<[(String, Parameter<Type>)]>,
    fixed_parameters: Arc<[ParameterValueFixed<T::Image, T::Audio>]>,
    variable_parameters_type: Arc<Vec<(String, Parameter<Type>)>>,
    variable_parameters: VectorSync<VariableParameterValue<ParameterNullableValue<T>>>,
    processor: ComponentProcessorWrapper<T>,
}

impl<T: ParameterValueType> Debug for ComponentInstance<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentInstance")
            .field("id", &self.id)
            .field("component_class", &self.component_class)
            .field("marker_left", &self.marker_left)
            .field("marker_right", &self.marker_right)
            .field("markers", &self.markers)
            .field("interprocess_pins", &self.interprocess_pins)
            .field("image_required_params", &self.image_required_params)
            .field("audio_required_params", &self.audio_required_params)
            .field("fixed_parameters_type", &self.fixed_parameters_type)
            .field("fixed_parameters", &self.fixed_parameters)
            .field("variable_parameters_type", &self.variable_parameters_type)
            .field("variable_parameters", &self.variable_parameters)
            .finish_non_exhaustive()
    }
}

impl<T: ParameterValueType> Clone for ComponentInstance<T> {
    fn clone(&self) -> Self {
        let ComponentInstance {
            id,
            component_class,
            marker_left,
            marker_right,
            markers,
            interprocess_pins,
            image_required_params,
            audio_required_params,
            fixed_parameters_type,
            fixed_parameters,
            variable_parameters_type,
            variable_parameters,
            processor,
        } = self;
        ComponentInstance {
            id: *id,
            component_class: component_class.clone(),
            marker_left: marker_left.clone(),
            marker_right: marker_right.clone(),
            markers: markers.clone(),
            interprocess_pins: interprocess_pins.clone(),
            image_required_params: image_required_params.clone(),
            audio_required_params: audio_required_params.clone(),
            fixed_parameters_type: fixed_parameters_type.clone(),
            fixed_parameters: fixed_parameters.clone(),
            variable_parameters_type: variable_parameters_type.clone(),
            variable_parameters: variable_parameters.clone(),
            processor: processor.clone(),
        }
    }
}

impl<T: ParameterValueType> Borrow<ComponentInstanceId> for ComponentInstance<T> {
    fn borrow(&self) -> &ComponentInstanceId {
        &self.id
    }
}

impl<T: ParameterValueType> Borrow<ComponentInstanceId> for &ComponentInstance<T> {
    fn borrow(&self) -> &ComponentInstanceId {
        &self.id
    }
}

impl<T: ParameterValueType> ComponentInstance<T> {
    pub fn builder(component_class: StaticPointer<RwLock<dyn ComponentClass<T>>>, marker_left: MarkerPin, marker_right: MarkerPin, markers: Vec<MarkerPin>, processor: impl Into<ComponentProcessorWrapper<T>>) -> ComponentInstanceBuilder<T> {
        ComponentInstanceBuilder::new(component_class, marker_left, marker_right, markers, processor)
    }
    pub fn id(&self) -> &ComponentInstanceId {
        &self.id
    }
    pub fn component_class(&self) -> &StaticPointer<RwLock<dyn ComponentClass<T>>> {
        &self.component_class
    }
    pub fn marker_left(&self) -> &MarkerPin {
        &self.marker_left
    }
    pub fn marker_left_mut(&mut self) -> &mut MarkerPin {
        &mut self.marker_left
    }
    pub fn marker_right(&self) -> &MarkerPin {
        &self.marker_right
    }
    pub fn marker_right_mut(&mut self) -> &mut MarkerPin {
        &mut self.marker_right
    }
    pub fn markers(&self) -> &[MarkerPin] {
        &self.markers
    }
    pub fn markers_mut(&mut self) -> &mut Vec<MarkerPin> {
        Arc::make_mut(&mut self.markers)
    }
    pub fn iter_all_markers(&self) -> impl Iterator<Item = &MarkerPin> + '_ {
        iter::once(&self.marker_left).chain(self.markers.iter()).chain(iter::once(&self.marker_right))
    }
    pub fn iter_all_markers_mut(&mut self) -> impl Iterator<Item = &mut MarkerPin> + '_ {
        iter::once(&mut self.marker_left).chain(Arc::make_mut(&mut self.markers).iter_mut()).chain(iter::once(&mut self.marker_right))
    }
    pub fn interprocess_pins(&self) -> &HashTrieSetSync<MarkerPinId> {
        &self.interprocess_pins
    }
    pub fn interprocess_pins_mut(&mut self) -> &mut HashTrieSetSync<MarkerPinId> {
        &mut self.interprocess_pins
    }
    pub fn image_required_params(&self) -> Option<&ImageRequiredParams> {
        self.image_required_params.as_deref()
    }
    pub fn image_required_params_mut(&mut self) -> Option<&mut ImageRequiredParams> {
        self.image_required_params.as_mut().map(Arc::make_mut)
    }
    pub fn set_image_required_params(&mut self, params: ImageRequiredParams) {
        if let Some(current_params) = self.image_required_params.as_mut() {
            *current_params = Arc::new(params);
        }
    }
    pub fn audio_required_params(&self) -> Option<&AudioRequiredParams> {
        self.audio_required_params.as_deref()
    }
    pub fn audio_required_params_mut(&mut self) -> Option<&mut AudioRequiredParams> {
        self.audio_required_params.as_mut().map(Arc::make_mut)
    }
    pub fn set_audio_required_params(&mut self, params: AudioRequiredParams) {
        if let Some(current_params) = self.audio_required_params.as_mut() {
            *current_params = Arc::new(params);
        }
    }
    pub fn fixed_parameters_type(&self) -> &Arc<[(String, Parameter<Type>)]> {
        &self.fixed_parameters_type
    }
    pub fn fixed_parameters(&self) -> &Arc<[ParameterValueFixed<T::Image, T::Audio>]> {
        &self.fixed_parameters
    }
    pub fn fixed_parameters_mut(&mut self) -> &mut [ParameterValueFixed<T::Image, T::Audio>] {
        // TODO: if let Some(_) = Arc::get_mut(_)でやると、ライフタイムのエラーになる　polonius待ち
        if Arc::get_mut(&mut self.fixed_parameters).is_none() {
            self.fixed_parameters = <&[_]>::into(&self.fixed_parameters);
        }
        Arc::get_mut(&mut self.fixed_parameters).unwrap()
    }
    pub fn variable_parameters_type(&self) -> &[(String, Parameter<Type>)] {
        &self.variable_parameters_type
    }
    pub fn variable_parameters(&self) -> &VectorSync<VariableParameterValue<ParameterNullableValue<T>>> {
        &self.variable_parameters
    }
    pub fn variable_parameters_mut(&mut self) -> &mut VectorSync<VariableParameterValue<ParameterNullableValue<T>>> {
        &mut self.variable_parameters
    }
    pub fn processor(&self) -> &ComponentProcessorWrapper<T> {
        &self.processor
    }
}
