use crate::component::class::ComponentClass;
use crate::component::instance::ComponentInstance;
use crate::component::link::MarkerLink;
use crate::edit::EditCommand;
use crate::project::{Project, RootComponentClass};
use crate::ptr::StaticPointer;
use crate::usecase::{
    EditUsecase, GetAvailableComponentClassesUsecase, GetInstancesUsecase, GetLoadedProjectsUsecase, GetProjectPathUsecase, GetRootComponentClassesUsecase, LoadProjectUsecase, NewProjectUsecase, NewRootComponentClassUsecase, RedoUsecase, RenderFrameUsecase, UndoUsecase, WriteProjectUsecase,
};
use async_trait::async_trait;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::sync::RwLock;

pub struct MPDeltaCore {}

#[derive(Debug)]
pub enum Infallible {}

impl Display for Infallible {
    fn fmt(&self, _: &mut Formatter<'_>) -> std::fmt::Result {
        unreachable!()
    }
}

impl Error for Infallible {}

#[async_trait]
impl LoadProjectUsecase for MPDeltaCore {
    type Err = Infallible;

    async fn load_project(&self, path: &Path) -> Result<StaticPointer<RwLock<Project>>, Self::Err> {
        todo!()
    }
}

#[async_trait]
impl WriteProjectUsecase for MPDeltaCore {
    type Err = Infallible;

    async fn write_project(&self, project: &StaticPointer<RwLock<Project>>, path: &Path) -> Result<(), Self::Err> {
        todo!()
    }
}

#[async_trait]
impl NewProjectUsecase for MPDeltaCore {
    async fn new_project(&self) -> StaticPointer<RwLock<Project>> {
        todo!()
    }
}

#[async_trait]
impl NewRootComponentClassUsecase for MPDeltaCore {
    async fn new_root_component_class(&self, project: &StaticPointer<RwLock<Project>>) -> StaticPointer<RwLock<RootComponentClass>> {
        todo!()
    }
}

#[async_trait]
impl GetLoadedProjectsUsecase for MPDeltaCore {
    async fn get_loaded_projects(&self) -> Box<[StaticPointer<RwLock<Project>>]> {
        todo!()
    }
}

#[async_trait]
impl GetRootComponentClassesUsecase for MPDeltaCore {
    async fn get_root_component_classes(&self, project: &StaticPointer<RwLock<Project>>) -> &[StaticPointer<RwLock<RootComponentClass>>] {
        todo!()
    }
}

#[async_trait]
impl GetProjectPathUsecase for MPDeltaCore {
    async fn get_project_path(&self, project: &StaticPointer<RwLock<Project>>) -> Option<&Path> {
        todo!()
    }
}

#[async_trait]
impl<T> GetInstancesUsecase<T> for MPDeltaCore {
    async fn get_instances(&self, component: &StaticPointer<RwLock<RootComponentClass>>) -> (&[StaticPointer<RwLock<ComponentInstance<T>>>], &[StaticPointer<RwLock<MarkerLink>>]) {
        todo!()
    }
}

#[async_trait]
impl<T> GetAvailableComponentClassesUsecase<T> for MPDeltaCore {
    async fn get_available_component_classes(&self) -> &[StaticPointer<RwLock<ComponentClass<T>>>] {
        todo!()
    }
}

#[async_trait]
impl RenderFrameUsecase for MPDeltaCore {
    async fn render_frame(&self) {
        todo!()
    }
}

#[async_trait]
impl EditUsecase for MPDeltaCore {
    type Err = Infallible;

    async fn edit(&self, command: EditCommand) -> Result<(), Self::Err> {
        todo!()
    }
}

#[async_trait]
impl UndoUsecase for MPDeltaCore {
    async fn undo(&self, component: &StaticPointer<RwLock<RootComponentClass>>) -> bool {
        todo!()
    }
}

#[async_trait]
impl RedoUsecase for MPDeltaCore {
    async fn redo(&self, component: &StaticPointer<RwLock<RootComponentClass>>) -> bool {
        todo!()
    }
}
