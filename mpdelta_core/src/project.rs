use crate::common::mixed_fraction::MixedFraction;
use crate::common::time_split_value_persistent::TimeSplitValuePersistent;
use crate::component::class::{ComponentClass, ComponentClassIdentifier};
use crate::component::instance::{ComponentInstance, ComponentInstanceId};
use crate::component::link::MarkerLink;
use crate::component::marker_pin::{MarkerPin, MarkerPinId, MarkerTime};
use crate::component::parameter::value::{DynEditableLerpEasingValue, EasingValue, LinearEasing};
use crate::component::parameter::{AudioRequiredParams, ImageRequiredParams, ImageRequiredParamsTransform, ParameterType, ParameterValueRaw, ParameterValueType, VariableParameterValue};
use crate::component::processor::{ComponentProcessor, ComponentProcessorComponent, ComponentProcessorWrapper, ComponentsLinksPair, ImageSize};
use crate::core::IdGenerator;
use crate::ptr::{StaticPointer, StaticPointerCow, StaticPointerOwned};
use crate::time::TimelineTime;
use arc_swap::ArcSwap;
use async_trait::async_trait;
use cgmath::{One, Quaternion, Vector3};
use rpds::{HashTrieMap, HashTrieMapSync, HashTrieSetSync, Vector, VectorSync};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::iter;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{Mutex, MutexGuard, RwLock};
use uuid::Uuid;

#[derive(Debug)]
pub struct Project<T: ParameterValueType> {
    id: Uuid,
    children: Vec<RootComponentClassHandleOwned<T>>,
}

pub type ProjectWithLock<T> = RwLock<Project<T>>;
pub type ProjectHandle<T> = StaticPointer<ProjectWithLock<T>>;
pub type ProjectHandleOwned<T> = StaticPointerOwned<ProjectWithLock<T>>;
pub type ProjectHandleCow<T> = StaticPointerCow<ProjectWithLock<T>>;

impl<T: ParameterValueType> PartialEq for Project<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: ParameterValueType> Eq for Project<T> {}

impl<T: ParameterValueType> Hash for Project<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

impl<T: ParameterValueType> Project<T> {
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn new_empty(id: Uuid) -> ProjectHandleOwned<T> {
        StaticPointerOwned::new(RwLock::new(Project { id, children: Vec::new() }))
    }

    pub fn with_children(id: Uuid, children: impl IntoIterator<Item = RootComponentClassHandleOwned<T>>) -> ProjectHandleOwned<T> {
        StaticPointerOwned::new(RwLock::new(Project { id, children: children.into_iter().collect() }))
    }

    pub fn children(&self) -> &Vec<RootComponentClassHandleOwned<T>> {
        &self.children
    }

    pub async fn add_child(&mut self, this: &ProjectHandle<T>, child: RootComponentClassHandleOwned<T>) {
        let mut child_guard = child.write().await;
        child_guard.parent = this.clone();
        child_guard.parent_id = self.id;
        drop(child_guard);
        self.children.push(child);
    }

    pub async fn add_children(&mut self, this: &ProjectHandle<T>, children: impl IntoIterator<Item = RootComponentClassHandleOwned<T>>) {
        for child in children {
            let mut child_guard = child.write().await;
            child_guard.parent = this.clone();
            child_guard.parent_id = self.id;
            drop(child_guard);
            self.children.push(child);
        }
    }

    pub fn remove_child(&mut self, child: &RootComponentClassHandle<T>) -> Option<RootComponentClassHandleOwned<T>> {
        let index = self.children.iter().position(|c| c == child)?;
        Some(self.children.remove(index))
    }
}

pub struct RootComponentClassItem<T: ParameterValueType> {
    left: MarkerPin,
    right: MarkerPin,
    components: HashTrieMapSync<ComponentInstanceId, Arc<ComponentInstance<T>>>,
    components_sorted: VectorSync<ComponentInstanceId>,
    interprocess_pins: VectorSync<MarkerPin>,
    links: HashTrieMapSync<[MarkerPinId; 2], MarkerLink>,
    links_sorted: VectorSync<[MarkerPinId; 2]>,
    link_map: HashTrieMapSync<MarkerPinId, HashTrieSetSync<MarkerPinId>>,
    pin_time_map: Arc<HashMap<MarkerPinId, TimelineTime>>,
    length: MarkerTime,
}

pub struct RootComponentClassItemViewBase<'a> {
    length: &'a mut MarkerTime,
}

pub struct RootComponentClassItemViewStructure<'a, T: ParameterValueType> {
    left: &'a mut MarkerPin,
    right: &'a mut MarkerPin,
    components: &'a mut HashTrieMapSync<ComponentInstanceId, Arc<ComponentInstance<T>>>,
    components_sorted: &'a mut VectorSync<ComponentInstanceId>,
    interprocess_pins: &'a mut VectorSync<MarkerPin>,
    links: &'a mut HashTrieMapSync<[MarkerPinId; 2], MarkerLink>,
    links_sorted: &'a mut VectorSync<[MarkerPinId; 2]>,
    link_map: &'a mut HashTrieMapSync<MarkerPinId, HashTrieSetSync<MarkerPinId>>,
}

pub struct RootComponentClassItemViewTimeMap<'a> {
    pin_time_map: &'a HashMap<MarkerPinId, TimelineTime>,
}

impl<T: ParameterValueType> Debug for RootComponentClassItem<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        struct DebugFn<F>(F);
        impl<F: for<'a> Fn(&mut Formatter<'a>) -> std::fmt::Result> Debug for DebugFn<F> {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                self.0(f)
            }
        }
        f.debug_struct("RootComponentClassItem")
            .field("left", &self.left)
            .field("right", &self.right)
            .field("components", &DebugFn(|f: &mut Formatter| f.debug_list().entries(self.components.keys()).finish()))
            .field("links", &DebugFn(|f: &mut Formatter| f.debug_list().entries(self.links.values()).finish()))
            .field("length", &self.length)
            .finish_non_exhaustive()
    }
}

impl<T: ParameterValueType> Clone for RootComponentClassItem<T> {
    fn clone(&self) -> Self {
        let RootComponentClassItem {
            left,
            right,
            components,
            components_sorted,
            interprocess_pins,
            links,
            links_sorted,
            link_map,
            pin_time_map,
            length,
        } = self;
        RootComponentClassItem {
            left: left.clone(),
            right: right.clone(),
            components: components.clone(),
            components_sorted: components_sorted.clone(),
            interprocess_pins: interprocess_pins.clone(),
            links: links.clone(),
            links_sorted: links_sorted.clone(),
            link_map: link_map.clone(),
            pin_time_map: pin_time_map.clone(),
            length: *length,
        }
    }
}

impl<T: ParameterValueType> ComponentsLinksPair<T> for RootComponentClassItem<T> {
    fn default_image_size(&self) -> ImageSize {
        ImageSize { width: 1920, height: 1080 }
    }

    fn frames_per_second(&self) -> u32 {
        60
    }

    fn components(&self) -> impl DoubleEndedIterator<Item = &Arc<ComponentInstance<T>>> + Send + Sync + '_
    where
        Self: Sized,
    {
        self.iter_components()
    }

    fn components_dyn(&self) -> Box<dyn DoubleEndedIterator<Item = &Arc<ComponentInstance<T>>> + Send + Sync + '_> {
        Box::new(self.iter_components())
    }

    fn links(&self) -> impl DoubleEndedIterator<Item = &MarkerLink> + Send + Sync + '_
    where
        Self: Sized,
    {
        self.iter_links()
    }

    fn links_dyn(&self) -> Box<dyn DoubleEndedIterator<Item = &MarkerLink> + Send + Sync + '_> {
        Box::new(self.iter_links())
    }

    fn left(&self) -> &MarkerPin {
        &self.left
    }

    fn right(&self) -> &MarkerPin {
        &self.right
    }
}

#[derive(Debug, Error)]
#[error("Not found")]
pub struct NotFound;

impl<T: ParameterValueType> RootComponentClassItem<T> {
    pub fn view(&mut self) -> (RootComponentClassItemViewBase, RootComponentClassItemViewStructure<T>, RootComponentClassItemViewTimeMap) {
        let RootComponentClassItem {
            left,
            right,
            components,
            components_sorted,
            interprocess_pins,
            links,
            links_sorted,
            link_map,
            pin_time_map,
            length,
        } = self;
        (
            RootComponentClassItemViewBase { length },
            RootComponentClassItemViewStructure {
                left,
                right,
                components,
                components_sorted,
                interprocess_pins,
                links,
                links_sorted,
                link_map,
            },
            RootComponentClassItemViewTimeMap { pin_time_map },
        )
    }
    pub fn left(&self) -> &MarkerPin {
        &self.left
    }
    pub fn left_mut(&mut self) -> &mut MarkerPin {
        &mut self.left
    }
    pub fn right(&self) -> &MarkerPin {
        &self.right
    }
    pub fn right_mut(&mut self) -> &mut MarkerPin {
        &mut self.right
    }
    pub fn iter_components(&self) -> impl DoubleEndedIterator<Item = &Arc<ComponentInstance<T>>> {
        self.components_sorted.iter().filter_map(|id| self.components.get(id))
    }
    pub fn component(&self, id: &ComponentInstanceId) -> Option<&Arc<ComponentInstance<T>>> {
        self.components.get(id)
    }
    pub fn component_mut(&mut self, id: &ComponentInstanceId) -> Option<&mut Arc<ComponentInstance<T>>> {
        self.components.get_mut(id)
    }
    pub fn add_component(&mut self, component: ComponentInstance<T>) {
        self.view().1.add_component(component);
    }
    pub fn insert_component_within(&mut self, component: &ComponentInstanceId, index: usize) -> Result<(), NotFound> {
        self.view().1.insert_component_within(component, index)
    }
    pub fn remove_component(&mut self, id: &ComponentInstanceId) -> Result<(), NotFound> {
        self.view().1.remove_component(id)
    }
    pub fn interprocess_pins(&self) -> &VectorSync<MarkerPin> {
        &self.interprocess_pins
    }
    pub fn interprocess_pins_mut(&mut self) -> &mut VectorSync<MarkerPin> {
        &mut self.interprocess_pins
    }
    pub fn iter_links(&self) -> impl DoubleEndedIterator<Item = &MarkerLink> {
        self.links_sorted.iter().filter_map(|key| self.links.get(key))
    }
    pub fn iter_link_connecting(&self, pin: MarkerPinId) -> impl Iterator<Item = &MarkerLink> + '_ {
        self.link_map.get(&pin).into_iter().flat_map(move |pins| pins.iter().filter_map(move |p| self.link(pin, *p)))
    }
    pub fn link(&self, pin1: MarkerPinId, pin2: MarkerPinId) -> Option<&MarkerLink> {
        let mut key = [pin1, pin2];
        key.sort_unstable();
        self.links.get(&key)
    }
    pub fn link_mut(&mut self, pin1: MarkerPinId, pin2: MarkerPinId) -> Option<&mut MarkerLink> {
        let mut key = [pin1, pin2];
        key.sort_unstable();
        self.links.get_mut(&key)
    }
    pub fn add_link(&mut self, link: MarkerLink) {
        self.view().1.add_link(link);
    }
    pub fn remove_link(&mut self, pin1: MarkerPinId, pin2: MarkerPinId) {
        self.view().1.remove_link(pin1, pin2);
    }
    pub fn time_of_pin(&self, pin: &MarkerPinId) -> Option<TimelineTime> {
        self.pin_time_map.get(pin).copied()
    }
    pub fn length(&self) -> MarkerTime {
        self.length
    }
    pub fn set_length(&mut self, length: MarkerTime) {
        self.length = length;
    }
}

impl<'a> RootComponentClassItemViewBase<'a> {
    pub fn length(&self) -> &MarkerTime {
        self.length
    }
    pub fn set_length(&mut self, length: MarkerTime) {
        *self.length = length;
    }
}

impl<'a, T> RootComponentClassItemViewStructure<'a, T>
where
    T: ParameterValueType,
{
    pub fn left(&self) -> &MarkerPin {
        self.left
    }
    pub fn left_mut(&mut self) -> &mut MarkerPin {
        self.left
    }
    pub fn right(&self) -> &MarkerPin {
        self.right
    }
    pub fn right_mut(&mut self) -> &mut MarkerPin {
        self.right
    }
    pub fn iter_components(&self) -> impl DoubleEndedIterator<Item = &Arc<ComponentInstance<T>>> {
        self.components_sorted.iter().filter_map(|id| self.components.get(id))
    }
    pub fn component(&self, id: &ComponentInstanceId) -> Option<&Arc<ComponentInstance<T>>> {
        self.components.get(id)
    }
    pub fn component_mut(&mut self, id: &ComponentInstanceId) -> Option<&mut Arc<ComponentInstance<T>>> {
        self.components.get_mut(id)
    }
    pub fn add_component(&mut self, component: ComponentInstance<T>) {
        self.components_sorted.push_back_mut(*component.id());
        self.components.insert_mut(*component.id(), Arc::new(component));
    }
    pub fn insert_component_within(&mut self, component: &ComponentInstanceId, index: usize) -> Result<(), NotFound> {
        if !self.components.contains_key(component) {
            return Err(NotFound);
        }
        let mut iter = self.components_sorted.iter().filter(|&c| self.components.contains_key(c) && c != component).cloned();
        let mut components_sorted = VectorSync::from_iter(iter.by_ref().take(index).chain(iter::once(*component)));
        components_sorted.extend(iter);
        *self.components_sorted = components_sorted;
        Ok(())
    }
    pub fn remove_component(&mut self, id: &ComponentInstanceId) -> Result<(), NotFound> {
        let component = self.components.get(id).ok_or(NotFound)?;
        [component.marker_left(), component.marker_right()].into_iter().chain(component.markers()).for_each(|pin| {
            let Some(other_pins) = self.link_map.get(pin.id()).cloned() else {
                return;
            };
            other_pins.iter().for_each(|p| {
                self.link_map.get_mut(p).unwrap().remove_mut(pin.id());
                let mut k = [*pin.id(), *p];
                k.sort_unstable();
                self.links.remove_mut(&k);
            });
            self.link_map.remove_mut(pin.id());
        });
        self.components.remove_mut(id);
        if self.components_sorted.len() * 2 < self.components.size() {
            let components_sorted = self.components_sorted.iter().copied().filter(|id| self.components.contains_key(id)).collect();
            *self.components_sorted = components_sorted;
        }
        if self.links_sorted.len() * 2 < self.links.size() {
            let links_sorted = self.links_sorted.iter().copied().filter(|key| self.links.contains_key(key)).collect();
            *self.links_sorted = links_sorted;
        }
        Ok(())
    }
    pub fn interprocess_pins(&self) -> &VectorSync<MarkerPin> {
        self.interprocess_pins
    }
    pub fn interprocess_pins_mut(&mut self) -> &mut VectorSync<MarkerPin> {
        self.interprocess_pins
    }
    pub fn iter_links(&self) -> impl DoubleEndedIterator<Item = &MarkerLink> {
        self.links_sorted.iter().filter_map(|key| self.links.get(key))
    }
    pub fn iter_link_connecting(&self, pin: MarkerPinId) -> impl Iterator<Item = &MarkerLink> + '_ {
        self.link_map.get(&pin).into_iter().flat_map(move |pins| pins.iter().filter_map(move |p| self.link(pin, *p)))
    }
    pub fn link(&self, pin1: MarkerPinId, pin2: MarkerPinId) -> Option<&MarkerLink> {
        let mut key = [pin1, pin2];
        key.sort_unstable();
        self.links.get(&key)
    }
    pub fn link_mut(&mut self, pin1: MarkerPinId, pin2: MarkerPinId) -> Option<&mut MarkerLink> {
        let mut key = [pin1, pin2];
        key.sort_unstable();
        self.links.get_mut(&key)
    }
    pub fn add_link(&mut self, link: MarkerLink) {
        let from_id = *link.from();
        let to_id = *link.to();
        let mut key = [from_id, to_id];
        key.sort_unstable();
        self.links_sorted.push_back_mut(key);
        self.links.insert_mut(key, link);
        if let Some(map) = self.link_map.get_mut(&from_id) {
            map.insert_mut(to_id);
        } else {
            self.link_map.insert_mut(from_id, HashTrieSetSync::from_iter(iter::once(to_id)));
        }
        if let Some(map) = self.link_map.get_mut(&to_id) {
            map.insert_mut(from_id);
        } else {
            self.link_map.insert_mut(to_id, HashTrieSetSync::from_iter(iter::once(from_id)));
        }
    }
    pub fn remove_link(&mut self, pin1: MarkerPinId, pin2: MarkerPinId) {
        let mut key = [pin1, pin2];
        key.sort_unstable();
        self.links.remove_mut(&key);
        self.link_map.get_mut(&pin1).unwrap().remove_mut(&pin2);
        self.link_map.get_mut(&pin2).unwrap().remove_mut(&pin1);
        if self.links_sorted.len() * 2 < self.links.size() {
            let links_sorted = self.links_sorted.iter().copied().filter(|key| self.links.contains_key(key)).collect();
            *self.links_sorted = links_sorted;
        }
    }
}

impl<'a> RootComponentClassItemViewTimeMap<'a> {
    pub fn time_of_pin(&self, pin: &MarkerPinId) -> Option<TimelineTime> {
        self.pin_time_map.get(pin).copied()
    }
}

pub trait TimelineTimeOfPin {
    fn time_of_pin(&self, pin: &MarkerPinId) -> Option<TimelineTime>;
}

impl<T: ParameterValueType> TimelineTimeOfPin for RootComponentClassItem<T> {
    fn time_of_pin(&self, pin: &MarkerPinId) -> Option<TimelineTime> {
        self.pin_time_map.get(pin).copied()
    }
}

impl<'a> TimelineTimeOfPin for RootComponentClassItemViewTimeMap<'a> {
    fn time_of_pin(&self, pin: &MarkerPinId) -> Option<TimelineTime> {
        self.pin_time_map.get(pin).copied()
    }
}

impl TimelineTimeOfPin for HashMap<MarkerPinId, TimelineTime> {
    fn time_of_pin(&self, pin: &MarkerPinId) -> Option<TimelineTime> {
        self.get(pin).copied()
    }
}

#[derive(Debug)]
struct RootComponentClassItemWrapper<T: ParameterValueType>(Arc<ArcSwap<RootComponentClassItem<T>>>);

impl<T: ParameterValueType> Clone for RootComponentClassItemWrapper<T> {
    fn clone(&self) -> Self {
        RootComponentClassItemWrapper(Arc::clone(&self.0))
    }
}

#[derive(Debug)]
pub struct RootComponentClass<T: ParameterValueType> {
    id: Uuid,
    parent: ProjectHandle<T>,
    parent_id: Uuid,
    item_write_lock: Mutex<()>,
    item: RootComponentClassItemWrapper<T>,
}

pub struct RootComponentClassItemWrite<'a, T: ParameterValueType> {
    _guard: MutexGuard<'a, ()>,
    slot: &'a RootComponentClassItemWrapper<T>,
    item: Arc<RootComponentClassItem<T>>,
}

impl<'a, T> RootComponentClassItemWrite<'a, T>
where
    T: ParameterValueType,
{
    pub fn commit_changes(mut this: Self, time_map: impl Into<Arc<HashMap<MarkerPinId, TimelineTime>>>) {
        Arc::make_mut(&mut this.item).pin_time_map = time_map.into();
        this.slot.0.store(this.item);
    }
}

impl<'a, T: ParameterValueType> Deref for RootComponentClassItemWrite<'a, T> {
    type Target = RootComponentClassItem<T>;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl<'a, T: ParameterValueType> DerefMut for RootComponentClassItemWrite<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.item)
    }
}

pub type RootComponentClassWithLock<T> = RwLock<RootComponentClass<T>>;
pub type RootComponentClassHandle<T> = StaticPointer<RootComponentClassWithLock<T>>;
pub type RootComponentClassHandleOwned<T> = StaticPointerOwned<RootComponentClassWithLock<T>>;
pub type RootComponentClassHandleCow<T> = StaticPointerCow<RootComponentClassWithLock<T>>;

#[async_trait]
impl<T: ParameterValueType + 'static> ComponentClass<T> for RootComponentClass<T> {
    fn human_readable_identifier(&self) -> &str {
        "RootComponentClass"
    }

    fn identifier(&self) -> ComponentClassIdentifier {
        ComponentClassIdentifier {
            namespace: Cow::Borrowed("mpdelta"),
            name: Cow::Borrowed("RootComponentClass"),
            inner_identifier: [self.parent_id, self.id],
        }
    }

    fn processor(&self) -> ComponentProcessorWrapper<T> {
        ComponentProcessorWrapper::Component(Arc::new(self.item.clone()))
    }

    async fn instantiate(&self, this: &StaticPointer<RwLock<dyn ComponentClass<T>>>, id: &dyn IdGenerator) -> ComponentInstance<T> {
        let guard = self.item.0.load();
        let marker_left = MarkerPin::new(id.generate_new(), MarkerTime::ZERO);
        let marker_right = MarkerPin::new(id.generate_new(), guard.length);
        let one = TimeSplitValuePersistent::new(*marker_left.id(), Some(EasingValue::new(DynEditableLerpEasingValue((1., 1.)), Arc::new(LinearEasing))), *marker_right.id());
        let one_value = VariableParameterValue::new(one);
        let one_vector3 = Arc::new(Vector3 {
            x: one_value.clone(),
            y: one_value.clone(),
            z: one_value.clone(),
        });
        let zero = VariableParameterValue::new(TimeSplitValuePersistent::new(*marker_left.id(), Some(EasingValue::new(DynEditableLerpEasingValue((0., 0.)), Arc::new(LinearEasing))), *marker_right.id()));
        let zero_vector3 = Arc::new(Vector3 { x: zero.clone(), y: zero.clone(), z: zero });
        let image_required_params = ImageRequiredParams {
            transform: Arc::new(ImageRequiredParamsTransform::Params {
                size: one_vector3.clone(),
                scale: one_vector3,
                translate: zero_vector3.clone(),
                rotate: Arc::new(TimeSplitValuePersistent::new(*marker_left.id(), EasingValue::new(DynEditableLerpEasingValue((Quaternion::one(), Quaternion::one())), Arc::new(LinearEasing)), *marker_right.id())),
                scale_center: zero_vector3.clone(),
                rotate_center: zero_vector3,
            }),
            background_color: [0; 4],
            opacity: TimeSplitValuePersistent::new(*marker_left.id(), EasingValue::new(DynEditableLerpEasingValue((1., 1.)), Arc::new(LinearEasing)), *marker_right.id()),
            blend_mode: TimeSplitValuePersistent::new(*marker_left.id(), Default::default(), *marker_right.id()),
            composite_operation: TimeSplitValuePersistent::new(*marker_left.id(), Default::default(), *marker_right.id()),
        };
        let audio_required_params = AudioRequiredParams {
            volume: Vector::from_iter([one_value.clone(), one_value]),
        };
        let processor = Arc::new(self.item.clone()) as Arc<dyn ComponentProcessorComponent<T>>;
        ComponentInstance::builder(this.clone(), marker_left, marker_right, Vec::new(), processor)
            .image_required_params(image_required_params)
            .audio_required_params(audio_required_params)
            .build(id)
    }
}

#[async_trait]
impl<T: ParameterValueType> ComponentProcessor<T> for RootComponentClassItemWrapper<T> {
    async fn fixed_parameter_types(&self) -> &[(String, ParameterType)] {
        &[]
    }

    async fn update_variable_parameter(&self, _: &[ParameterValueRaw<T::Image, T::Audio>], variable_parameters: &mut Vec<(String, ParameterType)>) {
        variable_parameters.clear();
    }

    async fn num_interprocess_pins(&self, _: &[ParameterValueRaw<T::Image, T::Audio>]) -> usize {
        self.0.load().interprocess_pins.len()
    }
}

#[async_trait]
impl<T: ParameterValueType> ComponentProcessorComponent<T> for RootComponentClassItemWrapper<T> {
    async fn natural_length(&self, _: &[ParameterValueRaw<T::Image, T::Audio>], _: &[MarkerPinId]) -> MarkerTime {
        let guard = self.0.load();
        guard.length
    }

    async fn process(
        &self,
        _fixed_parameters: &[ParameterValueRaw<T::Image, T::Audio>],
        _fixed_parameters_component: &[StaticPointer<RwLock<dyn ComponentClass<T>>>],
        interprocess_pins: &[MarkerPinId],
        _variable_parameters: &[StaticPointer<RwLock<dyn ComponentClass<T>>>],
        _variable_parameter_type: &[(String, ParameterType)],
    ) -> Arc<dyn ComponentsLinksPair<T>> {
        let mut items = self.0.load_full();
        if items.interprocess_pins.is_empty() {
            return items;
        }
        let items_ref = Arc::make_mut(&mut items);
        let items_interprocess_pins = items_ref.interprocess_pins.clone();
        for (p1, p2) in items_interprocess_pins.iter().zip(interprocess_pins) {
            items_ref.add_link(MarkerLink::new(*p1.id(), *p2, TimelineTime::ZERO));
        }
        items
    }
}

impl<T: ParameterValueType> PartialEq for RootComponentClass<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: ParameterValueType> Eq for RootComponentClass<T> {}

impl<T: ParameterValueType> Hash for RootComponentClass<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

impl<T: ParameterValueType> RootComponentClass<T> {
    pub fn new_empty(id: Uuid, parent: ProjectHandle<T>, parent_id: Uuid, id_generator: &(impl IdGenerator + ?Sized)) -> RootComponentClassHandleOwned<T> {
        let left = MarkerPin::new(id_generator.generate_new(), MarkerTime::new(MixedFraction::ZERO).unwrap());
        let right = MarkerPin::new(id_generator.generate_new(), MarkerTime::new(MixedFraction::from_integer(10)).unwrap());
        let pin_time_map = Arc::new(HashMap::from([(*left.id(), TimelineTime::ZERO), (*right.id(), TimelineTime::new(MixedFraction::from_integer(10)))]));
        StaticPointerOwned::new(RwLock::new(RootComponentClass {
            id,
            parent,
            parent_id,
            item_write_lock: Mutex::new(()),
            item: RootComponentClassItemWrapper(Arc::new(ArcSwap::from_pointee(RootComponentClassItem {
                left,
                right,
                components: HashTrieMap::new_sync(),
                components_sorted: Vector::new_sync(),
                interprocess_pins: Vector::new_sync(),
                links: HashTrieMap::new_sync(),
                links_sorted: Vector::new_sync(),
                link_map: HashTrieMap::new_sync(),
                pin_time_map,
                length: MarkerTime::new(MixedFraction::from_integer(10)).unwrap(),
            }))),
        }))
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn get(&self) -> impl Deref<Target = Arc<RootComponentClassItem<T>>> + '_ {
        self.item.0.load()
    }

    pub async fn get_mut(&self) -> RootComponentClassItemWrite<T> {
        let _guard = self.item_write_lock.lock().await;
        let item = self.item.0.load_full();
        RootComponentClassItemWrite { _guard, slot: &self.item, item }
    }

    pub fn left(&self) -> MarkerPin {
        self.item.0.load().left.clone()
    }

    pub fn right(&self) -> MarkerPin {
        self.item.0.load().right.clone()
    }

    pub fn parent(&self) -> &ProjectHandle<T> {
        &self.parent
    }
}
