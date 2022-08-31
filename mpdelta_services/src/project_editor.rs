use async_trait::async_trait;
use futures::FutureExt;
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::link::MarkerLink;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerTime};
use mpdelta_core::core::Editor;
use mpdelta_core::edit::{InstanceEditCommand, RootComponentEditCommand};
use mpdelta_core::project::RootComponentClass;
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use mpdelta_core::time::TimelineTime;
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
        let target = target.read().await;
        match command {
            RootComponentEditCommand::AddComponentInstance(instance) => {
                let base = if let Some(base) = target.get().await.component().last() { base.read().await.marker_left().reference() } else { target.left().await };
                let guard = instance.read().await;
                let left = guard.marker_left();
                let right = guard.marker_right();
                let link_for_zero = MarkerLink {
                    from: base,
                    to: left.reference(),
                    len: TimelineTime::new(1.0).unwrap(),
                };
                let link_for_length = MarkerLink {
                    from: left.reference(),
                    to: right.reference(),
                    len: TimelineTime::new(1.0).unwrap(),
                };
                drop(guard);
                let mut item = target.get_mut().await;
                item.component_mut().push(instance);
                item.link_mut().extend([StaticPointerOwned::new(RwLock::new(link_for_zero)), StaticPointerOwned::new(RwLock::new(link_for_length))]);
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::RemoveMarkerLink(link) => {
                target.get_mut().await.link_mut().retain(|l| *l != link);
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::EditMarkerLinkLength(link, len) => {
                if let Some(link) = link.upgrade() {
                    link.write().await.len = len;
                }
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
