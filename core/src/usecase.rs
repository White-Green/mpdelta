use crate::component::class::ComponentClass;
use crate::component::instance::ComponentInstance;
use crate::component::link::MarkerLink;
use crate::edit::EditCommand;
use crate::project::{Project, RootComponentClass};
use crate::ptr::StaticPointer;
use async_trait::async_trait;
use std::error::Error;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

#[async_trait]
pub trait LoadProjectUsecase {
    type Err: Error;
    async fn load_project(self: Arc<Self>, path: PathBuf) -> Result<StaticPointer<RwLock<Project>>, Self::Err>;
}

#[async_trait]
pub trait WriteProjectUsecase {
    type Err: Error;
    async fn write_project(self: Arc<Self>, project: StaticPointer<RwLock<Project>>, path: PathBuf) -> Result<(), Self::Err>;
}

#[async_trait]
pub trait NewProjectUsecase {
    async fn new_project(self: Arc<Self>) -> StaticPointer<RwLock<Project>>;
}

#[async_trait]
pub trait NewRootComponentClassUsecase {
    async fn new_root_component_class(self: Arc<Self>, project: StaticPointer<RwLock<Project>>) -> StaticPointer<RwLock<RootComponentClass>>;
}

#[async_trait]
pub trait GetLoadedProjectsUsecase {
    async fn get_loaded_projects(self: Arc<Self>) -> Box<[StaticPointer<Project>]>;
}

#[async_trait]
pub trait GetRootComponentClassesUsecase {
    async fn get_root_component_classes(self: Arc<Self>, project: StaticPointer<RwLock<Project>>) -> Box<[StaticPointer<RootComponentClass>]>;
}

#[async_trait]
pub trait GetProjectPathUsecase {
    async fn get_project_path(self: Arc<Self>, project: StaticPointer<Project>) -> Option<PathBuf>;
}

#[async_trait]
pub trait GetInstancesUsecase<T> {
    async fn get_instances(self: Arc<Self>, component: StaticPointer<RwLock<RootComponentClass>>) -> (Box<[StaticPointer<RwLock<ComponentInstance<T>>>]>, Box<[StaticPointer<RwLock<MarkerLink>>]>);
}

#[async_trait]
pub trait GetAvailableComponentClassesUsecase<T> {
    async fn get_available_component_classes(self: Arc<Self>) -> Box<[StaticPointer<RwLock<ComponentClass<T>>>]>;
}

#[async_trait]
pub trait RenderFrameUsecase {
    async fn render_frame(self: Arc<Self>); // TODO
}

#[async_trait]
pub trait MixAudioUsecase {
    async fn mix_audio(self: Arc<Self>); // TODO
}

#[async_trait]
pub trait EditUsecase {
    type Err: Error;
    async fn edit(self: Arc<Self>, command: EditCommand) -> Result<(), Self::Err>;
}

#[async_trait]
pub trait UndoUsecase {
    async fn undo(self: Arc<Self>, component: StaticPointer<RwLock<RootComponentClass>>) -> bool;
}

#[async_trait]
pub trait RedoUsecase {
    async fn redo(self: Arc<Self>, component: StaticPointer<RwLock<RootComponentClass>>) -> bool;
}

// 必須じゃないから後で
// #[async_trait]
// pub trait LoadSettingsUsecase {
//     async fn load_settings(self: Arc<Self>);
// }
//
// #[async_trait]
// pub trait WriteSettingsUsecase {
//     async fn write_settings(self: Arc<Self>);
// }
