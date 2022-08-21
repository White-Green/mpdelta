use async_trait::async_trait;
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::core::EditHistory;
use mpdelta_core::project::RootComponentClass;
use mpdelta_core::ptr::StaticPointer;
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

pub struct HistoryStore<Key, Log> {
    max_history: usize,
    history_map: HashMap<Key, (VecDeque<Log>, VecDeque<Log>)>,
}

impl<Key: Hash + Eq, Log> HistoryStore<Key, Log> {
    pub fn new(max_history: usize) -> HistoryStore<Key, Log> {
        HistoryStore { max_history, history_map: HashMap::new() }
    }

    pub fn push_history(&mut self, key: Key, log: Log) {
        let (history, future) = self.history_map.entry(key).or_default();
        history.push_back(log);
        let remove_len = history.len().saturating_sub(self.max_history);
        history.drain(..remove_len);
        future.clear();
    }

    pub fn pop_undo(&mut self, key: &Key) -> Option<&Log> {
        let (history, future) = self.history_map.get_mut(key)?;
        let log = history.pop_back()?;
        future.push_front(log);
        future.front()
    }

    pub fn pop_redo(&mut self, key: &Key) -> Option<&Log> {
        let (history, future) = self.history_map.get_mut(key)?;
        let log = future.pop_front()?;
        history.push_back(log);
        history.back()
    }
}

pub struct InMemoryEditHistoryStore<T, Log>(Mutex<HistoryStore<(StaticPointer<RwLock<RootComponentClass<T>>>, Option<StaticPointer<RwLock<ComponentInstance<T>>>>), Arc<Log>>>);

impl<T, Log> InMemoryEditHistoryStore<T, Log> {
    pub fn new(max_history: usize) -> InMemoryEditHistoryStore<T, Log> {
        InMemoryEditHistoryStore(Mutex::new(HistoryStore::new(max_history)))
    }
}

#[async_trait]
impl<T, Log: Send + Sync> EditHistory<T, Log> for InMemoryEditHistoryStore<T, Log> {
    async fn push_history(&self, root: &StaticPointer<RwLock<RootComponentClass<T>>>, target: Option<&StaticPointer<RwLock<ComponentInstance<T>>>>, log: Log) {
        self.0.lock().await.push_history((root.clone(), target.cloned()), Arc::new(log));
    }

    async fn undo(&self, root: &StaticPointer<RwLock<RootComponentClass<T>>>, target: Option<&StaticPointer<RwLock<ComponentInstance<T>>>>) -> Option<Arc<Log>> {
        self.0.lock().await.pop_undo(&(root.clone(), target.cloned())).map(Arc::clone)
    }

    async fn redo(&self, root: &StaticPointer<RwLock<RootComponentClass<T>>>, target: Option<&StaticPointer<RwLock<ComponentInstance<T>>>>) -> Option<Arc<Log>> {
        self.0.lock().await.pop_redo(&(root.clone(), target.cloned())).map(Arc::clone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_store() {
        let mut history_store = HistoryStore::<usize, &str>::new(5);
        history_store.push_history(0, "0A");
        history_store.push_history(0, "0B");
        history_store.push_history(0, "0C");
        history_store.push_history(0, "0D");
        history_store.push_history(0, "0E");
        history_store.push_history(0, "0F");
        history_store.push_history(1, "1A");
        history_store.push_history(1, "1B");
        history_store.push_history(1, "1C");

        assert_eq!(history_store.pop_redo(&0), None);
        assert_eq!(history_store.pop_undo(&0), Some(&"0F"));
        assert_eq!(history_store.pop_undo(&0), Some(&"0E"));
        assert_eq!(history_store.pop_undo(&0), Some(&"0D"));
        assert_eq!(history_store.pop_undo(&0), Some(&"0C"));
        assert_eq!(history_store.pop_undo(&0), Some(&"0B"));
        assert_eq!(history_store.pop_undo(&0), None);
        assert_eq!(history_store.pop_redo(&0), Some(&"0B"));
        assert_eq!(history_store.pop_redo(&0), Some(&"0C"));

        history_store.push_history(0, "0G");

        assert_eq!(history_store.pop_redo(&0), None);
        assert_eq!(history_store.pop_undo(&0), Some(&"0G"));
        assert_eq!(history_store.pop_undo(&0), Some(&"0C"));
        assert_eq!(history_store.pop_undo(&0), Some(&"0B"));
        assert_eq!(history_store.pop_undo(&0), None);

        assert_eq!(history_store.pop_undo(&1), Some(&"1C"));
        assert_eq!(history_store.pop_undo(&1), Some(&"1B"));
        assert_eq!(history_store.pop_undo(&1), Some(&"1A"));
        assert_eq!(history_store.pop_undo(&1), None);
        assert_eq!(history_store.pop_redo(&1), Some(&"1A"));
        assert_eq!(history_store.pop_redo(&1), Some(&"1B"));
        assert_eq!(history_store.pop_redo(&1), Some(&"1C"));
        assert_eq!(history_store.pop_redo(&1), None);
    }
}
