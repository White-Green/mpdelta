use crate::component::instance::{ComponentInstanceHandle, ComponentInstanceHandleOwned};
use crate::component::link::MarkerLinkHandle;
use crate::component::marker_pin::MarkerPinHandle;
use crate::component::parameter::{ImageRequiredParams, ParameterValueType};
use crate::time::TimelineTime;

pub enum RootComponentEditCommand<K: 'static, T: ParameterValueType> {
    AddComponentInstance(ComponentInstanceHandleOwned<K, T>),
    RemoveMarkerLink(MarkerLinkHandle<K>),
    EditMarkerLinkLength(MarkerLinkHandle<K>, TimelineTime),
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
    RemoveMarkerLink(&'a MarkerLinkHandle<K>),
    EditMarkerLinkLength(&'a MarkerLinkHandle<K>, TimelineTime),
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
