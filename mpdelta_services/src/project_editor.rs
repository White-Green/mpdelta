use async_trait::async_trait;
use futures::FutureExt;
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::core::Editor;
use mpdelta_core::edit::{InstanceEditCommand, RootComponentEditCommand};
use mpdelta_core::project::RootComponentClass;
use mpdelta_core::ptr::StaticPointer;
use thiserror::Error;
use tokio::sync::RwLock;

pub struct ProjectEditor {}

impl ProjectEditor {
    pub fn new() -> ProjectEditor {
        ProjectEditor {}
    }
}

pub enum ProjectEditLog {
    Unimplemented,
}

#[derive(Debug, Error)]
pub enum ProjectEditError {
    #[error("invalid target")]
    InvalidTarget,
}

#[async_trait]
impl<T> Editor<T> for ProjectEditor {
    type Log = ProjectEditLog;
    type Err = ProjectEditError;

    async fn edit(&self, target: &StaticPointer<RwLock<RootComponentClass<T>>>, command: RootComponentEditCommand<T>) -> Result<Self::Log, Self::Err> {
        let target = target.upgrade().ok_or(ProjectEditError::InvalidTarget)?;
        let target = target.write().await;
        match command {
            RootComponentEditCommand::AddComponentInstance(instance) => {
                target
                    .map(move |components: &mut _, _: &mut _| {
                        async {
                            components.push(instance);
                        }
                        .boxed()
                    })
                    .await;
                Ok(ProjectEditLog::Unimplemented)
            }
        }
    }

    async fn edit_instance(&self, root: &StaticPointer<RwLock<RootComponentClass<T>>>, target: &StaticPointer<RwLock<ComponentInstance<T>>>, command: InstanceEditCommand) -> Result<Self::Log, Self::Err> {
        match command {}
    }

    async fn edit_reverse(&self, log: &Self::Log) {
        match log {
            ProjectEditLog::Unimplemented => eprintln!("unimplemented"),
        }
    }

    async fn edit_by_log(&self, log: &Self::Log) {
        match log {
            ProjectEditLog::Unimplemented => eprintln!("unimplemented"),
        }
    }
}
