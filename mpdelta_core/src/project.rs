use crate::common::mixed_fraction::MixedFraction;
use crate::common::time_split_value::TimeSplitValue;
use crate::component::class::{ComponentClass, ComponentClassIdentifier};
use crate::component::instance::{ComponentInstance, ComponentInstanceHandle, ComponentInstanceHandleOwned};
use crate::component::link::{MarkerLinkHandle, MarkerLinkHandleOwned};
use crate::component::marker_pin::{MarkerPin, MarkerPinHandle, MarkerPinHandleOwned, MarkerTime};
use crate::component::parameter::value::{DynEditableSelfEasingValue, EasingValue, LinearEasing};
use crate::component::parameter::{AudioRequiredParams, ImageRequiredParams, ImageRequiredParamsTransform, ParameterType, ParameterValueRaw, ParameterValueType, VariableParameterValue};
use crate::component::processor::{ComponentProcessor, ComponentProcessorComponent, ComponentProcessorWrapper, ComponentsLinksPair};
use crate::ptr::{StaticPointer, StaticPointerCow, StaticPointerOwned};
use crate::time::TimelineTime;
use async_trait::async_trait;
use cgmath::{One, Quaternion, Vector3};
use qcell::TCell;
use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::mem;
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
use uuid::Uuid;

#[derive(Debug)]
pub struct Project<K: 'static, T: ParameterValueType> {
    id: Uuid,
    children: HashSet<RootComponentClassHandle<K, T>>,
}

pub type ProjectHandle<K, T> = StaticPointer<RwLock<Project<K, T>>>;
pub type ProjectHandleOwned<K, T> = StaticPointerOwned<RwLock<Project<K, T>>>;
pub type ProjectHandleCow<K, T> = StaticPointerCow<RwLock<Project<K, T>>>;

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
        StaticPointerOwned::new(RwLock::new(Project { id, children: HashSet::new() }))
    }

    pub fn with_children(id: Uuid, children: impl IntoIterator<Item = RootComponentClassHandle<K, T>>) -> ProjectHandleOwned<K, T> {
        StaticPointerOwned::new(RwLock::new(Project { id, children: children.into_iter().collect() }))
    }

    pub fn children(&self) -> &HashSet<RootComponentClassHandle<K, T>> {
        &self.children
    }

    pub fn children_mut(&mut self) -> &mut HashSet<RootComponentClassHandle<K, T>> {
        &mut self.children
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
    parent: Option<(ProjectHandle<K, T>, Uuid)>,
    item: RootComponentClassItemWrapper<K, T>,
}

pub type RootComponentClassHandle<K, T> = StaticPointer<RwLock<RootComponentClass<K, T>>>;
pub type RootComponentClassHandleOwned<K, T> = StaticPointerOwned<RwLock<RootComponentClass<K, T>>>;
pub type RootComponentClassHandleCow<K, T> = StaticPointerCow<RwLock<RootComponentClass<K, T>>>;

#[async_trait]
impl<K, T: ParameterValueType + 'static> ComponentClass<K, T> for RootComponentClass<K, T> {
    fn identifier(&self) -> ComponentClassIdentifier {
        ComponentClassIdentifier {
            namespace: Cow::Borrowed("mpdelta"),
            name: Cow::Borrowed("RootComponentClass"),
            inner_identifier: [self.parent.as_ref().map(|&(_, id)| id).unwrap_or_else(Uuid::nil), self.id],
        }
    }

    fn processor(&self) -> ComponentProcessorWrapper<K, T> {
        ComponentProcessorWrapper::Component(Arc::new(self.item.clone()))
    }

    async fn instantiate(&self, this: &StaticPointer<RwLock<dyn ComponentClass<K, T>>>) -> ComponentInstance<K, T> {
        let guard = self.item.0.read().await;
        let marker_left = StaticPointerOwned::reference(&guard.left).clone();
        let marker_right = StaticPointerOwned::reference(&guard.right).clone();
        let one = TimeSplitValue::new(marker_left.clone(), Some(EasingValue::new(DynEditableSelfEasingValue(1., 1.), Arc::new(LinearEasing))), marker_right.clone());
        let one_value = VariableParameterValue::new(one);
        let zero = VariableParameterValue::new(TimeSplitValue::new(marker_left.clone(), Some(EasingValue::new(DynEditableSelfEasingValue(0., 0.), Arc::new(LinearEasing))), marker_right.clone()));
        let image_required_params = ImageRequiredParams {
            aspect_ratio: (16, 9),
            transform: ImageRequiredParamsTransform::Params {
                scale: Vector3 {
                    x: one_value.clone(),
                    y: one_value.clone(),
                    z: one_value.clone(),
                },
                translate: Vector3 { x: zero.clone(), y: zero.clone(), z: zero.clone() },
                rotate: TimeSplitValue::new(marker_left.clone(), EasingValue::new(DynEditableSelfEasingValue(Quaternion::one(), Quaternion::one()), Arc::new(LinearEasing)), marker_right.clone()),
                scale_center: Vector3 { x: zero.clone(), y: zero.clone(), z: zero.clone() },
                rotate_center: Vector3 { x: zero.clone(), y: zero.clone(), z: zero },
            },
            background_color: [0; 4],
            opacity: TimeSplitValue::new(marker_left.clone(), EasingValue::new(DynEditableSelfEasingValue(1., 1.), Arc::new(LinearEasing)), marker_right.clone()),
            blend_mode: TimeSplitValue::new(marker_left.clone(), Default::default(), marker_right.clone()),
            composite_operation: TimeSplitValue::new(marker_left.clone(), Default::default(), marker_right.clone()),
        };
        let audio_required_params = AudioRequiredParams { volume: vec![one_value.clone(), one_value] };
        let processor = Arc::new(self.item.clone()) as Arc<dyn ComponentProcessorComponent<K, T>>;
        ComponentInstance::new(
            this.clone(),
            StaticPointerCow::Reference(marker_left),
            StaticPointerCow::Reference(marker_right),
            Vec::new(),
            Some(image_required_params),
            Some(audio_required_params),
            Box::new([]),
            Box::new([]),
            processor,
        )
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

    async fn natural_length(&self, _: &[ParameterValueRaw<T::Image, T::Audio>]) -> MarkerTime {
        let guard = self.0.read().await;
        guard.length
    }
}

#[async_trait]
impl<K, T: ParameterValueType> ComponentProcessorComponent<K, T> for RootComponentClassItemWrapper<K, T> {
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
    pub fn new_empty(id: Uuid) -> RootComponentClassHandleOwned<K, T> {
        StaticPointerOwned::new(RwLock::new(RootComponentClass {
            id,
            parent: None,
            item: RootComponentClassItemWrapper(Arc::new(RwLock::new(RootComponentClassItem {
                left: StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(MixedFraction::ZERO), MarkerTime::new(MixedFraction::ZERO).unwrap()))),
                right: StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(MixedFraction::from_integer(10)), MarkerTime::new(MixedFraction::from_integer(10)).unwrap()))),
                component: Vec::new(),
                link: Vec::new(),
                length: MarkerTime::new(MixedFraction::from_integer(10)).unwrap(),
            }))),
        }))
    }

    pub fn with_item(id: Uuid, component: Vec<ComponentInstanceHandleOwned<K, T>>, link: Vec<MarkerLinkHandleOwned<K>>) -> RootComponentClassHandleOwned<K, T> {
        StaticPointerOwned::new(RwLock::new(RootComponentClass {
            id,
            parent: None,
            item: RootComponentClassItemWrapper(Arc::new(RwLock::new(RootComponentClassItem {
                left: StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(MixedFraction::ZERO), MarkerTime::new(MixedFraction::ZERO).unwrap()))),
                right: StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(MixedFraction::from_integer(10)), MarkerTime::new(MixedFraction::from_integer(10)).unwrap()))),
                component,
                link,
                length: MarkerTime::new(MixedFraction::from_integer(10)).unwrap(),
            }))),
        }))
    }

    pub(crate) async fn set_parent(this: &RootComponentClassHandle<K, T>, parent: Option<ProjectHandle<K, T>>) -> Option<ProjectHandle<K, T>> {
        let this_strong_ref = this.upgrade()?;
        let mut this_guard = this_strong_ref.write().await;
        let parent_id = if let Some(parent) = &parent {
            let parent = parent.upgrade()?;
            let mut parent = parent.write().await;
            parent.children.insert(this.clone());
            parent.id
        } else {
            Uuid::nil()
        };
        let old_parent = mem::replace(&mut this_guard.parent, parent.map(|parent| (parent, parent_id)));
        let old_parent = old_parent.map(|(parent, _)| parent);
        if let Some(old_parent) = &old_parent.as_ref().and_then(StaticPointer::upgrade) {
            old_parent.write().await.children.remove(this);
        }
        old_parent
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct EmptyParameterValueType;

    impl ParameterValueType for EmptyParameterValueType {
        type Image = ();
        type Audio = ();
        type Binary = ();
        type String = ();
        type Integer = ();
        type RealNumber = ();
        type Boolean = ();
        type Dictionary = ();
        type Array = ();
        type ComponentClass = ();
    }

    #[tokio::test]
    async fn set_parent() {
        let project0 = Project::<(), EmptyParameterValueType>::new_empty(Uuid::from_u128(0));
        let project1 = Project::<(), EmptyParameterValueType>::new_empty(Uuid::from_u128(1));
        assert!(project0.read().await.children.is_empty());
        assert!(project1.read().await.children.is_empty());
        let component0 = RootComponentClass::<(), EmptyParameterValueType>::new_empty(Uuid::from_u128(0));
        let component1 = RootComponentClass::<(), EmptyParameterValueType>::new_empty(Uuid::from_u128(1));
        assert!(component0.read().await.parent.is_none());
        assert!(component1.read().await.parent.is_none());

        assert!(RootComponentClass::set_parent(StaticPointerOwned::reference(&component0), Some(StaticPointerOwned::reference(&project0).clone())).await.is_none());
        {
            let project0 = project0.read().await;
            assert_eq!(project0.children.len(), 1);
            assert_eq!(project0.children.iter().collect::<Vec<_>>(), vec![StaticPointerOwned::reference(&component0)]);
        }
        assert_eq!(component0.read().await.parent, Some((StaticPointerOwned::reference(&project0).clone(), Uuid::from_u128(0))));
        assert!(RootComponentClass::set_parent(StaticPointerOwned::reference(&component1), Some(StaticPointerOwned::reference(&project1).clone())).await.is_none());
        {
            let project1 = project1.read().await;
            assert_eq!(project1.children.len(), 1);
            assert_eq!(project1.children.iter().collect::<Vec<_>>(), vec![StaticPointerOwned::reference(&component1)]);
        }
        assert_eq!(component1.read().await.parent, Some((StaticPointerOwned::reference(&project1).clone(), Uuid::from_u128(1))));

        assert_eq!(
            RootComponentClass::set_parent(StaticPointerOwned::reference(&component0), Some(StaticPointerOwned::reference(&project1).clone())).await,
            Some(StaticPointerOwned::reference(&project0).clone())
        );
        {
            let project0 = project0.read().await;
            assert!(project0.children.is_empty());
            let project1 = project1.read().await;
            assert_eq!(project1.children.len(), 2);
            let children = project1.children.iter().collect::<Vec<_>>();
            assert!(children == vec![StaticPointerOwned::reference(&component0), StaticPointerOwned::reference(&component1)] || children == vec![StaticPointerOwned::reference(&component1), StaticPointerOwned::reference(&component0)]);
        }
        assert_eq!(RootComponentClass::set_parent(StaticPointerOwned::reference(&component1), None).await, Some(StaticPointerOwned::reference(&project1).clone()));
        {
            let project1 = project1.read().await;
            assert_eq!(project1.children.len(), 1);
            assert_eq!(project1.children.iter().collect::<Vec<_>>(), vec![StaticPointerOwned::reference(&component0)]);
        }
        assert!(component1.read().await.parent.is_none());
    }
}
