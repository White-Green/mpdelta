use crate::component::instance::ComponentInstance;
use crate::component::parameter::{ParameterType, ParameterValueType};
use crate::ptr::StaticPointer;
use async_trait::async_trait;
use tokio::sync::RwLock;

#[async_trait]
pub trait ComponentClass<K, T: ParameterValueType>: Send + Sync {
    async fn generate_image(&self) -> bool;
    async fn generate_audio(&self) -> bool;
    async fn fixed_parameter_type(&self) -> &[(String, ParameterType)];
    async fn default_variable_parameter_type(&self) -> &[(String, ParameterType)];
    async fn instantiate(&self, this: &StaticPointer<RwLock<dyn ComponentClass<K, T>>>) -> ComponentInstance<K, T>;
}
