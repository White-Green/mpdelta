use crate::component::class::ComponentClass;
use crate::component::instance::ComponentInstance;
use crate::component::link::MarkerLink;
use crate::edit::EditCommand;
use crate::project::{Project, RootComponentClass};
use crate::ptr::StaticPointer;
use async_trait::async_trait;
use std::error::Error;
use std::path::Path;
use std::sync::RwLock;

#[async_trait]
pub trait LoadProjectUsecase {
    type Err: Error;
    async fn load_project(&self, path: &Path) -> Result<StaticPointer<RwLock<Project>>, Self::Err>;
}

#[async_trait]
pub trait WriteProjectUsecase {
    type Err: Error;
    async fn write_project(&self, project: &StaticPointer<RwLock<Project>>, path: &Path) -> Result<(), Self::Err>;
}

#[async_trait]
pub trait NewProjectUsecase {
    async fn new_project(&self) -> StaticPointer<RwLock<Project>>;
}

#[async_trait]
pub trait NewRootComponentClassUsecase {
    async fn new_root_component_class(&self, project: &StaticPointer<RwLock<Project>>) -> StaticPointer<RwLock<RootComponentClass>>;
}

#[async_trait]
pub trait GetLoadedProjectsUsecase {
    async fn get_loaded_projects(&self) -> Box<[StaticPointer<RwLock<Project>>]>;
}

#[async_trait]
pub trait GetRootComponentClassesUsecase {
    async fn get_root_component_classes(&self, project: &StaticPointer<RwLock<Project>>) -> &[StaticPointer<RwLock<RootComponentClass>>];
}

#[async_trait]
pub trait GetProjectPathUsecase {
    async fn get_project_path(&self, project: &StaticPointer<RwLock<Project>>) -> Option<&Path>;
}

#[async_trait]
pub trait GetInstancesUsecase<T> {
    async fn get_instances(&self, component: &StaticPointer<RwLock<RootComponentClass>>) -> (&[StaticPointer<RwLock<ComponentInstance<T>>>], &[StaticPointer<RwLock<MarkerLink>>]);
}

#[async_trait]
pub trait GetAvailableComponentClassesUsecase<T> {
    async fn get_available_component_classes(&self) -> &[StaticPointer<RwLock<ComponentClass<T>>>];
}

#[async_trait]
pub trait RenderFrameUsecase {
    async fn render_frame(&self); // TODO
}

#[async_trait]
pub trait MixAudioUsecase {
    async fn mix_audio(&self); // TODO
}

#[async_trait]
pub trait EditUsecase {
    type Err: Error;
    async fn edit(&self, command: EditCommand) -> Result<(), Self::Err>;
}

#[async_trait]
pub trait UndoUsecase {
    async fn undo(&self, component: &StaticPointer<RwLock<RootComponentClass>>) -> bool;
}

#[async_trait]
pub trait RedoUsecase {
    async fn redo(&self, component: &StaticPointer<RwLock<RootComponentClass>>) -> bool;
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
