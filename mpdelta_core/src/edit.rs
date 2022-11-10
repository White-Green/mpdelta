use crate::component::instance::ComponentInstance;
use crate::component::link::MarkerLink;
use crate::ptr::{StaticPointer, StaticPointerOwned};
use crate::time::TimelineTime;
use qcell::TCell;

pub enum RootComponentEditCommand<K: 'static, T> {
    AddComponentInstance(StaticPointerOwned<TCell<K, ComponentInstance<K, T>>>),
    RemoveMarkerLink(StaticPointer<TCell<K, MarkerLink<K>>>),
    EditMarkerLinkLength(StaticPointer<TCell<K, MarkerLink<K>>>, TimelineTime),
}

pub enum InstanceEditCommand {/* TODO */}
