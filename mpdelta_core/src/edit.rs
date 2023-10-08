use crate::component::instance::{ComponentInstanceHandle, ComponentInstanceHandleOwned};
use crate::component::link::MarkerLink;
use crate::component::parameter::{ImageRequiredParams, ParameterValueType};
use crate::ptr::StaticPointer;
use crate::time::TimelineTime;
use qcell::TCell;

pub enum RootComponentEditCommand<K: 'static, T: ParameterValueType> {
    AddComponentInstance(ComponentInstanceHandleOwned<K, T>),
    RemoveMarkerLink(StaticPointer<TCell<K, MarkerLink<K>>>),
    EditMarkerLinkLength(StaticPointer<TCell<K, MarkerLink<K>>>, TimelineTime),
}

pub enum InstanceEditCommand<K: 'static, T: ParameterValueType> {
    UpdateImageRequiredParams(ImageRequiredParams<K, T>),
}

pub enum RootComponentEditEvent<'a, K: 'static, T: ParameterValueType> {
    AddComponentInstance(&'a ComponentInstanceHandle<K, T>),
    RemoveMarkerLink(&'a StaticPointer<TCell<K, MarkerLink<K>>>),
    EditMarkerLinkLength(&'a StaticPointer<TCell<K, MarkerLink<K>>>, TimelineTime),
}

pub enum InstanceEditEvent<'a, K: 'static, T: ParameterValueType> {
    UpdateImageRequiredParams(&'a ImageRequiredParams<K, T>),
}
