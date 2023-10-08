use crate::component::class::ComponentClass;
use crate::component::instance::ComponentInstanceHandleCow;
use crate::component::link::MarkerLink;
use crate::component::marker_pin::MarkerTime;
use crate::component::parameter::placeholder::{Placeholder, TagAudio, TagImage};
use crate::component::parameter::value::FrameVariableValue;
use crate::component::parameter::{AbstractFile, ComponentProcessorInputValue, Never, Parameter, ParameterFrameVariableValue, ParameterSelect, ParameterType, ParameterValueFixed, ParameterValueType};
use crate::native::processor::NativeProcessor;
use crate::ptr::{StaticPointer, StaticPointerCow};
use crate::time::TimelineTime;
use async_trait::async_trait;
use qcell::TCell;
use std::borrow::Cow;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

pub struct ComponentsLinksPair<K: 'static, T: ParameterValueType>(pub Vec<ComponentInstanceHandleCow<K, T>>, pub Vec<StaticPointerCow<TCell<K, MarkerLink<K>>>>);

#[derive(Clone)]
pub enum ComponentProcessorWrapper<K, T: ParameterValueType> {
    Native(Arc<dyn ComponentProcessorNative<K, T>>),
    Component(Arc<dyn ComponentProcessorComponent<K, T>>),
    GatherNative(Arc<dyn ComponentProcessorGatherNative<K, T>>),
    GatherComponent(Arc<dyn ComponentProcessorGatherComponent<K, T>>),
}

impl<K, T> From<Arc<dyn ComponentProcessorNative<K, T>>> for ComponentProcessorWrapper<K, T>
where
    T: ParameterValueType,
{
    fn from(value: Arc<dyn ComponentProcessorNative<K, T>>) -> ComponentProcessorWrapper<K, T> {
        ComponentProcessorWrapper::Native(value)
    }
}

impl<K, T> From<Arc<dyn ComponentProcessorComponent<K, T>>> for ComponentProcessorWrapper<K, T>
where
    T: ParameterValueType,
{
    fn from(value: Arc<dyn ComponentProcessorComponent<K, T>>) -> ComponentProcessorWrapper<K, T> {
        ComponentProcessorWrapper::Component(value)
    }
}

impl<K, T> From<Arc<dyn ComponentProcessorGatherNative<K, T>>> for ComponentProcessorWrapper<K, T>
where
    T: ParameterValueType,
{
    fn from(value: Arc<dyn ComponentProcessorGatherNative<K, T>>) -> ComponentProcessorWrapper<K, T> {
        ComponentProcessorWrapper::GatherNative(value)
    }
}

impl<K, T> From<Arc<dyn ComponentProcessorGatherComponent<K, T>>> for ComponentProcessorWrapper<K, T>
where
    T: ParameterValueType,
{
    fn from(value: Arc<dyn ComponentProcessorGatherComponent<K, T>>) -> ComponentProcessorWrapper<K, T> {
        ComponentProcessorWrapper::GatherComponent(value)
    }
}

#[async_trait]
pub trait ComponentProcessor<K, T: ParameterValueType>: Send + Sync {
    async fn update_variable_parameter(&self, fixed_params: &mut [ParameterValueFixed<T::Image, T::Audio>], variable_parameters: &mut Vec<(String, ParameterType)>);
    async fn natural_length(&self, fixed_params: &[ParameterValueFixed<T::Image, T::Audio>]) -> Duration;
}

#[async_trait]
pub trait ComponentProcessorNative<K, T: ParameterValueType>: ComponentProcessor<K, T> {
    fn supports_output_type(&self, out: Parameter<ParameterSelect>) -> bool;
    async fn process(&self, fixed_parameters: &[ParameterValueFixed<T::Image, T::Audio>], variable_parameters: &[ComponentProcessorInputValue], variable_parameter_type: &[(String, ParameterType)], time: TimelineTime, output_type: ParameterSelect) -> ParameterValueFixed<T::Image, T::Audio>;
}

#[async_trait]
pub trait ComponentProcessorComponent<K, T: ParameterValueType>: ComponentProcessor<K, T> {
    async fn process(
        &self,
        fixed_parameters: &[ParameterValueFixed<T::Image, T::Audio>],
        fixed_parameters_component: &[StaticPointer<RwLock<dyn ComponentClass<K, T>>>],
        variable_parameters: &[StaticPointer<RwLock<dyn ComponentClass<K, T>>>],
        variable_parameter_type: &[(String, ParameterType)],
    ) -> ComponentsLinksPair<K, T>;
}

// TODO:
#[async_trait]
pub trait ComponentProcessorGatherNative<K, T: ParameterValueType>: ComponentProcessor<K, T> {
    fn supports_output_type(&self, out: Parameter<ParameterSelect>) -> bool;
    async fn process(&self) -> ();
}

#[async_trait]
pub trait ComponentProcessorGatherComponent<K, T: ParameterValueType>: ComponentProcessor<K, T> {
    async fn process(
        &self,
        fixed_parameters: &[ParameterValueFixed<T::Image, T::Audio>],
        fixed_parameters_component: &[StaticPointer<RwLock<dyn ComponentClass<K, T>>>],
        variable_parameters: &[StaticPointer<RwLock<dyn ComponentClass<K, T>>>],
        variable_parameter_type: &[(String, ParameterType)],
    ) -> ComponentsLinksPair<K, T>;
}
