use crate::component::class::ComponentClass;
use crate::component::instance::ComponentInstance;
use crate::edit::{InstanceEditCommand, RootComponentEditCommand};
use crate::project::{Project, RootComponentClass};
use crate::ptr::{StaticPointer, StaticPointerOwned};
use crate::usecase::*;
use async_trait::async_trait;
use std::borrow::Cow;
use std::error::Error;
use std::path::Path;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct MPDeltaCore<IdGenerator, ProjectLoader, ProjectWriter, ProjectMemory, RootComponentClassMemory, ComponentClassLoader, ComponentRendererBuilder, Editor, EditHistory> {
    id_generator: IdGenerator,
    project_loader: ProjectLoader,
    project_writer: ProjectWriter,
    project_memory: ProjectMemory,
    root_component_class_memory: RootComponentClassMemory,
    component_class_loader: ComponentClassLoader,
    component_renderer_builder: ComponentRendererBuilder,
    editor: Editor,
    edit_history: EditHistory,
}

#[derive(Debug, Error)]
pub enum Infallible {}

#[async_trait]
pub trait IdGenerator: Send + Sync {
    async fn generate_new(&self) -> Uuid;
}

#[async_trait]
pub trait ProjectLoader: Send + Sync {
    type Err: Error + 'static;
    async fn load_project(&self, path: &Path) -> Result<StaticPointerOwned<RwLock<Project>>, Self::Err>;
}

#[async_trait]
pub trait ProjectMemory: Send + Sync {
    async fn contains(&self, path: &Path) -> bool {
        self.get_loaded_project(path).await.is_some()
    }
    async fn insert_new_project(&self, path: Option<&Path>, project: StaticPointerOwned<RwLock<Project>>);
    async fn get_loaded_project(&self, path: &Path) -> Option<StaticPointer<RwLock<Project>>>;
    async fn all_loaded_projects(&self) -> Cow<[StaticPointer<RwLock<Project>>]>;
}

#[derive(Debug, Error)]
pub enum LoadProjectError<PLErr> {
    #[error("error from ProjectLoader: {0}")]
    ProjectLoaderError(#[from] PLErr),
}

#[async_trait]
impl<T0: Send + Sync, PL: Send + Sync, T2: Send + Sync, PM: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync> LoadProjectUsecase for MPDeltaCore<T0, PL, T2, PM, T4, T5, T6, T7, T8>
where
    PL: ProjectLoader,
    PM: ProjectMemory,
{
    type Err = LoadProjectError<PL::Err>;

    async fn load_project(&self, path: impl AsRef<Path> + Send + Sync) -> Result<StaticPointer<RwLock<Project>>, Self::Err> {
        let path = path.as_ref();
        match self.project_memory.get_loaded_project(path).await {
            Some(project) => Ok(project),
            None => {
                let project = self.project_loader.load_project(path).await?;
                let result = StaticPointerOwned::reference(&project);
                self.project_memory.insert_new_project(Some(path), project).await;
                Ok(result)
            }
        }
    }
}

#[async_trait]
pub trait ProjectWriter: Send + Sync {
    type Err: Error + 'static;
    async fn write_project(&self, project: &StaticPointer<RwLock<Project>>, path: &Path) -> Result<(), Self::Err>;
}

#[derive(Debug, Error)]
pub enum WriteProjectError<PWErr> {
    #[error("error from ProjectWriter: {0}")]
    ProjectWriterError(#[from] PWErr),
}

#[async_trait]
impl<T0: Send + Sync, T1: Send + Sync, PW: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync> WriteProjectUsecase for MPDeltaCore<T0, T1, PW, T3, T4, T5, T6, T7, T8>
where
    PW: ProjectWriter,
{
    type Err = WriteProjectError<PW::Err>;

    async fn write_project(&self, project: &StaticPointer<RwLock<Project>>, path: impl AsRef<Path> + Send + Sync) -> Result<(), Self::Err> {
        self.project_writer.write_project(project, path.as_ref()).await.map_err(Into::into)
    }
}

#[async_trait]
impl<ID: Send + Sync, T1: Send + Sync, T2: Send + Sync, PM: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync> NewProjectUsecase for MPDeltaCore<ID, T1, T2, PM, T4, T5, T6, T7, T8>
where
    ID: IdGenerator,
    PM: ProjectMemory,
{
    async fn new_project(&self) -> StaticPointer<RwLock<Project>> {
        let project = Project::new_empty(self.id_generator.generate_new().await);
        let project = StaticPointerOwned::new(RwLock::new(project));
        let pointer = StaticPointerOwned::reference(&project);
        self.project_memory.insert_new_project(None, project).await;
        pointer
    }
}

#[async_trait]
pub trait RootComponentClassMemory: Send + Sync {
    async fn insert_new_root_component_class(&self, parent: Option<&StaticPointer<RwLock<Project>>>, root_component_class: StaticPointerOwned<RwLock<RootComponentClass>>);
    async fn set_parent(&self, root_component_class: &StaticPointer<RwLock<RootComponentClass>>, parent: Option<&StaticPointer<RwLock<Project>>>);
    async fn search_by_parent(&self, parent: &StaticPointer<RwLock<Project>>) -> Cow<[StaticPointer<RwLock<RootComponentClass>>]>;
    async fn get_parent_project(&self, path: &StaticPointer<RwLock<RootComponentClass>>) -> Option<StaticPointer<RwLock<Project>>>;
    async fn all_loaded_root_component_classes(&self) -> Cow<[StaticPointer<RwLock<RootComponentClass>>]>;
}

#[async_trait]
impl<ID: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, RM: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync> NewRootComponentClassUsecase for MPDeltaCore<ID, T1, T2, T3, RM, T5, T6, T7, T8>
where
    ID: IdGenerator,
    RM: RootComponentClassMemory,
{
    async fn new_root_component_class(&self) -> StaticPointer<RwLock<RootComponentClass>> {
        let root_component_class = RootComponentClass::new_empty(self.id_generator.generate_new().await);
        let root_component_class = StaticPointerOwned::new(RwLock::new(root_component_class));
        let pointer = StaticPointerOwned::reference(&root_component_class);
        self.root_component_class_memory.insert_new_root_component_class(None, root_component_class).await;
        pointer
    }
}

#[async_trait]
impl<T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, RM: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync> SetOwnerForRootComponentClassUsecase for MPDeltaCore<T0, T1, T2, T3, RM, T5, T6, T7, T8>
where
    RM: RootComponentClassMemory,
{
    async fn set_owner_for_root_component_class(&self, component: &StaticPointer<RwLock<RootComponentClass>>, owner: &StaticPointer<RwLock<Project>>) {
        self.root_component_class_memory.set_parent(component, Some(owner)).await;
    }
}

#[async_trait]
impl<T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, PM: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync> GetLoadedProjectsUsecase for MPDeltaCore<T0, T1, T2, PM, T4, T5, T6, T7, T8>
where
    PM: ProjectMemory,
{
    async fn get_loaded_projects(&self) -> Cow<[StaticPointer<RwLock<Project>>]> {
        self.project_memory.all_loaded_projects().await
    }
}

#[async_trait]
impl<T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, RM: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync> GetRootComponentClassesUsecase for MPDeltaCore<T0, T1, T2, T3, RM, T5, T6, T7, T8>
where
    RM: RootComponentClassMemory,
{
    async fn get_root_component_classes(&self, project: &StaticPointer<RwLock<Project>>) -> Cow<[StaticPointer<RwLock<RootComponentClass>>]> {
        self.root_component_class_memory.search_by_parent(project).await
    }
}

#[async_trait]
pub trait ComponentClassLoader<T>: Send + Sync {
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<ComponentClass<T>>>]>;
}

#[async_trait]
impl<T, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, CL: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync> GetAvailableComponentClassesUsecase<T> for MPDeltaCore<T0, T1, T2, T3, T4, CL, T6, T7, T8>
where
    CL: ComponentClassLoader<T>,
{
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<ComponentClass<T>>>]> {
        self.component_class_loader.get_available_component_classes().await
    }
}

#[async_trait]
pub trait ComponentRendererBuilder<T, F, A>: Send + Sync {
    type Renderer: RealtimeComponentRenderer<F, A> + Send + Sync;
    async fn create_renderer(&self, component: &StaticPointer<RwLock<ComponentInstance<T>>>) -> Self::Renderer;
}

#[async_trait]
impl<T, F, A, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, CR: Send + Sync, T7: Send + Sync, T8: Send + Sync> RealtimeRenderComponentUsecase<T, F, A> for MPDeltaCore<T0, T1, T2, T3, T4, T5, CR, T7, T8>
where
    StaticPointer<RwLock<ComponentInstance<T>>>: Sync,
    CR: ComponentRendererBuilder<T, F, A>,
{
    type Renderer = CR::Renderer;

    async fn render_component(&self, component: &StaticPointer<RwLock<ComponentInstance<T>>>) -> Self::Renderer {
        self.component_renderer_builder.create_renderer(component).await
    }
}

#[async_trait]
pub trait Editor<T>: Send + Sync {
    type Log: Send + Sync;
    type Err: Error + 'static;
    async fn edit(&self, target: &StaticPointer<RwLock<RootComponentClass>>, command: RootComponentEditCommand) -> Result<Self::Log, Self::Err>;
    async fn edit_instance(&self, root: &StaticPointer<RwLock<RootComponentClass>>, target: &StaticPointer<RwLock<ComponentInstance<T>>>, command: InstanceEditCommand) -> Result<Self::Log, Self::Err>;
    async fn edit_reverse(&self, log: &Self::Log);
    async fn edit_by_log(&self, log: &Self::Log);
}

#[async_trait]
pub trait EditHistory<T, Log>: Send + Sync {
    async fn push_history(&self, root: &StaticPointer<RwLock<RootComponentClass>>, target: Option<&StaticPointer<RwLock<ComponentInstance<T>>>>, log: Log);
    async fn undo(&self, root: &StaticPointer<RwLock<RootComponentClass>>, target: Option<&StaticPointer<RwLock<ComponentInstance<T>>>>) -> Option<&Log>;
    async fn redo(&self, root: &StaticPointer<RwLock<RootComponentClass>>, target: Option<&StaticPointer<RwLock<ComponentInstance<T>>>>) -> Option<&Log>;
}

#[async_trait]
impl<T, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, ED: Send + Sync, HS: Send + Sync> EditUsecase<T> for MPDeltaCore<T0, T1, T2, T3, T4, T5, T6, ED, HS>
where
    StaticPointer<RwLock<ComponentInstance<T>>>: Sync,
    ED: Editor<T>,
    HS: EditHistory<T, ED::Log>,
{
    type Err = ED::Err;

    async fn edit(&self, target: &StaticPointer<RwLock<RootComponentClass>>, command: RootComponentEditCommand) -> Result<(), Self::Err> {
        let log = self.editor.edit(target, command).await?;
        self.edit_history.push_history(target, None, log).await;
        Ok(())
    }

    async fn edit_instance(&self, root: &StaticPointer<RwLock<RootComponentClass>>, target: &StaticPointer<RwLock<ComponentInstance<T>>>, command: InstanceEditCommand) -> Result<(), Self::Err> {
        let log = self.editor.edit_instance(root, target, command).await?;
        self.edit_history.push_history(root, Some(target), log).await;
        Ok(())
    }
}

#[async_trait]
impl<T, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, ED: Send + Sync, HS: Send + Sync> UndoUsecase<T> for MPDeltaCore<T0, T1, T2, T3, T4, T5, T6, ED, HS>
where
    StaticPointer<RwLock<ComponentInstance<T>>>: Sync,
    ED: Editor<T>,
    HS: EditHistory<T, ED::Log>,
{
    async fn undo(&self, component: &StaticPointer<RwLock<RootComponentClass>>) -> bool {
        if let Some(log) = self.edit_history.undo(component, None).await {
            self.editor.edit_reverse(log).await;
            true
        } else {
            false
        }
    }

    async fn undo_instance(&self, root: &StaticPointer<RwLock<RootComponentClass>>, target: &StaticPointer<RwLock<ComponentInstance<T>>>) -> bool {
        if let Some(log) = self.edit_history.undo(root, Some(target)).await {
            self.editor.edit_reverse(log).await;
            true
        } else {
            false
        }
    }
}

#[async_trait]
impl<T, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, ED: Send + Sync, HS: Send + Sync> RedoUsecase<T> for MPDeltaCore<T0, T1, T2, T3, T4, T5, T6, ED, HS>
where
    StaticPointer<RwLock<ComponentInstance<T>>>: Sync,
    ED: Editor<T>,
    HS: EditHistory<T, ED::Log>,
{
    async fn redo(&self, component: &StaticPointer<RwLock<RootComponentClass>>) -> bool {
        if let Some(log) = self.edit_history.redo(component, None).await {
            self.editor.edit_by_log(log).await;
            true
        } else {
            false
        }
    }

    async fn redo_instance(&self, root: &StaticPointer<RwLock<RootComponentClass>>, target: &StaticPointer<RwLock<ComponentInstance<T>>>) -> bool {
        if let Some(log) = self.edit_history.redo(root, Some(target)).await {
            self.editor.edit_by_log(log).await;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic;
    use std::sync::atomic::{AtomicU64, AtomicUsize};

    #[derive(Debug, Error)]
    #[error("Error")]
    struct EmptyError;

    #[derive(Default)]
    struct ID(AtomicU64);

    #[async_trait]
    impl IdGenerator for ID {
        async fn generate_new(&self) -> Uuid {
            Uuid::from_u128(self.0.fetch_add(1, atomic::Ordering::SeqCst) as u128)
        }
    }

    #[tokio::test]
    async fn load_project() {
        #[derive(Default)]
        struct PL1(RwLock<u128>);
        #[async_trait]
        impl ProjectLoader for PL1 {
            type Err = EmptyError;

            async fn load_project(&self, _: &Path) -> Result<StaticPointerOwned<RwLock<Project>>, Self::Err> {
                let mut guard = self.0.write().await;
                let id = *guard;
                *guard += 1;
                Ok(StaticPointerOwned::new(RwLock::new(Project::new_empty(Uuid::from_u128(id)))))
            }
        }
        #[derive(Default)]
        struct PM {
            memory: RwLock<Vec<(Option<PathBuf>, StaticPointerOwned<RwLock<Project>>)>>,
        }
        #[async_trait]
        impl ProjectMemory for PM {
            async fn insert_new_project(&self, path: Option<&Path>, project: StaticPointerOwned<RwLock<Project>>) {
                self.memory.write().await.push((path.map(Path::to_path_buf), project));
            }

            async fn get_loaded_project(&self, path: &Path) -> Option<StaticPointer<RwLock<Project>>> {
                self.memory.read().await.iter().find(|(p, _)| p.as_deref() == Some(path)).map(|(_, p)| StaticPointerOwned::reference(p))
            }

            async fn all_loaded_projects(&self) -> Cow<[StaticPointer<RwLock<Project>>]> {
                unreachable!()
            }
        }
        let core = MPDeltaCore {
            id_generator: (),
            project_loader: PL1::default(),
            project_writer: (),
            project_memory: PM::default(),
            root_component_class_memory: (),
            component_class_loader: (),
            component_renderer_builder: (),
            editor: (),
            edit_history: (),
        };
        assert_eq!(*LoadProjectUsecase::load_project(&core, "1").await.unwrap().upgrade().unwrap().read().await, Project::new_empty(Uuid::from_u128(0)));
        assert_eq!(*LoadProjectUsecase::load_project(&core, "1").await.unwrap().upgrade().unwrap().read().await, Project::new_empty(Uuid::from_u128(0)));
        assert_eq!(*LoadProjectUsecase::load_project(&core, "3").await.unwrap().upgrade().unwrap().read().await, Project::new_empty(Uuid::from_u128(1)));

        assert_eq!(*core.project_loader.0.read().await, 2);
        let memory = core.project_memory.memory.read().await;
        assert_eq!(memory.len(), 2);
        assert_eq!(memory[0].0.as_deref(), Some("1").map(AsRef::as_ref));
        assert_eq!(*memory[0].1.read().await, Project::new_empty(Uuid::from_u128(0)));
        assert_eq!(memory[1].0.as_deref(), Some("3").map(AsRef::as_ref));
        assert_eq!(*memory[1].1.read().await, Project::new_empty(Uuid::from_u128(1)));

        #[derive(Default)]
        struct PL2;
        #[async_trait]
        impl ProjectLoader for PL2 {
            type Err = EmptyError;

            async fn load_project(&self, path: &Path) -> Result<StaticPointerOwned<RwLock<Project>>, Self::Err> {
                Err(EmptyError)
            }
        }
        let core = MPDeltaCore {
            id_generator: (),
            project_loader: PL2::default(),
            project_writer: (),
            project_memory: PM::default(),
            root_component_class_memory: (),
            component_class_loader: (),
            component_renderer_builder: (),
            editor: (),
            edit_history: (),
        };
        assert!(LoadProjectUsecase::load_project(&core, "1").await.is_err());
    }

    #[tokio::test]
    async fn write_project() {
        #[derive(Default)]
        struct PW1(AtomicUsize);
        #[async_trait]
        impl ProjectWriter for PW1 {
            type Err = EmptyError;

            async fn write_project(&self, project: &StaticPointer<RwLock<Project>>, path: &Path) -> Result<(), Self::Err> {
                self.0.fetch_add(1, atomic::Ordering::SeqCst);
                Ok(())
            }
        }
        let core = MPDeltaCore {
            id_generator: (),
            project_loader: (),
            project_writer: PW1::default(),
            project_memory: (),
            root_component_class_memory: (),
            component_class_loader: (),
            component_renderer_builder: (),
            editor: (),
            edit_history: (),
        };
        let project = StaticPointerOwned::new(RwLock::new(Project::new_empty(Uuid::nil())));
        let project = StaticPointerOwned::reference(&project);
        assert!(WriteProjectUsecase::write_project(&core, &project, "").await.is_ok());
        assert_eq!(core.project_writer.0.load(atomic::Ordering::SeqCst), 1);
        assert!(WriteProjectUsecase::write_project(&core, &project, "").await.is_ok());
        assert_eq!(core.project_writer.0.load(atomic::Ordering::SeqCst), 2);
        assert!(WriteProjectUsecase::write_project(&core, &project, "").await.is_ok());
        assert_eq!(core.project_writer.0.load(atomic::Ordering::SeqCst), 3);
        #[derive(Default)]
        struct PW2(AtomicUsize);
        #[async_trait]
        impl ProjectWriter for PW2 {
            type Err = EmptyError;

            async fn write_project(&self, project: &StaticPointer<RwLock<Project>>, path: &Path) -> Result<(), Self::Err> {
                self.0.fetch_add(1, atomic::Ordering::SeqCst);
                Err(EmptyError)
            }
        }
        let core = MPDeltaCore {
            id_generator: (),
            project_loader: (),
            project_writer: PW2::default(),
            project_memory: (),
            root_component_class_memory: (),
            component_class_loader: (),
            component_renderer_builder: (),
            editor: (),
            edit_history: (),
        };
        assert!(WriteProjectUsecase::write_project(&core, &project, "").await.is_err());
        assert_eq!(core.project_writer.0.load(atomic::Ordering::SeqCst), 1);
        assert!(WriteProjectUsecase::write_project(&core, &project, "").await.is_err());
        assert_eq!(core.project_writer.0.load(atomic::Ordering::SeqCst), 2);
        assert!(WriteProjectUsecase::write_project(&core, &project, "").await.is_err());
        assert_eq!(core.project_writer.0.load(atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn new_project() {
        #[derive(Default)]
        struct PM(RwLock<Vec<(Option<PathBuf>, StaticPointerOwned<RwLock<Project>>)>>);
        #[async_trait]
        impl ProjectMemory for PM {
            async fn insert_new_project(&self, path: Option<&Path>, project: StaticPointerOwned<RwLock<Project>>) {
                self.0.write().await.push((path.map(Path::to_path_buf), project));
            }

            async fn get_loaded_project(&self, _: &Path) -> Option<StaticPointer<RwLock<Project>>> {
                unreachable!()
            }

            async fn all_loaded_projects(&self) -> Cow<[StaticPointer<RwLock<Project>>]> {
                unreachable!()
            }
        }
        let core = MPDeltaCore {
            id_generator: ID::default(),
            project_loader: (),
            project_writer: (),
            project_memory: PM::default(),
            root_component_class_memory: (),
            component_class_loader: (),
            component_renderer_builder: (),
            editor: (),
            edit_history: (),
        };
        assert_eq!(*NewProjectUsecase::new_project(&core).await.upgrade().unwrap().read().await, Project::new_empty(Uuid::from_u128(0)));
        assert_eq!(*NewProjectUsecase::new_project(&core).await.upgrade().unwrap().read().await, Project::new_empty(Uuid::from_u128(1)));
        assert_eq!(*NewProjectUsecase::new_project(&core).await.upgrade().unwrap().read().await, Project::new_empty(Uuid::from_u128(2)));
        assert_eq!(core.id_generator.0.load(atomic::Ordering::SeqCst), 3);
        let guard = core.project_memory.0.read().await;
        assert_eq!(guard.len(), 3);
        assert_eq!(guard[0].0, None);
        assert_eq!(*guard[0].1.read().await, Project::new_empty(Uuid::from_u128(0)));
        assert_eq!(guard[1].0, None);
        assert_eq!(*guard[1].1.read().await, Project::new_empty(Uuid::from_u128(1)));
        assert_eq!(guard[2].0, None);
        assert_eq!(*guard[2].1.read().await, Project::new_empty(Uuid::from_u128(2)));
    }

    #[tokio::test]
    async fn new_root_component_class() {
        #[derive(Default)]
        struct RM(RwLock<Vec<(Option<StaticPointer<RwLock<Project>>>, StaticPointerOwned<RwLock<RootComponentClass>>)>>);
        #[async_trait]
        impl RootComponentClassMemory for RM {
            async fn insert_new_root_component_class(&self, parent: Option<&StaticPointer<RwLock<Project>>>, root_component_class: StaticPointerOwned<RwLock<RootComponentClass>>) {
                self.0.write().await.push((parent.cloned(), root_component_class));
            }

            async fn set_parent(&self, _: &StaticPointer<RwLock<RootComponentClass>>, _: Option<&StaticPointer<RwLock<Project>>>) {
                unreachable!()
            }

            async fn search_by_parent(&self, _: &StaticPointer<RwLock<Project>>) -> Cow<[StaticPointer<RwLock<RootComponentClass>>]> {
                unreachable!()
            }

            async fn get_parent_project(&self, _: &StaticPointer<RwLock<RootComponentClass>>) -> Option<StaticPointer<RwLock<Project>>> {
                unreachable!()
            }

            async fn all_loaded_root_component_classes(&self) -> Cow<[StaticPointer<RwLock<RootComponentClass>>]> {
                unreachable!()
            }
        }
        let core = MPDeltaCore {
            id_generator: ID::default(),
            project_loader: (),
            project_writer: (),
            project_memory: (),
            root_component_class_memory: RM::default(),
            component_class_loader: (),
            component_renderer_builder: (),
            editor: (),
            edit_history: (),
        };
        assert_eq!(*NewRootComponentClassUsecase::new_root_component_class(&core).await.upgrade().unwrap().read().await, RootComponentClass::new_empty(Uuid::from_u128(0)));
        assert_eq!(*NewRootComponentClassUsecase::new_root_component_class(&core).await.upgrade().unwrap().read().await, RootComponentClass::new_empty(Uuid::from_u128(1)));
        assert_eq!(*NewRootComponentClassUsecase::new_root_component_class(&core).await.upgrade().unwrap().read().await, RootComponentClass::new_empty(Uuid::from_u128(2)));
        assert_eq!(core.id_generator.0.load(atomic::Ordering::SeqCst), 3);
        let guard = core.root_component_class_memory.0.read().await;
        assert_eq!(guard.len(), 3);
        assert_eq!(guard[0].0, None);
        assert_eq!(*guard[0].1.read().await, RootComponentClass::new_empty(Uuid::from_u128(0)));
        assert_eq!(guard[1].0, None);
        assert_eq!(*guard[1].1.read().await, RootComponentClass::new_empty(Uuid::from_u128(1)));
        assert_eq!(guard[2].0, None);
        assert_eq!(*guard[2].1.read().await, RootComponentClass::new_empty(Uuid::from_u128(2)));
    }

    #[tokio::test]
    async fn set_owner_for_root_component_class() {
        #[derive(Default)]
        struct RM(RwLock<Vec<(Option<StaticPointer<RwLock<Project>>>, StaticPointerOwned<RwLock<RootComponentClass>>)>>);
        #[async_trait]
        impl RootComponentClassMemory for RM {
            async fn insert_new_root_component_class(&self, parent: Option<&StaticPointer<RwLock<Project>>>, root_component_class: StaticPointerOwned<RwLock<RootComponentClass>>) {
                unreachable!()
            }

            async fn set_parent(&self, root_component_class: &StaticPointer<RwLock<RootComponentClass>>, parent: Option<&StaticPointer<RwLock<Project>>>) {
                if let Some((p, _)) = self.0.write().await.iter_mut().find(|(_, c)| c == root_component_class) {
                    *p = parent.cloned();
                }
            }

            async fn search_by_parent(&self, _: &StaticPointer<RwLock<Project>>) -> Cow<[StaticPointer<RwLock<RootComponentClass>>]> {
                unreachable!()
            }

            async fn get_parent_project(&self, _: &StaticPointer<RwLock<RootComponentClass>>) -> Option<StaticPointer<RwLock<Project>>> {
                unreachable!()
            }

            async fn all_loaded_root_component_classes(&self) -> Cow<[StaticPointer<RwLock<RootComponentClass>>]> {
                unreachable!()
            }
        }
        let project0 = StaticPointerOwned::new(RwLock::new(Project::new_empty(Uuid::from_u128(0))));
        let project1 = StaticPointerOwned::new(RwLock::new(Project::new_empty(Uuid::from_u128(0))));
        let project2 = StaticPointerOwned::new(RwLock::new(Project::new_empty(Uuid::from_u128(0))));
        let c0 = StaticPointerOwned::new(RwLock::new(RootComponentClass::new_empty(Uuid::from_u128(0))));
        let c1 = StaticPointerOwned::new(RwLock::new(RootComponentClass::new_empty(Uuid::from_u128(1))));
        let c2 = StaticPointerOwned::new(RwLock::new(RootComponentClass::new_empty(Uuid::from_u128(2))));
        let component0 = StaticPointerOwned::reference(&c0);
        let component1 = StaticPointerOwned::reference(&c1);
        let component2 = StaticPointerOwned::reference(&c2);
        let core = MPDeltaCore {
            id_generator: (),
            project_loader: (),
            project_writer: (),
            project_memory: (),
            root_component_class_memory: RM(RwLock::new(vec![(None, c0), (Some(StaticPointerOwned::reference(&project1)), c1), (Some(StaticPointerOwned::reference(&project0)), c2)])),
            component_class_loader: (),
            component_renderer_builder: (),
            editor: (),
            edit_history: (),
        };

        SetOwnerForRootComponentClassUsecase::set_owner_for_root_component_class(&core, &component0, &StaticPointerOwned::reference(&project0)).await;
        SetOwnerForRootComponentClassUsecase::set_owner_for_root_component_class(&core, &component1, &StaticPointerOwned::reference(&project1)).await;
        SetOwnerForRootComponentClassUsecase::set_owner_for_root_component_class(&core, &component2, &StaticPointerOwned::reference(&project2)).await;
        let guard = core.root_component_class_memory.0.read().await;
        assert_eq!(guard.len(), 3);
        assert_eq!(guard[0].0, Some(StaticPointerOwned::reference(&project0)));
        assert_eq!(guard[0].1, component0);
        assert_eq!(guard[1].0, Some(StaticPointerOwned::reference(&project1)));
        assert_eq!(guard[1].1, component1);
        assert_eq!(guard[2].0, Some(StaticPointerOwned::reference(&project2)));
        assert_eq!(guard[2].1, component2);
    }

    #[tokio::test]
    async fn get_loaded_projects() {
        struct PM(RwLock<Vec<StaticPointerOwned<RwLock<Project>>>>);
        #[async_trait]
        impl ProjectMemory for PM {
            async fn insert_new_project(&self, path: Option<&Path>, project: StaticPointerOwned<RwLock<Project>>) {
                unreachable!()
            }

            async fn get_loaded_project(&self, path: &Path) -> Option<StaticPointer<RwLock<Project>>> {
                unreachable!()
            }

            async fn all_loaded_projects(&self) -> Cow<[StaticPointer<RwLock<Project>>]> {
                Cow::Owned(self.0.read().await.iter().map(StaticPointerOwned::reference).collect())
            }
        }
        let core = MPDeltaCore {
            id_generator: (),
            project_loader: (),
            project_writer: (),
            project_memory: PM(RwLock::new(vec![
                StaticPointerOwned::new(RwLock::new(Project::new_empty(Uuid::from_u128(0)))),
                StaticPointerOwned::new(RwLock::new(Project::new_empty(Uuid::from_u128(1)))),
                StaticPointerOwned::new(RwLock::new(Project::new_empty(Uuid::from_u128(2)))),
            ])),
            root_component_class_memory: (),
            component_class_loader: (),
            component_renderer_builder: (),
            editor: (),
            edit_history: (),
        };
        let projects = GetLoadedProjectsUsecase::get_loaded_projects(&core).await;
        assert_eq!(projects.len(), 3);
        assert_eq!(*projects[0].upgrade().unwrap().read().await, Project::new_empty(Uuid::from_u128(0)));
        assert_eq!(*projects[1].upgrade().unwrap().read().await, Project::new_empty(Uuid::from_u128(1)));
        assert_eq!(*projects[2].upgrade().unwrap().read().await, Project::new_empty(Uuid::from_u128(2)));
    }

    #[tokio::test]
    async fn get_root_component_classes() {
        struct RM(RwLock<Vec<(Option<StaticPointer<RwLock<Project>>>, StaticPointerOwned<RwLock<RootComponentClass>>)>>);
        #[async_trait]
        impl RootComponentClassMemory for RM {
            async fn insert_new_root_component_class(&self, parent: Option<&StaticPointer<RwLock<Project>>>, root_component_class: StaticPointerOwned<RwLock<RootComponentClass>>) {
                unreachable!()
            }

            async fn set_parent(&self, root_component_class: &StaticPointer<RwLock<RootComponentClass>>, parent: Option<&StaticPointer<RwLock<Project>>>) {
                unreachable!()
            }

            async fn search_by_parent(&self, parent: &StaticPointer<RwLock<Project>>) -> Cow<[StaticPointer<RwLock<RootComponentClass>>]> {
                Cow::Owned(self.0.read().await.iter().filter(|(p, _)| p.as_ref() == Some(parent)).map(|(_, c)| StaticPointerOwned::reference(c)).collect())
            }

            async fn get_parent_project(&self, path: &StaticPointer<RwLock<RootComponentClass>>) -> Option<StaticPointer<RwLock<Project>>> {
                unreachable!()
            }

            async fn all_loaded_root_component_classes(&self) -> Cow<[StaticPointer<RwLock<RootComponentClass>>]> {
                unreachable!()
            }
        }
        let project0 = StaticPointerOwned::new(RwLock::new(Project::new_empty(Uuid::from_u128(0))));
        let project1 = StaticPointerOwned::new(RwLock::new(Project::new_empty(Uuid::from_u128(0))));
        let project2 = StaticPointerOwned::new(RwLock::new(Project::new_empty(Uuid::from_u128(0))));
        let core = MPDeltaCore {
            id_generator: (),
            project_loader: (),
            project_writer: (),
            project_memory: (),
            root_component_class_memory: RM(RwLock::new(vec![
                (None, StaticPointerOwned::new(RwLock::new(RootComponentClass::new_empty(Uuid::from_u128(0))))),
                (Some(StaticPointerOwned::reference(&project0)), StaticPointerOwned::new(RwLock::new(RootComponentClass::new_empty(Uuid::from_u128(0))))),
                (Some(StaticPointerOwned::reference(&project0)), StaticPointerOwned::new(RwLock::new(RootComponentClass::new_empty(Uuid::from_u128(1))))),
                (Some(StaticPointerOwned::reference(&project1)), StaticPointerOwned::new(RwLock::new(RootComponentClass::new_empty(Uuid::from_u128(2))))),
                (Some(StaticPointerOwned::reference(&project2)), StaticPointerOwned::new(RwLock::new(RootComponentClass::new_empty(Uuid::from_u128(3))))),
            ])),
            component_class_loader: (),
            component_renderer_builder: (),
            editor: (),
            edit_history: (),
        };
        let child0 = GetRootComponentClassesUsecase::get_root_component_classes(&core, &StaticPointerOwned::reference(&project0)).await;
        assert_eq!(child0.len(), 2);
        assert_eq!(*child0[0].upgrade().unwrap().read().await, RootComponentClass::new_empty(Uuid::from_u128(0)));
        assert_eq!(*child0[1].upgrade().unwrap().read().await, RootComponentClass::new_empty(Uuid::from_u128(1)));
        let child1 = GetRootComponentClassesUsecase::get_root_component_classes(&core, &StaticPointerOwned::reference(&project1)).await;
        assert_eq!(*child1[0].upgrade().unwrap().read().await, RootComponentClass::new_empty(Uuid::from_u128(2)));
        let child2 = GetRootComponentClassesUsecase::get_root_component_classes(&core, &StaticPointerOwned::reference(&project2)).await;
        assert_eq!(*child2[0].upgrade().unwrap().read().await, RootComponentClass::new_empty(Uuid::from_u128(3)));
    }

    #[tokio::test]
    async fn get_available_component_classes() {
        struct CL;
        #[async_trait]
        impl ComponentClassLoader<()> for CL {
            async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<ComponentClass<()>>>]> {
                Cow::Owned(vec![])
            }
        }
        let core = MPDeltaCore {
            id_generator: (),
            project_loader: (),
            project_writer: (),
            project_memory: (),
            root_component_class_memory: (),
            component_class_loader: CL,
            component_renderer_builder: (),
            editor: (),
            edit_history: (),
        };
        assert_eq!(GetAvailableComponentClassesUsecase::get_available_component_classes(&core).await.len(), 0);
    }

    #[tokio::test]
    async fn realtime_render_component() {
        struct RD;
        #[async_trait]
        impl RealtimeComponentRenderer<(), ()> for RD {
            fn get_frame_count(&self) -> usize {
                unreachable!()
            }

            async fn render_frame(&mut self, frame: usize) -> () {
                unreachable!()
            }

            fn sampling_rate(&self) -> u32 {
                unreachable!()
            }

            async fn mix_audio(&mut self, offset: usize, length: usize) -> () {
                unreachable!()
            }
        }
        struct CR;
        #[async_trait]
        impl ComponentRendererBuilder<(), (), ()> for CR {
            type Renderer = RD;

            async fn create_renderer(&self, _: &StaticPointer<RwLock<ComponentInstance<()>>>) -> Self::Renderer {
                RD
            }
        }
        let core = MPDeltaCore {
            id_generator: (),
            project_loader: (),
            project_writer: (),
            project_memory: (),
            root_component_class_memory: (),
            component_class_loader: (),
            component_renderer_builder: CR,
            editor: (),
            edit_history: (),
        };
        let _: RD = RealtimeRenderComponentUsecase::render_component(&core, &StaticPointer::new()).await;
    }
}
