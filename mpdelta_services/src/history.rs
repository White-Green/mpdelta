use async_trait::async_trait;
use mpdelta_core::component::instance::ComponentInstanceHandle;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::EditHistory;
use mpdelta_core::project::RootComponentClassHandle;
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::Arc;
use tokio::sync::Mutex;

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

type HistoryKey<K, T> = (RootComponentClassHandle<K, T>, Option<ComponentInstanceHandle<K, T>>);

pub struct InMemoryEditHistoryStore<K: 'static, T: ParameterValueType, Log> {
    store: Mutex<HistoryStore<HistoryKey<K, T>, Arc<Log>>>,
}

impl<K, T, Log> InMemoryEditHistoryStore<K, T, Log>
where
    K: 'static,
    T: ParameterValueType,
{
    pub fn new(max_history: usize) -> InMemoryEditHistoryStore<K, T, Log> {
        InMemoryEditHistoryStore { store: Mutex::new(HistoryStore::new(max_history)) }
    }
}

#[async_trait]
impl<K, T, Log> EditHistory<K, T, Log> for InMemoryEditHistoryStore<K, T, Log>
where
    K: 'static,
    T: ParameterValueType,
    Log: Send + Sync,
{
    async fn push_history(&self, root: &RootComponentClassHandle<K, T>, target: Option<&ComponentInstanceHandle<K, T>>, log: Log) {
        self.store.lock().await.push_history((root.clone(), target.cloned()), Arc::new(log));
    }

    async fn undo(&self, root: &RootComponentClassHandle<K, T>, target: Option<&ComponentInstanceHandle<K, T>>) -> Option<Arc<Log>> {
        self.store.lock().await.pop_undo(&(root.clone(), target.cloned())).map(Arc::clone)
    }

    async fn redo(&self, root: &RootComponentClassHandle<K, T>, target: Option<&ComponentInstanceHandle<K, T>>) -> Option<Arc<Log>> {
        self.store.lock().await.pop_redo(&(root.clone(), target.cloned())).map(Arc::clone)
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
