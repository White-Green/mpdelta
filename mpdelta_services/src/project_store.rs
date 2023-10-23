use async_trait::async_trait;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::{ProjectMemory, RootComponentClassMemory};
use mpdelta_core::project::{Project, ProjectHandle, ProjectHandleOwned, RootComponentClass, RootComponentClassHandle, RootComponentClassHandleOwned};
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;

#[derive(Debug)]
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

    pub fn remove_root(&mut self, root: &StaticPointer<Root>) -> Option<StaticPointerOwned<Root>> {
        let (i, _) = self.root_list.iter().enumerate().find(|(_, (_, r))| r == root)?;
        Some(self.root_list.swap_remove(i).1)
    }

    pub fn search_root_by_key(&self, key: &impl PartialEq<RootKey>) -> Option<StaticPointer<Root>> {
        self.root_list.iter().find_map(|(k, value)| (key == k.as_ref()?).then_some(value).map(StaticPointerOwned::reference).cloned())
    }

    pub fn all_root(&self) -> impl Iterator<Item = StaticPointer<Root>> + '_ {
        self.root_list.iter().map(|(_, root)| StaticPointerOwned::reference(root)).cloned()
    }

    pub fn insert_child(&mut self, parent: Option<&StaticPointer<Root>>, child: StaticPointerOwned<Child>) {
        let child_reference = StaticPointerOwned::reference(&child).clone();
        self.children.push(child);
        if let Some(parent) = parent {
            self.child_root_map.insert(child_reference, parent.clone());
        }
    }

    pub fn remove_child(&mut self, child: &StaticPointer<Child>) -> Option<StaticPointerOwned<Child>> {
        let (i, _) = self.children.iter().enumerate().find(|(_, c)| *c == child)?;
        self.child_root_map.remove(child);
        Some(self.children.remove(i))
    }

    pub fn get_root(&self, child: &StaticPointer<Child>) -> Option<&StaticPointer<Root>> {
        self.child_root_map.get(child)
    }

    pub fn set_root(&mut self, child: &StaticPointer<Child>, root: &StaticPointer<Root>) {
        self.child_root_map.insert(child.clone(), root.clone());
    }

    pub fn unset_root(&mut self, child: &StaticPointer<Child>) {
        self.child_root_map.remove(child);
    }

    pub fn children_by_root<'a>(&'a self, root: &'a StaticPointer<Root>) -> impl Iterator<Item = StaticPointer<Child>> + 'a {
        self.children.iter().map(StaticPointerOwned::reference).filter(|child| self.child_root_map.get(child) == Some(root)).cloned()
    }

    pub fn all_children(&self) -> impl Iterator<Item = StaticPointer<Child>> + '_ {
        self.children.iter().map(StaticPointerOwned::reference).cloned()
    }
}

impl<RootKey: PartialEq, Root, Child> Default for ForestMap<RootKey, Root, Child> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct InMemoryProjectStore<K: 'static, T: ParameterValueType>(RwLock<ForestMap<PathBuf, RwLock<Project<K, T>>, RwLock<RootComponentClass<K, T>>>>);

impl<K, T> InMemoryProjectStore<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    pub fn new() -> InMemoryProjectStore<K, T> {
        InMemoryProjectStore(RwLock::new(ForestMap::new()))
    }
}

impl<K, T> Default for InMemoryProjectStore<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<K, T> ProjectMemory<K, T> for InMemoryProjectStore<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    async fn insert_new_project(&self, path: Option<&Path>, project: ProjectHandleOwned<K, T>) {
        self.0.write().await.insert_root(path.map(Path::to_path_buf), project);
    }

    async fn get_loaded_project(&self, path: &Path) -> Option<ProjectHandle<K, T>> {
        self.0.read().await.search_root_by_key(&path)
    }

    async fn all_loaded_projects(&self) -> Cow<[ProjectHandle<K, T>]> {
        Cow::Owned(self.0.read().await.all_root().collect())
    }
}

#[async_trait]
impl<K, T> RootComponentClassMemory<K, T> for InMemoryProjectStore<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    async fn insert_new_root_component_class(&self, parent: Option<&ProjectHandle<K, T>>, root_component_class: RootComponentClassHandleOwned<K, T>) {
        self.0.write().await.insert_child(parent, root_component_class);
    }

    async fn set_parent(&self, root_component_class: &RootComponentClassHandle<K, T>, parent: Option<&ProjectHandle<K, T>>) {
        if let Some(parent) = parent {
            self.0.write().await.set_root(root_component_class, parent);
        } else {
            self.0.write().await.unset_root(root_component_class);
        }
    }

    async fn search_by_parent(&self, parent: &ProjectHandle<K, T>) -> Cow<[RootComponentClassHandle<K, T>]> {
        Cow::Owned(self.0.read().await.children_by_root(parent).collect())
    }

    async fn get_parent_project(&self, root_component_class: &RootComponentClassHandle<K, T>) -> Option<ProjectHandle<K, T>> {
        self.0.read().await.get_root(root_component_class).cloned()
    }

    async fn all_loaded_root_component_classes(&self) -> Cow<[RootComponentClassHandle<K, T>]> {
        Cow::Owned(self.0.read().await.all_children().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn forest_map() {
        let new_root = StaticPointerOwned::<u32>::new;
        let new_child = StaticPointerOwned::<i32>::new;
        let mut map = ForestMap::<String, u32, i32>::new();
        let mut roots = Vec::new();
        map.insert_root(Some("A".to_string()), {
            let root = new_root(0);
            roots.push(StaticPointerOwned::reference(&root).clone());
            root
        });
        map.insert_root(Some("B".to_string()), {
            let root = new_root(1);
            roots.push(StaticPointerOwned::reference(&root).clone());
            root
        });
        map.insert_root(None, {
            let root = new_root(2);
            roots.push(StaticPointerOwned::reference(&root).clone());
            root
        });

        let mut children = Vec::new();
        map.insert_child(Some(&roots[0]), {
            let child = new_child(0);
            children.push(StaticPointerOwned::reference(&child).clone());
            child
        });
        map.insert_child(Some(&roots[0]), {
            let child = new_child(1);
            children.push(StaticPointerOwned::reference(&child).clone());
            child
        });
        map.insert_child(Some(&roots[1]), {
            let child = new_child(2);
            children.push(StaticPointerOwned::reference(&child).clone());
            child
        });
        map.insert_child(None, {
            let child = new_child(3);
            children.push(StaticPointerOwned::reference(&child).clone());
            child
        });
        map.insert_child(None, {
            let child = new_child(4);
            children.push(StaticPointerOwned::reference(&child).clone());
            child
        });

        assert_eq!(map.search_root_by_key(&"A"), Some(roots[0].clone()));
        assert_eq!(map.search_root_by_key(&"B"), Some(roots[1].clone()));
        assert_eq!(map.search_root_by_key(&"C"), None);

        assert_eq!(map.get_root(&children[0]), Some(&roots[0]));
        assert_eq!(map.get_root(&children[1]), Some(&roots[0]));
        assert_eq!(map.get_root(&children[2]), Some(&roots[1]));
        assert_eq!(map.get_root(&children[3]), None);
        assert_eq!(map.get_root(&children[4]), None);

        assert_eq!(map.children_by_root(&roots[0]).collect::<HashSet<_>>(), children[..2].iter().cloned().collect::<HashSet<_>>());
        assert_eq!(map.children_by_root(&roots[1]).collect::<HashSet<_>>(), children[2..3].iter().cloned().collect::<HashSet<_>>());
        assert_eq!(map.children_by_root(&roots[2]).collect::<HashSet<_>>(), HashSet::new());

        assert_eq!(map.all_root().collect::<HashSet<_>>(), roots.iter().cloned().collect::<HashSet<_>>());
        assert_eq!(map.all_children().collect::<HashSet<_>>(), children.iter().cloned().collect::<HashSet<_>>());

        map.set_root(&children[3], &roots[2]);

        assert_eq!(map.get_root(&children[0]), Some(&roots[0]));
        assert_eq!(map.get_root(&children[1]), Some(&roots[0]));
        assert_eq!(map.get_root(&children[2]), Some(&roots[1]));
        assert_eq!(map.get_root(&children[3]), Some(&roots[2]));
        assert_eq!(map.get_root(&children[4]), None);

        assert_eq!(map.children_by_root(&roots[0]).collect::<HashSet<_>>(), children[..2].iter().cloned().collect::<HashSet<_>>());
        assert_eq!(map.children_by_root(&roots[1]).collect::<HashSet<_>>(), children[2..3].iter().cloned().collect::<HashSet<_>>());
        assert_eq!(map.children_by_root(&roots[2]).collect::<HashSet<_>>(), children[3..4].iter().cloned().collect::<HashSet<_>>());

        assert_eq!(map.all_root().collect::<HashSet<_>>(), roots.iter().cloned().collect::<HashSet<_>>());
        assert_eq!(map.all_children().collect::<HashSet<_>>(), children.iter().cloned().collect::<HashSet<_>>());

        map.unset_root(&children[2]);
        map.unset_root(&children[4]);

        assert_eq!(map.get_root(&children[0]), Some(&roots[0]));
        assert_eq!(map.get_root(&children[1]), Some(&roots[0]));
        assert_eq!(map.get_root(&children[2]), None);
        assert_eq!(map.get_root(&children[3]), Some(&roots[2]));
        assert_eq!(map.get_root(&children[4]), None);

        assert_eq!(map.children_by_root(&roots[0]).collect::<HashSet<_>>(), children[..2].iter().cloned().collect::<HashSet<_>>());
        assert_eq!(map.children_by_root(&roots[1]).collect::<HashSet<_>>(), HashSet::new());
        assert_eq!(map.children_by_root(&roots[2]).collect::<HashSet<_>>(), children[3..4].iter().cloned().collect::<HashSet<_>>());

        assert_eq!(map.all_root().collect::<HashSet<_>>(), roots.iter().cloned().collect::<HashSet<_>>());
        assert_eq!(map.all_children().collect::<HashSet<_>>(), children.iter().cloned().collect::<HashSet<_>>());
    }
}
