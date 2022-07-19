use async_trait::async_trait;
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::core::{ComponentClassLoader, IdGenerator, ProjectLoader, ProjectMemory, ProjectWriter, RootComponentClassMemory};
use mpdelta_core::project::{Project, RootComponentClass};
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::atomic;
use std::sync::atomic::AtomicU64;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::v1::Timestamp;
use uuid::Uuid;

#[derive(Debug)]
pub struct UniqueIdGenerator {
    context: uuid::v1::Context,
    counter: AtomicU64,
}

impl UniqueIdGenerator {
    pub fn new() -> UniqueIdGenerator {
        UniqueIdGenerator {
            context: uuid::v1::Context::new_random(),
            counter: AtomicU64::new(0),
        }
    }
}

impl Default for UniqueIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl IdGenerator for UniqueIdGenerator {
    async fn generate_new(&self) -> Uuid {
        let now = time::OffsetDateTime::now_utc();
        let secs = now.unix_timestamp();
        let nanos = now.unix_timestamp_nanos();
        let counter = self.counter.fetch_add(1, atomic::Ordering::AcqRel);
        Uuid::new_v1(Timestamp::from_unix(&self.context, secs as u64, (nanos % 1_000_000_000) as u32), <&[u8; 6]>::try_from(&counter.to_be_bytes()[2..]).unwrap())
    }
}

pub struct TemporaryProjectLoader;

#[derive(Debug, Error)]
pub enum Infallible {}

#[async_trait]
impl<T> ProjectLoader<T> for TemporaryProjectLoader {
    type Err = Infallible;

    async fn load_project(&self, _: &Path) -> Result<StaticPointerOwned<RwLock<Project<T>>>, Self::Err> {
        todo!("ProjectLoader is not implemented yet")
    }
}

pub struct TemporaryProjectWriter;

#[async_trait]
impl<T> ProjectWriter<T> for TemporaryProjectWriter {
    type Err = Infallible;

    async fn write_project(&self, _: &StaticPointer<RwLock<Project<T>>>, _: &Path) -> Result<(), Self::Err> {
        todo!("ProjectWriter is not implemented yet")
    }
}

pub struct TemporaryComponentClassLoader;

#[async_trait]
impl<T> ComponentClassLoader<T> for TemporaryComponentClassLoader {
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<T>>>]> {
        Cow::Borrowed(&[])
    }
}

pub struct ForestMap<RootKey, Root, Child> {
    root_list: Vec<(Option<RootKey>, StaticPointerOwned<Root>)>,
    children: Vec<StaticPointerOwned<Child>>,
    child_root_map: HashMap<StaticPointer<Child>, StaticPointer<Root>>,
}

impl<RootKey: PartialEq, Root, Child> ForestMap<RootKey, Root, Child> {
    pub fn new() -> ForestMap<RootKey, Root, Child> {
        ForestMap {
            root_list: Vec::new(),
            children: Vec::new(),
            child_root_map: HashMap::new(),
        }
    }

    pub fn insert_root(&mut self, key: Option<RootKey>, root: StaticPointerOwned<Root>) {
        self.root_list.push((key, root));
    }

    pub fn search_root_by_key(&self, key: &impl PartialEq<RootKey>) -> Option<StaticPointer<Root>> {
        self.root_list.iter().find_map(|(k, value)| (key == k.as_ref()?).then_some(value).map(StaticPointerOwned::reference))
    }

    pub fn all_root(&self) -> impl Iterator<Item = StaticPointer<Root>> + '_ {
        self.root_list.iter().map(|(_, root)| StaticPointerOwned::reference(root))
    }

    pub fn insert_child(&mut self, parent: Option<&StaticPointer<Root>>, child: StaticPointerOwned<Child>) {
        let child_reference = StaticPointerOwned::reference(&child);
        self.children.push(child);
        if let Some(parent) = parent {
            self.child_root_map.insert(child_reference, parent.clone());
        }
    }

    pub fn get_root(&self, child: &StaticPointer<Child>) -> Option<&StaticPointer<Root>> {
        self.child_root_map.get(child)
    }

    pub fn set_root(&mut self, child: &StaticPointer<Child>, root: &StaticPointer<Root>) {
        self.child_root_map.insert(child.clone(), root.clone());
    }

    pub fn remove_root(&mut self, child: &StaticPointer<Child>) {
        self.child_root_map.remove(child);
    }

    pub fn children_by_root<'a>(&'a self, root: &'a StaticPointer<Root>) -> impl Iterator<Item = StaticPointer<Child>> + 'a {
        self.children.iter().map(StaticPointerOwned::reference).filter(|child| self.child_root_map.get(child) == Some(root))
    }

    pub fn all_children(&self) -> impl Iterator<Item = StaticPointer<Child>> + '_ {
        self.children.iter().map(StaticPointerOwned::reference)
    }
}

impl<RootKey: PartialEq, Root, Child> Default for ForestMap<RootKey, Root, Child> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct InMemoryProjectStore<T>(RwLock<ForestMap<PathBuf, RwLock<Project<T>>, RwLock<RootComponentClass<T>>>>);

#[async_trait]
impl<T> ProjectMemory<T> for InMemoryProjectStore<T> {
    async fn insert_new_project(&self, path: Option<&Path>, project: StaticPointerOwned<RwLock<Project<T>>>) {
        self.0.write().await.insert_root(path.map(Path::to_path_buf), project);
    }

    async fn get_loaded_project(&self, path: &Path) -> Option<StaticPointer<RwLock<Project<T>>>> {
        self.0.read().await.search_root_by_key(&path)
    }

    async fn all_loaded_projects(&self) -> Cow<[StaticPointer<RwLock<Project<T>>>]> {
        Cow::Owned(self.0.read().await.all_root().collect())
    }
}

#[async_trait]
impl<T> RootComponentClassMemory<T> for InMemoryProjectStore<T> {
    async fn insert_new_root_component_class(&self, parent: Option<&StaticPointer<RwLock<Project<T>>>>, root_component_class: StaticPointerOwned<RwLock<RootComponentClass<T>>>) {
        self.0.write().await.insert_child(parent, root_component_class);
    }

    async fn set_parent(&self, root_component_class: &StaticPointer<RwLock<RootComponentClass<T>>>, parent: Option<&StaticPointer<RwLock<Project<T>>>>) {
        if let Some(parent) = parent {
            self.0.write().await.set_root(root_component_class, parent);
        } else {
            self.0.write().await.remove_root(root_component_class);
        }
    }

    async fn search_by_parent(&self, parent: &StaticPointer<RwLock<Project<T>>>) -> Cow<[StaticPointer<RwLock<RootComponentClass<T>>>]> {
        Cow::Owned(self.0.read().await.children_by_root(parent).collect())
    }

    async fn get_parent_project(&self, root_component_class: &StaticPointer<RwLock<RootComponentClass<T>>>) -> Option<StaticPointer<RwLock<Project<T>>>> {
        self.0.read().await.get_root(root_component_class).cloned()
    }

    async fn all_loaded_root_component_classes(&self) -> Cow<[StaticPointer<RwLock<RootComponentClass<T>>>]> {
        Cow::Owned(self.0.read().await.all_children().collect())
    }
}

// #[async_trait]
// impl Editor<T> for _ {
//     type Log = ();
//     type Err = ();
//
//     async fn edit(&self, target: &StaticPointer<tokio::sync::rwlock::RwLock<RootComponentClass<T>>>, command: RootComponentEditCommand) -> Result<Self::Log, Self::Err> {
//         todo!()
//     }
//
//     async fn edit_instance(&self, root: &StaticPointer<tokio::sync::rwlock::RwLock<RootComponentClass<T>>>, target: &StaticPointer<tokio::sync::rwlock::RwLock<ComponentInstance<T>>>, command: InstanceEditCommand) -> Result<Self::Log, Self::Err> {
//         todo!()
//     }
//
//     async fn edit_reverse(&self, log: &Self::Log) {
//         todo!()
//     }
//
//     async fn edit_by_log(&self, log: &Self::Log) {
//         todo!()
//     }
// }
//
// #[async_trait]
// impl EditHistory<T, Log> for _ {
//     async fn push_history(&self, root: &StaticPointer<tokio::sync::rwlock::RwLock<RootComponentClass<T>>>, target: Option<&StaticPointer<tokio::sync::rwlock::RwLock<ComponentInstance<T>>>>, log: Log) {
//         todo!()
//     }
//
//     async fn undo(&self, root: &StaticPointer<tokio::sync::rwlock::RwLock<RootComponentClass<T>>>, target: Option<&StaticPointer<tokio::sync::rwlock::RwLock<ComponentInstance<T>>>>) -> Option<&Log> {
//         todo!()
//     }
//
//     async fn redo(&self, root: &StaticPointer<tokio::sync::rwlock::RwLock<RootComponentClass<T>>>, target: Option<&StaticPointer<tokio::sync::rwlock::RwLock<ComponentInstance<T>>>>) -> Option<&Log> {
//         todo!()
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::iter;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_unique_id_generator() {
        let unique_id_generator = Arc::new(UniqueIdGenerator::new());
        let mut set = HashSet::new();
        let threads = iter::repeat(unique_id_generator).take(100_000).map(|gen| tokio::spawn(async move { gen.generate_new().await })).collect::<Vec<_>>();
        for t in threads {
            assert!(set.insert(t.await.unwrap()));
        }
    }
}
