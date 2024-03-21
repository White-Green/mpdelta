use async_trait::async_trait;
use futures::{stream, StreamExt};
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::{ProjectMemory, RootComponentClassMemory};
use mpdelta_core::project::{Project, ProjectHandle, ProjectHandleOwned, ProjectWithLock, RootComponentClassHandle, RootComponentClassHandleOwned, RootComponentClassWithLock};
use mpdelta_core::ptr::StaticPointerOwned;
use std::borrow::Cow;
use std::iter;
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug)]
struct ProjectData<K: 'static, T: ParameterValueType> {
    path: Option<PathBuf>,
    project: ProjectHandleOwned<K, T>,
}

#[derive(Debug)]
pub struct InMemoryProjectStore<K: 'static, T: ParameterValueType> {
    default_project: ProjectHandleOwned<K, T>,
    store: RwLock<Vec<ProjectData<K, T>>>,
}

impl<K, T> InMemoryProjectStore<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    pub fn new() -> InMemoryProjectStore<K, T> {
        InMemoryProjectStore {
            default_project: Project::new_empty(Uuid::nil()),
            store: RwLock::new(Vec::new()),
        }
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
    fn default_project(&self) -> ProjectHandle<K, T> {
        StaticPointerOwned::reference(&self.default_project).clone()
    }

    async fn insert_new_project(&self, path: Option<&Path>, project: ProjectHandleOwned<K, T>) {
        self.store.write().await.push(ProjectData { path: path.map(Path::to_path_buf), project });
    }

    async fn get_loaded_project(&self, path: &Path) -> Option<ProjectHandle<K, T>> {
        self.store.read().await.iter().find_map(|ProjectData { path: p, project }| (p.as_ref().map(AsRef::as_ref) == Some(path)).then(|| StaticPointerOwned::reference(project).clone()))
    }

    async fn all_loaded_projects(&self) -> Cow<[ProjectHandle<K, T>]> {
        Cow::Owned(self.store.read().await.iter().map(|ProjectData { project, .. }| project).chain(iter::once(&self.default_project)).map(StaticPointerOwned::reference).cloned().collect())
    }
}

#[async_trait]
impl<K, T> RootComponentClassMemory<K, T> for InMemoryProjectStore<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    async fn insert_new_root_component_class(&self, parent: Option<&ProjectHandle<K, T>>, root_component_class: RootComponentClassHandleOwned<K, T>) {
        if let Some(project) = parent {
            if let Some(project_ref) = project.upgrade() {
                project_ref.write().await.add_child(project, root_component_class).await;
            }
        } else {
            self.default_project.write().await.add_child(StaticPointerOwned::reference(&self.default_project), root_component_class).await;
        }
    }

    async fn set_parent(&self, root_component_class: &RootComponentClassHandle<K, T>, parent: Option<&ProjectHandle<K, T>>) {
        async fn set_parent<K: 'static, T: ParameterValueType>(component: &RootComponentClassWithLock<K, T>, root_component_class: &RootComponentClassHandle<K, T>, project: &ProjectWithLock<K, T>, project_handle: &ProjectHandle<K, T>) {
            loop {
                let component_read_guard = component.read().await;
                let current_parent = component_read_guard.parent();
                let Some(current_parent) = current_parent.upgrade() else {
                    continue;
                };
                let mut current_parent = current_parent.write().await;
                let Some(owned) = current_parent.remove_child(root_component_class) else {
                    continue;
                };
                drop(current_parent);
                drop(component_read_guard);
                project.write().await.add_child(project_handle, owned).await;
                break;
            }
        }
        if let Some(component_ref) = root_component_class.upgrade() {
            'block: {
                if let Some(parent) = parent {
                    if let Some(parent_ref) = parent.upgrade() {
                        set_parent(&component_ref, root_component_class, &parent_ref, parent).await;
                        break 'block;
                    }
                }
                set_parent(&component_ref, root_component_class, &self.default_project, StaticPointerOwned::reference(&self.default_project)).await;
            };
        }
    }

    async fn search_by_parent(&self, parent: &ProjectHandle<K, T>) -> Cow<[RootComponentClassHandle<K, T>]> {
        let Some(project) = parent.upgrade() else {
            return Cow::Borrowed(&[]);
        };
        let project = project.read().await;
        Cow::Owned(project.children().iter().map(StaticPointerOwned::reference).cloned().collect())
    }

    async fn get_parent_project(&self, root_component_class: &RootComponentClassHandle<K, T>) -> Option<ProjectHandle<K, T>> {
        let root_component_class = root_component_class.upgrade()?;
        let root_component_class = root_component_class.read().await;
        let parent = root_component_class.parent();
        if parent == &self.default_project {
            None
        } else {
            Some(parent.clone())
        }
    }

    async fn all_loaded_root_component_classes(&self) -> Cow<[RootComponentClassHandle<K, T>]> {
        Cow::Owned(
            stream::iter(self.store.read().await.iter().map(|ProjectData { project, .. }| project).chain(iter::once(&self.default_project)))
                .fold(Vec::new(), |mut acc, project| async {
                    acc.extend(project.read().await.children().iter().map(StaticPointerOwned::reference).cloned());
                    acc
                })
                .await,
        )
    }
}
