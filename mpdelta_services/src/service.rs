use async_trait::async_trait;
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::core::{ComponentClassLoader, IdGenerator, ProjectLoader, ProjectWriter};
use mpdelta_core::project::Project;
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use std::borrow::Cow;
use std::path::Path;
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

// #[async_trait]
// impl ProjectMemory<T> for _ {
//     async fn insert_new_project(&self, path: Option<&Path>, project: StaticPointerOwned<tokio::sync::rwlock::RwLock<Project<T>>>) {
//         todo!()
//     }
//
//     async fn get_loaded_project(&self, path: &Path) -> Option<StaticPointer<tokio::sync::rwlock::RwLock<Project<T>>>> {
//         todo!()
//     }
//
//     async fn all_loaded_projects(&self) -> Cow<[StaticPointer<tokio::sync::rwlock::RwLock<Project<T>>>]> {
//         todo!()
//     }
// }
//
// #[async_trait]
// impl RootComponentClassMemory<T> for _ {
//     async fn insert_new_root_component_class(&self, parent: Option<&StaticPointer<tokio::sync::rwlock::RwLock<Project<T>>>>, root_component_class: StaticPointerOwned<tokio::sync::rwlock::RwLock<RootComponentClass<T>>>) {
//         todo!()
//     }
//
//     async fn set_parent(&self, root_component_class: &StaticPointer<tokio::sync::rwlock::RwLock<RootComponentClass<T>>>, parent: Option<&StaticPointer<tokio::sync::rwlock::RwLock<Project<T>>>>) {
//         todo!()
//     }
//
//     async fn search_by_parent(&self, parent: &StaticPointer<tokio::sync::rwlock::RwLock<Project<T>>>) -> Cow<[StaticPointer<tokio::sync::rwlock::RwLock<RootComponentClass<T>>>]> {
//         todo!()
//     }
//
//     async fn get_parent_project(&self, path: &StaticPointer<tokio::sync::rwlock::RwLock<RootComponentClass<T>>>) -> Option<StaticPointer<tokio::sync::rwlock::RwLock<Project<T>>>> {
//         todo!()
//     }
//
//     async fn all_loaded_root_component_classes(&self) -> Cow<[StaticPointer<tokio::sync::rwlock::RwLock<RootComponentClass<T>>>]> {
//         todo!()
//     }
// }
//
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
