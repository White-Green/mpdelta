use crate::component::instance::ComponentInstance;
use crate::component::link::MarkerLink;
use crate::component::parameter::ImageRequiredParams;
use crate::ptr::{StaticPointer, StaticPointerOwned};
use crate::time::TimelineTime;
use qcell::TCell;

pub enum RootComponentEditCommand<K: 'static, T> {
    AddComponentInstance(StaticPointerOwned<TCell<K, ComponentInstance<K, T>>>),
    RemoveMarkerLink(StaticPointer<TCell<K, MarkerLink<K>>>),
    EditMarkerLinkLength(StaticPointer<TCell<K, MarkerLink<K>>>, TimelineTime),
}

pub enum InstanceEditCommand<K: 'static, T> {
    UpdateImageRequiredParams(ImageRequiredParams<K, T>),
}

pub enum RootComponentEditEvent<'a, K: 'static, T> {
    AddComponentInstance(&'a StaticPointer<TCell<K, ComponentInstance<K, T>>>),
    RemoveMarkerLink(&'a StaticPointer<TCell<K, MarkerLink<K>>>),
    EditMarkerLinkLength(&'a StaticPointer<TCell<K, MarkerLink<K>>>, TimelineTime),
}

pub enum InstanceEditEvent<'a, K: 'static, T> {
    UpdateImageRequiredParams(&'a ImageRequiredParams<K, T>),
}
