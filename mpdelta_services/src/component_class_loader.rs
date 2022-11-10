use async_trait::async_trait;
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::core::ComponentClassLoader;
use mpdelta_core::ptr::StaticPointer;
use std::borrow::Cow;
use tokio::sync::RwLock;

pub struct TemporaryComponentClassLoader;

#[async_trait]
impl<K, T> ComponentClassLoader<K, T> for TemporaryComponentClassLoader {
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<K, T>>>]> {
        Cow::Borrowed(&[])
    }
}
