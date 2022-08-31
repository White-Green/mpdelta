use crate::component::instance::ComponentInstance;
use crate::component::link::MarkerLink;
use crate::ptr::{StaticPointer, StaticPointerOwned};
use crate::time::TimelineTime;
use tokio::sync::RwLock;

pub enum RootComponentEditCommand<T> {
    AddComponentInstance(StaticPointerOwned<RwLock<ComponentInstance<T>>>),
    RemoveMarkerLink(StaticPointer<RwLock<MarkerLink>>),
    EditMarkerLinkLength(StaticPointer<RwLock<MarkerLink>>, TimelineTime),
}

pub enum InstanceEditCommand {/* TODO */}
