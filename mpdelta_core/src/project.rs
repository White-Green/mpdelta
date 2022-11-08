use crate::common::time_split_value::TimeSplitValue;
use crate::component::class::ComponentClass;
use crate::component::instance::ComponentInstance;
use crate::component::link::MarkerLink;
use crate::component::marker_pin::{MarkerPin, MarkerTime};
use crate::component::parameter::value::{DefaultEasing, EasingValue};
use crate::component::parameter::{AudioRequiredParams, ComponentProcessorInputValue, ImageRequiredParams, ImageRequiredParamsTransform, ParameterType, ParameterValueFixed, VariableParameterValue};
use crate::component::processor::{ComponentProcessor, ComponentProcessorBody, ProcessorComponentBuilder};
use crate::ptr::{StaticPointer, StaticPointerCow, StaticPointerOwned};
use crate::time::TimelineTime;
use async_trait::async_trait;
use cgmath::{One, Quaternion, Vector3};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::Duration;
use std::{iter, mem};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug)]
pub struct Project<T> {
    id: Uuid,
    children: HashSet<StaticPointer<RwLock<RootComponentClass<T>>>>,
}

impl<T> PartialEq for Project<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Project<T> {}

impl<T> Hash for Project<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

impl<T> Project<T> {
    pub(crate) fn new_empty(id: Uuid) -> StaticPointerOwned<RwLock<Project<T>>> {
        StaticPointerOwned::new(RwLock::new(Project { id, children: HashSet::new() }))
    }
}

#[derive(Debug)]
pub struct RootComponentClassItem<T> {
    left: StaticPointerOwned<RwLock<MarkerPin>>,
    right: StaticPointerOwned<RwLock<MarkerPin>>,
    component: Vec<StaticPointerOwned<RwLock<ComponentInstance<T>>>>,
    link: Vec<StaticPointerOwned<RwLock<MarkerLink>>>,
}

impl<T> RootComponentClassItem<T> {
    pub fn left(&self) -> &StaticPointerOwned<RwLock<MarkerPin>> {
        &self.left
    }
    pub fn right(&self) -> &StaticPointerOwned<RwLock<MarkerPin>> {
        &self.right
    }
    pub fn component(&self) -> &[StaticPointerOwned<RwLock<ComponentInstance<T>>>] {
        &self.component
    }
    pub fn component_mut(&mut self) -> &mut Vec<StaticPointerOwned<RwLock<ComponentInstance<T>>>> {
        &mut self.component
    }
    pub fn link(&self) -> &[StaticPointerOwned<RwLock<MarkerLink>>] {
        &self.link
    }
    pub fn link_mut(&mut self) -> &mut Vec<StaticPointerOwned<RwLock<MarkerLink>>> {
        &mut self.link
    }
}

#[derive(Debug)]
struct RootComponentClassItemWrapper<T>(RwLock<RootComponentClassItem<T>>);

#[derive(Debug)]
pub struct RootComponentClass<T> {
    id: Uuid,
    parent: Option<StaticPointer<RwLock<Project<T>>>>,
    item: Arc<RootComponentClassItemWrapper<T>>,
}

#[async_trait]
impl<T: 'static> ComponentClass<T> for RootComponentClass<T> {
    async fn generate_image(&self) -> bool {
        true
    }

    async fn generate_audio(&self) -> bool {
        true
    }

    async fn fixed_parameter_type(&self) -> &[(String, ParameterType)] {
        &[]
    }

    async fn default_variable_parameter_type(&self) -> &[(String, ParameterType)] {
        &[]
    }

    async fn instantiate(&self, this: &StaticPointer<RwLock<dyn ComponentClass<T>>>) -> ComponentInstance<T> {
        let guard = self.item.0.read().await;
        let marker_left = StaticPointerOwned::reference(&guard.left).clone();
        let marker_right = StaticPointerOwned::reference(&guard.right).clone();
        let one = TimeSplitValue::new(marker_left.clone(), EasingValue { from: 1., to: 1., easing: Arc::new(DefaultEasing) }, marker_right.clone());
        let one_value = VariableParameterValue::Manually(one);
        let zero = VariableParameterValue::Manually(TimeSplitValue::new(marker_left.clone(), EasingValue { from: 0., to: 0., easing: Arc::new(DefaultEasing) }, marker_right.clone()));
        let image_required_params = ImageRequiredParams {
            aspect_ratio: (16, 9),
            transform: ImageRequiredParamsTransform::Params {
                scale: Vector3 {
                    x: one_value.clone(),
                    y: one_value.clone(),
                    z: one_value.clone(),
                },
                translate: Vector3 { x: zero.clone(), y: zero.clone(), z: zero.clone() },
                rotate: TimeSplitValue::new(
                    marker_left.clone(),
                    EasingValue {
                        from: Quaternion::one(),
                        to: Quaternion::one(),
                        easing: Arc::new(DefaultEasing),
                    },
                    marker_right.clone(),
                ),
                scale_center: Vector3 { x: zero.clone(), y: zero.clone(), z: zero.clone() },
                rotate_center: Vector3 { x: zero.clone(), y: zero.clone(), z: zero },
            },
            background_color: [0; 4],
            opacity: TimeSplitValue::new(marker_left.clone(), EasingValue { from: 1., to: 1., easing: Arc::new(DefaultEasing) }, marker_right.clone()),
            blend_mode: TimeSplitValue::new(marker_left.clone(), Default::default(), marker_right.clone()),
            composite_operation: TimeSplitValue::new(marker_left.clone(), Default::default(), marker_right.clone()),
        };
        let audio_required_params = AudioRequiredParams { volume: vec![one_value.clone(), one_value] };
        let processor = Arc::clone(&self.item) as _;
        ComponentInstance::new_no_param(this.clone(), StaticPointerCow::Reference(marker_left), StaticPointerCow::Reference(marker_right), Some(image_required_params), Some(audio_required_params), processor)
    }
}

struct CloneComponentBuilder<T> {
    components: Vec<StaticPointerCow<RwLock<ComponentInstance<T>>>>,
    links: Vec<StaticPointerCow<RwLock<MarkerLink>>>,
}

impl<T> ProcessorComponentBuilder<T> for CloneComponentBuilder<T> {
    fn build(&self, _: &[ParameterValueFixed], _: &[ComponentProcessorInputValue], _: &[(String, ParameterType)], _: &mut dyn Iterator<Item = TimelineTime>, _: &dyn Fn(TimelineTime) -> MarkerTime) -> (Vec<StaticPointerCow<RwLock<ComponentInstance<T>>>>, Vec<StaticPointerCow<RwLock<MarkerLink>>>) {
        (self.components.clone(), self.links.clone())
    }
}

#[async_trait]
impl<T> ComponentProcessor<T> for RootComponentClassItemWrapper<T> {
    async fn update_variable_parameter(&self, _: &mut [ParameterValueFixed], _: &mut Vec<(String, ParameterType)>) {}

    async fn natural_length(&self, _: &[ParameterValueFixed]) -> Duration {
        let guard = self.0.read().await;
        let time = guard.right.read().await.cached_timeline_time().value() - guard.left.read().await.cached_timeline_time().value();
        Duration::from_secs_f64(time)
    }

    async fn get_processor(&self) -> ComponentProcessorBody<'_, T> {
        let guard = self.0.read().await;
        let components = guard.component.iter().map(Into::into).collect::<Vec<_>>();
        let links = guard.link.iter().map(Into::into).collect::<Vec<_>>();
        ComponentProcessorBody::Component(Arc::new(CloneComponentBuilder { components, links }))
    }
}

impl<T> PartialEq for RootComponentClass<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for RootComponentClass<T> {}

impl<T> Hash for RootComponentClass<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

impl<T> RootComponentClass<T> {
    pub(crate) fn new_empty(id: Uuid) -> StaticPointerOwned<RwLock<RootComponentClass<T>>> {
        StaticPointerOwned::new(RwLock::new(RootComponentClass {
            id,
            parent: None,
            item: Arc::new(RootComponentClassItemWrapper(RwLock::new(RootComponentClassItem {
                left: StaticPointerOwned::new(RwLock::new(MarkerPin::new(TimelineTime::new(0.).unwrap(), MarkerTime::new(0.).unwrap()))),
                right: StaticPointerOwned::new(RwLock::new(MarkerPin::new(TimelineTime::new(10.).unwrap(), MarkerTime::new(10.).unwrap()))),
                component: Vec::new(),
                link: Vec::new(),
            }))),
        }))
    }

    pub(crate) async fn set_parent(this: &StaticPointer<RwLock<RootComponentClass<T>>>, parent: Option<StaticPointer<RwLock<Project<T>>>>) -> Option<StaticPointer<RwLock<Project<T>>>> {
        let this_strong_ref = this.upgrade()?;
        let mut this_guard = this_strong_ref.write().await;
        if let Some(parent) = &parent {
            parent.upgrade()?.write().await.children.insert(this.clone());
        }
        let old_parent = mem::replace(&mut this_guard.parent, parent);
        if let Some(old_parent) = &old_parent.as_ref().and_then(StaticPointer::upgrade) {
            old_parent.write().await.children.remove(this);
        }
        old_parent
    }

    pub async fn get(&self) -> impl Deref<Target = RootComponentClassItem<T>> + '_ {
        self.item.0.read().await
    }

    pub async fn get_mut(&self) -> impl DerefMut<Target = RootComponentClassItem<T>> + '_ {
        self.item.0.write().await
    }

    pub async fn left(&self) -> StaticPointer<RwLock<MarkerPin>> {
        StaticPointerOwned::reference(&self.item.0.read().await.left).clone()
    }

    pub async fn right(&self) -> StaticPointer<RwLock<MarkerPin>> {
        StaticPointerOwned::reference(&self.item.0.read().await.right).clone()
    }

    pub async fn components(&self) -> impl Iterator<Item = StaticPointer<RwLock<ComponentInstance<T>>>> + '_ {
        let guard = self.item.0.read().await;
        let mut i = 0;
        iter::from_fn(move || {
            let ret = guard.component.get(i).map(|component| StaticPointerOwned::reference(component).clone());
            i += 1;
            ret
        })
    }

    pub async fn links(&self) -> impl Iterator<Item = StaticPointer<RwLock<MarkerLink>>> + '_ {
        let guard = self.item.0.read().await;
        let mut i = 0;
        iter::from_fn(move || {
            let ret = guard.link.get(i).map(|component| StaticPointerOwned::reference(component).clone());
            i += 1;
            ret
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn set_parent() {
        let project0 = Project::<()>::new_empty(Uuid::from_u128(0));
        let project1 = Project::<()>::new_empty(Uuid::from_u128(1));
        assert!(project0.read().await.children.is_empty());
        assert!(project1.read().await.children.is_empty());
        let component0 = RootComponentClass::<()>::new_empty(Uuid::from_u128(0));
        let component1 = RootComponentClass::<()>::new_empty(Uuid::from_u128(1));
        assert!(component0.read().await.parent.is_none());
        assert!(component1.read().await.parent.is_none());

        assert!(RootComponentClass::set_parent(StaticPointerOwned::reference(&component0), Some(StaticPointerOwned::reference(&project0).clone())).await.is_none());
        {
            let project0 = project0.read().await;
            assert_eq!(project0.children.len(), 1);
            assert_eq!(project0.children.iter().collect::<Vec<_>>(), vec![StaticPointerOwned::reference(&component0)]);
        }
        assert_eq!(component0.read().await.parent, Some(StaticPointerOwned::reference(&project0).clone()));
        assert!(RootComponentClass::set_parent(StaticPointerOwned::reference(&component1), Some(StaticPointerOwned::reference(&project1).clone())).await.is_none());
        {
            let project1 = project1.read().await;
            assert_eq!(project1.children.len(), 1);
            assert_eq!(project1.children.iter().collect::<Vec<_>>(), vec![StaticPointerOwned::reference(&component1)]);
        }
        assert_eq!(component1.read().await.parent, Some(StaticPointerOwned::reference(&project1).clone()));

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
