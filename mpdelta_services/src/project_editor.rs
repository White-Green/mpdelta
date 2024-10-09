use crate::project_editor::dsa::union_find::UnionFind;
use async_trait::async_trait;
use cgmath::Vector3;
use dashmap::DashMap;
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::common::time_split_value_persistent::TimeSplitValuePersistent;
use mpdelta_core::component::instance::{ComponentInstance, ComponentInstanceId};
use mpdelta_core::component::link::MarkerLink;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerPinId, MarkerTime};
use mpdelta_core::component::parameter::{AudioRequiredParams, ImageRequiredParams, ImageRequiredParamsTransform, Never, Parameter, ParameterNullableValue, ParameterValueType, PinSplitValue, SingleChannelVolume, VariableParameterValue, Vector3Params};
use mpdelta_core::core::{EditEventListener, Editor, IdGenerator};
use mpdelta_core::edit::{InstanceEditCommand, InstanceEditEvent, RootComponentEditCommand, RootComponentEditEvent};
use mpdelta_core::project::{RootComponentClassHandle, RootComponentClassItemWrite};
use mpdelta_core::time::TimelineTime;
use mpdelta_differential::CollectCachedTimeError;
use rpds::Vector;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::AtomicUsize;
use std::sync::{atomic, Arc};
use std::{iter, mem};
use thiserror::Error;

mod dsa;
#[cfg(test)]
mod tests;

// TODO: Listenerをusizeで管理してるので、overflowしたらバグる(ほとんど気にしなくても良さそうではあるが)
pub struct ProjectEditor<T, Id> {
    id_generator: Id,
    edit_event_listener_id: AtomicUsize,
    edit_event_listeners: Arc<DashMap<usize, Box<dyn EditEventListener<T>>>>,
}

impl<T, Id> ProjectEditor<T, Id> {
    pub fn new(id_generator: Id) -> ProjectEditor<T, Id> {
        ProjectEditor {
            id_generator,
            edit_event_listener_id: AtomicUsize::default(),
            edit_event_listeners: Arc::new(DashMap::new()),
        }
    }
}

#[derive(Debug)]
pub enum ProjectEditLog {
    Unimplemented,
}

#[derive(Debug, Error)]
pub enum ProjectEditError {
    #[error("invalid target")]
    InvalidTarget,
    #[error("component instance not found")]
    ComponentInstanceNotFound,
    #[error("invalid marker pin")]
    InvalidMarkerPin,
    #[error("cannot unlock for avoid floating")]
    CannotUnlockForAvoidFloating,
    #[error("marker pin not found")]
    MarkerPinNotFound,
    #[error("invalid marker pin add position")]
    InvalidMarkerPinAddPosition,
    #[error("parameter type mismatch")]
    ParameterTypeMismatch,
    #[error("marker pins are same")]
    MarkerPinsAreSame,
    #[error("marker pins already connected")]
    PinsAlreadyConnected,
    #[error("marker pin should locked")]
    MarkerPinShouldLocked,
    #[error("cannot split for avoid floating")]
    CannotSplitForAvoidFloating,
    #[error("marker link not found")]
    MarkerLinkNotFound,
    #[error("{0}")]
    CollectCachedTimeError(#[from] CollectCachedTimeError),
}

pub struct ProjectEditListenerGuard<T> {
    id: usize,
    edit_event_listeners: Arc<DashMap<usize, Box<dyn EditEventListener<T>>>>,
}

impl<T> Drop for ProjectEditListenerGuard<T> {
    fn drop(&mut self) {
        self.edit_event_listeners.remove(&self.id);
    }
}

#[async_trait]
impl<T, Id> Editor<T> for ProjectEditor<T, Id>
where
    T: ParameterValueType,
    Id: IdGenerator,
{
    type Log = ProjectEditLog;
    type Err = ProjectEditError;
    type EditEventListenerGuard = ProjectEditListenerGuard<T>;

    fn add_edit_event_listener(&self, listener: impl EditEventListener<T> + 'static) -> Self::EditEventListenerGuard {
        let id = self.edit_event_listener_id.fetch_add(1, atomic::Ordering::AcqRel);
        self.edit_event_listeners.insert(id, Box::new(listener));
        ProjectEditListenerGuard {
            id,
            edit_event_listeners: Arc::clone(&self.edit_event_listeners),
        }
    }

    async fn edit(&self, target_ref: &RootComponentClassHandle<T>, command: RootComponentEditCommand<T>) -> Result<Self::Log, Self::Err> {
        let target = target_ref.upgrade().ok_or(ProjectEditError::InvalidTarget)?;
        let target = target.read().await;
        match command {
            RootComponentEditCommand::AddComponentInstance(instance) => {
                let instance_id = *instance.id();
                {
                    let mut item = target.get_mut().await;
                    let base = if let Some(base) = item.iter_components().next_back() { *base.marker_left().id() } else { *target.left().id() };
                    let left = instance.marker_left();
                    let right = instance.marker_right();
                    let link_for_zero = MarkerLink::new(base, *left.id(), TimelineTime::new(MixedFraction::from_integer(1)));
                    let link_for_length = MarkerLink::new(*left.id(), *right.id(), TimelineTime::new(MixedFraction::from_integer(1)));
                    item.add_component(instance);
                    item.add_link(link_for_zero);
                    item.add_link(link_for_length);

                    let time_map = mpdelta_differential::collect_cached_time(&*item)?;
                    RootComponentClassItemWrite::commit_changes(item, time_map);
                }

                // TODO: このへんもうちょっとバグりにくい構造を探したいよね
                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::AddComponentInstance(&instance_id)));
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::InsertComponentInstanceTo(component, index) => {
                {
                    let mut item = target.get_mut().await;
                    if item.insert_component_within(&component, index).is_err() {
                        return Err(ProjectEditError::ComponentInstanceNotFound);
                    }

                    let time_map = mpdelta_differential::collect_cached_time(&*item)?;
                    RootComponentClassItemWrite::commit_changes(item, time_map);
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::InsertComponentInstanceTo(&component, index)));
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::RemoveMarkerLink(link) => {
                {
                    let mut item = target.get_mut().await;
                    item.remove_link(*link.from(), *link.to());

                    let time_map = mpdelta_differential::collect_cached_time(&*item)?;
                    RootComponentClassItemWrite::commit_changes(item, time_map);
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::RemoveMarkerLink(&link)));
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::EditMarkerLinkLength(link, len) => {
                {
                    let mut item = target.get_mut().await;
                    item.link_mut(*link.from(), *link.to()).ok_or(ProjectEditError::MarkerLinkNotFound)?.set_len(len);

                    let time_map = mpdelta_differential::collect_cached_time(&*item)?;
                    RootComponentClassItemWrite::commit_changes(item, time_map);
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::EditMarkerLinkLength(&link, len)));
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::DeleteComponentInstance(instance) => {
                {
                    let mut item = target.get_mut().await;
                    let instance_ref = item.component(&instance).ok_or(ProjectEditError::ComponentInstanceNotFound)?;
                    let delete_target_pins = [instance_ref.marker_left(), instance_ref.marker_right()].into_iter().chain(instance_ref.markers()).map(MarkerPin::id).collect::<HashSet<_>>();
                    let mut pin_union_find = UnionFind::new();
                    for link in item.iter_links() {
                        if !delete_target_pins.contains(&link.from()) && !delete_target_pins.contains(&link.to()) {
                            pin_union_find.union(*link.from(), *link.to());
                        }
                    }
                    for component in item.iter_components() {
                        let mut locked_pins = [component.marker_left(), component.marker_right()].into_iter().chain(component.markers()).filter_map(|pin| pin.locked_component_time().is_some().then_some(pin.id()));
                        let Some(base_pin) = locked_pins.next() else { continue };
                        for p in locked_pins {
                            pin_union_find.union(*base_pin, *p);
                        }
                    }
                    let adjacent_pins = delete_target_pins.iter().flat_map(|&p| item.iter_link_connecting(*p).map(move |link| if link.from() == p { *link.to() } else { *link.from() })).collect::<HashSet<_>>();
                    let left_pin_root = pin_union_find.get_root(*item.left().id());
                    if let Some(connection_base) = adjacent_pins.iter().copied().filter(|&p| pin_union_find.get_root(p) == left_pin_root).min_by_key(|p| item.time_of_pin(p).unwrap()) {
                        let from_time = item.time_of_pin(&connection_base).unwrap();
                        let additional_links = adjacent_pins
                            .iter()
                            .filter_map(|&p| {
                                if pin_union_find.get_root(p) == left_pin_root {
                                    return None;
                                }
                                let to_time = item.time_of_pin(&p).unwrap();
                                Some(MarkerLink::new(connection_base, p, to_time - from_time))
                            })
                            .collect::<Vec<_>>();
                        for link in additional_links {
                            item.add_link(link)
                        }
                    }
                    if item.remove_component(&instance).is_err() {
                        return Err(ProjectEditError::ComponentInstanceNotFound);
                    }

                    let time_map = mpdelta_differential::collect_cached_time(&*item)?;
                    RootComponentClassItemWrite::commit_changes(item, time_map);
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::DeleteComponentInstance(&instance)));
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::EditComponentLength(length) => {
                {
                    let mut item = target.get_mut().await;
                    item.right_mut().set_locked_component_time(Some(length));
                    item.set_length(length);
                    let left = item.left();
                    let right = item.right();
                    let diff = TimelineTime::new(length.value() - item.length().value());

                    let mut pin_union_find = UnionFind::new();
                    item.iter_links().for_each(|link| {
                        if link.from() != right.id() && link.to() != right.id() {
                            pin_union_find.union(*link.from(), *link.to());
                        }
                    });
                    let left_root = pin_union_find.get_root(*left.id());
                    let link_len_update = item
                        .iter_links()
                        .filter_map(|link| {
                            if link.to() == right.id() && pin_union_find.get_root(*link.from()) == left_root {
                                Some((*link.from(), *link.to(), link.len() + diff))
                            } else if link.from() == right.id() && pin_union_find.get_root(*link.to()) == left_root {
                                Some((*link.from(), *link.to(), link.len() - diff))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    for (from, to, len) in link_len_update {
                        item.link_mut(from, to).unwrap().set_len(len);
                    }

                    let time_map = mpdelta_differential::collect_cached_time(&*item)?;
                    RootComponentClassItemWrite::commit_changes(item, time_map);
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::EditComponentLength(length)));
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::ConnectMarkerPins(from, to) => {
                {
                    if from == to {
                        return Err(ProjectEditError::MarkerPinsAreSame);
                    }

                    let mut item = target.get_mut().await;

                    let mut pin_union_find = UnionFind::new();
                    let mut connected_pins = HashMap::<_, HashMap<_, _>>::new();
                    for link in item.iter_links() {
                        assert_ne!(link.from(), link.to());
                        connected_pins.entry(*link.from()).or_default().insert(*link.to(), Some(link.clone()));
                        connected_pins.entry(*link.to()).or_default().insert(*link.from(), Some(link.clone()));
                        if ![&from, &to].contains(&link.from()) && ![&from, &to].contains(&link.to()) {
                            pin_union_find.union(*link.from(), *link.to());
                        }
                    }
                    for component in item.iter_components() {
                        let locked_pins = component.markers().iter().chain([component.marker_left(), component.marker_right()]).filter(|pin| pin.locked_component_time().is_some());
                        for pin in locked_pins.clone() {
                            let pin = *pin.id();
                            let connection = connected_pins.entry(pin).or_default();
                            for other_pin in locked_pins.clone().filter(|p| p.id() != &pin) {
                                connection.entry(*other_pin.id()).or_insert(None);
                            }
                        }
                        let mut all_pins = locked_pins.clone().filter(|pin| pin.id() != &from && pin.id() != &to);
                        let Some(base_pin) = all_pins.next() else {
                            continue;
                        };
                        for pin in all_pins {
                            pin_union_find.union(*base_pin.id(), *pin.id());
                        }
                    }

                    if let Some(link) = connected_pins[&from].get(&to) {
                        if link.is_some() {
                            return Err(ProjectEditError::PinsAlreadyConnected);
                        }
                    }
                    let mut prev = HashMap::from([(&from, &from)]);
                    let mut q = VecDeque::from([&from]);
                    while let Some(pin) = q.pop_front() {
                        for other_pin in connected_pins[pin].iter().filter_map(|(p, l)| l.is_some().then_some(p)) {
                            if prev.contains_key(other_pin) {
                                continue;
                            }
                            prev.insert(other_pin, pin);
                            q.push_back(other_pin);
                        }
                    }
                    if prev.contains_key(&to) {
                        let link = connected_pins[&to][prev[&to]].as_ref().unwrap();
                        item.remove_link(*link.from(), *link.to());
                    }
                    let len = item.time_of_pin(&to).unwrap() - item.time_of_pin(&from).unwrap();
                    item.add_link(MarkerLink::new(from, to, len));

                    let time_map = mpdelta_differential::collect_cached_time(&*item)?;
                    RootComponentClassItemWrite::commit_changes(item, time_map);
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::ConnectMarkerPins(&from, &to)));
                Ok(ProjectEditLog::Unimplemented)
            }
        }
    }

    async fn edit_instance(&self, root_ref: &RootComponentClassHandle<T>, target_ref: &ComponentInstanceId, command: InstanceEditCommand<T>) -> Result<Self::Log, Self::Err> {
        let root = root_ref.upgrade().ok_or(ProjectEditError::InvalidTarget)?;
        let root = root.read().await;
        match command {
            InstanceEditCommand::UpdateFixedParams(params) => {
                {
                    let mut item = root.get_mut().await;
                    let component = item.component_mut(target_ref).ok_or(ProjectEditError::ComponentInstanceNotFound)?;
                    let slot = component.fixed_parameters_mut();
                    if slot.len() != params.len() {
                        return Err(ProjectEditError::ParameterTypeMismatch);
                    }
                    for (slot, value) in slot.iter_mut().zip(params.iter()) {
                        if slot.select() != value.select() {
                            return Err(ProjectEditError::ParameterTypeMismatch);
                        }
                        *slot = value.clone();
                    }

                    let time_map = mpdelta_differential::collect_cached_time(&*item)?;
                    RootComponentClassItemWrite::commit_changes(item, time_map);
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::UpdateFixedParams(&params)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::UpdateVariableParams(params) => {
                {
                    let mut item = root.get_mut().await;
                    let component = item.component_mut(target_ref).ok_or(ProjectEditError::ComponentInstanceNotFound)?;
                    let slot = component.variable_parameters_mut();
                    if slot.len() != params.len() {
                        return Err(ProjectEditError::ParameterTypeMismatch);
                    }
                    *slot = slot
                        .iter()
                        .zip(params.iter())
                        .map(|(slot, value)| {
                            if slot.params.select() != value.params.select() {
                                return Err(ProjectEditError::ParameterTypeMismatch);
                            }
                            Ok(value.clone())
                        })
                        .collect::<Result<_, _>>()?;

                    let time_map = mpdelta_differential::collect_cached_time(&*item)?;
                    RootComponentClassItemWrite::commit_changes(item, time_map);
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::UpdateVariableParams(&params)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::UpdateImageRequiredParams(params) => {
                {
                    let mut item = root.get_mut().await;
                    let component = item.component_mut(target_ref).ok_or(ProjectEditError::ComponentInstanceNotFound)?;
                    component.set_image_required_params(params.clone());

                    let time_map = mpdelta_differential::collect_cached_time(&*item)?;
                    RootComponentClassItemWrite::commit_changes(item, time_map);
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::UpdateImageRequiredParams(&params)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::MoveComponentInstance(to) => {
                {
                    let mut item = root.get_mut().await;
                    let target_raw_ref = item.component(target_ref).ok_or(ProjectEditError::ComponentInstanceNotFound)?;
                    let target_left = target_raw_ref.marker_left();
                    let target_right = target_raw_ref.marker_right();
                    let target_contains_pins = target_raw_ref.markers().iter().chain([target_left, target_right]).map(MarkerPin::id).copied().collect::<HashSet<_>>();
                    let mut pin_union_find = UnionFind::new();
                    let connected_links = {
                        let mut connected_links = HashMap::<_, HashSet<_>>::new();
                        for link in item.iter_links() {
                            connected_links.entry(*link.from()).or_default().insert(link.clone());
                            connected_links.entry(*link.to()).or_default().insert(link.clone());
                            if !target_contains_pins.contains(link.from()) && !target_contains_pins.contains(link.to()) {
                                pin_union_find.union(*link.from(), *link.to());
                            }
                        }
                        connected_links
                    };
                    let mut pins = HashSet::new();
                    for component in item.iter_components() {
                        let all_pins = component.markers().iter().chain([component.marker_left(), component.marker_right()]);
                        pins.extend(all_pins.clone());
                        let mut all_pins = all_pins.filter(|pin| pin.locked_component_time().is_some());
                        let Some(base_pin) = all_pins.next() else {
                            continue;
                        };
                        for pin in all_pins {
                            pin_union_find.union(*base_pin.id(), *pin.id());
                        }
                    }
                    let current_left_time = item.time_of_pin(target_left.id()).unwrap();
                    let delta = to - current_left_time;

                    let mut edit_list = Vec::new();
                    let zero_pin_root = pin_union_find.get_root(*item.left().id());
                    for pin_handle in &target_contains_pins {
                        let pin = pins.get(pin_handle).unwrap();
                        if pin.locked_component_time().is_none() {
                            continue;
                        }
                        let Some(link) = connected_links.get(pin_handle) else {
                            continue;
                        };
                        for link in link {
                            let other_pin = if link.to() == pin_handle { link.from() } else { link.to() };
                            if target_contains_pins.contains(other_pin) {
                                continue;
                            }
                            if pin_union_find.get_root(*other_pin) != zero_pin_root {
                                continue;
                            }
                            let new_len = if link.to() == pin_handle { link.len() + delta } else { link.len() - delta };
                            edit_list.push((*link.from(), *link.to(), new_len));
                        }
                    }
                    for (from, to, len) in edit_list {
                        item.link_mut(from, to).unwrap().set_len(len);
                    }

                    let time_map = mpdelta_differential::collect_cached_time(&*item)?;
                    RootComponentClassItemWrite::commit_changes(item, time_map);
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::MoveComponentInstance(to)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::MoveMarkerPin(pin, to) => {
                {
                    let mut item = root.get_mut().await;
                    let (_, mut item_structure, time_map) = item.view();
                    let mut next_links = HashSet::new();
                    let mut pin_union_find = UnionFind::new();
                    for link in item_structure.iter_links() {
                        if link.from() == &pin || link.to() == &pin {
                            next_links.insert(link.clone());
                        } else {
                            pin_union_find.union(*link.from(), *link.to());
                        }
                    }
                    for component in item_structure.iter_components() {
                        let mut locked_markers = [component.marker_left(), component.marker_right()].into_iter().chain(component.markers()).filter(|p| p.id() != &pin).filter(|p| p.locked_component_time().is_some());
                        let Some(base_marker) = locked_markers.next() else {
                            continue;
                        };
                        for marker in locked_markers {
                            pin_union_find.union(*base_marker.id(), *marker.id());
                        }
                    }

                    let root_left_root = pin_union_find.get_root(*item_structure.left().id());
                    let time_diff = to - time_map.time_of_pin(&pin).unwrap();
                    let mut edit_list = Vec::new();
                    for link in &next_links {
                        let other_pin = if link.from() == &pin { *link.to() } else { *link.from() };
                        if pin_union_find.get_root(other_pin) == root_left_root {
                            let new_len = if link.to() == &pin { link.len() + time_diff } else { link.len() - time_diff };
                            edit_list.push((*link.from(), *link.to(), new_len));
                        }
                    }
                    for (from, to, len) in edit_list.iter().copied() {
                        item_structure.link_mut(from, to).unwrap().set_len(len);
                    }
                    if edit_list.is_empty() {
                        let target = item_structure.component_mut(target_ref).ok_or(ProjectEditError::ComponentInstanceNotFound)?;
                        let (left, right) = if target.marker_left().id() == &pin {
                            let mut pins_iter = target.markers().iter().chain(iter::once(target.marker_right())).filter_map(|pin| pin.locked_component_time().map(|locked| (locked, time_map.time_of_pin(pin.id()).unwrap())));
                            let left = pins_iter.next().expect("broken component structure");
                            let right = pins_iter.next();
                            (left, right)
                        } else if target.marker_right().id() == &pin {
                            let mut pins_iter = iter::once(target.marker_left()).chain(target.markers()).filter_map(|pin| pin.locked_component_time().map(|locked| (locked, time_map.time_of_pin(pin.id()).unwrap()))).rev();
                            let right = pins_iter.next().expect("broken component structure");
                            if let Some(left) = pins_iter.next() {
                                (left, Some(right))
                            } else {
                                (right, None)
                            }
                        } else {
                            let mut all_pins = iter::once(target.marker_left()).chain(target.markers()).chain(iter::once(target.marker_right()));
                            let left = all_pins
                                .by_ref()
                                .take_while(|p| p.id() != &pin)
                                .filter_map(|pin| pin.locked_component_time().map(|locked| (locked, time_map.time_of_pin(pin.id()).unwrap())))
                                .fold([None, None], |[_, left], right| [left, Some(right)]);
                            let mut right_ptr = all_pins.filter_map(|pin| pin.locked_component_time().map(|locked| (locked, time_map.time_of_pin(pin.id()).unwrap())));
                            let right = [right_ptr.next(), right_ptr.next()];
                            match (left, right) {
                                ([_, Some(left_next)], [Some(right_next), _]) => (left_next, Some(right_next)),
                                ([Some(left_next), Some(right_next)], _) => (left_next, Some(right_next)),
                                (_, [Some(left_next), Some(right_next)]) => (left_next, Some(right_next)),
                                ([_, Some(base)], _) => (base, None),
                                (_, [Some(base), _]) => (base, None),
                                _ => panic!("broken component structure"),
                            }
                        };
                        let lock_time = if let Some(right) = right {
                            let p = (to - left.1).value() / (right.1 - left.1).value();
                            left.0.value() + (right.0.value() - left.0.value()) * p
                        } else {
                            let base = left.1.value();
                            left.0.value() + (to.value() - base)
                        };
                        let target_pin = if target.marker_left().id() == &pin {
                            target.marker_left_mut()
                        } else if target.marker_right().id() == &pin {
                            target.marker_right_mut()
                        } else {
                            target.markers_mut().iter_mut().find(|p| p.id() == &pin).unwrap()
                        };

                        target_pin.set_locked_component_time(MarkerTime::new(lock_time));
                    }

                    let time_map = mpdelta_differential::collect_cached_time(&*item)?;
                    RootComponentClassItemWrite::commit_changes(item, time_map);
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::MoveMarkerPin(&pin, to)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::AddMarkerPin(at) => {
                {
                    let mut item = root.get_mut().await;
                    let (_, mut item_structure, time_map) = item.view();
                    let target = item_structure.component_mut(target_ref).ok_or(ProjectEditError::ComponentInstanceNotFound)?;
                    let Err(insert_index) = target.markers().binary_search_by_key(&at, |p| time_map.time_of_pin(p.id()).unwrap()) else {
                        return Err(ProjectEditError::InvalidMarkerPinAddPosition);
                    };
                    if at <= time_map.time_of_pin(target.marker_left().id()).unwrap() || time_map.time_of_pin(target.marker_right().id()).unwrap() <= at {
                        return Err(ProjectEditError::InvalidMarkerPinAddPosition);
                    }
                    let mut iter_left = iter::once(target.marker_left())
                        .chain(target.markers().get(..insert_index).into_iter().flatten())
                        .filter_map(|p| p.locked_component_time().map(|locked| (locked, time_map.time_of_pin(p.id()).unwrap())))
                        .rev();
                    let mut iter_right = target
                        .markers()
                        .get(insert_index..)
                        .into_iter()
                        .flatten()
                        .chain(iter::once(target.marker_right()))
                        .filter_map(|p| p.locked_component_time().map(|locked| (locked, time_map.time_of_pin(p.id()).unwrap())));
                    let (left, right) = match (iter_left.next(), iter_right.next()) {
                        (Some(left), Some(right)) => (left, Some(right)),
                        (Some(left), None) => {
                            if let Some(l) = iter_left.next() {
                                (l, Some(left))
                            } else {
                                (left, None)
                            }
                        }
                        (None, Some(right)) => (right, iter_right.next()),
                        (None, None) => panic!("broken component structure"),
                    };
                    let lock_time = if let Some(right) = right {
                        let p = ((at) - (left.1)).value() / ((right.1) - (left.1)).value();
                        left.0.value() + (right.0.value() - left.0.value()) * p
                    } else {
                        let base = left.1.value();
                        left.0.value() + (at.value() - base)
                    };

                    target.markers_mut().insert(insert_index, MarkerPin::new(self.id_generator.generate_new(), MarkerTime::new(lock_time).unwrap()));

                    let time_map = mpdelta_differential::collect_cached_time(&*item)?;
                    RootComponentClassItemWrite::commit_changes(item, time_map);
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::AddMarkerPin(at)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::DeleteMarkerPin(pin) => {
                {
                    let mut item = root.get_mut().await;
                    let (_, mut item_structure, time_map) = item.view();
                    let target = item_structure.component(target_ref).ok_or(ProjectEditError::ComponentInstanceNotFound)?;
                    let Some((remove_marker_index, _)) = target.markers().iter().enumerate().find(|&(_, p)| p.id() == &pin) else {
                        return Err(ProjectEditError::MarkerPinNotFound);
                    };
                    let mut all_pins = [target.marker_left(), target.marker_right()].into_iter().chain(target.markers());
                    let locked_pins = all_pins.clone().filter(|&p| p.id() != &pin).filter(|p| p.locked_component_time().is_some()).count();
                    if locked_pins == 0 {
                        return Err(ProjectEditError::MarkerPinNotFound);
                    }
                    let near_locked_pin = *all_pins
                        .by_ref()
                        .take_while(|&p| p.id() != &pin)
                        .filter(|p| p.locked_component_time().is_some())
                        .last()
                        .or_else(|| all_pins.find(|p| p.locked_component_time().is_some()))
                        .unwrap()
                        .id();
                    drop(all_pins);

                    let mut pin_union_find = UnionFind::new();
                    let mut next_link = HashSet::new();
                    for link in item_structure.iter_links() {
                        if link.from() == &pin || link.to() == &pin {
                            next_link.insert(link.clone());
                        } else {
                            pin_union_find.union(*link.from(), *link.to());
                        }
                    }
                    let mut all_locked_markers = HashSet::new();
                    for component in item_structure.iter_components() {
                        let mut locked_markers = [component.marker_left(), component.marker_right()]
                            .into_iter()
                            .chain(component.markers().iter())
                            .filter(|&p| p.id() != &pin)
                            .filter(|p| p.locked_component_time().is_some())
                            .map(|p| *p.id());
                        all_locked_markers.extend(locked_markers.clone());
                        let Some(base_marker) = locked_markers.next() else {
                            continue;
                        };
                        for marker in locked_markers {
                            pin_union_find.union(base_marker, marker);
                        }
                    }

                    let root_left_root = pin_union_find.get_root(*item_structure.left().id());
                    let base_pin = if pin_union_find.get_root(near_locked_pin) == root_left_root {
                        near_locked_pin
                    } else {
                        next_link.iter().map(|link| if link.from() == &pin { *link.to() } else { *link.from() }).find(|p| all_locked_markers.contains(p)).unwrap()
                    };
                    let base_pin_time = time_map.time_of_pin(&base_pin).unwrap();
                    let floating_pins = next_link
                        .iter()
                        .map(|link| if link.from() == &pin { *link.to() } else { *link.from() })
                        .filter(|p| p != &base_pin)
                        .filter(|p| pin_union_find.get_root(*p) == root_left_root)
                        .collect::<Vec<_>>();
                    let remove_links = item_structure.iter_links().filter(|link| link.from() == &pin || link.to() == &pin).cloned().collect::<Vec<_>>();
                    for link in remove_links {
                        item_structure.remove_link(*link.from(), *link.to());
                    }
                    for p in floating_pins {
                        let to_time = time_map.time_of_pin(&p).unwrap();
                        let link = MarkerLink::new(base_pin, p, to_time - base_pin_time);
                        item_structure.add_link(link)
                    }

                    let target = item_structure.component_mut(target_ref).unwrap();
                    target.markers_mut().remove(remove_marker_index);
                    fn remove_pin<T>(value: &mut PinSplitValue<T>, pin: &MarkerPinId) {
                        'out: for i in 0.. {
                            loop {
                                let Some((_, p, _)) = value.get_time(i) else {
                                    break 'out;
                                };
                                if p == pin {
                                    value.merge_two_values_by_left(i).unwrap();
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                    fn remove_pin3(value: &mut Arc<Vector3Params>, pin: &MarkerPinId) {
                        'out: for i in 0.. {
                            loop {
                                let Some((_, p, _)) = value.x.params.get_time(i) else {
                                    break 'out;
                                };
                                if p == pin {
                                    Arc::make_mut(value).x.params.merge_two_values_by_left(i).unwrap();
                                } else {
                                    break;
                                }
                            }
                        }
                        'out: for i in 0.. {
                            loop {
                                let Some((_, p, _)) = value.y.params.get_time(i) else {
                                    break 'out;
                                };
                                if p == pin {
                                    Arc::make_mut(value).y.params.merge_two_values_by_left(i).unwrap();
                                } else {
                                    break;
                                }
                            }
                        }
                        'out: for i in 0.. {
                            loop {
                                let Some((_, p, _)) = value.y.params.get_time(i) else {
                                    break 'out;
                                };
                                if p == pin {
                                    Arc::make_mut(value).y.params.merge_two_values_by_left(i).unwrap();
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                    if let Some(params) = target.image_required_params_mut() {
                        let ImageRequiredParams {
                            transform,
                            background_color: _,
                            opacity,
                            blend_mode,
                            composite_operation,
                        } = params;
                        match Arc::make_mut(transform) {
                            ImageRequiredParamsTransform::Params {
                                size,
                                scale,
                                translate,
                                rotate,
                                scale_center,
                                rotate_center,
                            } => {
                                remove_pin3(size, &pin);
                                remove_pin3(scale, &pin);
                                remove_pin3(translate, &pin);
                                remove_pin(Arc::make_mut(rotate), &pin);
                                remove_pin3(scale_center, &pin);
                                remove_pin3(rotate_center, &pin);
                            }
                            ImageRequiredParamsTransform::Free { left_top, right_top, left_bottom, right_bottom } => {
                                remove_pin3(left_top, &pin);
                                remove_pin3(right_top, &pin);
                                remove_pin3(left_bottom, &pin);
                                remove_pin3(right_bottom, &pin);
                            }
                        }
                        remove_pin(opacity, &pin);
                        remove_pin(blend_mode, &pin);
                        remove_pin(composite_operation, &pin);
                    }

                    if let Some(params) = target.audio_required_params_mut() {
                        let AudioRequiredParams { volume } = params;
                        for channel in 0..volume.len() {
                            let SingleChannelVolume { params, .. } = volume.get_mut(channel).unwrap();
                            'out: for i in 0.. {
                                loop {
                                    let Some((_, p, _)) = params.get_time(i) else {
                                        break 'out;
                                    };
                                    if p == &pin {
                                        params.merge_two_values_by_left(i).unwrap();
                                    } else {
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    let params = target.variable_parameters_mut();
                    for i in 0..params.len() {
                        let value = params.get_mut(i).unwrap();
                        match &mut value.params {
                            Parameter::None => {}
                            Parameter::Image(value) => remove_pin(value, &pin),
                            Parameter::Audio(value) => remove_pin(value, &pin),
                            Parameter::Binary(value) => remove_pin(value, &pin),
                            Parameter::String(value) => remove_pin(value, &pin),
                            Parameter::Integer(value) => remove_pin(value, &pin),
                            Parameter::RealNumber(value) => remove_pin(value, &pin),
                            Parameter::Boolean(value) => remove_pin(value, &pin),
                            Parameter::Dictionary(value) => {
                                let _: &mut Never = value;
                                unreachable!();
                            }
                            Parameter::Array(value) => {
                                let _: &mut Never = value;
                                unreachable!();
                            }
                            Parameter::ComponentClass(_) => {}
                        }
                    }

                    let time_map = mpdelta_differential::collect_cached_time(&*item)?;
                    RootComponentClassItemWrite::commit_changes(item, time_map);
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::DeleteMarkerPin(&pin)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::LockMarkerPin(pin) => {
                {
                    let mut item = root.get_mut().await;
                    let (_, mut item_structure, time_map) = item.view();
                    let target = item_structure.component_mut(target_ref).ok_or(ProjectEditError::ComponentInstanceNotFound)?;
                    let mut all_pins = iter::once(target.marker_left()).chain(target.markers()).chain(iter::once(target.marker_right()));
                    if all_pins.clone().find(|p| p.id() == &pin).ok_or(ProjectEditError::MarkerPinNotFound)?.locked_component_time().is_some() {
                        return Ok(ProjectEditLog::Unimplemented);
                    }
                    let left_next = all_pins.by_ref().take_while(|&p| p.id() != &pin).filter(|p| p.locked_component_time().is_some()).fold([None, None], |[_, left], p| [left, Some(p)]);
                    let mut right_pins = all_pins.filter(|p| p.locked_component_time().is_some());
                    let right_next = [right_pins.next(), right_pins.next()];
                    let (left_next, right_next) = match (left_next, right_next) {
                        ([_, Some(left_next)], [Some(right_next), _]) => (left_next, Some(right_next)),
                        ([Some(left_next), Some(right_next)], _) => (left_next, Some(right_next)),
                        (_, [Some(left_next), Some(right_next)]) => (left_next, Some(right_next)),
                        ([_, Some(base)], _) => (base, None),
                        (_, [Some(base), _]) => (base, None),
                        _ => panic!("broken component structure"),
                    };
                    let time = time_map.time_of_pin(&pin).unwrap();
                    let lock_time = if let Some(right_next) = right_next {
                        let p = (time - time_map.time_of_pin(left_next.id()).unwrap()).value() / (time_map.time_of_pin(right_next.id()).unwrap() - time_map.time_of_pin(left_next.id()).unwrap()).value();
                        left_next.locked_component_time().unwrap().value() + (right_next.locked_component_time().unwrap().value() - left_next.locked_component_time().unwrap().value()) * p
                    } else {
                        let base = time_map.time_of_pin(left_next.id()).unwrap().value();
                        left_next.locked_component_time().unwrap().value() + (time.value() - base)
                    };
                    let target_pin = if target.marker_left().id() == &pin {
                        target.marker_left_mut()
                    } else if target.marker_right().id() == &pin {
                        target.marker_right_mut()
                    } else {
                        target.markers_mut().iter_mut().find(|p| p.id() == &pin).unwrap()
                    };
                    target_pin.set_locked_component_time(Some(MarkerTime::new(lock_time).unwrap()));

                    let time_map = mpdelta_differential::collect_cached_time(&*item)?;
                    RootComponentClassItemWrite::commit_changes(item, time_map);
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::LockMarkerPin(&pin)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::UnlockMarkerPin(pin) => {
                {
                    let mut item = root.get_mut().await;
                    let (_, mut item_structure, time_map) = item.view();
                    let target = item_structure.component(target_ref).ok_or(ProjectEditError::ComponentInstanceNotFound)?;
                    if target.iter_all_markers().find(|p| p.id() == &pin).ok_or(ProjectEditError::MarkerPinNotFound)?.locked_component_time().is_none() {
                        return Ok(ProjectEditLog::Unimplemented);
                    }
                    let mut pin_union_find = UnionFind::new();
                    for link in item_structure.iter_links() {
                        pin_union_find.union(*link.from(), *link.to());
                    }
                    for component in item_structure.iter_components() {
                        let mut iter = component.iter_all_markers().filter(|&p| p.id() != &pin).filter(|p| p.locked_component_time().is_some());
                        let Some(base_pin) = iter.next() else {
                            continue;
                        };
                        for p in iter {
                            pin_union_find.union(*base_pin.id(), *p.id());
                        }
                    }
                    'edit: {
                        if pin_union_find.get_root(pin) == pin_union_find.get_root(*item_structure.left().id()) {
                            item_structure.component_mut(target_ref).unwrap().iter_all_markers_mut().find(|p| p.id() == &pin).unwrap().set_locked_component_time(None);
                            break 'edit;
                        }
                        let mut locked_pins = target.iter_all_markers().filter(|p| p.locked_component_time().is_some());
                        let left_next_pin = locked_pins.by_ref().take_while(|&p| p.id() != &pin).last();
                        let Some(next_pin) = left_next_pin.or_else(|| locked_pins.next()) else {
                            return Err(ProjectEditError::CannotUnlockForAvoidFloating);
                        };
                        let next_pin = *next_pin.id();
                        drop(locked_pins);
                        item_structure.component_mut(target_ref).unwrap().iter_all_markers_mut().find(|p| p.id() == &pin).unwrap().set_locked_component_time(None);
                        item_structure.add_link(MarkerLink::new(pin, next_pin, time_map.time_of_pin(&next_pin).unwrap() - time_map.time_of_pin(&pin).unwrap()));
                    }

                    let time_map = mpdelta_differential::collect_cached_time(&*item)?;
                    RootComponentClassItemWrite::commit_changes(item, time_map);
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::UnlockMarkerPin(&pin)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::SplitAtPin(pin) => {
                {
                    let mut root = root.get_mut().await;
                    let instance = root.component(target_ref).ok_or(ProjectEditError::ComponentInstanceNotFound)?;
                    let (i, pin_owned) = instance.markers().iter().enumerate().find(|&(_, p)| p.id() == &pin).ok_or(ProjectEditError::MarkerPinNotFound)?;
                    let right_pins = instance.markers()[i..].iter().chain(iter::once(instance.marker_right())).map(MarkerPin::id).copied().collect::<HashSet<_>>();
                    let cloned_pin = if let Some(component_time) = pin_owned.locked_component_time() {
                        MarkerPin::new(self.id_generator.generate_new(), component_time)
                    } else {
                        return Err(ProjectEditError::MarkerPinShouldLocked);
                    };
                    if root.iter_links().all(|link| !right_pins.contains(link.from()) && !right_pins.contains(link.to())) {
                        return Err(ProjectEditError::CannotSplitForAvoidFloating);
                    }
                    let cloned_pin_weak = *cloned_pin.id();

                    fn split_time_split_value<T: Clone>(value: &mut PinSplitValue<T>, right_pins: &HashSet<MarkerPinId>, split_target_pin: &MarkerPinId, new_pin: &MarkerPinId) -> PinSplitValue<T> {
                        let pin_position = value.binary_search_by(|p| {
                            if p == split_target_pin {
                                Ordering::Equal
                            } else if right_pins.contains(p) {
                                Ordering::Greater
                            } else {
                                Ordering::Less
                            }
                        });
                        if let Err(p) = pin_position {
                            value.split_value_by_clone(p.checked_sub(1).unwrap(), *split_target_pin).unwrap();
                        }
                        let mut stack = Vec::new();
                        while value.last().unwrap().1 != split_target_pin {
                            let v = value.last().unwrap();
                            let v = (v.0.clone(), *v.1);
                            value.pop_last().unwrap();
                            stack.push(v);
                        }
                        let mut end = *new_pin;
                        let mut data = Vector::new_sync();
                        for (v, t) in stack.into_iter().rev() {
                            data.push_back_mut((mem::replace(&mut end, t), v));
                        }
                        TimeSplitValuePersistent::by_data_end(data, end)
                    }
                    fn split_variable_parameter_value<V>(value: &mut VariableParameterValue<PinSplitValue<V>>, left_pins: &HashSet<MarkerPinId>, split_target_pin: &MarkerPinId, new_pin: &MarkerPinId) -> VariableParameterValue<PinSplitValue<V>>
                    where
                        V: Clone,
                    {
                        let &mut VariableParameterValue { ref mut params, ref components, priority } = value;
                        VariableParameterValue {
                            params: split_time_split_value(params, left_pins, split_target_pin, new_pin),
                            components: components.clone(),
                            priority,
                        }
                    }

                    let instance = root.component_mut(target_ref).ok_or(ProjectEditError::ComponentInstanceNotFound)?;
                    let image_required_params = instance.image_required_params_mut().map(|image_required_params| {
                        let &mut ImageRequiredParams {
                            ref mut transform,
                            background_color,
                            ref mut opacity,
                            ref mut blend_mode,
                            ref mut composite_operation,
                        } = image_required_params;
                        let transform = match Arc::make_mut(transform) {
                            ImageRequiredParamsTransform::Params {
                                size,
                                scale,
                                translate,
                                rotate,
                                scale_center,
                                rotate_center,
                            } => ImageRequiredParamsTransform::Params {
                                size: Arc::new(Vector3::from(AsMut::<[_; 3]>::as_mut(Arc::make_mut(size)).each_mut().map(|value| split_variable_parameter_value(value, &right_pins, &pin, &cloned_pin_weak)))),
                                scale: Arc::new(Vector3::from(AsMut::<[_; 3]>::as_mut(Arc::make_mut(scale)).each_mut().map(|value| split_variable_parameter_value(value, &right_pins, &pin, &cloned_pin_weak)))),
                                translate: Arc::new(Vector3::from(AsMut::<[_; 3]>::as_mut(Arc::make_mut(translate)).each_mut().map(|value| split_variable_parameter_value(value, &right_pins, &pin, &cloned_pin_weak)))),
                                rotate: Arc::new(split_time_split_value(Arc::make_mut(rotate), &right_pins, &pin, &cloned_pin_weak)),
                                scale_center: Arc::new(Vector3::from(AsMut::<[_; 3]>::as_mut(Arc::make_mut(scale_center)).each_mut().map(|value| split_variable_parameter_value(value, &right_pins, &pin, &cloned_pin_weak)))),
                                rotate_center: Arc::new(Vector3::from(AsMut::<[_; 3]>::as_mut(Arc::make_mut(rotate_center)).each_mut().map(|value| split_variable_parameter_value(value, &right_pins, &pin, &cloned_pin_weak)))),
                            },
                            ImageRequiredParamsTransform::Free { left_top, right_top, left_bottom, right_bottom } => ImageRequiredParamsTransform::Free {
                                left_top: Arc::new(Vector3::from(AsMut::<[_; 3]>::as_mut(Arc::make_mut(left_top)).each_mut().map(|value| split_variable_parameter_value(value, &right_pins, &pin, &cloned_pin_weak)))),
                                right_top: Arc::new(Vector3::from(AsMut::<[_; 3]>::as_mut(Arc::make_mut(right_top)).each_mut().map(|value| split_variable_parameter_value(value, &right_pins, &pin, &cloned_pin_weak)))),
                                left_bottom: Arc::new(Vector3::from(AsMut::<[_; 3]>::as_mut(Arc::make_mut(left_bottom)).each_mut().map(|value| split_variable_parameter_value(value, &right_pins, &pin, &cloned_pin_weak)))),
                                right_bottom: Arc::new(Vector3::from(AsMut::<[_; 3]>::as_mut(Arc::make_mut(right_bottom)).each_mut().map(|value| split_variable_parameter_value(value, &right_pins, &pin, &cloned_pin_weak)))),
                            },
                        };
                        ImageRequiredParams {
                            transform: Arc::new(transform),
                            background_color,
                            opacity: split_time_split_value(opacity, &right_pins, &pin, &cloned_pin_weak),
                            blend_mode: split_time_split_value(blend_mode, &right_pins, &pin, &cloned_pin_weak),
                            composite_operation: split_time_split_value(composite_operation, &right_pins, &pin, &cloned_pin_weak),
                        }
                    });
                    let audio_required_params = instance.audio_required_params_mut().map(|audio_required_params| {
                        let AudioRequiredParams { volume } = audio_required_params;
                        AudioRequiredParams {
                            volume: (0..volume.len()).map(|i| split_variable_parameter_value(volume.get_mut(i).unwrap(), &right_pins, &pin, &cloned_pin_weak)).collect(),
                        }
                    });
                    let variable_parameters_type = instance.variable_parameters_type().to_vec();
                    let variable_parameters = instance.variable_parameters_mut();
                    let variable_parameters = (0..variable_parameters.len())
                        .map(|i| {
                            let &mut VariableParameterValue { ref mut params, ref components, priority } = variable_parameters.get_mut(i).unwrap();
                            let params = match params {
                                ParameterNullableValue::None => ParameterNullableValue::None,
                                ParameterNullableValue::Image(value) => ParameterNullableValue::Image(split_time_split_value(value, &right_pins, &pin, &cloned_pin_weak)),
                                ParameterNullableValue::Audio(value) => ParameterNullableValue::Audio(split_time_split_value(value, &right_pins, &pin, &cloned_pin_weak)),
                                ParameterNullableValue::Binary(value) => ParameterNullableValue::Binary(split_time_split_value(value, &right_pins, &pin, &cloned_pin_weak)),
                                ParameterNullableValue::String(value) => ParameterNullableValue::String(split_time_split_value(value, &right_pins, &pin, &cloned_pin_weak)),
                                ParameterNullableValue::Integer(value) => ParameterNullableValue::Integer(split_time_split_value(value, &right_pins, &pin, &cloned_pin_weak)),
                                ParameterNullableValue::RealNumber(value) => ParameterNullableValue::RealNumber(split_time_split_value(value, &right_pins, &pin, &cloned_pin_weak)),
                                ParameterNullableValue::Boolean(value) => ParameterNullableValue::Boolean(split_time_split_value(value, &right_pins, &pin, &cloned_pin_weak)),
                                &mut ParameterNullableValue::Dictionary(value) => ParameterNullableValue::Dictionary(value),
                                &mut ParameterNullableValue::Array(value) => ParameterNullableValue::Array(value),
                                &mut ParameterNullableValue::ComponentClass(value) => ParameterNullableValue::ComponentClass(value),
                            };
                            VariableParameterValue { params, components: components.clone(), priority }
                        })
                        .collect();
                    let fixed_parameters_type = Arc::clone(instance.fixed_parameters_type());
                    let fixed_parameters = Arc::clone(instance.fixed_parameters());
                    let component_class = instance.component_class().clone();
                    let processor = instance.processor().clone();

                    let mut pins = instance.markers_mut().drain(i..);
                    let new_right = pins.next().unwrap();
                    let right_markers = pins.collect();
                    let right_right = mem::replace(instance.marker_right_mut(), new_right);
                    let mut builder = ComponentInstance::builder(component_class, cloned_pin, right_right, right_markers, processor);
                    builder = builder.fixed_parameters(fixed_parameters_type, fixed_parameters).variable_parameters(variable_parameters_type, variable_parameters);
                    if let Some(image_required_params) = image_required_params {
                        builder = builder.image_required_params(image_required_params);
                    }
                    if let Some(audio_required_params) = audio_required_params {
                        builder = builder.audio_required_params(audio_required_params);
                    }
                    let new_instance = builder.build(&self.id_generator);
                    root.add_component(new_instance);
                    let new_links = root
                        .iter_links()
                        .filter_map(|link| {
                            if link.from() == &pin {
                                Some(MarkerLink::new(cloned_pin_weak, *link.to(), link.len()))
                            } else if link.to() == &pin {
                                Some(MarkerLink::new(*link.from(), cloned_pin_weak, link.len()))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    for link in new_links {
                        root.add_link(link);
                    }

                    let time_map = mpdelta_differential::collect_cached_time(&*root)?;
                    RootComponentClassItemWrite::commit_changes(root, time_map);
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::SplitAtPin(&pin)));
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
