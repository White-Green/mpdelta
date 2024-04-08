use crate::project_editor::dsa::union_find::UnionFind;
use async_trait::async_trait;
use dashmap::DashMap;
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::instance::ComponentInstanceHandle;
use mpdelta_core::component::link::MarkerLink;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerPinHandle, MarkerTime};
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::{EditEventListener, Editor};
use mpdelta_core::edit::{InstanceEditCommand, InstanceEditEvent, RootComponentEditCommand, RootComponentEditEvent};
use mpdelta_core::project::RootComponentClassHandle;
use mpdelta_core::ptr::StaticPointerOwned;
use mpdelta_core::time::TimelineTime;
use qcell::{TCell, TCellOwner};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::iter;
use std::sync::atomic::AtomicUsize;
use std::sync::{atomic, Arc};
use thiserror::Error;
use tokio::sync::RwLock;

mod dsa;

// TODO: Listenerをusizeで管理してるので、overflowしたらバグる(ほとんど気にしなくても良さそうではあるが)
pub struct ProjectEditor<K: 'static, T> {
    key: Arc<RwLock<TCellOwner<K>>>,
    edit_event_listener_id: AtomicUsize,
    edit_event_listeners: Arc<DashMap<usize, Box<dyn EditEventListener<K, T>>>>,
}

impl<K, T> ProjectEditor<K, T> {
    pub fn new(key: Arc<RwLock<TCellOwner<K>>>) -> ProjectEditor<K, T> {
        ProjectEditor {
            key,
            edit_event_listener_id: AtomicUsize::default(),
            edit_event_listeners: Arc::new(DashMap::new()),
        }
    }
}

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
}

pub struct ProjectEditListenerGuard<K: 'static, T> {
    id: usize,
    edit_event_listeners: Arc<DashMap<usize, Box<dyn EditEventListener<K, T>>>>,
}

impl<K, T> Drop for ProjectEditListenerGuard<K, T> {
    fn drop(&mut self) {
        self.edit_event_listeners.remove(&self.id);
    }
}

#[async_trait]
impl<K, T> Editor<K, T> for ProjectEditor<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    type Log = ProjectEditLog;
    type Err = ProjectEditError;
    type EditEventListenerGuard = ProjectEditListenerGuard<K, T>;

    fn add_edit_event_listener(&self, listener: impl EditEventListener<K, T> + 'static) -> Self::EditEventListenerGuard {
        let id = self.edit_event_listener_id.fetch_add(1, atomic::Ordering::AcqRel);
        self.edit_event_listeners.insert(id, Box::new(listener));
        ProjectEditListenerGuard {
            id,
            edit_event_listeners: Arc::clone(&self.edit_event_listeners),
        }
    }

    async fn edit(&self, target_ref: &RootComponentClassHandle<K, T>, command: RootComponentEditCommand<K, T>) -> Result<Self::Log, Self::Err> {
        let target = target_ref.upgrade().ok_or(ProjectEditError::InvalidTarget)?;
        let target = target.read().await;
        match command {
            RootComponentEditCommand::AddComponentInstance(instance) => {
                let instance_ref = StaticPointerOwned::reference(&instance).clone();
                {
                    let key = self.key.read().await;
                    let base = if let Some(base) = target.get().await.component().last() {
                        StaticPointerOwned::reference(base.ro(&key).marker_left()).clone()
                    } else {
                        target.left().await.clone()
                    };
                    let guard = instance.ro(&key);
                    let left = guard.marker_left();
                    let right = guard.marker_right();
                    let link_for_zero = MarkerLink::new(base, StaticPointerOwned::reference(left).clone(), TimelineTime::new(MixedFraction::from_integer(1)));
                    let link_for_length = MarkerLink::new(StaticPointerOwned::reference(left).clone(), StaticPointerOwned::reference(right).clone(), TimelineTime::new(MixedFraction::from_integer(1)));
                    let mut item = target.get_mut().await;
                    item.component_mut().push(instance);
                    item.link_mut().extend([StaticPointerOwned::new(TCell::new(link_for_zero)), StaticPointerOwned::new(TCell::new(link_for_length))]);
                    let item = item.downgrade();
                    if let Err(err) = mpdelta_differential::collect_cached_time(item.component(), item.link(), item.left().as_ref(), item.right().as_ref(), &key) {
                        eprintln!("{err}");
                    }
                }

                // TODO: このへんもうちょっとバグりにくい構造を探したいよね
                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::AddComponentInstance(&instance_ref)));
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::RemoveMarkerLink(link) => {
                {
                    let mut item = target.get_mut().await;
                    item.link_mut().retain(|l| *l != link);
                    let item = item.downgrade();
                    let key = self.key.read().await;
                    if let Err(err) = mpdelta_differential::collect_cached_time(item.component(), item.link(), item.left().as_ref(), item.right().as_ref(), &key) {
                        eprintln!("{err}");
                    }
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::RemoveMarkerLink(&link)));
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::EditMarkerLinkLength(link, len) => {
                {
                    if let Some(link) = link.upgrade() {
                        let mut key = self.key.write().await;
                        link.rw(&mut key).set_len(len);
                        let key = key.downgrade();
                        let item = target.get().await;
                        if let Err(err) = mpdelta_differential::collect_cached_time(item.component(), item.link(), item.left().as_ref(), item.right().as_ref(), &key) {
                            eprintln!("{err}");
                        }
                    }
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::EditMarkerLinkLength(&link, len)));
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::InsertComponentInstanceTo(instance, index) => {
                {
                    let mut item = target.get_mut().await;
                    let components = item.component_mut();
                    let index = if components.len() <= index { components.len() - 1 } else { index };
                    let Some(i) = components.iter().enumerate().find_map(|(i, instance_owned)| (*instance_owned == instance).then_some(i)) else {
                        return Err(ProjectEditError::ComponentInstanceNotFound);
                    };
                    match i.cmp(&index) {
                        Ordering::Less => (i..index).for_each(|i| components.swap(i, i + 1)),
                        Ordering::Greater => (index..i).rev().for_each(|i| components.swap(i, i + 1)),
                        Ordering::Equal => {}
                    }
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::InsertComponentInstanceTo(&instance, index)));
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::DeleteComponentInstance(instance) => {
                {
                    let mut item = target.get_mut().await;
                    let key = self.key.write().await;
                    let Some(instance_ref) = instance.upgrade() else {
                        return Err(ProjectEditError::ComponentInstanceNotFound);
                    };
                    let instance_ref = instance_ref.ro(&key);
                    let delete_target_pins = [instance_ref.marker_left(), instance_ref.marker_right()].into_iter().chain(instance_ref.markers()).map(StaticPointerOwned::reference).collect::<HashSet<_>>();
                    let mut pin_union_find = UnionFind::new();
                    let connected_links = {
                        let mut connected_links = HashMap::<_, HashSet<_>>::new();
                        for link in item.link() {
                            let link_ptr = link.as_ref();
                            let Some(link) = link_ptr.upgrade() else {
                                continue;
                            };
                            let link = link.ro(&key);
                            connected_links.entry(link.from().clone()).or_default().insert(link.to().clone());
                            connected_links.entry(link.to().clone()).or_default().insert(link.from().clone());
                            if !delete_target_pins.contains(&link.from()) && !delete_target_pins.contains(&link.to()) {
                                pin_union_find.union(link.from().clone(), link.to().clone());
                            }
                        }
                        connected_links
                    };
                    let adjacent_pins = delete_target_pins.iter().copied().filter_map(|p| connected_links.get(p)).flatten().collect::<HashSet<_>>();
                    let left_pin_root = pin_union_find.get_root(StaticPointerOwned::reference(item.left()).clone());
                    if let Some(connection_base) = adjacent_pins.iter().copied().find(|&p| pin_union_find.get_root(p.clone()) == left_pin_root) {
                        if let Some(connection_base_ref) = connection_base.upgrade() {
                            let from_time = connection_base_ref.ro(&key).cached_timeline_time();
                            item.link_mut().extend(
                                adjacent_pins
                                    .iter()
                                    .filter_map(|&p| {
                                        if pin_union_find.get_root(p.clone()) == left_pin_root {
                                            return None;
                                        }
                                        let to_time = p.upgrade()?.ro(&key).cached_timeline_time();
                                        Some(MarkerLink::new((*connection_base).clone(), p.clone(), to_time - from_time))
                                    })
                                    .map(TCell::new)
                                    .map(StaticPointerOwned::new),
                            );
                        }
                    }
                    item.link_mut().retain(|link| {
                        let link = link.ro(&key);
                        !delete_target_pins.contains(&link.from()) && !delete_target_pins.contains(&link.to())
                    });
                    let components = item.component_mut();
                    let components_len = components.len();
                    components.retain(|i| *i != instance);
                    let new_components_len = components.len();
                    if let Err(err) = mpdelta_differential::collect_cached_time(item.component(), item.link(), item.left().as_ref(), item.right().as_ref(), &key) {
                        eprintln!("{err}");
                    }

                    if new_components_len == components_len {
                        return Err(ProjectEditError::ComponentInstanceNotFound);
                    }
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::DeleteComponentInstance(&instance)));
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::EditComponentLength(length) => {
                {
                    let mut key = self.key.write().await;
                    let mut item = target.get_mut().await;
                    let left_pin = StaticPointerOwned::reference(item.left()).clone();
                    let right = item.right();
                    let right_pin = StaticPointerOwned::reference(right).clone();
                    let right = right.rw(&mut key);
                    right.set_locked_component_time(Some(length));
                    right.cache_timeline_time(TimelineTime::new(length.value()));
                    let diff = TimelineTime::new(length.value() - item.length().value());
                    item.set_length(length);

                    let mut pin_union_find = UnionFind::new();
                    item.link().iter().for_each(|link| {
                        let link = link.ro(&key);
                        if link.from() != &right_pin && link.to() != &right_pin {
                            pin_union_find.union(link.from().clone(), link.to().clone());
                        }
                    });
                    let left_root = pin_union_find.get_root(left_pin.clone());
                    for link in item.link() {
                        let link = link.rw(&mut key);
                        if link.to() == &right_pin && pin_union_find.get_root(link.from().clone()) == left_root {
                            link.set_len(link.len() + diff);
                        } else if link.from() == &right_pin && pin_union_find.get_root(link.to().clone()) == left_root {
                            link.set_len(link.len() - diff);
                        }
                    }

                    if let Err(err) = mpdelta_differential::collect_cached_time(item.component(), item.link(), item.left().as_ref(), item.right().as_ref(), &key) {
                        eprintln!("{err}");
                    }
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::EditComponentLength(length)));
                Ok(ProjectEditLog::Unimplemented)
            }
            RootComponentEditCommand::ConnectMarkerPins(from, to) => {
                {
                    let key = self.key.write().await;
                    let mut item = target.get_mut().await;

                    if from == to {
                        return Err(ProjectEditError::MarkerPinsAreSame);
                    }

                    let Some(from_strong_ref) = from.upgrade() else {
                        return Err(ProjectEditError::InvalidMarkerPin);
                    };
                    let Some(to_strong_ref) = to.upgrade() else {
                        return Err(ProjectEditError::InvalidMarkerPin);
                    };

                    let mut pin_union_find = UnionFind::new();
                    let mut connected_pins = HashMap::<_, HashMap<_, _>>::new();
                    for link in item.link() {
                        let link_ptr = link.as_ref();
                        let Some(link) = link_ptr.upgrade() else {
                            continue;
                        };
                        let link = link.ro(&key);
                        assert_ne!(link.from(), link.to());
                        connected_pins.entry(link.from().clone()).or_default().insert(link.to().clone(), Some(link_ptr.clone()));
                        connected_pins.entry(link.to().clone()).or_default().insert(link.from().clone(), Some(link_ptr.clone()));
                        if ![&from, &to].contains(&link.from()) && ![&from, &to].contains(&link.to()) {
                            pin_union_find.union(link.from().clone(), link.to().clone());
                        }
                    }
                    for component in item.component() {
                        let component = component.ro(&key);
                        let locked_pins = component.markers().iter().chain([component.marker_left(), component.marker_right()]).filter(|pin| pin.ro(&key).locked_component_time().is_some());
                        for pin in locked_pins.clone() {
                            let pin = StaticPointerOwned::reference(pin);
                            let connection = connected_pins.entry(pin.clone()).or_default();
                            for other_pin in locked_pins.clone().filter(|&p| p != pin) {
                                connection.entry(StaticPointerOwned::reference(other_pin).clone()).or_insert(None);
                            }
                        }
                        let mut all_pins = locked_pins.clone().filter(|&pin| pin != &from && pin != &to);
                        let Some(base_pin) = all_pins.next() else {
                            continue;
                        };
                        for pin in all_pins {
                            pin_union_find.union(StaticPointerOwned::reference(base_pin).clone(), StaticPointerOwned::reference(pin).clone());
                        }
                    }

                    'edit: {
                        let left = StaticPointerOwned::reference(item.left());
                        let mut prev = HashMap::from([(left.clone(), left.clone())]);
                        let mut q = VecDeque::from([left.clone()]);
                        while let Some(pin) = q.pop_front() {
                            for other_pin in connected_pins[&pin].keys() {
                                if prev.contains_key(other_pin) {
                                    continue;
                                }
                                prev.insert(other_pin.clone(), pin.clone());
                                q.push_back(other_pin.clone());
                            }
                        }
                        let path = iter::successors(Some(&from), |&pin| {
                            let prev = &prev[pin];
                            (prev != pin).then_some(prev)
                        })
                        .collect::<HashSet<_>>();
                        let lca = iter::successors(Some(&to), |&pin| {
                            let prev = &prev[pin];
                            (prev != pin).then_some(prev)
                        })
                        .find(|pin| path.contains(pin))
                        .unwrap();

                        if let Some(link) = connected_pins[&from].get(&to) {
                            if link.is_some() {
                                return Err(ProjectEditError::PinsAlreadyConnected);
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
                            let links = item.link_mut();
                            if prev.contains_key(&to) {
                                let link = connected_pins[&to][prev[&to]].as_ref().unwrap();
                                links.retain(|l| l != link);
                            }
                            let len = to_strong_ref.ro(&key).cached_timeline_time() - from_strong_ref.ro(&key).cached_timeline_time();
                            links.push(StaticPointerOwned::new(TCell::new(MarkerLink::new(from.clone(), to.clone(), len))));
                            break 'edit;
                        }

                        let link = iter::successors(Some((&to, &prev[&to])), |&(_, pin)| {
                            let prev = &prev[pin];
                            (prev != pin).then_some((pin, prev))
                        })
                        .take_while(|&(pin, _)| pin != lca)
                        .find_map(|(pin, prev)| connected_pins[pin][prev].clone());
                        if let Some(link) = link {
                            let links = item.link_mut();
                            let len = to_strong_ref.ro(&key).cached_timeline_time() - from_strong_ref.ro(&key).cached_timeline_time();
                            links.retain(|l| l != &link);
                            links.push(StaticPointerOwned::new(TCell::new(MarkerLink::new(from.clone(), to.clone(), len))));
                            break 'edit;
                        }

                        let link = iter::successors(Some((&from, &prev[&from])), |&(_, pin)| {
                            let prev = &prev[pin];
                            (prev != pin).then_some((pin, prev))
                        })
                        .take_while(|&(pin, _)| pin != lca)
                        .find_map(|(pin, prev)| connected_pins[pin][prev].clone());
                        if let Some(link) = link {
                            let links = item.link_mut();
                            let len = to_strong_ref.ro(&key).cached_timeline_time() - from_strong_ref.ro(&key).cached_timeline_time();
                            links.retain(|l| l != &link);
                            links.push(StaticPointerOwned::new(TCell::new(MarkerLink::new(from.clone(), to.clone(), len))));
                            break 'edit;
                        }

                        unreachable!();
                    }

                    if let Err(err) = mpdelta_differential::collect_cached_time(item.component(), item.link(), item.left().as_ref(), item.right().as_ref(), &key) {
                        eprintln!("{err}");
                    }
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit(target_ref, RootComponentEditEvent::ConnectMarkerPins(&from, &to)));
                Ok(ProjectEditLog::Unimplemented)
            }
        }
    }

    async fn edit_instance(&self, root_ref: &RootComponentClassHandle<K, T>, target_ref: &ComponentInstanceHandle<K, T>, command: InstanceEditCommand<K, T>) -> Result<Self::Log, Self::Err> {
        let target = target_ref.upgrade().ok_or(ProjectEditError::InvalidTarget)?;
        match command {
            InstanceEditCommand::UpdateFixedParams(params) => {
                {
                    let mut key = self.key.write().await;
                    let slot = target.rw(&mut key).fixed_parameters_mut();
                    if slot.len() != params.len() {
                        return Err(ProjectEditError::ParameterTypeMismatch);
                    }
                    for (slot, value) in slot.iter_mut().zip(params.iter()) {
                        if slot.select() != value.select() {
                            return Err(ProjectEditError::ParameterTypeMismatch);
                        }
                        *slot = value.clone();
                    }
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::UpdateFixedParams(&params)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::UpdateVariableParams(params) => {
                {
                    let mut key = self.key.write().await;
                    let slot = target.rw(&mut key).variable_parameters_mut();
                    if slot.len() != params.len() {
                        return Err(ProjectEditError::ParameterTypeMismatch);
                    }
                    for (slot, value) in slot.iter_mut().zip(params.iter()) {
                        if slot.params.select() != value.params.select() {
                            return Err(ProjectEditError::ParameterTypeMismatch);
                        }
                        *slot = value.clone();
                    }
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::UpdateVariableParams(&params)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::UpdateImageRequiredParams(params) => {
                {
                    let mut key = self.key.write().await;
                    target.rw(&mut key).set_image_required_params(params.clone());
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::UpdateImageRequiredParams(&params)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::MoveComponentInstance(to) => {
                {
                    let root = root_ref.upgrade().ok_or(ProjectEditError::InvalidTarget)?;
                    let mut key = self.key.write().await;
                    let root = root.read().await;
                    let target_raw_ref = target.ro(&key);
                    let target_left = target_raw_ref.marker_left();
                    let target_right = target_raw_ref.marker_right();
                    let target_contains_pins = target_raw_ref.markers().iter().chain([target_left, target_right]).map(StaticPointerOwned::reference).cloned().collect::<HashSet<_>>();
                    let root = root.get().await;
                    let components = root.component();
                    let mut pin_union_find = UnionFind::new();
                    let connected_links = {
                        let mut connected_links = HashMap::<_, HashSet<_>>::new();
                        for link in root.link() {
                            let link_ptr = link.as_ref();
                            let Some(link) = link_ptr.upgrade() else {
                                continue;
                            };
                            let link = link.ro(&key);
                            connected_links.entry(link.from().clone()).or_default().insert(link_ptr.clone());
                            connected_links.entry(link.to().clone()).or_default().insert(link_ptr.clone());
                            if !target_contains_pins.contains(link.from()) && !target_contains_pins.contains(link.to()) {
                                pin_union_find.union(link.from().clone(), link.to().clone());
                            }
                        }
                        connected_links
                    };
                    for component in components {
                        let component = component.as_ref();
                        let Some(component) = component.upgrade() else {
                            continue;
                        };
                        let component = component.ro(&key);
                        let left = component.marker_left();
                        let right = component.marker_right();
                        let mut all_pins = component
                            .markers()
                            .iter()
                            .map(|pin| (StaticPointerOwned::reference(pin), &**pin))
                            .chain([(StaticPointerOwned::reference(component.marker_left()), &**left), (StaticPointerOwned::reference(component.marker_right()), &**right)])
                            .filter_map(|(pin_handle, pin)| pin.ro(&key).locked_component_time().map(|_| pin_handle));
                        let Some(base_pin) = all_pins.next() else {
                            continue;
                        };
                        for pin in all_pins {
                            pin_union_find.union(base_pin.clone(), pin.clone());
                        }
                    }
                    let current_left_time = target_left.ro(&key).cached_timeline_time();
                    let delta = to - current_left_time;

                    let zero_pin_root = pin_union_find.get_root(StaticPointerOwned::reference(root.left()).clone());
                    for pin_handle in &target_contains_pins {
                        let Some(pin) = pin_handle.upgrade() else {
                            continue;
                        };
                        if pin.ro(&key).locked_component_time().is_none() {
                            continue;
                        }
                        let Some(link) = connected_links.get(pin_handle) else {
                            continue;
                        };
                        for link_handle in link {
                            let Some(link) = link_handle.upgrade() else {
                                continue;
                            };
                            let link = link.rw(&mut key);
                            let other_pin = if link.to() == pin_handle { link.from() } else { link.to() };
                            if target_contains_pins.contains(other_pin) {
                                continue;
                            }
                            if pin_union_find.get_root(other_pin.clone()) != zero_pin_root {
                                continue;
                            }
                            link.set_len(if link.to() == pin_handle { link.len() + delta } else { link.len() - delta });
                        }
                    }

                    if let Err(err) = mpdelta_differential::collect_cached_time(root.component(), root.link(), root.left().as_ref(), root.right().as_ref(), &key) {
                        eprintln!("{err}");
                    }
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::MoveComponentInstance(to)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::MoveMarkerPin(pin, to) => {
                {
                    let root = root_ref.upgrade().ok_or(ProjectEditError::InvalidTarget)?;
                    let (root, mut key) = tokio::join!(root.read(), self.key.write());
                    let root = root.get().await;

                    let mut next_links = HashSet::new();
                    let mut pin_union_find = UnionFind::new();
                    for link in root.link() {
                        let link_ptr = link.as_ref();
                        let Some(link) = link_ptr.upgrade() else {
                            continue;
                        };
                        let link = link.ro(&key);
                        if link.from() == &pin || link.to() == &pin {
                            next_links.insert(link_ptr.clone());
                        } else {
                            pin_union_find.union(link.from().clone(), link.to().clone());
                        }
                    }
                    for component in root.component() {
                        let component = component.ro(&key);
                        let mut locked_markers = [component.marker_left(), component.marker_right()].into_iter().chain(component.markers()).filter(|&p| p != &pin).filter(|p| p.ro(&key).locked_component_time().is_some());
                        let Some(base_marker) = locked_markers.next() else {
                            continue;
                        };
                        for marker in locked_markers {
                            pin_union_find.union(StaticPointerOwned::reference(base_marker).clone(), StaticPointerOwned::reference(marker).clone());
                        }
                    }

                    let root_left_root = pin_union_find.get_root(StaticPointerOwned::reference(root.left()).clone());
                    let Some(target_pin) = pin.upgrade() else {
                        return Err(ProjectEditError::InvalidMarkerPin);
                    };
                    let time_diff = to - target_pin.ro(&key).cached_timeline_time();
                    let mut edited = false;
                    for l in &next_links {
                        let link = l.upgrade().unwrap();
                        let other_pin = if link.ro(&key).from() == &pin { link.ro(&key).to().clone() } else { link.ro(&key).from().clone() };
                        if pin_union_find.get_root(other_pin) == root_left_root {
                            let link = link.rw(&mut key);
                            link.set_len(if link.to() == &pin { link.len() + time_diff } else { link.len() - time_diff });
                            edited = true;
                        }
                    }
                    if !edited {
                        let target = target.ro(&key);
                        fn pin_upgrade_fn<K>(key: &TCellOwner<K>) -> impl for<'a> FnMut(&'a MarkerPinHandle<K>) -> Option<(MarkerTime, TimelineTime)> + '_ {
                            move |p| p.upgrade().and_then(|p| p.ro(key).locked_component_time().map(|locked| (locked, p.ro(key).cached_timeline_time())))
                        }
                        let (left, right) = if target.marker_left() == &pin {
                            let mut pins_iter = target.markers().iter().chain(iter::once(target.marker_right())).map(StaticPointerOwned::reference).filter_map(pin_upgrade_fn(&key));
                            let left = pins_iter.next().expect("broken component structure");
                            let right = pins_iter.next();
                            (left, right)
                        } else if target.marker_right() == &pin {
                            let mut pins_iter = iter::once(target.marker_left()).chain(target.markers()).map(StaticPointerOwned::reference).filter_map(pin_upgrade_fn(&key)).rev();
                            let right = pins_iter.next().expect("broken component structure");
                            if let Some(left) = pins_iter.next() {
                                (left, Some(right))
                            } else {
                                (right, None)
                            }
                        } else {
                            let mut all_pins = iter::once(target.marker_left()).chain(target.markers()).chain(iter::once(target.marker_right())).map(StaticPointerOwned::reference);
                            let left = all_pins.by_ref().take_while(|&p| p != &pin).filter_map(pin_upgrade_fn(&key)).fold([None, None], |[_, left], right| [left, Some(right)]);
                            let mut right_ptr = all_pins.filter_map(pin_upgrade_fn(&key));
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
                        target_pin.rw(&mut key).set_locked_component_time(MarkerTime::new(lock_time));
                    }

                    if let Err(err) = mpdelta_differential::collect_cached_time(root.component(), root.link(), root.left().as_ref(), root.right().as_ref(), &key) {
                        eprintln!("{err}");
                    }
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::MoveMarkerPin(&pin, to)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::AddMarkerPin(at) => {
                {
                    let mut key = self.key.write().await;
                    let target_read = target.ro(&key);
                    let Err(insert_index) = target_read.markers().binary_search_by_key(&at, |p| p.ro(&key).cached_timeline_time()) else {
                        return Err(ProjectEditError::InvalidMarkerPinAddPosition);
                    };
                    if at <= target_read.marker_left().ro(&key).cached_timeline_time() || target_read.marker_right().ro(&key).cached_timeline_time() <= at {
                        return Err(ProjectEditError::InvalidMarkerPinAddPosition);
                    }
                    let mut iter_left = iter::once(target_read.marker_left())
                        .chain(target_read.markers().get(..insert_index).into_iter().flatten())
                        .filter_map(|p| {
                            let p = p.ro(&key);
                            p.locked_component_time().map(|locked| (locked, p.cached_timeline_time()))
                        })
                        .rev();
                    let mut iter_right = target_read.markers().get(insert_index..).into_iter().flatten().chain(iter::once(target_read.marker_right())).filter_map(|p| {
                        let p = p.ro(&key);
                        p.locked_component_time().map(|locked| (locked, p.cached_timeline_time()))
                    });
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
                    target.rw(&mut key).markers_mut().insert(insert_index, StaticPointerOwned::new(TCell::new(MarkerPin::new(at, MarkerTime::new(lock_time).unwrap()))));
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::AddMarkerPin(at)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::DeleteMarkerPin(pin) => {
                {
                    let root = root_ref.upgrade().ok_or(ProjectEditError::InvalidTarget)?;
                    let (root, mut key) = tokio::join!(root.read(), self.key.write());
                    let mut root = root.get_mut().await;

                    let Some((remove_marker_index, _)) = target.ro(&key).markers().iter().enumerate().find(|&(_, p)| p == &pin) else {
                        return Err(ProjectEditError::MarkerPinNotFound);
                    };
                    let mut all_pins = [target.ro(&key).marker_left(), target.ro(&key).marker_right()].into_iter().chain(target.ro(&key).markers()).map(StaticPointerOwned::reference);
                    let locked_pins = all_pins.clone().filter(|&p| p != &pin).filter(|p| p.upgrade().is_some_and(|p| p.ro(&key).locked_component_time().is_some())).count();
                    if locked_pins == 0 {
                        return Err(ProjectEditError::MarkerPinNotFound);
                    }
                    let near_locked_pin = all_pins
                        .by_ref()
                        .take_while(|&p| p != &pin)
                        .filter(|p| p.upgrade().is_some_and(|p| p.ro(&key).locked_component_time().is_some()))
                        .last()
                        .or_else(|| all_pins.find(|p| p.upgrade().is_some_and(|p| p.ro(&key).locked_component_time().is_some())))
                        .unwrap();

                    let mut pin_union_find = UnionFind::new();
                    let mut next_link = HashSet::new();
                    for link in root.link() {
                        let link_ptr = StaticPointerOwned::reference(link);
                        let Some(link) = link_ptr.upgrade() else {
                            continue;
                        };
                        let link = link.ro(&key);
                        if link.from() == &pin || link.to() == &pin {
                            next_link.insert(link_ptr.clone());
                        } else {
                            pin_union_find.union(link.from().clone(), link.to().clone());
                        }
                    }
                    for component in root.component() {
                        let component = component.ro(&key);
                        let mut locked_markers = [component.marker_left(), component.marker_right()]
                            .into_iter()
                            .chain(component.markers().iter())
                            .map(StaticPointerOwned::reference)
                            .filter(|&p| p != &pin)
                            .filter(|p| p.upgrade().is_some_and(|p| p.ro(&key).locked_component_time().is_some()));
                        let Some(base_marker) = locked_markers.next() else {
                            continue;
                        };
                        for marker in locked_markers {
                            pin_union_find.union(base_marker.clone(), marker.clone());
                        }
                    }

                    let root_left_root = pin_union_find.get_root(StaticPointerOwned::reference(root.left()).clone());
                    let base_pin = if pin_union_find.get_root(near_locked_pin.clone()) == root_left_root {
                        near_locked_pin.clone()
                    } else {
                        next_link
                            .iter()
                            .map(|link| {
                                let link = link.upgrade().unwrap();
                                let link = link.ro(&key);
                                if link.from() == &pin {
                                    link.to().clone()
                                } else {
                                    link.from().clone()
                                }
                            })
                            .find(|p| p.upgrade().is_some_and(|p| p.ro(&key).locked_component_time().is_some()))
                            .unwrap()
                    };
                    let base_pin_time = base_pin.upgrade().unwrap().ro(&key).cached_timeline_time();
                    let floating_pins = next_link
                        .iter()
                        .map(|link| {
                            let link = link.upgrade().unwrap();
                            let link = link.ro(&key);
                            if link.from() == &pin {
                                link.to().clone()
                            } else {
                                link.from().clone()
                            }
                        })
                        .filter(|p| pin_union_find.get_root(p.clone()) == root_left_root)
                        .collect::<Vec<_>>();
                    let all_links = root.link_mut();
                    all_links.retain(|link| {
                        let link = link.ro(&key);
                        link.from() != &pin && link.to() != &pin
                    });
                    for p in floating_pins {
                        let to_time = p.upgrade().unwrap().ro(&key).cached_timeline_time();
                        let link = MarkerLink::new(base_pin.clone(), p, to_time - base_pin_time);
                        all_links.push(StaticPointerOwned::new(TCell::new(link)));
                    }
                    drop(all_pins);
                    target.rw(&mut key).markers_mut().remove(remove_marker_index);

                    if let Err(err) = mpdelta_differential::collect_cached_time(root.component(), root.link(), root.left().as_ref(), root.right().as_ref(), &key) {
                        eprintln!("{err}");
                    }
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::DeleteMarkerPin(&pin)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::LockMarkerPin(pin) => {
                {
                    let root = root_ref.upgrade().ok_or(ProjectEditError::InvalidTarget)?;
                    let (root, mut key) = tokio::join!(root.read(), self.key.write());
                    let root = root.get().await;
                    let Some(pin_ref) = pin.upgrade() else {
                        return Err(ProjectEditError::InvalidMarkerPin);
                    };
                    if pin_ref.ro(&key).locked_component_time().is_some() {
                        return Ok(ProjectEditLog::Unimplemented);
                    }
                    let target = target.ro(&key);
                    let mut all_pins = iter::once(target.marker_left()).chain(target.markers()).chain(iter::once(target.marker_right())).map(StaticPointerOwned::reference);
                    if all_pins.clone().all(|p| p != &pin) {
                        return Err(ProjectEditError::MarkerPinNotFound);
                    }
                    let left_next = all_pins
                        .by_ref()
                        .take_while(|&p| p != &pin)
                        .filter_map(|p| p.upgrade().and_then(|p| p.ro(&key).locked_component_time().is_some().then_some(p)))
                        .fold([None, None], |[_, left], p| [left, Some(p)]);
                    let mut right_pins = all_pins.filter_map(|p| p.upgrade().and_then(|p| p.ro(&key).locked_component_time().is_some().then_some(p)));
                    let right_next = [right_pins.next(), right_pins.next()];
                    let (left_next, right_next) = match (left_next, right_next) {
                        ([_, Some(left_next)], [Some(right_next), _]) => (left_next, Some(right_next)),
                        ([Some(left_next), Some(right_next)], _) => (left_next, Some(right_next)),
                        (_, [Some(left_next), Some(right_next)]) => (left_next, Some(right_next)),
                        ([_, Some(base)], _) => (base, None),
                        (_, [Some(base), _]) => (base, None),
                        _ => panic!("broken component structure"),
                    };
                    let time = pin_ref.ro(&key).cached_timeline_time();
                    let lock_time = if let Some(right_next) = right_next {
                        let p = (time - left_next.ro(&key).cached_timeline_time()).value() / (right_next.ro(&key).cached_timeline_time() - left_next.ro(&key).cached_timeline_time()).value();
                        left_next.ro(&key).locked_component_time().unwrap().value() + (right_next.ro(&key).locked_component_time().unwrap().value() - left_next.ro(&key).locked_component_time().unwrap().value()) * p
                    } else {
                        let base = left_next.ro(&key).cached_timeline_time().value();
                        left_next.ro(&key).locked_component_time().unwrap().value() + (time.value() - base)
                    };
                    pin_ref.rw(&mut key).set_locked_component_time(Some(MarkerTime::new(lock_time).unwrap()));

                    if let Err(err) = mpdelta_differential::collect_cached_time(root.component(), root.link(), root.left().as_ref(), root.right().as_ref(), &key) {
                        eprintln!("{err}");
                    }
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::LockMarkerPin(&pin)));
                Ok(ProjectEditLog::Unimplemented)
            }
            InstanceEditCommand::UnlockMarkerPin(pin) => {
                {
                    let root = root_ref.upgrade().ok_or(ProjectEditError::InvalidTarget)?;
                    let (root, mut key) = tokio::join!(root.read(), self.key.write());
                    let mut root = root.get_mut().await;
                    let Some(pin_ref) = pin.upgrade() else {
                        return Err(ProjectEditError::InvalidMarkerPin);
                    };
                    if pin_ref.ro(&key).locked_component_time().is_none() {
                        return Ok(ProjectEditLog::Unimplemented);
                    }
                    let mut pin_union_find = UnionFind::new();
                    for link in root.link() {
                        let link_ptr = link.as_ref();
                        let Some(link) = link_ptr.upgrade() else {
                            continue;
                        };
                        let link = link.ro(&key);
                        pin_union_find.union(link.from().clone(), link.to().clone());
                    }
                    for component in root.component() {
                        let component = component.ro(&key);
                        let mut iter = [component.marker_left(), component.marker_right()]
                            .into_iter()
                            .chain(component.markers())
                            .map(StaticPointerOwned::reference)
                            .filter(|&p| p != &pin)
                            .filter(|p| p.upgrade().is_some_and(|p| p.ro(&key).locked_component_time().is_some()));
                        let Some(base_pin) = iter.next() else {
                            continue;
                        };
                        for p in iter {
                            pin_union_find.union(base_pin.clone(), p.clone());
                        }
                    }
                    if pin_union_find.get_root(pin.clone()) == pin_union_find.get_root(StaticPointerOwned::reference(root.left()).clone()) {
                        let Some(pin) = pin.upgrade() else {
                            return Err(ProjectEditError::InvalidMarkerPin);
                        };
                        pin.rw(&mut key).set_locked_component_time(None);
                        return Ok(ProjectEditLog::Unimplemented);
                    }
                    let target = target.ro(&key);
                    let all_pins = iter::once(target.marker_left()).chain(target.markers()).chain(iter::once(target.marker_right())).map(StaticPointerOwned::reference);
                    if !all_pins.clone().any(|p| p == &pin) {
                        return Err(ProjectEditError::MarkerPinNotFound);
                    };
                    let mut locked_pins = all_pins.filter(|p| p.upgrade().is_some_and(|p| p.ro(&key).locked_component_time().is_some()));
                    let left_next_pin = locked_pins.by_ref().take_while(|&p| p != &pin).last();
                    let Some(next_pin) = left_next_pin.or_else(|| locked_pins.next()) else {
                        return Err(ProjectEditError::CannotUnlockForAvoidFloating);
                    };
                    root.link_mut().push(StaticPointerOwned::new(TCell::new(MarkerLink::new(
                        pin.clone(),
                        next_pin.clone(),
                        next_pin.upgrade().unwrap().ro(&key).cached_timeline_time() - pin.upgrade().unwrap().ro(&key).cached_timeline_time(),
                    ))));
                    let Some(pin_ref) = pin.upgrade() else {
                        return Err(ProjectEditError::InvalidMarkerPin);
                    };
                    pin_ref.rw(&mut key).set_locked_component_time(None);

                    if let Err(err) = mpdelta_differential::collect_cached_time(root.component(), root.link(), root.left().as_ref(), root.right().as_ref(), &key) {
                        eprintln!("{err}");
                    }
                }

                self.edit_event_listeners.iter().for_each(|listener| listener.on_edit_instance(root_ref, target_ref, InstanceEditEvent::UnlockMarkerPin(&pin)));
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
