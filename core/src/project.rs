use crate::component::instance::ComponentInstance;
use crate::component::link::MarkerLink;
use crate::ptr::{StaticPointer, StaticPointerOwned};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug)]
pub struct Project<T> {
    id: Uuid,
    children: Vec<StaticPointer<RootComponentClass<T>>>,
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
        StaticPointerOwned::new(RwLock::new(Project { id, children: Vec::new() }))
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
    parent: Option<StaticPointer<Project<T>>>,
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
}
