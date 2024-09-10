use crate::component::instance::{ComponentInstance, ComponentInstanceId};
use crate::component::link::MarkerLink;
use crate::component::marker_pin::{MarkerPinId, MarkerTime};
use crate::component::parameter::{ImageRequiredParams, ParameterNullableValue, ParameterValueFixed, ParameterValueType, VariableParameterValue};
use crate::time::TimelineTime;

pub enum RootComponentEditCommand<T: ParameterValueType> {
    AddComponentInstance(ComponentInstance<T>),
    InsertComponentInstanceTo(ComponentInstanceId, usize),
    RemoveMarkerLink(MarkerLink),
    EditMarkerLinkLength(MarkerLink, TimelineTime),
    DeleteComponentInstance(ComponentInstanceId),
    EditComponentLength(MarkerTime),
    ConnectMarkerPins(MarkerPinId, MarkerPinId),
}

pub enum InstanceEditCommand<T: ParameterValueType> {
    UpdateFixedParams(Box<[ParameterValueFixed<T::Image, T::Audio>]>),
    UpdateVariableParams(Vec<VariableParameterValue<ParameterNullableValue<T>>>),
    UpdateImageRequiredParams(ImageRequiredParams),
    MoveComponentInstance(TimelineTime),
    MoveMarkerPin(MarkerPinId, TimelineTime),
    AddMarkerPin(TimelineTime),
    DeleteMarkerPin(MarkerPinId),
    LockMarkerPin(MarkerPinId),
    UnlockMarkerPin(MarkerPinId),
    SplitAtPin(MarkerPinId),
}

pub enum RootComponentEditEvent<'a> {
    AddComponentInstance(&'a ComponentInstanceId),
    InsertComponentInstanceTo(&'a ComponentInstanceId, usize),
    RemoveMarkerLink(&'a MarkerLink),
    EditMarkerLinkLength(&'a MarkerLink, TimelineTime),
    DeleteComponentInstance(&'a ComponentInstanceId),
    EditComponentLength(MarkerTime),
    ConnectMarkerPins(&'a MarkerPinId, &'a MarkerPinId),
}

pub enum InstanceEditEvent<'a, T: ParameterValueType> {
    UpdateFixedParams(&'a [ParameterValueFixed<T::Image, T::Audio>]),
    UpdateVariableParams(&'a [VariableParameterValue<ParameterNullableValue<T>>]),
    UpdateImageRequiredParams(&'a ImageRequiredParams),
    MoveComponentInstance(TimelineTime),
    MoveMarkerPin(&'a MarkerPinId, TimelineTime),
    AddMarkerPin(TimelineTime),
    DeleteMarkerPin(&'a MarkerPinId),
    LockMarkerPin(&'a MarkerPinId),
    UnlockMarkerPin(&'a MarkerPinId),
    SplitAtPin(&'a MarkerPinId),
}
