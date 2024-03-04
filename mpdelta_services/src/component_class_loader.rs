use async_trait::async_trait;
use mpdelta_core::component::class::{ComponentClass, ComponentClassIdentifier};
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::ComponentClassLoader;
use mpdelta_core::ptr::StaticPointer;
use std::borrow::Cow;
use tokio::sync::RwLock;

pub struct TemporaryComponentClassLoader;

#[async_trait]
impl<K, T: ParameterValueType> ComponentClassLoader<K, T> for TemporaryComponentClassLoader {
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<K, T>>>]> {
        Cow::Borrowed(&[])
    }

    async fn component_class_by_identifier(&self, _identifier: ComponentClassIdentifier<'_>) -> Option<StaticPointer<RwLock<dyn ComponentClass<K, T>>>> {
        None
    }
}
