use async_trait::async_trait;
use dashmap::DashMap;
use mpdelta_core::component::instance::ComponentInstanceHandle;
use mpdelta_core::component::link::MarkerLink;
use mpdelta_core::core::{EditEventListener, Editor};
use mpdelta_core::edit::{InstanceEditCommand, InstanceEditEvent, RootComponentEditCommand, RootComponentEditEvent};
use mpdelta_core::project::RootComponentClassHandle;
use mpdelta_core::ptr::StaticPointerOwned;
use mpdelta_core::time::TimelineTime;
use qcell::{TCell, TCellOwner};
use std::sync::atomic::AtomicUsize;
use std::sync::{atomic, Arc};
use thiserror::Error;
use tokio::sync::RwLock;

// TODO: Listenerをusizeで管理してるので、overflowしたらバグる(ほとんど気にしなくても良さそうではあるが)
pub struct ProjectEditor<K: 'static, T> {
    key: Arc<RwLock<TCellOwner<K>>>,
    edit_event_listener_id: AtomicUsize,
    edit_event_listeners: Arc<DashMap<usize, Box<dyn EditEventListener<K, T>>>>,
}

impl<K, T> ProjectEditor<K, T> {
    pub fn new(key: Arc<RwLock<TCellOwner<K>>>) -> ProjectEditor<K, T> {
        ProjectEditor {
            key,
            edit_event_listener_id: AtomicUsize::default(),
            edit_event_listeners: Arc::new(DashMap::new()),
        }
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

pub struct ProjectEditListenerGuard<K: 'static, T> {
    id: usize,
    edit_event_listeners: Arc<DashMap<usize, Box<dyn EditEventListener<K, T>>>>,
}

impl<K, T> Drop for ProjectEditListenerGuard<K, T> {
    fn drop(&mut self) {
        self.edit_event_listeners.remove(&self.id);
    }
}

#[async_trait]
impl<K, T: 'static> Editor<K, T> for ProjectEditor<K, T> {
    type Log = ProjectEditLog;
    type Err = ProjectEditError;
    type EditEventListenerGuard = ProjectEditListenerGuard<K, T>;

    fn add_edit_event_listener(&self, listener: impl EditEventListener<K, T> + 'static) -> Self::EditEventListenerGuard {
        let id = self.edit_event_listener_id.fetch_add(1, atomic::Ordering::AcqRel);
        self.edit_event_listeners.insert(id, Box::new(listener));
        ProjectEditListenerGuard {
            id,
            edit_event_listeners: Arc::clone(&self.edit_event_listeners),
        }
    }

    async fn edit(&self, target_ref: &RootComponentClassHandle<K, T>, command: RootComponentEditCommand<K, T>) -> Result<Self::Log, Self::Err> {
        let target = target_ref.upgrade().ok_or(ProjectEditError::InvalidTarget)?;
        let target = target.read().await;
        match command {
            RootComponentEditCommand::AddComponentInstance(instance) => {
                let instance_ref = StaticPointerOwned::reference(&instance).clone();
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
                let mut item = target.get_mut().await;
                item.component_mut().push(instance);
                item.link_mut().extend([StaticPointerOwned::new(TCell::new(link_for_zero)), StaticPointerOwned::new(TCell::new(link_for_length))]);
                drop(item);
                drop(target);
                drop(key);
                // TODO: このへんもうちょっとバグりにくい構造を探したいよね
                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::AddComponentInstance(&instance_ref)));
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::RemoveMarkerLink(link) => {
                target.get_mut().await.link_mut().retain(|l| *l != link);
                drop(target);
                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::RemoveMarkerLink(&link)));
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::EditMarkerLinkLength(link, len) => {
                if let Some(link) = link.upgrade() {
                    let mut key = self.key.write().await;
                    link.rw(&mut key).len = len;
                }
                drop(target);
                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::EditMarkerLinkLength(&link, len)));
                Ok(ProjectEditLog::Unimplemented)
            }
        }
    }

    async fn edit_instance(&self, root_ref: &RootComponentClassHandle<K, T>, target_ref: &ComponentInstanceHandle<K, T>, command: InstanceEditCommand<K, T>) -> Result<Self::Log, Self::Err> {
        let target = target_ref.upgrade().ok_or(ProjectEditError::InvalidTarget)?;
        match command {
            InstanceEditCommand::UpdateImageRequiredParams(params) => {
                let mut key = self.key.write().await;
                target.rw(&mut key).set_image_required_params(params.clone());
                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::UpdateImageRequiredParams(&params)));
                Ok(ProjectEditLog::Unimplemented)
            }
        }
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
