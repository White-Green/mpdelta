use async_trait::async_trait;
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::link::MarkerLink;
use mpdelta_core::core::Editor;
use mpdelta_core::edit::{InstanceEditCommand, RootComponentEditCommand};
use mpdelta_core::project::RootComponentClass;
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use mpdelta_core::time::TimelineTime;
use qcell::{TCell, TCellOwner};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

pub struct ProjectEditor<K: 'static> {
    key: Arc<RwLock<TCellOwner<K>>>,
}

impl<K> ProjectEditor<K> {
    pub fn new(key: Arc<RwLock<TCellOwner<K>>>) -> ProjectEditor<K> {
        ProjectEditor { key }
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
impl<K, T> Editor<K, T> for ProjectEditor<K> {
    type Log = ProjectEditLog;
    type Err = ProjectEditError;

    async fn edit(&self, target: &StaticPointer<RwLock<RootComponentClass<K, T>>>, command: RootComponentEditCommand<K, T>) -> Result<Self::Log, Self::Err> {
        let target = target.upgrade().ok_or(ProjectEditError::InvalidTarget)?;
        let target = target.read().await;
        match command {
            RootComponentEditCommand::AddComponentInstance(instance) => {
                let key = self.key.read().await;
                let base = if let Some(base) = target.get().await.component().last() { base.ro(&key).marker_left().reference() } else { target.left().await };
                let guard = instance.ro(&key);
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
                item.link_mut().extend([StaticPointerOwned::new(TCell::new(link_for_zero)), StaticPointerOwned::new(TCell::new(link_for_length))]);
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::RemoveMarkerLink(link) => {
                target.get_mut().await.link_mut().retain(|l| *l != link);
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::EditMarkerLinkLength(link, len) => {
                if let Some(link) = link.upgrade() {
                    let mut key = self.key.write().await;
                    link.rw(&mut key).len = len;
                }
                Ok(ProjectEditLog::Unimplemented)
            }
        }
    }

    async fn edit_instance(&self, _root: &StaticPointer<RwLock<RootComponentClass<K, T>>>, _target: &StaticPointer<TCell<K, ComponentInstance<K, T>>>, command: InstanceEditCommand) -> Result<Self::Log, Self::Err> {
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
