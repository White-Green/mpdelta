use crate::component::class::ComponentClass;
use crate::component::instance::ComponentInstance;
use crate::component::parameter::{Parameter, ParameterSelect, ParameterValueType};
use crate::edit::{InstanceEditCommand, RootComponentEditCommand};
use crate::project::{Project, RootComponentClass};
use crate::ptr::StaticPointer;
use async_trait::async_trait;
use qcell::TCell;
use std::borrow::Cow;
use std::error::Error;
use std::path::Path;
use tokio::sync::RwLock;

#[async_trait]
pub trait LoadProjectUsecase<K, T> {
    type Err: Error + 'static;
    async fn load_project(&self, path: impl AsRef<Path> + Send + Sync) -> Result<StaticPointer<RwLock<Project<K, T>>>, Self::Err>;
}

#[async_trait]
pub trait WriteProjectUsecase<K, T> {
    type Err: Error + 'static;
    async fn write_project(&self, project: &StaticPointer<RwLock<Project<K, T>>>, path: impl AsRef<Path> + Send + Sync) -> Result<(), Self::Err>;
}

#[async_trait]
pub trait NewProjectUsecase<K, T> {
    async fn new_project(&self) -> StaticPointer<RwLock<Project<K, T>>>;
}

#[async_trait]
pub trait NewRootComponentClassUsecase<K, T> {
    async fn new_root_component_class(&self) -> StaticPointer<RwLock<RootComponentClass<K, T>>>;
}

#[async_trait]
pub trait SetOwnerForRootComponentClassUsecase<K, T> {
    async fn set_owner_for_root_component_class(&self, component: &StaticPointer<RwLock<RootComponentClass<K, T>>>, owner: &StaticPointer<RwLock<Project<K, T>>>);
}

#[async_trait]
pub trait GetLoadedProjectsUsecase<K, T> {
    async fn get_loaded_projects(&self) -> Cow<[StaticPointer<RwLock<Project<K, T>>>]>;
}

#[async_trait]
pub trait GetRootComponentClassesUsecase<K, T> {
    async fn get_root_component_classes(&self, project: &StaticPointer<RwLock<Project<K, T>>>) -> Cow<[StaticPointer<RwLock<RootComponentClass<K, T>>>]>;
}

#[async_trait]
pub trait GetAvailableComponentClassesUsecase<K, T> {
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<K, T>>>]>;
}

pub trait RealtimeComponentRenderer<T: ParameterValueType> {
    type Err: Error + 'static;
    fn get_frame_count(&self) -> usize;
    fn render_frame(&self, frame: usize) -> Result<T::Image, Self::Err>;
    fn sampling_rate(&self) -> u32;
    fn mix_audio(&self, offset: usize, length: usize) -> Result<T::Audio, Self::Err>;
    fn render_param(&self, param: Parameter<ParameterSelect>) -> Result<Parameter<T>, Self::Err>;
}

#[async_trait]
pub trait RealtimeRenderComponentUsecase<K, T: ParameterValueType> {
    type Err: Error + 'static;
    type Renderer: RealtimeComponentRenderer<T>;
    async fn render_component(&self, component: &StaticPointer<TCell<K, ComponentInstance<K, T>>>) -> Result<Self::Renderer, Self::Err>;
}

#[async_trait]
pub trait EditUsecase<K, T> {
    type Err: Error + 'static;
    async fn edit(&self, target: &StaticPointer<RwLock<RootComponentClass<K, T>>>, command: RootComponentEditCommand<K, T>) -> Result<(), Self::Err>;
    async fn edit_instance(&self, root: &StaticPointer<RwLock<RootComponentClass<K, T>>>, target: &StaticPointer<TCell<K, ComponentInstance<K, T>>>, command: InstanceEditCommand) -> Result<(), Self::Err>;
}

#[async_trait]
pub trait UndoUsecase<K, T> {
    async fn undo(&self, component: &StaticPointer<RwLock<RootComponentClass<K, T>>>) -> bool;
    async fn undo_instance(&self, root: &StaticPointer<RwLock<RootComponentClass<K, T>>>, target: &StaticPointer<TCell<K, ComponentInstance<K, T>>>) -> bool;
}

#[async_trait]
pub trait RedoUsecase<K, T> {
    async fn redo(&self, component: &StaticPointer<RwLock<RootComponentClass<K, T>>>) -> bool;
    async fn redo_instance(&self, root: &StaticPointer<RwLock<RootComponentClass<K, T>>>, target: &StaticPointer<TCell<K, ComponentInstance<K, T>>>) -> bool;
}

// 必須じゃないから後で
// #[async_trait]
// pub trait LoadSettingsUsecase {
//     async fn load_settings(&self);
// }
//
// #[async_trait]
// pub trait WriteSettingsUsecase {
//     async fn write_settings(&self);
// }
