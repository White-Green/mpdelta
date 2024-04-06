use crate::component::instance::{ComponentInstanceHandle, ComponentInstanceHandleOwned};
use crate::component::link::MarkerLinkHandle;
use crate::component::marker_pin::{MarkerPinHandle, MarkerTime};
use crate::component::parameter::{ImageRequiredParams, ParameterNullableValue, ParameterValueFixed, ParameterValueType, VariableParameterValue};
use crate::time::TimelineTime;

pub enum RootComponentEditCommand<K: 'static, T: ParameterValueType> {
    AddComponentInstance(ComponentInstanceHandleOwned<K, T>),
    RemoveMarkerLink(MarkerLinkHandle<K>),
    EditMarkerLinkLength(MarkerLinkHandle<K>, TimelineTime),
    InsertComponentInstanceTo(ComponentInstanceHandle<K, T>, usize),
    DeleteComponentInstance(ComponentInstanceHandle<K, T>),
    EditComponentLength(MarkerTime),
    ConnectMarkerPins(MarkerPinHandle<K>, MarkerPinHandle<K>),
}

pub enum InstanceEditCommand<K: 'static, T: ParameterValueType> {
    UpdateFixedParams(Box<[ParameterValueFixed<T::Image, T::Audio>]>),
    UpdateVariableParams(Vec<VariableParameterValue<K, T, ParameterNullableValue<K, T>>>),
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
    InsertComponentInstanceTo(&'a ComponentInstanceHandle<K, T>, usize),
    DeleteComponentInstance(&'a ComponentInstanceHandle<K, T>),
    EditComponentLength(MarkerTime),
    ConnectMarkerPins(&'a MarkerPinHandle<K>, &'a MarkerPinHandle<K>),
}

pub enum InstanceEditEvent<'a, K: 'static, T: ParameterValueType> {
    UpdateFixedParams(&'a [ParameterValueFixed<T::Image, T::Audio>]),
    UpdateVariableParams(&'a [VariableParameterValue<K, T, ParameterNullableValue<K, T>>]),
    UpdateImageRequiredParams(&'a ImageRequiredParams<K, T>),
    MoveComponentInstance(TimelineTime),
    MoveMarkerPin(&'a MarkerPinHandle<K>, TimelineTime),
    AddMarkerPin(TimelineTime),
    DeleteMarkerPin(&'a MarkerPinHandle<K>),
    LockMarkerPin(&'a MarkerPinHandle<K>),
    UnlockMarkerPin(&'a MarkerPinHandle<K>),
}
