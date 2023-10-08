use crate::component::class::ComponentClass;
use crate::component::instance::ComponentInstanceHandle;
use crate::component::parameter::ParameterValueType;
use crate::edit::{InstanceEditCommand, InstanceEditEvent, RootComponentEditCommand, RootComponentEditEvent};
use crate::project::{Project, ProjectHandle, ProjectHandleOwned, RootComponentClass, RootComponentClassHandle, RootComponentClassHandleOwned};
use crate::ptr::{StaticPointer, StaticPointerOwned};
use crate::usecase::*;
use async_trait::async_trait;
use qcell::TCellOwner;
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Debug, Formatter};
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct MPDeltaCore<K: 'static, IdGenerator, ProjectLoader, ProjectWriter, ProjectMemory, RootComponentClassMemory, ComponentClassLoader, ComponentRendererBuilder, Editor, EditHistory> {
    id_generator: Arc<IdGenerator>,
    project_loader: Arc<ProjectLoader>,
    project_writer: Arc<ProjectWriter>,
    project_memory: Arc<ProjectMemory>,
    root_component_class_memory: Arc<RootComponentClassMemory>,
    component_class_loader: Arc<ComponentClassLoader>,
    component_renderer_builder: Arc<ComponentRendererBuilder>,
    editor: Arc<Editor>,
    edit_history: Arc<EditHistory>,
    key: Arc<RwLock<TCellOwner<K>>>,
}

impl<K, IdGenerator: Debug, ProjectLoader: Debug, ProjectWriter: Debug, ProjectMemory: Debug, RootComponentClassMemory: Debug, ComponentClassLoader: Debug, ComponentRendererBuilder: Debug, Editor: Debug, EditHistory: Debug> Debug
    for MPDeltaCore<K, IdGenerator, ProjectLoader, ProjectWriter, ProjectMemory, RootComponentClassMemory, ComponentClassLoader, ComponentRendererBuilder, Editor, EditHistory>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MPDeltaCore")
            .field("id_generator", &self.id_generator)
            .field("project_loader", &self.project_loader)
            .field("project_writer", &self.project_writer)
            .field("project_memory", &self.project_memory)
            .field("root_component_class_memory", &self.root_component_class_memory)
            .field("component_class_loader", &self.component_class_loader)
            .field("component_renderer_builder", &self.component_renderer_builder)
            .field("editor", &self.editor)
            .field("edit_history", &self.edit_history)
            .finish_non_exhaustive()
    }
}

impl<K, IdGenerator, ProjectLoader, ProjectWriter, ProjectMemory, RootComponentClassMemory, ComponentClassLoader, ComponentRendererBuilder, Editor, EditHistory>
    MPDeltaCore<K, IdGenerator, ProjectLoader, ProjectWriter, ProjectMemory, RootComponentClassMemory, ComponentClassLoader, ComponentRendererBuilder, Editor, EditHistory>
{
    pub fn new(
        id_generator: Arc<IdGenerator>,
        project_loader: Arc<ProjectLoader>,
        project_writer: Arc<ProjectWriter>,
        project_memory: Arc<ProjectMemory>,
        root_component_class_memory: Arc<RootComponentClassMemory>,
        component_class_loader: Arc<ComponentClassLoader>,
        component_renderer_builder: Arc<ComponentRendererBuilder>,
        editor: Arc<Editor>,
        edit_history: Arc<EditHistory>,
        key: Arc<RwLock<TCellOwner<K>>>,
    ) -> MPDeltaCore<K, IdGenerator, ProjectLoader, ProjectWriter, ProjectMemory, RootComponentClassMemory, ComponentClassLoader, ComponentRendererBuilder, Editor, EditHistory> {
        MPDeltaCore {
            id_generator,
            project_loader,
            project_writer,
            project_memory,
            root_component_class_memory,
            component_class_loader,
            component_renderer_builder,
            editor,
            edit_history,
            key,
        }
    }
}

#[async_trait]
pub trait IdGenerator: Send + Sync {
    fn generate_new(&self) -> Uuid;
}

#[async_trait]
pub trait ProjectLoader<K, T: ParameterValueType>: Send + Sync {
    type Err: Error + 'static;
    async fn load_project(&self, path: &Path) -> Result<ProjectHandleOwned<K, T>, Self::Err>;
}

#[async_trait]
pub trait ProjectMemory<K: 'static, T: ParameterValueType>: Send + Sync {
    async fn contains(&self, path: &Path) -> bool {
        self.get_loaded_project(path).await.is_some()
    }
    async fn insert_new_project(&self, path: Option<&Path>, project: ProjectHandleOwned<K, T>);
    async fn get_loaded_project(&self, path: &Path) -> Option<ProjectHandle<K, T>>;
    async fn all_loaded_projects(&self) -> Cow<[ProjectHandle<K, T>]>;
}

#[derive(Debug, Error)]
pub enum LoadProjectError<PLErr> {
    #[error("error from ProjectLoader: {0}")]
    ProjectLoaderError(#[from] PLErr),
}

#[async_trait]
impl<K, T: ParameterValueType, T0: Send + Sync, PL: Send + Sync, T2: Send + Sync, PM: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync> LoadProjectUsecase<K, T> for MPDeltaCore<K, T0, PL, T2, PM, T4, T5, T6, T7, T8>
where
    PL: ProjectLoader<K, T>,
    PM: ProjectMemory<K, T>,
{
    type Err = LoadProjectError<PL::Err>;

    async fn load_project(&self, path: impl AsRef<Path> + Send + Sync) -> Result<ProjectHandle<K, T>, Self::Err> {
        let path = path.as_ref();
        match self.project_memory.get_loaded_project(path).await {
            Some(project) => Ok(project),
            None => {
                let project = self.project_loader.load_project(path).await?;
                let result = StaticPointerOwned::reference(&project).clone();
                self.project_memory.insert_new_project(Some(path), project).await;
                Ok(result)
            }
        }
    }
}

#[async_trait]
pub trait ProjectWriter<K, T: ParameterValueType>: Send + Sync {
    type Err: Error + 'static;
    async fn write_project(&self, project: &ProjectHandle<K, T>, path: &Path) -> Result<(), Self::Err>;
}

#[derive(Debug, Error)]
pub enum WriteProjectError<PWErr> {
    #[error("error from ProjectWriter: {0}")]
    ProjectWriterError(#[from] PWErr),
}

#[async_trait]
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, PW: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync> WriteProjectUsecase<K, T> for MPDeltaCore<K, T0, T1, PW, T3, T4, T5, T6, T7, T8>
where
    PW: ProjectWriter<K, T>,
{
    type Err = WriteProjectError<PW::Err>;

    async fn write_project(&self, project: &ProjectHandle<K, T>, path: impl AsRef<Path> + Send + Sync) -> Result<(), Self::Err> {
        self.project_writer.write_project(project, path.as_ref()).await.map_err(Into::into)
    }
}

#[async_trait]
impl<K, T: ParameterValueType, ID: Send + Sync, T1: Send + Sync, T2: Send + Sync, PM: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync> NewProjectUsecase<K, T> for MPDeltaCore<K, ID, T1, T2, PM, T4, T5, T6, T7, T8>
where
    ID: IdGenerator,
    PM: ProjectMemory<K, T>,
{
    async fn new_project(&self) -> ProjectHandle<K, T> {
        let project = Project::new_empty(self.id_generator.generate_new());
        let pointer = StaticPointerOwned::reference(&project).clone();
        self.project_memory.insert_new_project(None, project).await;
        pointer
    }
}

#[async_trait]
pub trait RootComponentClassMemory<K, T: ParameterValueType>: Send + Sync {
    async fn insert_new_root_component_class(&self, parent: Option<&ProjectHandle<K, T>>, root_component_class: RootComponentClassHandleOwned<K, T>);
    async fn set_parent(&self, root_component_class: &RootComponentClassHandle<K, T>, parent: Option<&ProjectHandle<K, T>>);
    async fn search_by_parent(&self, parent: &ProjectHandle<K, T>) -> Cow<[RootComponentClassHandle<K, T>]>;
    async fn get_parent_project(&self, path: &RootComponentClassHandle<K, T>) -> Option<ProjectHandle<K, T>>;
    async fn all_loaded_root_component_classes(&self) -> Cow<[RootComponentClassHandle<K, T>]>;
}

#[async_trait]
impl<K, T: ParameterValueType, ID: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, RM: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync> NewRootComponentClassUsecase<K, T> for MPDeltaCore<K, ID, T1, T2, T3, RM, T5, T6, T7, T8>
where
    ID: IdGenerator,
    RM: RootComponentClassMemory<K, T>,
{
    async fn new_root_component_class(&self) -> RootComponentClassHandle<K, T> {
        let root_component_class = RootComponentClass::new_empty(self.id_generator.generate_new());
        let pointer = StaticPointerOwned::reference(&root_component_class).clone();
        self.root_component_class_memory.insert_new_root_component_class(None, root_component_class).await;
        pointer
    }
}

#[async_trait]
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, RM: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync> SetOwnerForRootComponentClassUsecase<K, T> for MPDeltaCore<K, T0, T1, T2, T3, RM, T5, T6, T7, T8>
where
    RM: RootComponentClassMemory<K, T>,
{
    async fn set_owner_for_root_component_class(&self, component: &RootComponentClassHandle<K, T>, owner: &ProjectHandle<K, T>) {
        self.root_component_class_memory.set_parent(component, Some(owner)).await;
    }
}

#[async_trait]
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, PM: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync> GetLoadedProjectsUsecase<K, T> for MPDeltaCore<K, T0, T1, T2, PM, T4, T5, T6, T7, T8>
where
    PM: ProjectMemory<K, T>,
{
    async fn get_loaded_projects(&self) -> Cow<[ProjectHandle<K, T>]> {
        self.project_memory.all_loaded_projects().await
    }
}

#[async_trait]
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, RM: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync> GetRootComponentClassesUsecase<K, T> for MPDeltaCore<K, T0, T1, T2, T3, RM, T5, T6, T7, T8>
where
    RM: RootComponentClassMemory<K, T>,
{
    async fn get_root_component_classes(&self, project: &ProjectHandle<K, T>) -> Cow<[RootComponentClassHandle<K, T>]> {
        self.root_component_class_memory.search_by_parent(project).await
    }
}

#[async_trait]
pub trait ComponentClassLoader<K, T: ParameterValueType>: Send + Sync {
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<K, T>>>]>;
}

#[async_trait]
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, CL: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync> GetAvailableComponentClassesUsecase<K, T> for MPDeltaCore<K, T0, T1, T2, T3, T4, CL, T6, T7, T8>
where
    CL: ComponentClassLoader<K, T>,
{
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<K, T>>>]> {
        self.component_class_loader.get_available_component_classes().await
    }
}

#[async_trait]
pub trait ComponentRendererBuilder<K, T: ParameterValueType>: Send + Sync {
    type Err: Error + 'static;
    type Renderer: RealtimeComponentRenderer<T> + Send + Sync + 'static;
    async fn create_renderer(&self, component: &ComponentInstanceHandle<K, T>) -> Result<Self::Renderer, Self::Err>;
}

#[async_trait]
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, CR: Send + Sync, T7: Send + Sync, T8: Send + Sync> RealtimeRenderComponentUsecase<K, T> for MPDeltaCore<K, T0, T1, T2, T3, T4, T5, CR, T7, T8>
where
    ComponentInstanceHandle<K, T>: Sync,
    CR: ComponentRendererBuilder<K, T>,
{
    type Err = CR::Err;
    type Renderer = CR::Renderer;

    async fn render_component(&self, component: &ComponentInstanceHandle<K, T>) -> Result<Self::Renderer, Self::Err> {
        self.component_renderer_builder.create_renderer(component).await
    }
}

pub trait EditEventListener<K, T: ParameterValueType>: Send + Sync {
    fn on_edit(&self, target: &RootComponentClassHandle<K, T>, event: RootComponentEditEvent<K, T>);
    fn on_edit_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>, command: InstanceEditEvent<K, T>);
}

impl<K, T: ParameterValueType, O> EditEventListener<K, T> for O
where
    O: Deref + Send + Sync,
    O::Target: EditEventListener<K, T>,
{
    fn on_edit(&self, target: &RootComponentClassHandle<K, T>, event: RootComponentEditEvent<K, T>) {
        self.deref().on_edit(target, event)
    }

    fn on_edit_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>, command: InstanceEditEvent<K, T>) {
        self.deref().on_edit_instance(root, target, command)
    }
}

#[async_trait]
pub trait Editor<K, T: ParameterValueType>: Send + Sync {
    type Log: Send + Sync;
    type Err: Error + 'static;
    type EditEventListenerGuard: Send + Sync + 'static;
    fn add_edit_event_listener(&self, listener: impl EditEventListener<K, T> + 'static) -> Self::EditEventListenerGuard;
    async fn edit(&self, target: &RootComponentClassHandle<K, T>, command: RootComponentEditCommand<K, T>) -> Result<Self::Log, Self::Err>;
    async fn edit_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>, command: InstanceEditCommand<K, T>) -> Result<Self::Log, Self::Err>;
    async fn edit_reverse(&self, log: &Self::Log);
    async fn edit_by_log(&self, log: &Self::Log);
}

#[async_trait]
pub trait EditHistory<K, T: ParameterValueType, Log>: Send + Sync {
    async fn push_history(&self, root: &RootComponentClassHandle<K, T>, target: Option<&ComponentInstanceHandle<K, T>>, log: Log);
    async fn undo(&self, root: &RootComponentClassHandle<K, T>, target: Option<&ComponentInstanceHandle<K, T>>) -> Option<Arc<Log>>;
    async fn redo(&self, root: &RootComponentClassHandle<K, T>, target: Option<&ComponentInstanceHandle<K, T>>) -> Option<Arc<Log>>;
}

#[async_trait]
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, ED: Send + Sync, HS: Send + Sync> EditUsecase<K, T> for MPDeltaCore<K, T0, T1, T2, T3, T4, T5, T6, ED, HS>
where
    ComponentInstanceHandle<K, T>: Sync,
    ED: Editor<K, T>,
    HS: EditHistory<K, T, ED::Log>,
{
    type Err = ED::Err;

    async fn edit(&self, target: &RootComponentClassHandle<K, T>, command: RootComponentEditCommand<K, T>) -> Result<(), Self::Err> {
        let log = self.editor.edit(target, command).await?;
        self.edit_history.push_history(target, None, log).await;
        Ok(())
    }

    async fn edit_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>, command: InstanceEditCommand<K, T>) -> Result<(), Self::Err> {
        let log = self.editor.edit_instance(root, target, command).await?;
        self.edit_history.push_history(root, Some(target), log).await;
        Ok(())
    }
}

#[async_trait]
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, ED: Send + Sync, T8: Send + Sync> SubscribeEditEventUsecase<K, T> for MPDeltaCore<K, T0, T1, T2, T3, T4, T5, T6, ED, T8>
where
    ComponentInstanceHandle<K, T>: Sync,
    ED: Editor<K, T>,
{
    type EditEventListenerGuard = ED::EditEventListenerGuard;

    fn add_edit_event_listener(&self, listener: impl EditEventListener<K, T> + 'static) -> Self::EditEventListenerGuard {
        self.editor.add_edit_event_listener(listener)
    }
}

#[async_trait]
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, ED: Send + Sync, HS: Send + Sync> UndoUsecase<K, T> for MPDeltaCore<K, T0, T1, T2, T3, T4, T5, T6, ED, HS>
where
    ComponentInstanceHandle<K, T>: Sync,
    ED: Editor<K, T>,
    HS: EditHistory<K, T, ED::Log>,
{
    async fn undo(&self, component: &RootComponentClassHandle<K, T>) -> bool {
        if let Some(log) = self.edit_history.undo(component, None).await {
            self.editor.edit_reverse(&log).await;
            true
        } else {
            false
        }
    }

    async fn undo_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>) -> bool {
        if let Some(log) = self.edit_history.undo(root, Some(target)).await {
            self.editor.edit_reverse(&log).await;
            true
        } else {
            false
        }
    }
}

#[async_trait]
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, ED: Send + Sync, HS: Send + Sync> RedoUsecase<K, T> for MPDeltaCore<K, T0, T1, T2, T3, T4, T5, T6, ED, HS>
where
    ComponentInstanceHandle<K, T>: Sync,
    ED: Editor<K, T>,
    HS: EditHistory<K, T, ED::Log>,
{
    async fn redo(&self, component: &RootComponentClassHandle<K, T>) -> bool {
        if let Some(log) = self.edit_history.redo(component, None).await {
            self.editor.edit_by_log(&log).await;
            true
        } else {
            false
        }
    }

    async fn redo_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>) -> bool {
        if let Some(log) = self.edit_history.redo(root, Some(target)).await {
            self.editor.edit_by_log(&log).await;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::parameter::{Parameter, ParameterSelect};
    use std::path::PathBuf;
    use std::sync::atomic;
    use std::sync::atomic::{AtomicU64, AtomicUsize};

    #[derive(Debug, Error)]
    #[error("Error")]
    struct EmptyError;

    #[derive(Default)]
    struct ID(AtomicU64);

    impl IdGenerator for ID {
        fn generate_new(&self) -> Uuid {
            Uuid::from_u128(self.0.fetch_add(1, atomic::Ordering::SeqCst) as u128)
        }
    }

    #[derive(Debug)]
    struct EmptyParameterValueType;

    impl ParameterValueType for EmptyParameterValueType {
        type Image = ();
        type Audio = ();
        type Binary = ();
        type String = ();
        type Integer = ();
        type RealNumber = ();
        type Boolean = ();
        type Dictionary = ();
        type Array = ();
        type ComponentClass = ();
    }

    #[tokio::test]
    async fn load_project() {
        #[derive(Debug)]
        struct K;
        let key = Arc::new(RwLock::new(TCellOwner::<K>::new()));
        #[derive(Default)]
        struct PL1(RwLock<u128>);
        #[async_trait]
        impl ProjectLoader<K, EmptyParameterValueType> for PL1 {
            type Err = EmptyError;

            async fn load_project(&self, _: &Path) -> Result<StaticPointerOwned<RwLock<Project<K, EmptyParameterValueType>>>, Self::Err> {
                let mut guard = self.0.write().await;
                let id = *guard;
                *guard += 1;
                Ok(Project::new_empty(Uuid::from_u128(id)))
            }
        }
        #[derive(Default)]
        struct PM {
            memory: RwLock<Vec<(Option<PathBuf>, StaticPointerOwned<RwLock<Project<K, EmptyParameterValueType>>>)>>,
        }
        #[async_trait]
        impl ProjectMemory<K, EmptyParameterValueType> for PM {
            async fn insert_new_project(&self, path: Option<&Path>, project: StaticPointerOwned<RwLock<Project<K, EmptyParameterValueType>>>) {
                self.memory.write().await.push((path.map(Path::to_path_buf), project));
            }

            async fn get_loaded_project(&self, path: &Path) -> Option<StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>> {
                self.memory.read().await.iter().find(|(p, _)| p.as_deref() == Some(path)).map(|(_, p)| StaticPointerOwned::reference(p).clone())
            }

            async fn all_loaded_projects(&self) -> Cow<[StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>]> {
                unreachable!()
            }
        }
        let core = MPDeltaCore {
            id_generator: Arc::new(()),
            project_loader: Arc::new(PL1::default()),
            project_writer: Arc::new(()),
            project_memory: Arc::new(PM::default()),
            root_component_class_memory: Arc::new(()),
            component_class_loader: Arc::new(()),
            component_renderer_builder: Arc::new(()),
            editor: Arc::new(()),
            edit_history: Arc::new(()),
            key: Arc::clone(&key),
        };
        assert_eq!(*LoadProjectUsecase::load_project(&core, "1").await.unwrap().upgrade().unwrap().read().await, *Project::new_empty(Uuid::from_u128(0)).read().await);
        assert_eq!(*LoadProjectUsecase::load_project(&core, "1").await.unwrap().upgrade().unwrap().read().await, *Project::new_empty(Uuid::from_u128(0)).read().await);
        assert_eq!(*LoadProjectUsecase::load_project(&core, "3").await.unwrap().upgrade().unwrap().read().await, *Project::new_empty(Uuid::from_u128(1)).read().await);

        assert_eq!(*core.project_loader.0.read().await, 2);
        let memory = core.project_memory.memory.read().await;
        assert_eq!(memory.len(), 2);
        assert_eq!(memory[0].0.as_deref(), Some("1").map(AsRef::as_ref));
        assert_eq!(*memory[0].1.read().await, *Project::new_empty(Uuid::from_u128(0)).read().await);
        assert_eq!(memory[1].0.as_deref(), Some("3").map(AsRef::as_ref));
        assert_eq!(*memory[1].1.read().await, *Project::new_empty(Uuid::from_u128(1)).read().await);

        #[derive(Default)]
        struct PL2;
        #[async_trait]
        impl ProjectLoader<K, EmptyParameterValueType> for PL2 {
            type Err = EmptyError;

            async fn load_project(&self, _: &Path) -> Result<StaticPointerOwned<RwLock<Project<K, EmptyParameterValueType>>>, Self::Err> {
                Err(EmptyError)
            }
        }
        let core = MPDeltaCore {
            id_generator: Arc::new(()),
            project_loader: Arc::new(PL2),
            project_writer: Arc::new(()),
            project_memory: Arc::new(PM::default()),
            root_component_class_memory: Arc::new(()),
            component_class_loader: Arc::new(()),
            component_renderer_builder: Arc::new(()),
            editor: Arc::new(()),
            edit_history: Arc::new(()),
            key: Arc::clone(&key),
        };
        assert!(LoadProjectUsecase::load_project(&core, "1").await.is_err());
    }

    #[tokio::test]
    async fn write_project() {
        #[derive(Debug)]
        struct K;
        let key = Arc::new(RwLock::new(TCellOwner::<K>::new()));
        #[derive(Default)]
        struct PW1(AtomicUsize);
        #[async_trait]
        impl ProjectWriter<K, EmptyParameterValueType> for PW1 {
            type Err = EmptyError;

            async fn write_project(&self, _: &StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>, _: &Path) -> Result<(), Self::Err> {
                self.0.fetch_add(1, atomic::Ordering::SeqCst);
                Ok(())
            }
        }
        let core = MPDeltaCore {
            id_generator: Arc::new(()),
            project_loader: Arc::new(()),
            project_writer: Arc::new(PW1::default()),
            project_memory: Arc::new(()),
            root_component_class_memory: Arc::new(()),
            component_class_loader: Arc::new(()),
            component_renderer_builder: Arc::new(()),
            editor: Arc::new(()),
            edit_history: Arc::new(()),
            key: Arc::clone(&key),
        };
        let project = Project::new_empty(Uuid::nil());
        let project = StaticPointerOwned::reference(&project).clone();
        assert!(WriteProjectUsecase::write_project(&core, &project, "").await.is_ok());
        assert_eq!(core.project_writer.0.load(atomic::Ordering::SeqCst), 1);
        assert!(WriteProjectUsecase::write_project(&core, &project, "").await.is_ok());
        assert_eq!(core.project_writer.0.load(atomic::Ordering::SeqCst), 2);
        assert!(WriteProjectUsecase::write_project(&core, &project, "").await.is_ok());
        assert_eq!(core.project_writer.0.load(atomic::Ordering::SeqCst), 3);
        #[derive(Default)]
        struct PW2(AtomicUsize);
        #[async_trait]
        impl ProjectWriter<K, EmptyParameterValueType> for PW2 {
            type Err = EmptyError;

            async fn write_project(&self, _: &StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>, _: &Path) -> Result<(), Self::Err> {
                self.0.fetch_add(1, atomic::Ordering::SeqCst);
                Err(EmptyError)
            }
        }
        let core = MPDeltaCore {
            id_generator: Arc::new(()),
            project_loader: Arc::new(()),
            project_writer: Arc::new(PW2::default()),
            project_memory: Arc::new(()),
            root_component_class_memory: Arc::new(()),
            component_class_loader: Arc::new(()),
            component_renderer_builder: Arc::new(()),
            editor: Arc::new(()),
            edit_history: Arc::new(()),
            key: Arc::clone(&key),
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
        #[derive(Debug)]
        struct K;
        let key = Arc::new(RwLock::new(TCellOwner::<K>::new()));
        #[derive(Default)]
        struct PM(RwLock<Vec<(Option<PathBuf>, StaticPointerOwned<RwLock<Project<K, EmptyParameterValueType>>>)>>);
        #[async_trait]
        impl ProjectMemory<K, EmptyParameterValueType> for PM {
            async fn insert_new_project(&self, path: Option<&Path>, project: StaticPointerOwned<RwLock<Project<K, EmptyParameterValueType>>>) {
                self.0.write().await.push((path.map(Path::to_path_buf), project));
            }

            async fn get_loaded_project(&self, _: &Path) -> Option<StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>> {
                unreachable!()
            }

            async fn all_loaded_projects(&self) -> Cow<[StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>]> {
                unreachable!()
            }
        }
        let core = MPDeltaCore {
            id_generator: Arc::new(ID::default()),
            project_loader: Arc::new(()),
            project_writer: Arc::new(()),
            project_memory: Arc::new(PM::default()),
            root_component_class_memory: Arc::new(()),
            component_class_loader: Arc::new(()),
            component_renderer_builder: Arc::new(()),
            editor: Arc::new(()),
            edit_history: Arc::new(()),
            key: Arc::clone(&key),
        };
        assert_eq!(*NewProjectUsecase::new_project(&core).await.upgrade().unwrap().read().await, *Project::new_empty(Uuid::from_u128(0)).read().await);
        assert_eq!(*NewProjectUsecase::new_project(&core).await.upgrade().unwrap().read().await, *Project::new_empty(Uuid::from_u128(1)).read().await);
        assert_eq!(*NewProjectUsecase::new_project(&core).await.upgrade().unwrap().read().await, *Project::new_empty(Uuid::from_u128(2)).read().await);
        assert_eq!(core.id_generator.0.load(atomic::Ordering::SeqCst), 3);
        let guard = core.project_memory.0.read().await;
        assert_eq!(guard.len(), 3);
        assert_eq!(guard[0].0, None);
        assert_eq!(*guard[0].1.read().await, *Project::new_empty(Uuid::from_u128(0)).read().await);
        assert_eq!(guard[1].0, None);
        assert_eq!(*guard[1].1.read().await, *Project::new_empty(Uuid::from_u128(1)).read().await);
        assert_eq!(guard[2].0, None);
        assert_eq!(*guard[2].1.read().await, *Project::new_empty(Uuid::from_u128(2)).read().await);
    }

    #[tokio::test]
    async fn new_root_component_class() {
        #[derive(Debug)]
        struct K;
        let key = Arc::new(RwLock::new(TCellOwner::<K>::new()));
        #[derive(Default)]
        struct RM(RwLock<Vec<(Option<StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>>, StaticPointerOwned<RwLock<RootComponentClass<K, EmptyParameterValueType>>>)>>);
        #[async_trait]
        impl RootComponentClassMemory<K, EmptyParameterValueType> for RM {
            async fn insert_new_root_component_class(&self, parent: Option<&StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>>, root_component_class: StaticPointerOwned<RwLock<RootComponentClass<K, EmptyParameterValueType>>>) {
                self.0.write().await.push((parent.cloned(), root_component_class));
            }

            async fn set_parent(&self, _: &StaticPointer<RwLock<RootComponentClass<K, EmptyParameterValueType>>>, _: Option<&StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>>) {
                unreachable!()
            }

            async fn search_by_parent(&self, _: &StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>) -> Cow<[StaticPointer<RwLock<RootComponentClass<K, EmptyParameterValueType>>>]> {
                unreachable!()
            }

            async fn get_parent_project(&self, _: &StaticPointer<RwLock<RootComponentClass<K, EmptyParameterValueType>>>) -> Option<StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>> {
                unreachable!()
            }

            async fn all_loaded_root_component_classes(&self) -> Cow<[StaticPointer<RwLock<RootComponentClass<K, EmptyParameterValueType>>>]> {
                unreachable!()
            }
        }
        let core = MPDeltaCore {
            id_generator: Arc::new(ID::default()),
            project_loader: Arc::new(()),
            project_writer: Arc::new(()),
            project_memory: Arc::new(()),
            root_component_class_memory: Arc::new(RM::default()),
            component_class_loader: Arc::new(()),
            component_renderer_builder: Arc::new(()),
            editor: Arc::new(()),
            edit_history: Arc::new(()),
            key: Arc::clone(&key),
        };
        assert_eq!(*NewRootComponentClassUsecase::new_root_component_class(&core).await.upgrade().unwrap().read().await, *RootComponentClass::new_empty(Uuid::from_u128(0)).read().await);
        assert_eq!(*NewRootComponentClassUsecase::new_root_component_class(&core).await.upgrade().unwrap().read().await, *RootComponentClass::new_empty(Uuid::from_u128(1)).read().await);
        assert_eq!(*NewRootComponentClassUsecase::new_root_component_class(&core).await.upgrade().unwrap().read().await, *RootComponentClass::new_empty(Uuid::from_u128(2)).read().await);
        assert_eq!(core.id_generator.0.load(atomic::Ordering::SeqCst), 3);
        let guard = core.root_component_class_memory.0.read().await;
        assert_eq!(guard.len(), 3);
        assert_eq!(guard[0].0, None);
        assert_eq!(*guard[0].1.read().await, *RootComponentClass::new_empty(Uuid::from_u128(0)).read().await);
        assert_eq!(guard[1].0, None);
        assert_eq!(*guard[1].1.read().await, *RootComponentClass::new_empty(Uuid::from_u128(1)).read().await);
        assert_eq!(guard[2].0, None);
        assert_eq!(*guard[2].1.read().await, *RootComponentClass::new_empty(Uuid::from_u128(2)).read().await);
    }

    #[tokio::test]
    async fn set_owner_for_root_component_class() {
        #[derive(Debug)]
        struct K;
        let key = Arc::new(RwLock::new(TCellOwner::<K>::new()));
        #[derive(Default)]
        struct RM(RwLock<Vec<(Option<StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>>, StaticPointerOwned<RwLock<RootComponentClass<K, EmptyParameterValueType>>>)>>);
        #[async_trait]
        impl RootComponentClassMemory<K, EmptyParameterValueType> for RM {
            async fn insert_new_root_component_class(&self, _: Option<&StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>>, _: StaticPointerOwned<RwLock<RootComponentClass<K, EmptyParameterValueType>>>) {
                unreachable!()
            }

            async fn set_parent(&self, root_component_class: &StaticPointer<RwLock<RootComponentClass<K, EmptyParameterValueType>>>, parent: Option<&StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>>) {
                if let Some((p, _)) = self.0.write().await.iter_mut().find(|(_, c)| c == root_component_class) {
                    *p = parent.cloned();
                }
            }

            async fn search_by_parent(&self, _: &StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>) -> Cow<[StaticPointer<RwLock<RootComponentClass<K, EmptyParameterValueType>>>]> {
                unreachable!()
            }

            async fn get_parent_project(&self, _: &StaticPointer<RwLock<RootComponentClass<K, EmptyParameterValueType>>>) -> Option<StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>> {
                unreachable!()
            }

            async fn all_loaded_root_component_classes(&self) -> Cow<[StaticPointer<RwLock<RootComponentClass<K, EmptyParameterValueType>>>]> {
                unreachable!()
            }
        }
        let project0 = Project::new_empty(Uuid::from_u128(0));
        let project1 = Project::new_empty(Uuid::from_u128(0));
        let project2 = Project::new_empty(Uuid::from_u128(0));
        let c0 = RootComponentClass::new_empty(Uuid::from_u128(0));
        let c1 = RootComponentClass::new_empty(Uuid::from_u128(1));
        let c2 = RootComponentClass::new_empty(Uuid::from_u128(2));
        let component0 = StaticPointerOwned::reference(&c0).clone();
        let component1 = StaticPointerOwned::reference(&c1).clone();
        let component2 = StaticPointerOwned::reference(&c2).clone();
        let core = MPDeltaCore {
            id_generator: Arc::new(()),
            project_loader: Arc::new(()),
            project_writer: Arc::new(()),
            project_memory: Arc::new(()),
            root_component_class_memory: Arc::new(RM(RwLock::new(vec![(None, c0), (Some(StaticPointerOwned::reference(&project1).clone()), c1), (Some(StaticPointerOwned::reference(&project0).clone()), c2)]))),
            component_class_loader: Arc::new(()),
            component_renderer_builder: Arc::new(()),
            editor: Arc::new(()),
            edit_history: Arc::new(()),
            key: Arc::clone(&key),
        };

        SetOwnerForRootComponentClassUsecase::set_owner_for_root_component_class(&core, &component0, StaticPointerOwned::reference(&project0)).await;
        SetOwnerForRootComponentClassUsecase::set_owner_for_root_component_class(&core, &component1, StaticPointerOwned::reference(&project1)).await;
        SetOwnerForRootComponentClassUsecase::set_owner_for_root_component_class(&core, &component2, StaticPointerOwned::reference(&project2)).await;
        let guard = core.root_component_class_memory.0.read().await;
        assert_eq!(guard.len(), 3);
        assert_eq!(guard[0].0, Some(StaticPointerOwned::reference(&project0).clone()));
        assert_eq!(guard[0].1, component0);
        assert_eq!(guard[1].0, Some(StaticPointerOwned::reference(&project1).clone()));
        assert_eq!(guard[1].1, component1);
        assert_eq!(guard[2].0, Some(StaticPointerOwned::reference(&project2).clone()));
        assert_eq!(guard[2].1, component2);
    }

    #[tokio::test]
    async fn get_loaded_projects() {
        #[derive(Debug)]
        struct K;
        let key = Arc::new(RwLock::new(TCellOwner::<K>::new()));
        struct PM(RwLock<Vec<StaticPointerOwned<RwLock<Project<K, EmptyParameterValueType>>>>>);
        #[async_trait]
        impl ProjectMemory<K, EmptyParameterValueType> for PM {
            async fn insert_new_project(&self, _: Option<&Path>, _: StaticPointerOwned<RwLock<Project<K, EmptyParameterValueType>>>) {
                unreachable!()
            }

            async fn get_loaded_project(&self, _: &Path) -> Option<StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>> {
                unreachable!()
            }

            async fn all_loaded_projects(&self) -> Cow<[StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>]> {
                Cow::Owned(self.0.read().await.iter().map(StaticPointerOwned::reference).cloned().collect())
            }
        }
        let core = MPDeltaCore {
            id_generator: Arc::new(()),
            project_loader: Arc::new(()),
            project_writer: Arc::new(()),
            project_memory: Arc::new(PM(RwLock::new(vec![Project::new_empty(Uuid::from_u128(0)), Project::new_empty(Uuid::from_u128(1)), Project::new_empty(Uuid::from_u128(2))]))),
            root_component_class_memory: Arc::new(()),
            component_class_loader: Arc::new(()),
            component_renderer_builder: Arc::new(()),
            editor: Arc::new(()),
            edit_history: Arc::new(()),
            key: Arc::clone(&key),
        };
        let projects = GetLoadedProjectsUsecase::get_loaded_projects(&core).await;
        assert_eq!(projects.len(), 3);
        assert_eq!(*projects[0].upgrade().unwrap().read().await, *Project::new_empty(Uuid::from_u128(0)).read().await);
        assert_eq!(*projects[1].upgrade().unwrap().read().await, *Project::new_empty(Uuid::from_u128(1)).read().await);
        assert_eq!(*projects[2].upgrade().unwrap().read().await, *Project::new_empty(Uuid::from_u128(2)).read().await);
    }

    #[tokio::test]
    async fn get_root_component_classes() {
        #[derive(Debug)]
        struct K;
        let key = Arc::new(RwLock::new(TCellOwner::<K>::new()));
        struct RM(RwLock<Vec<(Option<StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>>, StaticPointerOwned<RwLock<RootComponentClass<K, EmptyParameterValueType>>>)>>);
        #[async_trait]
        impl RootComponentClassMemory<K, EmptyParameterValueType> for RM {
            async fn insert_new_root_component_class(&self, _: Option<&StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>>, _: StaticPointerOwned<RwLock<RootComponentClass<K, EmptyParameterValueType>>>) {
                unreachable!()
            }

            async fn set_parent(&self, _: &StaticPointer<RwLock<RootComponentClass<K, EmptyParameterValueType>>>, _: Option<&StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>>) {
                unreachable!()
            }

            async fn search_by_parent(&self, parent: &StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>) -> Cow<[StaticPointer<RwLock<RootComponentClass<K, EmptyParameterValueType>>>]> {
                Cow::Owned(self.0.read().await.iter().filter(|(p, _)| p.as_ref() == Some(parent)).map(|(_, c)| StaticPointerOwned::reference(c).clone()).collect())
            }

            async fn get_parent_project(&self, _: &StaticPointer<RwLock<RootComponentClass<K, EmptyParameterValueType>>>) -> Option<StaticPointer<RwLock<Project<K, EmptyParameterValueType>>>> {
                unreachable!()
            }

            async fn all_loaded_root_component_classes(&self) -> Cow<[StaticPointer<RwLock<RootComponentClass<K, EmptyParameterValueType>>>]> {
                unreachable!()
            }
        }
        let project0 = Project::new_empty(Uuid::from_u128(0));
        let project1 = Project::new_empty(Uuid::from_u128(0));
        let project2 = Project::new_empty(Uuid::from_u128(0));
        let core = MPDeltaCore {
            id_generator: Arc::new(()),
            project_loader: Arc::new(()),
            project_writer: Arc::new(()),
            project_memory: Arc::new(()),
            root_component_class_memory: Arc::new(RM(RwLock::new(vec![
                (None, RootComponentClass::new_empty(Uuid::from_u128(0))),
                (Some(StaticPointerOwned::reference(&project0).clone()), RootComponentClass::new_empty(Uuid::from_u128(0))),
                (Some(StaticPointerOwned::reference(&project0).clone()), RootComponentClass::new_empty(Uuid::from_u128(1))),
                (Some(StaticPointerOwned::reference(&project1).clone()), RootComponentClass::new_empty(Uuid::from_u128(2))),
                (Some(StaticPointerOwned::reference(&project2).clone()), RootComponentClass::new_empty(Uuid::from_u128(3))),
            ]))),
            component_class_loader: Arc::new(()),
            component_renderer_builder: Arc::new(()),
            editor: Arc::new(()),
            edit_history: Arc::new(()),
            key: Arc::clone(&key),
        };
        let child0 = GetRootComponentClassesUsecase::get_root_component_classes(&core, StaticPointerOwned::reference(&project0)).await;
        assert_eq!(child0.len(), 2);
        assert_eq!(*child0[0].upgrade().unwrap().read().await, *RootComponentClass::new_empty(Uuid::from_u128(0)).read().await);
        assert_eq!(*child0[1].upgrade().unwrap().read().await, *RootComponentClass::new_empty(Uuid::from_u128(1)).read().await);
        let child1 = GetRootComponentClassesUsecase::get_root_component_classes(&core, StaticPointerOwned::reference(&project1)).await;
        assert_eq!(*child1[0].upgrade().unwrap().read().await, *RootComponentClass::new_empty(Uuid::from_u128(2)).read().await);
        let child2 = GetRootComponentClassesUsecase::get_root_component_classes(&core, StaticPointerOwned::reference(&project2)).await;
        assert_eq!(*child2[0].upgrade().unwrap().read().await, *RootComponentClass::new_empty(Uuid::from_u128(3)).read().await);
    }

    #[tokio::test]
    async fn get_available_component_classes() {
        #[derive(Debug)]
        struct K;
        let key = Arc::new(RwLock::new(TCellOwner::<K>::new()));
        struct CL;
        #[async_trait]
        impl ComponentClassLoader<K, EmptyParameterValueType> for CL {
            async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<K, EmptyParameterValueType>>>]> {
                Cow::Owned(vec![])
            }
        }
        let core = MPDeltaCore {
            id_generator: Arc::new(()),
            project_loader: Arc::new(()),
            project_writer: Arc::new(()),
            project_memory: Arc::new(()),
            root_component_class_memory: Arc::new(()),
            component_class_loader: Arc::new(CL),
            component_renderer_builder: Arc::new(()),
            editor: Arc::new(()),
            edit_history: Arc::new(()),
            key: Arc::clone(&key),
        };
        assert_eq!(GetAvailableComponentClassesUsecase::get_available_component_classes(&core).await.len(), 0);
    }

    #[tokio::test]
    async fn realtime_render_component() {
        #[derive(Debug)]
        struct K;
        let key = Arc::new(RwLock::new(TCellOwner::<K>::new()));

        struct RD;
        #[async_trait]
        impl RealtimeComponentRenderer<EmptyParameterValueType> for RD {
            type Err = std::convert::Infallible;
            fn get_frame_count(&self) -> usize {
                unreachable!()
            }

            fn render_frame(&self, _: usize) -> Result<(), Self::Err> {
                unreachable!()
            }

            fn sampling_rate(&self) -> u32 {
                unreachable!()
            }

            fn mix_audio(&self, _: usize, _: usize) -> Result<(), Self::Err> {
                unreachable!()
            }

            fn render_param(&self, _param: Parameter<ParameterSelect>) -> Result<Parameter<EmptyParameterValueType>, Self::Err> {
                unreachable!()
            }
        }
        struct CR;
        #[async_trait]
        impl ComponentRendererBuilder<K, EmptyParameterValueType> for CR {
            type Err = std::convert::Infallible;
            type Renderer = RD;

            async fn create_renderer(&self, _: &ComponentInstanceHandle<K, EmptyParameterValueType>) -> Result<Self::Renderer, Self::Err> {
                Ok(RD)
            }
        }
        let core = MPDeltaCore {
            id_generator: Arc::new(()),
            project_loader: Arc::new(()),
            project_writer: Arc::new(()),
            project_memory: Arc::new(()),
            root_component_class_memory: Arc::new(()),
            component_class_loader: Arc::new(()),
            component_renderer_builder: Arc::new(CR),
            editor: Arc::new(()),
            edit_history: Arc::new(()),
            key: Arc::clone(&key),
        };
        let _: RD = RealtimeRenderComponentUsecase::render_component(&core, &StaticPointer::new()).await.unwrap();
    }
}
