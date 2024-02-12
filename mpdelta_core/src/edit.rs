use crate::component::instance::{ComponentInstanceHandle, ComponentInstanceHandleOwned};
use crate::component::link::MarkerLink;
use crate::component::marker_pin::MarkerPinHandle;
use crate::component::parameter::{ImageRequiredParams, ParameterValueType};
use crate::ptr::StaticPointer;
use crate::time::TimelineTime;
use qcell::TCell;

pub enum RootComponentEditCommand<K: 'static, T: ParameterValueType> {
    AddComponentInstance(ComponentInstanceHandleOwned<K, T>),
    RemoveMarkerLink(StaticPointer<TCell<K, MarkerLink<K>>>),
    EditMarkerLinkLength(StaticPointer<TCell<K, MarkerLink<K>>>, TimelineTime),
    DeleteComponentInstance(ComponentInstanceHandle<K, T>),
}

pub enum InstanceEditCommand<K: 'static, T: ParameterValueType> {
    UpdateImageRequiredParams(ImageRequiredParams<K, T>),
    MoveComponentInstance(TimelineTime),
    MoveMarkerPin(MarkerPinHandle<K>, TimelineTime),
    AddMarkerPin(TimelineTime),
    DeleteMarkerPin(MarkerPinHandle<K>),
    LockMarkerPin(MarkerPinHandle<K>),
    UnlockMarkerPin(MarkerPinHandle<K>),
}

pub enum RootComponentEditEvent<'a, K: 'static, T: ParameterValueType> {
    AddComponentInstance(&'a ComponentInstanceHandle<K, T>),
    RemoveMarkerLink(&'a StaticPointer<TCell<K, MarkerLink<K>>>),
    EditMarkerLinkLength(&'a StaticPointer<TCell<K, MarkerLink<K>>>, TimelineTime),
    DeleteComponentInstance(&'a ComponentInstanceHandle<K, T>),
}

pub enum InstanceEditEvent<'a, K: 'static, T: ParameterValueType> {
    UpdateImageRequiredParams(&'a ImageRequiredParams<K, T>),
    MoveComponentInstance(TimelineTime),
    MoveMarkerPin(&'a MarkerPinHandle<K>, TimelineTime),
    AddMarkerPin(TimelineTime),
    DeleteMarkerPin(&'a MarkerPinHandle<K>),
    LockMarkerPin(&'a MarkerPinHandle<K>),
    UnlockMarkerPin(&'a MarkerPinHandle<K>),
}
