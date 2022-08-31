use crate::component::instance::ComponentInstance;
use crate::component::link::MarkerLink;
use crate::ptr::{StaticPointer, StaticPointerOwned};
use tokio::sync::RwLock;

pub enum RootComponentEditCommand<T> {
    AddComponentInstance(StaticPointerOwned<RwLock<ComponentInstance<T>>>),
    RemoveMarkerLink(StaticPointer<RwLock<MarkerLink>>),
}

pub enum InstanceEditCommand {/* TODO */}
