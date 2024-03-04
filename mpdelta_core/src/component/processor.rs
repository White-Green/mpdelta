use crate::component::class::ComponentClass;
use crate::component::instance::ComponentInstanceHandleCow;
use crate::component::link::MarkerLinkHandleCow;
use crate::component::marker_pin::MarkerTime;
use crate::component::parameter::{Parameter, ParameterSelect, ParameterType, ParameterValueRaw, ParameterValueType};
use crate::ptr::StaticPointer;
use crate::time::TimelineTime;
use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ComponentsLinksPair<K: 'static, T: ParameterValueType>(pub Vec<ComponentInstanceHandleCow<K, T>>, pub Vec<MarkerLinkHandleCow<K>>);

#[derive(Clone)]
pub enum ComponentProcessorWrapper<K, T: ParameterValueType> {
    Native(Arc<dyn ComponentProcessorNative<K, T>>),
    Component(Arc<dyn ComponentProcessorComponent<K, T>>),
    GatherNative(Arc<dyn ComponentProcessorGatherNative<K, T>>),
    GatherComponent(Arc<dyn ComponentProcessorGatherComponent<K, T>>),
}

impl<K, T: ParameterValueType> ComponentProcessor<K, T> for ComponentProcessorWrapper<K, T> {
    fn fixed_parameter_types<'life0, 'async_trait>(&'life0 self) -> Pin<Box<dyn Future<Output = &'life0 [(String, ParameterType)]> + Send + 'async_trait>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
    {
        match self {
            ComponentProcessorWrapper::Native(processor) => processor.fixed_parameter_types(),
            ComponentProcessorWrapper::Component(processor) => processor.fixed_parameter_types(),
            ComponentProcessorWrapper::GatherNative(processor) => processor.fixed_parameter_types(),
            ComponentProcessorWrapper::GatherComponent(processor) => processor.fixed_parameter_types(),
        }
    }

    fn update_variable_parameter<'life0, 'life1, 'life2, 'async_trait>(&'life0 self, fixed_params: &'life1 [ParameterValueRaw<T::Image, T::Audio>], variable_parameters: &'life2 mut Vec<(String, ParameterType)>) -> Pin<Box<dyn Future<Output = ()> + Send + 'async_trait>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
    {
        match self {
            ComponentProcessorWrapper::Native(processor) => processor.update_variable_parameter(fixed_params, variable_parameters),
            ComponentProcessorWrapper::Component(processor) => processor.update_variable_parameter(fixed_params, variable_parameters),
            ComponentProcessorWrapper::GatherNative(processor) => processor.update_variable_parameter(fixed_params, variable_parameters),
            ComponentProcessorWrapper::GatherComponent(processor) => processor.update_variable_parameter(fixed_params, variable_parameters),
        }
    }

    fn natural_length<'life0, 'life1, 'async_trait>(&'life0 self, fixed_params: &'life1 [ParameterValueRaw<T::Image, T::Audio>]) -> Pin<Box<dyn Future<Output = MarkerTime> + Send + 'async_trait>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        match self {
            ComponentProcessorWrapper::Native(processor) => processor.natural_length(fixed_params),
            ComponentProcessorWrapper::Component(processor) => processor.natural_length(fixed_params),
            ComponentProcessorWrapper::GatherNative(processor) => processor.natural_length(fixed_params),
            ComponentProcessorWrapper::GatherComponent(processor) => processor.natural_length(fixed_params),
        }
    }
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
    async fn fixed_parameter_types(&self) -> &[(String, ParameterType)];
    async fn update_variable_parameter(&self, fixed_params: &[ParameterValueRaw<T::Image, T::Audio>], variable_parameters: &mut Vec<(String, ParameterType)>);
    async fn natural_length(&self, fixed_params: &[ParameterValueRaw<T::Image, T::Audio>]) -> MarkerTime;
}

#[async_trait]
pub trait ComponentProcessorNative<K, T: ParameterValueType>: ComponentProcessor<K, T> {
    fn supports_output_type(&self, out: Parameter<ParameterSelect>) -> bool;
    async fn process(
        &self,
        fixed_parameters: &[ParameterValueRaw<T::Image, T::Audio>],
        variable_parameters: &[ParameterValueRaw<T::Image, T::Audio>],
        variable_parameter_type: &[(String, ParameterType)],
        time: TimelineTime,
        output_type: Parameter<ParameterSelect>,
    ) -> ParameterValueRaw<T::Image, T::Audio>;
}

#[async_trait]
pub trait ComponentProcessorComponent<K, T: ParameterValueType>: ComponentProcessor<K, T> {
    async fn process(
        &self,
        fixed_parameters: &[ParameterValueRaw<T::Image, T::Audio>],
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
        fixed_parameters: &[ParameterValueRaw<T::Image, T::Audio>],
        fixed_parameters_component: &[StaticPointer<RwLock<dyn ComponentClass<K, T>>>],
        variable_parameters: &[StaticPointer<RwLock<dyn ComponentClass<K, T>>>],
        variable_parameter_type: &[(String, ParameterType)],
    ) -> ComponentsLinksPair<K, T>;
}
