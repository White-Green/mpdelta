use async_trait::async_trait;
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::core::Editor;
use mpdelta_core::edit::{InstanceEditCommand, RootComponentEditCommand};
use mpdelta_core::project::RootComponentClass;
use mpdelta_core::ptr::StaticPointer;
use thiserror::Error;
use tokio::sync::RwLock;

pub struct ProjectEditor {}

pub enum ProjectEditLog {}

#[derive(Debug, Error)]
pub enum ProjectEditError {}

#[async_trait]
impl<T> Editor<T> for ProjectEditor {
    type Log = ProjectEditLog;
    type Err = ProjectEditError;

    async fn edit(&self, target: &StaticPointer<RwLock<RootComponentClass<T>>>, command: RootComponentEditCommand) -> Result<Self::Log, Self::Err> {
        match command {}
    }

    async fn edit_instance(&self, root: &StaticPointer<RwLock<RootComponentClass<T>>>, target: &StaticPointer<RwLock<ComponentInstance<T>>>, command: InstanceEditCommand) -> Result<Self::Log, Self::Err> {
        match command {}
    }

    async fn edit_reverse(&self, log: &Self::Log) {
        match *log {}
    }

    async fn edit_by_log(&self, log: &Self::Log) {
        match *log {}
    }
}
