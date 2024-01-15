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
use std::future::Future;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct MPDeltaCore<K: 'static, IdGenerator, ProjectLoader, ProjectWriter, ProjectMemory, RootComponentClassMemory, ComponentClassLoader, ComponentRendererBuilder, VideoEncoder, Editor, EditHistory> {
    id_generator: Arc<IdGenerator>,
    project_loader: Arc<ProjectLoader>,
    project_writer: Arc<ProjectWriter>,
    project_memory: Arc<ProjectMemory>,
    root_component_class_memory: Arc<RootComponentClassMemory>,
    component_class_loader: Arc<ComponentClassLoader>,
    component_renderer_builder: Arc<ComponentRendererBuilder>,
    video_encoder: Arc<VideoEncoder>,
    editor: Arc<Editor>,
    edit_history: Arc<EditHistory>,
    key: Arc<RwLock<TCellOwner<K>>>,
}

impl<K, IdGenerator: Debug, ProjectLoader: Debug, ProjectWriter: Debug, ProjectMemory: Debug, RootComponentClassMemory: Debug, ComponentClassLoader: Debug, ComponentRendererBuilder: Debug, VideoEncoder: Debug, Editor: Debug, EditHistory: Debug> Debug
    for MPDeltaCore<K, IdGenerator, ProjectLoader, ProjectWriter, ProjectMemory, RootComponentClassMemory, ComponentClassLoader, ComponentRendererBuilder, VideoEncoder, Editor, EditHistory>
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
            .field("video_encoder", &self.video_encoder)
            .field("editor", &self.editor)
            .field("edit_history", &self.edit_history)
            .finish_non_exhaustive()
    }
}

impl<K, IdGenerator, ProjectLoader, ProjectWriter, ProjectMemory, RootComponentClassMemory, ComponentClassLoader, ComponentRendererBuilder, VideoEncoder, Editor, EditHistory>
    MPDeltaCore<K, IdGenerator, ProjectLoader, ProjectWriter, ProjectMemory, RootComponentClassMemory, ComponentClassLoader, ComponentRendererBuilder, VideoEncoder, Editor, EditHistory>
{
    pub fn new(
        id_generator: Arc<IdGenerator>,
        project_loader: Arc<ProjectLoader>,
        project_writer: Arc<ProjectWriter>,
        project_memory: Arc<ProjectMemory>,
        root_component_class_memory: Arc<RootComponentClassMemory>,
        component_class_loader: Arc<ComponentClassLoader>,
        component_renderer_builder: Arc<ComponentRendererBuilder>,
        video_encoder: Arc<VideoEncoder>,
        editor: Arc<Editor>,
        edit_history: Arc<EditHistory>,
        key: Arc<RwLock<TCellOwner<K>>>,
    ) -> MPDeltaCore<K, IdGenerator, ProjectLoader, ProjectWriter, ProjectMemory, RootComponentClassMemory, ComponentClassLoader, ComponentRendererBuilder, VideoEncoder, Editor, EditHistory> {
        MPDeltaCore {
            id_generator,
            project_loader,
            project_writer,
            project_memory,
            root_component_class_memory,
            component_class_loader,
            component_renderer_builder,
            video_encoder,
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
    type Err: Error + Send + 'static;
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
impl<K, T: ParameterValueType, T0: Send + Sync, PL: Send + Sync, T2: Send + Sync, PM: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync, T9: Send + Sync> LoadProjectUsecase<K, T> for MPDeltaCore<K, T0, PL, T2, PM, T4, T5, T6, T7, T8, T9>
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
    type Err: Error + Send + 'static;
    async fn write_project(&self, project: &ProjectHandle<K, T>, path: &Path) -> Result<(), Self::Err>;
}

#[derive(Debug, Error)]
pub enum WriteProjectError<PWErr> {
    #[error("error from ProjectWriter: {0}")]
    ProjectWriterError(#[from] PWErr),
}

#[async_trait]
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, PW: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync, T9: Send + Sync> WriteProjectUsecase<K, T> for MPDeltaCore<K, T0, T1, PW, T3, T4, T5, T6, T7, T8, T9>
where
    PW: ProjectWriter<K, T>,
{
    type Err = WriteProjectError<PW::Err>;

    async fn write_project(&self, project: &ProjectHandle<K, T>, path: impl AsRef<Path> + Send + Sync) -> Result<(), Self::Err> {
        self.project_writer.write_project(project, path.as_ref()).await.map_err(Into::into)
    }
}

#[async_trait]
impl<K, T: ParameterValueType, ID: Send + Sync, T1: Send + Sync, T2: Send + Sync, PM: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync, T9: Send + Sync> NewProjectUsecase<K, T> for MPDeltaCore<K, ID, T1, T2, PM, T4, T5, T6, T7, T8, T9>
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
impl<K, T: ParameterValueType, ID: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, RM: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync, T9: Send + Sync> NewRootComponentClassUsecase<K, T> for MPDeltaCore<K, ID, T1, T2, T3, RM, T5, T6, T7, T8, T9>
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
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, RM: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync, T9: Send + Sync> SetOwnerForRootComponentClassUsecase<K, T>
    for MPDeltaCore<K, T0, T1, T2, T3, RM, T5, T6, T7, T8, T9>
where
    RM: RootComponentClassMemory<K, T>,
{
    async fn set_owner_for_root_component_class(&self, component: &RootComponentClassHandle<K, T>, owner: &ProjectHandle<K, T>) {
        self.root_component_class_memory.set_parent(component, Some(owner)).await;
    }
}

#[async_trait]
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, PM: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync, T9: Send + Sync> GetLoadedProjectsUsecase<K, T> for MPDeltaCore<K, T0, T1, T2, PM, T4, T5, T6, T7, T8, T9>
where
    PM: ProjectMemory<K, T>,
{
    async fn get_loaded_projects(&self) -> Cow<[ProjectHandle<K, T>]> {
        self.project_memory.all_loaded_projects().await
    }
}

#[async_trait]
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, RM: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync, T9: Send + Sync> GetRootComponentClassesUsecase<K, T> for MPDeltaCore<K, T0, T1, T2, T3, RM, T5, T6, T7, T8, T9>
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
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, CL: Send + Sync, T6: Send + Sync, T7: Send + Sync, T8: Send + Sync, T9: Send + Sync> GetAvailableComponentClassesUsecase<K, T>
    for MPDeltaCore<K, T0, T1, T2, T3, T4, CL, T6, T7, T8, T9>
where
    CL: ComponentClassLoader<K, T>,
{
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<K, T>>>]> {
        self.component_class_loader.get_available_component_classes().await
    }
}

#[async_trait]
pub trait ComponentRendererBuilder<K, T: ParameterValueType>: Send + Sync {
    type Err: Error + Send + 'static;
    type Renderer: RealtimeComponentRenderer<T> + Send + Sync + 'static;
    async fn create_renderer(&self, component: &ComponentInstanceHandle<K, T>) -> Result<Self::Renderer, Self::Err>;
}

#[async_trait]
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, CR: Send + Sync, T7: Send + Sync, T8: Send + Sync, T9: Send + Sync> RealtimeRenderComponentUsecase<K, T> for MPDeltaCore<K, T0, T1, T2, T3, T4, T5, CR, T7, T8, T9>
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

pub trait ComponentEncoder<K, T: ParameterValueType, Encoder>: Send + Sync {
    type Err: Error + Send + 'static;
    fn render_and_encode<'life0, 'life1, 'async_trait>(&'life0 self, component: &'life1 ComponentInstanceHandle<K, T>, encoder: Encoder) -> impl Future<Output = Result<(), Self::Err>> + Send + 'async_trait
    where
        'life0: 'async_trait,
        'life1: 'async_trait;
}

impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, VE: Send + Sync, T8: Send + Sync, T9: Send + Sync, Encoder> RenderWholeComponentUsecase<K, T, Encoder>
    for MPDeltaCore<K, T0, T1, T2, T3, T4, T5, T6, VE, T8, T9>
where
    VE: ComponentEncoder<K, T, Encoder>,
{
    type Err = VE::Err;

    fn render_and_encode<'life0, 'life1, 'async_trait>(&'life0 self, component: &'life1 ComponentInstanceHandle<K, T>, encoder: Encoder) -> impl Future<Output = Result<(), Self::Err>> + Send + 'async_trait
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        self.video_encoder.render_and_encode(component, encoder)
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
    type Err: Error + Send + 'static;
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
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, ED: Send + Sync, HS: Send + Sync> EditUsecase<K, T> for MPDeltaCore<K, T0, T1, T2, T3, T4, T5, T6, T7, ED, HS>
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
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, ED: Send + Sync, T9: Send + Sync> SubscribeEditEventUsecase<K, T> for MPDeltaCore<K, T0, T1, T2, T3, T4, T5, T6, T7, ED, T9>
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
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, ED: Send + Sync, HS: Send + Sync> UndoUsecase<K, T> for MPDeltaCore<K, T0, T1, T2, T3, T4, T5, T6, T7, ED, HS>
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
impl<K, T: ParameterValueType, T0: Send + Sync, T1: Send + Sync, T2: Send + Sync, T3: Send + Sync, T4: Send + Sync, T5: Send + Sync, T6: Send + Sync, T7: Send + Sync, ED: Send + Sync, HS: Send + Sync> RedoUsecase<K, T> for MPDeltaCore<K, T0, T1, T2, T3, T4, T5, T6, T7, ED, HS>
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
