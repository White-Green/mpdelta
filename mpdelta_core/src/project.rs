use crate::common::mixed_fraction::MixedFraction;
use crate::common::time_split_value::TimeSplitValue;
use crate::component::class::{ComponentClass, ComponentClassIdentifier};
use crate::component::instance::{ComponentInstance, ComponentInstanceHandle, ComponentInstanceHandleOwned};
use crate::component::link::{MarkerLinkHandle, MarkerLinkHandleOwned};
use crate::component::marker_pin::{MarkerPin, MarkerPinHandle, MarkerPinHandleOwned, MarkerTime};
use crate::component::parameter::value::{DynEditableLerpEasingValue, EasingValue, LinearEasing};
use crate::component::parameter::{AudioRequiredParams, ImageRequiredParams, ImageRequiredParamsTransform, ParameterType, ParameterValueRaw, ParameterValueType, VariableParameterValue};
use crate::component::processor::{ComponentProcessor, ComponentProcessorComponent, ComponentProcessorWrapper, ComponentsLinksPair};
use crate::ptr::{StaticPointer, StaticPointerCow, StaticPointerOwned};
use crate::time::TimelineTime;
use async_trait::async_trait;
use cgmath::{One, Quaternion, Vector3};
use qcell::TCell;
use std::borrow::Cow;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
use uuid::Uuid;

#[derive(Debug)]
pub struct Project<K: 'static, T: ParameterValueType> {
    id: Uuid,
    children: Vec<RootComponentClassHandleOwned<K, T>>,
}

pub type ProjectWithLock<K, T> = RwLock<Project<K, T>>;
pub type ProjectHandle<K, T> = StaticPointer<ProjectWithLock<K, T>>;
pub type ProjectHandleOwned<K, T> = StaticPointerOwned<ProjectWithLock<K, T>>;
pub type ProjectHandleCow<K, T> = StaticPointerCow<ProjectWithLock<K, T>>;

impl<K, T: ParameterValueType> PartialEq for Project<K, T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<K, T: ParameterValueType> Eq for Project<K, T> {}

impl<K, T: ParameterValueType> Hash for Project<K, T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

impl<K, T: ParameterValueType> Project<K, T> {
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn new_empty(id: Uuid) -> ProjectHandleOwned<K, T> {
        StaticPointerOwned::new(RwLock::new(Project { id, children: Vec::new() }))
    }

    pub fn with_children(id: Uuid, children: impl IntoIterator<Item = RootComponentClassHandleOwned<K, T>>) -> ProjectHandleOwned<K, T> {
        StaticPointerOwned::new(RwLock::new(Project { id, children: children.into_iter().collect() }))
    }

    pub fn children(&self) -> &Vec<RootComponentClassHandleOwned<K, T>> {
        &self.children
    }

    pub async fn add_child(&mut self, this: &ProjectHandle<K, T>, child: RootComponentClassHandleOwned<K, T>) {
        let mut child_guard = child.write().await;
        child_guard.parent = this.clone();
        child_guard.parent_id = self.id;
        drop(child_guard);
        self.children.push(child);
    }

    pub async fn add_children(&mut self, this: &ProjectHandle<K, T>, children: impl IntoIterator<Item = RootComponentClassHandleOwned<K, T>>) {
        for child in children {
            let mut child_guard = child.write().await;
            child_guard.parent = this.clone();
            child_guard.parent_id = self.id;
            drop(child_guard);
            self.children.push(child);
        }
    }

    pub fn remove_child(&mut self, child: &RootComponentClassHandle<K, T>) -> Option<RootComponentClassHandleOwned<K, T>> {
        let index = self.children.iter().position(|c| c == child)?;
        Some(self.children.remove(index))
    }
}

pub struct RootComponentClassItem<K: 'static, T: ParameterValueType> {
    left: MarkerPinHandleOwned<K>,
    right: MarkerPinHandleOwned<K>,
    component: Vec<ComponentInstanceHandleOwned<K, T>>,
    link: Vec<MarkerLinkHandleOwned<K>>,
    length: MarkerTime,
}

impl<K, T: ParameterValueType> Debug for RootComponentClassItem<K, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        struct DebugFn<F>(F);
        impl<F: for<'a> Fn(&mut Formatter<'a>) -> std::fmt::Result> Debug for DebugFn<F> {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                self.0(f)
            }
        }
        f.debug_struct("RootComponentClassItem")
            .field("left", StaticPointerOwned::reference(&self.left))
            .field("right", StaticPointerOwned::reference(&self.right))
            .field("component", &DebugFn(|f: &mut Formatter| f.debug_list().entries(self.component.iter().map(StaticPointerOwned::reference)).finish()))
            .field("link", &DebugFn(|f: &mut Formatter| f.debug_list().entries(self.link.iter().map(StaticPointerOwned::reference)).finish()))
            .finish_non_exhaustive()
    }
}

impl<K, T: ParameterValueType> RootComponentClassItem<K, T> {
    pub fn left(&self) -> &MarkerPinHandleOwned<K> {
        &self.left
    }
    pub fn right(&self) -> &MarkerPinHandleOwned<K> {
        &self.right
    }
    pub fn component(&self) -> &[ComponentInstanceHandleOwned<K, T>] {
        &self.component
    }
    pub fn component_mut(&mut self) -> &mut Vec<ComponentInstanceHandleOwned<K, T>> {
        &mut self.component
    }
    pub fn link(&self) -> &[MarkerLinkHandleOwned<K>] {
        &self.link
    }
    pub fn link_mut(&mut self) -> &mut Vec<MarkerLinkHandleOwned<K>> {
        &mut self.link
    }
    pub fn length(&self) -> MarkerTime {
        self.length
    }
    pub fn set_length(&mut self, length: MarkerTime) {
        self.length = length;
    }
}

#[derive(Debug)]
struct RootComponentClassItemWrapper<K: 'static, T: ParameterValueType>(Arc<RwLock<RootComponentClassItem<K, T>>>);

impl<K: 'static, T: ParameterValueType> Clone for RootComponentClassItemWrapper<K, T> {
    fn clone(&self) -> Self {
        RootComponentClassItemWrapper(Arc::clone(&self.0))
    }
}

#[derive(Debug)]
pub struct RootComponentClass<K: 'static, T: ParameterValueType> {
    id: Uuid,
    parent: ProjectHandle<K, T>,
    parent_id: Uuid,
    item: RootComponentClassItemWrapper<K, T>,
}

pub type RootComponentClassWithLock<K, T> = RwLock<RootComponentClass<K, T>>;
pub type RootComponentClassHandle<K, T> = StaticPointer<RootComponentClassWithLock<K, T>>;
pub type RootComponentClassHandleOwned<K, T> = StaticPointerOwned<RootComponentClassWithLock<K, T>>;
pub type RootComponentClassHandleCow<K, T> = StaticPointerCow<RootComponentClassWithLock<K, T>>;

#[async_trait]
impl<K, T: ParameterValueType + 'static> ComponentClass<K, T> for RootComponentClass<K, T> {
    fn identifier(&self) -> ComponentClassIdentifier {
        ComponentClassIdentifier {
            namespace: Cow::Borrowed("mpdelta"),
            name: Cow::Borrowed("RootComponentClass"),
            inner_identifier: [self.parent_id, self.id],
        }
    }

    fn processor(&self) -> ComponentProcessorWrapper<K, T> {
        ComponentProcessorWrapper::Component(Arc::new(self.item.clone()))
    }

    async fn instantiate(&self, this: &StaticPointer<RwLock<dyn ComponentClass<K, T>>>) -> ComponentInstance<K, T> {
        let guard = self.item.0.read().await;
        let marker_left = StaticPointerOwned::reference(&guard.left).clone();
        let marker_right = StaticPointerOwned::reference(&guard.right).clone();
        let one = TimeSplitValue::new(marker_left.clone(), Some(EasingValue::new(DynEditableLerpEasingValue((1., 1.)), Arc::new(LinearEasing))), marker_right.clone());
        let one_value = VariableParameterValue::new(one);
        let zero = VariableParameterValue::new(TimeSplitValue::new(marker_left.clone(), Some(EasingValue::new(DynEditableLerpEasingValue((0., 0.)), Arc::new(LinearEasing))), marker_right.clone()));
        let image_required_params = ImageRequiredParams {
            transform: ImageRequiredParamsTransform::Params {
                size: Vector3 {
                    x: one_value.clone(),
                    y: one_value.clone(),
                    z: one_value.clone(),
                },
                scale: Vector3 {
                    x: one_value.clone(),
                    y: one_value.clone(),
                    z: one_value.clone(),
                },
                translate: Vector3 { x: zero.clone(), y: zero.clone(), z: zero.clone() },
                rotate: TimeSplitValue::new(marker_left.clone(), EasingValue::new(DynEditableLerpEasingValue((Quaternion::one(), Quaternion::one())), Arc::new(LinearEasing)), marker_right.clone()),
                scale_center: Vector3 { x: zero.clone(), y: zero.clone(), z: zero.clone() },
                rotate_center: Vector3 { x: zero.clone(), y: zero.clone(), z: zero },
            },
            background_color: [0; 4],
            opacity: TimeSplitValue::new(marker_left.clone(), EasingValue::new(DynEditableLerpEasingValue((1., 1.)), Arc::new(LinearEasing)), marker_right.clone()),
            blend_mode: TimeSplitValue::new(marker_left.clone(), Default::default(), marker_right.clone()),
            composite_operation: TimeSplitValue::new(marker_left.clone(), Default::default(), marker_right.clone()),
        };
        let audio_required_params = AudioRequiredParams { volume: vec![one_value.clone(), one_value] };
        let processor = Arc::new(self.item.clone()) as Arc<dyn ComponentProcessorComponent<K, T>>;
        ComponentInstance::builder(this.clone(), StaticPointerCow::Reference(marker_left), StaticPointerCow::Reference(marker_right), Vec::new(), processor)
            .image_required_params(image_required_params)
            .audio_required_params(audio_required_params)
            .build()
    }
}

#[async_trait]
impl<K, T: ParameterValueType> ComponentProcessor<K, T> for RootComponentClassItemWrapper<K, T> {
    async fn fixed_parameter_types(&self) -> &[(String, ParameterType)] {
        &[]
    }

    async fn update_variable_parameter(&self, _: &[ParameterValueRaw<T::Image, T::Audio>], variable_parameters: &mut Vec<(String, ParameterType)>) {
        variable_parameters.clear();
    }
}

#[async_trait]
impl<K, T: ParameterValueType> ComponentProcessorComponent<K, T> for RootComponentClassItemWrapper<K, T> {
    async fn natural_length(&self, _: &[ParameterValueRaw<T::Image, T::Audio>]) -> MarkerTime {
        let guard = self.0.read().await;
        guard.length
    }

    async fn process(
        &self,
        _fixed_parameters: &[ParameterValueRaw<T::Image, T::Audio>],
        _fixed_parameters_component: &[StaticPointer<RwLock<dyn ComponentClass<K, T>>>],
        _variable_parameters: &[StaticPointer<RwLock<dyn ComponentClass<K, T>>>],
        _variable_parameter_type: &[(String, ParameterType)],
    ) -> ComponentsLinksPair<K, T> {
        let guard = self.0.read().await;
        let components = guard.component.iter().map(Into::into).collect::<Vec<_>>();
        let links = guard.link.iter().map(Into::into).collect::<Vec<_>>();
        ComponentsLinksPair(components, links)
    }
}

impl<K, T: ParameterValueType> PartialEq for RootComponentClass<K, T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<K, T: ParameterValueType> Eq for RootComponentClass<K, T> {}

impl<K, T: ParameterValueType> Hash for RootComponentClass<K, T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

impl<K, T: ParameterValueType> RootComponentClass<K, T> {
    pub fn new_empty(id: Uuid, parent: ProjectHandle<K, T>, parent_id: Uuid) -> RootComponentClassHandleOwned<K, T> {
        StaticPointerOwned::new(RwLock::new(RootComponentClass {
            id,
            parent,
            parent_id,
            item: RootComponentClassItemWrapper(Arc::new(RwLock::new(RootComponentClassItem {
                left: StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(MixedFraction::ZERO), MarkerTime::new(MixedFraction::ZERO).unwrap()))),
                right: StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(MixedFraction::from_integer(10)), MarkerTime::new(MixedFraction::from_integer(10)).unwrap()))),
                component: Vec::new(),
                link: Vec::new(),
                length: MarkerTime::new(MixedFraction::from_integer(10)).unwrap(),
            }))),
        }))
    }

    pub fn with_item(id: Uuid, parent: ProjectHandle<K, T>, parent_id: Uuid, component: Vec<ComponentInstanceHandleOwned<K, T>>, link: Vec<MarkerLinkHandleOwned<K>>) -> RootComponentClassHandleOwned<K, T> {
        StaticPointerOwned::new(RwLock::new(RootComponentClass {
            id,
            parent,
            parent_id,
            item: RootComponentClassItemWrapper(Arc::new(RwLock::new(RootComponentClassItem {
                left: StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(MixedFraction::ZERO), MarkerTime::new(MixedFraction::ZERO).unwrap()))),
                right: StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(MixedFraction::from_integer(10)), MarkerTime::new(MixedFraction::from_integer(10)).unwrap()))),
                component,
                link,
                length: MarkerTime::new(MixedFraction::from_integer(10)).unwrap(),
            }))),
        }))
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub async fn get(&self) -> RwLockReadGuard<'_, RootComponentClassItem<K, T>> {
        self.item.0.read().await
    }

    pub async fn get_owned(&self) -> OwnedRwLockReadGuard<RootComponentClassItem<K, T>> {
        Arc::clone(&self.item.0).read_owned().await
    }

    pub async fn get_mut(&self) -> RwLockWriteGuard<'_, RootComponentClassItem<K, T>> {
        self.item.0.write().await
    }

    pub async fn get_owned_mut(&self) -> OwnedRwLockWriteGuard<RootComponentClassItem<K, T>> {
        Arc::clone(&self.item.0).write_owned().await
    }

    pub async fn left(&self) -> impl Deref<Target = MarkerPinHandle<K>> + '_ {
        RwLockReadGuard::map(self.item.0.read().await, |guard| StaticPointerOwned::reference(&guard.left))
    }

    pub async fn right(&self) -> impl Deref<Target = MarkerPinHandle<K>> + '_ {
        RwLockReadGuard::map(self.item.0.read().await, |guard| StaticPointerOwned::reference(&guard.right))
    }

    pub async fn components(&self) -> impl Deref<Target = [impl AsRef<ComponentInstanceHandle<K, T>>]> + '_ {
        RwLockReadGuard::map(self.item.0.read().await, |guard| guard.component.as_ref())
    }

    pub async fn links(&self) -> impl Deref<Target = [impl AsRef<MarkerLinkHandle<K>>]> + '_ {
        RwLockReadGuard::map(self.item.0.read().await, |guard| guard.link.as_ref())
    }

    pub fn parent(&self) -> &ProjectHandle<K, T> {
        &self.parent
    }
}
