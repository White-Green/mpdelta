use crate::component::class::ComponentClass;
use crate::component::instance::ComponentInstance;
use crate::component::parameter::{Parameter, ParameterSelect, ParameterValueType};
use crate::edit::{InstanceEditCommand, RootComponentEditCommand};
use crate::project::{Project, RootComponentClass};
use crate::ptr::StaticPointer;
use async_trait::async_trait;
use std::borrow::Cow;
use std::error::Error;
use std::path::Path;
use tokio::sync::RwLock;

#[async_trait]
pub trait LoadProjectUsecase<T> {
    type Err: Error + 'static;
    async fn load_project(&self, path: impl AsRef<Path> + Send + Sync) -> Result<StaticPointer<RwLock<Project<T>>>, Self::Err>;
}

#[async_trait]
pub trait WriteProjectUsecase<T> {
    type Err: Error + 'static;
    async fn write_project(&self, project: &StaticPointer<RwLock<Project<T>>>, path: impl AsRef<Path> + Send + Sync) -> Result<(), Self::Err>;
}

#[async_trait]
pub trait NewProjectUsecase<T> {
    async fn new_project(&self) -> StaticPointer<RwLock<Project<T>>>;
}

#[async_trait]
pub trait NewRootComponentClassUsecase<T> {
    async fn new_root_component_class(&self) -> StaticPointer<RwLock<RootComponentClass<T>>>;
}

#[async_trait]
pub trait SetOwnerForRootComponentClassUsecase<T> {
    async fn set_owner_for_root_component_class(&self, component: &StaticPointer<RwLock<RootComponentClass<T>>>, owner: &StaticPointer<RwLock<Project<T>>>);
}

#[async_trait]
pub trait GetLoadedProjectsUsecase<T> {
    async fn get_loaded_projects(&self) -> Cow<[StaticPointer<RwLock<Project<T>>>]>;
}

#[async_trait]
pub trait GetRootComponentClassesUsecase<T> {
    async fn get_root_component_classes(&self, project: &StaticPointer<RwLock<Project<T>>>) -> Cow<[StaticPointer<RwLock<RootComponentClass<T>>>]>;
}

#[async_trait]
pub trait GetAvailableComponentClassesUsecase<T> {
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<T>>>]>;
}

#[async_trait]
pub trait RealtimeComponentRenderer<T: ParameterValueType<'static>> {
    fn get_frame_count(&self) -> usize;
    async fn render_frame(&mut self, frame: usize) -> T::Image;
    fn sampling_rate(&self) -> u32;
    async fn mix_audio(&mut self, offset: usize, length: usize) -> T::Audio;
    async fn render_param(&mut self, param: Parameter<'static, ParameterSelect>) -> Parameter<'static, T>;
}

#[async_trait]
pub trait RealtimeRenderComponentUsecase<T: ParameterValueType<'static>> {
    type Renderer: RealtimeComponentRenderer<T>;
    async fn render_component(&self, component: &StaticPointer<RwLock<ComponentInstance<T>>>) -> Self::Renderer;
}

#[async_trait]
pub trait EditUsecase<T> {
    type Err: Error + 'static;
    async fn edit(&self, target: &StaticPointer<RwLock<RootComponentClass<T>>>, command: RootComponentEditCommand) -> Result<(), Self::Err>;
    async fn edit_instance(&self, root: &StaticPointer<RwLock<RootComponentClass<T>>>, target: &StaticPointer<RwLock<ComponentInstance<T>>>, command: InstanceEditCommand) -> Result<(), Self::Err>;
}

#[async_trait]
pub trait UndoUsecase<T> {
    async fn undo(&self, component: &StaticPointer<RwLock<RootComponentClass<T>>>) -> bool;
    async fn undo_instance(&self, root: &StaticPointer<RwLock<RootComponentClass<T>>>, target: &StaticPointer<RwLock<ComponentInstance<T>>>) -> bool;
}

#[async_trait]
pub trait RedoUsecase<T> {
    async fn redo(&self, component: &StaticPointer<RwLock<RootComponentClass<T>>>) -> bool;
    async fn redo_instance(&self, root: &StaticPointer<RwLock<RootComponentClass<T>>>, target: &StaticPointer<RwLock<ComponentInstance<T>>>) -> bool;
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
