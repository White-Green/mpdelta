use crate::component::instance::ComponentInstance;
use crate::component::link::MarkerLink;
use crate::ptr::{StaticPointer, StaticPointerOwned};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::mem;
use std::sync::Arc;
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
struct RootComponentClassItem<T> {
    component: Vec<StaticPointerOwned<RwLock<ComponentInstance<T>>>>,
    link: Vec<StaticPointerOwned<RwLock<MarkerLink>>>,
}

#[derive(Debug)]
pub struct RootComponentClass<T> {
    id: Uuid,
    parent: Option<StaticPointer<RwLock<Project<T>>>>,
    item: Arc<RwLock<RootComponentClassItem<T>>>,
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
            item: Arc::new(RwLock::new(RootComponentClassItem { component: Vec::new(), link: Vec::new() })),
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

        assert!(RootComponentClass::set_parent(&StaticPointerOwned::reference(&component0), Some(StaticPointerOwned::reference(&project0))).await.is_none());
        {
            let project0 = project0.read().await;
            assert_eq!(project0.children.len(), 1);
            assert_eq!(project0.children.iter().collect::<Vec<_>>(), vec![&StaticPointerOwned::reference(&component0)]);
        }
        assert_eq!(component0.read().await.parent, Some(StaticPointerOwned::reference(&project0)));
        assert!(RootComponentClass::set_parent(&StaticPointerOwned::reference(&component1), Some(StaticPointerOwned::reference(&project1))).await.is_none());
        {
            let project1 = project1.read().await;
            assert_eq!(project1.children.len(), 1);
            assert_eq!(project1.children.iter().collect::<Vec<_>>(), vec![&StaticPointerOwned::reference(&component1)]);
        }
        assert_eq!(component1.read().await.parent, Some(StaticPointerOwned::reference(&project1)));

        assert_eq!(RootComponentClass::set_parent(&StaticPointerOwned::reference(&component0), Some(StaticPointerOwned::reference(&project1))).await, Some(StaticPointerOwned::reference(&project0)));
        {
            let project0 = project0.read().await;
            assert!(project0.children.is_empty());
            let project1 = project1.read().await;
            assert_eq!(project1.children.len(), 2);
            let children = project1.children.iter().collect::<Vec<_>>();
            assert!(children == vec![&StaticPointerOwned::reference(&component0), &StaticPointerOwned::reference(&component1)] || children == vec![&StaticPointerOwned::reference(&component1), &StaticPointerOwned::reference(&component0)]);
        }
        assert_eq!(RootComponentClass::set_parent(&StaticPointerOwned::reference(&component1), None).await, Some(StaticPointerOwned::reference(&project1)));
        {
            let project1 = project1.read().await;
            assert_eq!(project1.children.len(), 1);
            assert_eq!(project1.children.iter().collect::<Vec<_>>(), vec![&StaticPointerOwned::reference(&component0)]);
        }
        assert!(component1.read().await.parent.is_none());
    }
}
