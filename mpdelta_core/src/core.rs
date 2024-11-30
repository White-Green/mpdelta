use crate::component::class::{ComponentClass, ComponentClassIdentifier};
use crate::component::instance::{ComponentInstance, ComponentInstanceId};
use crate::component::parameter::value::{DynEditableEasingValueIdentifier, DynEditableEasingValueManager, DynEditableSingleValueIdentifier, DynEditableSingleValueManager, Easing, EasingIdentifier};
use crate::component::parameter::ParameterValueType;
use crate::edit::{InstanceEditCommand, InstanceEditEvent, RootComponentEditCommand, RootComponentEditEvent};
use crate::project::{Project, ProjectHandle, ProjectHandleOwned, RootComponentClass, RootComponentClassHandle, RootComponentClassHandleOwned};
use crate::ptr::{StaticPointer, StaticPointerOwned};
use crate::usecase::*;
use async_trait::async_trait;
use std::borrow::Cow;
use std::error::Error;
use std::fmt::Debug;
use std::future::Future;
use std::io::{Read, Write};
use std::ops::Deref;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug)]
pub struct MPDeltaCore<IdGenerator, ProjectSerializer, ProjectLoader, ProjectWriter, ProjectMemory, RootComponentClassMemory, ComponentClassLoader, ComponentRendererBuilder, VideoEncoder, Editor, EditHistory> {
    id_generator: Arc<IdGenerator>,
    project_serializer: Arc<ProjectSerializer>,
    project_loader: Arc<ProjectLoader>,
    project_writer: Arc<ProjectWriter>,
    project_memory: Arc<ProjectMemory>,
    root_component_class_memory: Arc<RootComponentClassMemory>,
    component_class_loader: Arc<ComponentClassLoader>,
    component_renderer_builder: Arc<ComponentRendererBuilder>,
    video_encoder: Arc<VideoEncoder>,
    editor: Arc<Editor>,
    edit_history: Arc<EditHistory>,
}

pub trait NewWithArgs {
    type Args;
    fn new(args: Self::Args) -> Self;
}

pub struct MPDeltaCoreArgs<IdGenerator, ProjectSerializer, ProjectLoader, ProjectWriter, ProjectMemory, RootComponentClassMemory, ComponentClassLoader, ComponentRendererBuilder, VideoEncoder, Editor, EditHistory> {
    pub id_generator: Arc<IdGenerator>,
    pub project_serializer: Arc<ProjectSerializer>,
    pub project_loader: Arc<ProjectLoader>,
    pub project_writer: Arc<ProjectWriter>,
    pub project_memory: Arc<ProjectMemory>,
    pub root_component_class_memory: Arc<RootComponentClassMemory>,
    pub component_class_loader: Arc<ComponentClassLoader>,
    pub component_renderer_builder: Arc<ComponentRendererBuilder>,
    pub video_encoder: Arc<VideoEncoder>,
    pub editor: Arc<Editor>,
    pub edit_history: Arc<EditHistory>,
}

impl<IdGenerator, ProjectSerializer, ProjectLoader, ProjectWriter, ProjectMemory, RootComponentClassMemory, ComponentClassLoader, ComponentRendererBuilder, VideoEncoder, Editor, EditHistory> NewWithArgs
    for MPDeltaCore<IdGenerator, ProjectSerializer, ProjectLoader, ProjectWriter, ProjectMemory, RootComponentClassMemory, ComponentClassLoader, ComponentRendererBuilder, VideoEncoder, Editor, EditHistory>
{
    type Args = MPDeltaCoreArgs<IdGenerator, ProjectSerializer, ProjectLoader, ProjectWriter, ProjectMemory, RootComponentClassMemory, ComponentClassLoader, ComponentRendererBuilder, VideoEncoder, Editor, EditHistory>;
    fn new(
        MPDeltaCoreArgs {
            id_generator,
            project_serializer,
            project_loader,
            project_writer,
            project_memory,
            root_component_class_memory,
            component_class_loader,
            component_renderer_builder,
            video_encoder,
            editor,
            edit_history,
        }: Self::Args,
    ) -> Self {
        MPDeltaCore {
            id_generator,
            project_serializer,
            project_loader,
            project_writer,
            project_memory,
            root_component_class_memory,
            component_class_loader,
            component_renderer_builder,
            video_encoder,
            editor,
            edit_history,
        }
    }
}

pub trait IdGenerator: Send + Sync {
    fn generate_new(&self) -> Uuid;
}

impl<O> IdGenerator for O
where
    O: Deref + Send + Sync,
    O::Target: IdGenerator,
{
    fn generate_new(&self) -> Uuid {
        self.deref().generate_new()
    }
}

#[async_trait]
pub trait ProjectSerializer<T: ParameterValueType>: Send + Sync {
    type SerializeError: Error + Send + 'static;
    type DeserializeError: Error + Send + 'static;
    async fn serialize_project(&self, project: &ProjectHandle<T>, out: impl Write + Send) -> Result<(), Self::SerializeError>;
    async fn deserialize_project(&self, data: impl Read + Send) -> Result<ProjectHandleOwned<T>, Self::DeserializeError>;
}

#[async_trait]
pub trait ProjectLoader<T: ParameterValueType>: Send + Sync {
    type Err: Error + Send + 'static;
    type ProjectRead<'a>: Read + Send + 'a
    where
        Self: 'a;
    async fn load_project<'a>(&'a self, path: &Path) -> Result<Self::ProjectRead<'a>, Self::Err>;
}

#[async_trait]
pub trait ProjectMemory<T: ParameterValueType>: Send + Sync {
    fn default_project(&self) -> ProjectHandle<T>;
    async fn contains(&self, path: &Path) -> bool {
        self.get_loaded_project(path).await.is_some()
    }
    async fn insert_new_project(&self, path: Option<&Path>, project: ProjectHandleOwned<T>);
    async fn get_loaded_project(&self, path: &Path) -> Option<ProjectHandle<T>>;
    async fn all_loaded_projects(&self) -> Cow<'_, [ProjectHandle<T>]>;
}

#[derive(Debug, Error)]
pub enum LoadProjectError<PLErr, PSErr> {
    #[error("error from ProjectLoader: {0}")]
    ProjectLoaderError(PLErr),
    #[error("error from ProjectSerializer: {0}")]
    ProjectDeserializeError(PSErr),
}

#[async_trait]
impl<T: ParameterValueType, T0, PS, PL, T3, PM, T5, T6, T7, T8, T9, T10> LoadProjectUsecase<T> for MPDeltaCore<T0, PS, PL, T3, PM, T5, T6, T7, T8, T9, T10>
where
    Self: Send + Sync,
    PS: ProjectSerializer<T>,
    PL: ProjectLoader<T>,
    PM: ProjectMemory<T>,
    T5: RootComponentClassMemory<T>,
{
    type Err = LoadProjectError<PL::Err, PS::DeserializeError>;

    async fn load_project(&self, path: impl AsRef<Path> + Send + Sync) -> Result<ProjectHandle<T>, Self::Err> {
        let path = path.as_ref();
        match self.project_memory.get_loaded_project(path).await {
            Some(project) => Ok(project),
            None => {
                let project_load = self.project_loader.load_project(path).await.map_err(LoadProjectError::ProjectLoaderError)?;
                let project = self.project_serializer.deserialize_project(project_load).await.map_err(LoadProjectError::ProjectDeserializeError)?;
                let project_ref = StaticPointerOwned::reference(&project).clone();
                self.project_memory.insert_new_project(Some(path), project).await;
                Ok(project_ref)
            }
        }
    }
}

#[async_trait]
pub trait ProjectWriter<T: ParameterValueType>: Send + Sync {
    type Err: Error + Send + 'static;
    type ProjectWrite<'a>: Write + Send + 'a
    where
        Self: 'a;
    async fn write_project<'a>(&'a self, path: &Path) -> Result<Self::ProjectWrite<'a>, Self::Err>;
}

#[derive(Debug, Error)]
pub enum WriteProjectError<PWErr, PSErr> {
    #[error("error from ProjectWriter: {0}")]
    ProjectWriterError(PWErr),
    #[error("error from ProjectSerializer: {0}")]
    ProjectSerializeError(PSErr),
}

#[async_trait]
impl<T: ParameterValueType, T0, PS, T2, PW, T4, T5, T6, T7, T8, T9, T10> WriteProjectUsecase<T> for MPDeltaCore<T0, PS, T2, PW, T4, T5, T6, T7, T8, T9, T10>
where
    Self: Send + Sync,
    PS: ProjectSerializer<T>,
    PW: ProjectWriter<T>,
{
    type Err = WriteProjectError<PW::Err, PS::SerializeError>;

    async fn write_project(&self, project: &ProjectHandle<T>, path: impl AsRef<Path> + Send + Sync) -> Result<(), Self::Err> {
        let out = self.project_writer.write_project(path.as_ref()).await.map_err(WriteProjectError::ProjectWriterError)?;
        self.project_serializer.serialize_project(project, out).await.map_err(WriteProjectError::ProjectSerializeError)
    }
}

#[async_trait]
impl<T: ParameterValueType, ID, T1, T2, T3, PM, T5, T6, T7, T8, T9, T10> NewProjectUsecase<T> for MPDeltaCore<ID, T1, T2, T3, PM, T5, T6, T7, T8, T9, T10>
where
    Self: Send + Sync,
    ID: IdGenerator,
    PM: ProjectMemory<T>,
{
    async fn new_project(&self) -> ProjectHandle<T> {
        let project = Project::new_empty(self.id_generator.generate_new());
        let pointer = StaticPointerOwned::reference(&project).clone();
        self.project_memory.insert_new_project(None, project).await;
        pointer
    }
}

#[async_trait]
pub trait RootComponentClassMemory<T: ParameterValueType>: Send + Sync {
    async fn insert_new_root_component_class(&self, parent: Option<&ProjectHandle<T>>, root_component_class: RootComponentClassHandleOwned<T>);
    async fn set_parent(&self, root_component_class: &RootComponentClassHandle<T>, parent: Option<&ProjectHandle<T>>);
    async fn search_by_parent(&self, parent: &ProjectHandle<T>) -> Cow<'_, [RootComponentClassHandle<T>]>;
    async fn get_parent_project(&self, path: &RootComponentClassHandle<T>) -> Option<ProjectHandle<T>>;
    async fn all_loaded_root_component_classes(&self) -> Cow<'_, [RootComponentClassHandle<T>]>;
}

#[async_trait]
impl<T: ParameterValueType, ID, T1, T2, T3, PM, RM, T6, T7, T8, T9, T10> NewRootComponentClassUsecase<T> for MPDeltaCore<ID, T1, T2, T3, PM, RM, T6, T7, T8, T9, T10>
where
    Self: Send + Sync,
    ID: IdGenerator,
    PM: ProjectMemory<T>,
    RM: RootComponentClassMemory<T>,
{
    async fn new_root_component_class(&self) -> RootComponentClassHandle<T> {
        let project = self.project_memory.default_project();
        let project_id = project.upgrade().unwrap().read().await.id();
        let root_component_class = RootComponentClass::new_empty(self.id_generator.generate_new(), project.clone(), project_id, &self.id_generator);
        let pointer = StaticPointerOwned::reference(&root_component_class).clone();
        self.root_component_class_memory.insert_new_root_component_class(None, root_component_class).await;
        pointer
    }
}

#[async_trait]
impl<T: ParameterValueType, T0, T1, T2, T3, T4, RM, T6, T7, T8, T9, T10> SetOwnerForRootComponentClassUsecase<T> for MPDeltaCore<T0, T1, T2, T3, T4, RM, T6, T7, T8, T9, T10>
where
    Self: Send + Sync,
    RM: RootComponentClassMemory<T>,
{
    async fn set_owner_for_root_component_class(&self, component: &RootComponentClassHandle<T>, owner: &ProjectHandle<T>) {
        self.root_component_class_memory.set_parent(component, Some(owner)).await;
    }
}

#[async_trait]
impl<T: ParameterValueType, T0, T1, T2, T3, PM, T5, T6, T7, T8, T9, T10> GetLoadedProjectsUsecase<T> for MPDeltaCore<T0, T1, T2, T3, PM, T5, T6, T7, T8, T9, T10>
where
    Self: Send + Sync,
    PM: ProjectMemory<T>,
{
    async fn get_loaded_projects(&self) -> Cow<'_, [ProjectHandle<T>]> {
        self.project_memory.all_loaded_projects().await
    }
}

#[async_trait]
impl<T: ParameterValueType, T0, T1, T2, T3, T4, RM, T6, T7, T8, T9, T10> GetRootComponentClassesUsecase<T> for MPDeltaCore<T0, T1, T2, T3, T4, RM, T6, T7, T8, T9, T10>
where
    Self: Send + Sync,
    RM: RootComponentClassMemory<T>,
{
    async fn get_root_component_classes(&self, project: &ProjectHandle<T>) -> Cow<'_, [RootComponentClassHandle<T>]> {
        self.root_component_class_memory.search_by_parent(project).await
    }
}

#[async_trait]
pub trait ComponentClassLoader<T: ParameterValueType>: Send + Sync {
    async fn get_available_component_classes(&self) -> Cow<'_, [StaticPointer<RwLock<dyn ComponentClass<T>>>]>;
    async fn component_class_by_identifier(&self, identifier: ComponentClassIdentifier<'_>) -> Option<StaticPointer<RwLock<dyn ComponentClass<T>>>>;
}

impl<T, O> ComponentClassLoader<T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: ComponentClassLoader<T>,
{
    fn get_available_component_classes<'life0, 'async_trait>(&'life0 self) -> Pin<Box<dyn Future<Output = Cow<'life0, [StaticPointer<RwLock<dyn ComponentClass<T>>>]>> + Send + 'async_trait>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
    {
        self.deref().get_available_component_classes()
    }

    fn component_class_by_identifier<'life0, 'life1, 'async_trait>(&'life0 self, identifier: ComponentClassIdentifier<'life1>) -> Pin<Box<dyn Future<Output = Option<StaticPointer<RwLock<dyn ComponentClass<T>>>>> + Send + 'async_trait>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        self.deref().component_class_by_identifier(identifier)
    }
}

#[async_trait]
pub trait ValueManagerLoader<T>: Send + Sync {
    async fn get_available_single_value(&self) -> Cow<'_, [Arc<dyn DynEditableSingleValueManager<T>>]>;
    async fn single_value_by_identifier(&self, identifier: DynEditableSingleValueIdentifier<'_>) -> Option<Arc<dyn DynEditableSingleValueManager<T>>>;
    async fn get_available_easing_value(&self) -> Cow<'_, [Arc<dyn DynEditableEasingValueManager<T>>]>;
    async fn easing_value_by_identifier(&self, identifier: DynEditableEasingValueIdentifier<'_>) -> Option<Arc<dyn DynEditableEasingValueManager<T>>>;
}

impl<T, O> ValueManagerLoader<T> for O
where
    O: Deref + Send + Sync,
    O::Target: ValueManagerLoader<T>,
{
    fn get_available_single_value<'life0, 'async_trait>(&'life0 self) -> Pin<Box<dyn Future<Output = Cow<'life0, [Arc<dyn DynEditableSingleValueManager<T>>]>> + Send + 'async_trait>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
    {
        self.deref().get_available_single_value()
    }

    fn single_value_by_identifier<'life0, 'life1, 'async_trait>(&'life0 self, identifier: DynEditableSingleValueIdentifier<'life1>) -> Pin<Box<dyn Future<Output = Option<Arc<dyn DynEditableSingleValueManager<T>>>> + Send + 'async_trait>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        self.deref().single_value_by_identifier(identifier)
    }

    fn get_available_easing_value<'life0, 'async_trait>(&'life0 self) -> Pin<Box<dyn Future<Output = Cow<'life0, [Arc<dyn DynEditableEasingValueManager<T>>]>> + Send + 'async_trait>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
    {
        self.deref().get_available_easing_value()
    }

    fn easing_value_by_identifier<'life0, 'life1, 'async_trait>(&'life0 self, identifier: DynEditableEasingValueIdentifier<'life1>) -> Pin<Box<dyn Future<Output = Option<Arc<dyn DynEditableEasingValueManager<T>>>> + Send + 'async_trait>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        self.deref().easing_value_by_identifier(identifier)
    }
}

#[async_trait]
pub trait EasingLoader: Send + Sync {
    async fn get_available_easing(&self) -> Cow<'_, [Arc<dyn Easing>]>;
    async fn easing_by_identifier(&self, identifier: EasingIdentifier<'_>) -> Option<Arc<dyn Easing>>;
}

impl<O> EasingLoader for O
where
    O: Deref + Send + Sync,
    O::Target: EasingLoader,
{
    fn get_available_easing<'life0, 'async_trait>(&'life0 self) -> Pin<Box<dyn Future<Output = Cow<'life0, [Arc<dyn Easing>]>> + Send + 'async_trait>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
    {
        self.deref().get_available_easing()
    }

    fn easing_by_identifier<'life0, 'life1, 'async_trait>(&'life0 self, identifier: EasingIdentifier<'life1>) -> Pin<Box<dyn Future<Output = Option<Arc<dyn Easing>>> + Send + 'async_trait>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        self.deref().easing_by_identifier(identifier)
    }
}

#[async_trait]
impl<T: ParameterValueType, T0, T1, T2, T3, T4, T5, CL, T7, T8, T9, T10> GetAvailableComponentClassesUsecase<T> for MPDeltaCore<T0, T1, T2, T3, T4, T5, CL, T7, T8, T9, T10>
where
    Self: Send + Sync,
    CL: ComponentClassLoader<T>,
{
    async fn get_available_component_classes(&self) -> Cow<'_, [StaticPointer<RwLock<dyn ComponentClass<T>>>]> {
        self.component_class_loader.get_available_component_classes().await
    }
}

#[async_trait]
pub trait ComponentRendererBuilder<T: ParameterValueType>: Send + Sync {
    type Err: Error + Send + 'static;
    type Renderer: RealtimeComponentRenderer<T> + Send + Sync + 'static;
    async fn create_renderer(&self, component: Arc<ComponentInstance<T>>) -> Result<Self::Renderer, Self::Err>;
}

#[async_trait]
impl<T: ParameterValueType, T0, T1, T2, T3, T4, T5, T6, CR, T8, T9, T10> RealtimeRenderComponentUsecase<T> for MPDeltaCore<T0, T1, T2, T3, T4, T5, T6, CR, T8, T9, T10>
where
    Self: Send + Sync,
    CR: ComponentRendererBuilder<T>,
{
    type Err = CR::Err;
    type Renderer = CR::Renderer;

    async fn render_component(&self, component: Arc<ComponentInstance<T>>) -> Result<Self::Renderer, Self::Err> {
        self.component_renderer_builder.create_renderer(component).await
    }
}

pub trait ComponentEncoder<T: ParameterValueType, Encoder>: Send + Sync {
    type Err: Error + Send + 'static;
    fn render_and_encode<'life0, 'async_trait>(&'life0 self, component: Arc<ComponentInstance<T>>, encoder: Encoder) -> impl Future<Output = Result<(), Self::Err>> + Send + 'async_trait
    where
        'life0: 'async_trait;
}

impl<T: ParameterValueType, T0, T1, T2, T3, T4, T5, T6, T7, VE, T9, T10, Encoder> RenderWholeComponentUsecase<T, Encoder> for MPDeltaCore<T0, T1, T2, T3, T4, T5, T6, T7, VE, T9, T10>
where
    Self: Send + Sync,
    VE: ComponentEncoder<T, Encoder>,
{
    type Err = VE::Err;

    fn render_and_encode<'life0, 'async_trait>(&'life0 self, component: Arc<ComponentInstance<T>>, encoder: Encoder) -> impl Future<Output = Result<(), Self::Err>> + Send + 'async_trait
    where
        'life0: 'async_trait,
    {
        self.video_encoder.render_and_encode(component, encoder)
    }
}

pub trait EditEventListener<T: ParameterValueType>: Send + Sync {
    fn on_edit(&self, target: &RootComponentClassHandle<T>, event: RootComponentEditEvent);
    fn on_edit_instance(&self, root: &RootComponentClassHandle<T>, target: &ComponentInstanceId, command: InstanceEditEvent<T>);
}

impl<T: ParameterValueType, O> EditEventListener<T> for O
where
    O: Deref + Send + Sync,
    O::Target: EditEventListener<T>,
{
    fn on_edit(&self, target: &RootComponentClassHandle<T>, event: RootComponentEditEvent) {
        self.deref().on_edit(target, event)
    }

    fn on_edit_instance(&self, root: &RootComponentClassHandle<T>, target: &ComponentInstanceId, command: InstanceEditEvent<T>) {
        self.deref().on_edit_instance(root, target, command)
    }
}

#[async_trait]
pub trait Editor<T: ParameterValueType>: Send + Sync {
    type Log: Send + Sync;
    type Err: Error + Send + 'static;
    type EditEventListenerGuard: Send + Sync + 'static;
    fn add_edit_event_listener(&self, listener: impl EditEventListener<T> + 'static) -> Self::EditEventListenerGuard;
    async fn edit(&self, target: &RootComponentClassHandle<T>, command: RootComponentEditCommand<T>) -> Result<Self::Log, Self::Err>;
    async fn edit_instance(&self, root: &RootComponentClassHandle<T>, target: &ComponentInstanceId, command: InstanceEditCommand<T>) -> Result<Self::Log, Self::Err>;
    async fn edit_reverse(&self, log: &Self::Log);
    async fn edit_by_log(&self, log: &Self::Log);
}

#[async_trait]
pub trait EditHistory<T: ParameterValueType, Log>: Send + Sync {
    async fn push_history(&self, root: &RootComponentClassHandle<T>, target: Option<&ComponentInstanceId>, log: Log);
    async fn undo(&self, root: &RootComponentClassHandle<T>, target: Option<&ComponentInstanceId>) -> Option<Arc<Log>>;
    async fn redo(&self, root: &RootComponentClassHandle<T>, target: Option<&ComponentInstanceId>) -> Option<Arc<Log>>;
}

#[async_trait]
impl<T: ParameterValueType, T0, T1, T2, T3, T4, T5, T6, T7, T8, ED, HS> EditUsecase<T> for MPDeltaCore<T0, T1, T2, T3, T4, T5, T6, T7, T8, ED, HS>
where
    Self: Send + Sync,
    ED: Editor<T>,
    HS: EditHistory<T, ED::Log>,
{
    type Err = ED::Err;

    async fn edit(&self, target: &RootComponentClassHandle<T>, command: RootComponentEditCommand<T>) -> Result<(), Self::Err> {
        let log = self.editor.edit(target, command).await?;
        self.edit_history.push_history(target, None, log).await;
        Ok(())
    }

    async fn edit_instance(&self, root: &RootComponentClassHandle<T>, target: &ComponentInstanceId, command: InstanceEditCommand<T>) -> Result<(), Self::Err> {
        let log = self.editor.edit_instance(root, target, command).await?;
        self.edit_history.push_history(root, Some(target), log).await;
        Ok(())
    }
}

#[async_trait]
impl<T: ParameterValueType, T0, T1, T2, T3, T4, T5, T6, T7, T8, ED, T10> SubscribeEditEventUsecase<T> for MPDeltaCore<T0, T1, T2, T3, T4, T5, T6, T7, T8, ED, T10>
where
    Self: Send + Sync,
    ED: Editor<T>,
{
    type EditEventListenerGuard = ED::EditEventListenerGuard;

    fn add_edit_event_listener(&self, listener: impl EditEventListener<T> + 'static) -> Self::EditEventListenerGuard {
        self.editor.add_edit_event_listener(listener)
    }
}

#[async_trait]
impl<T: ParameterValueType, T0, T1, T2, T3, T4, T5, T6, T7, T8, ED, HS> UndoUsecase<T> for MPDeltaCore<T0, T1, T2, T3, T4, T5, T6, T7, T8, ED, HS>
where
    Self: Send + Sync,
    ED: Editor<T>,
    HS: EditHistory<T, ED::Log>,
{
    async fn undo(&self, component: &RootComponentClassHandle<T>) -> bool {
        if let Some(log) = self.edit_history.undo(component, None).await {
            self.editor.edit_reverse(&log).await;
            true
        } else {
            false
        }
    }

    async fn undo_instance(&self, root: &RootComponentClassHandle<T>, target: &ComponentInstanceId) -> bool {
        if let Some(log) = self.edit_history.undo(root, Some(target)).await {
            self.editor.edit_reverse(&log).await;
            true
        } else {
            false
        }
    }
}

#[async_trait]
impl<T: ParameterValueType, T0, T1, T2, T3, T4, T5, T6, T7, T8, ED, HS> RedoUsecase<T> for MPDeltaCore<T0, T1, T2, T3, T4, T5, T6, T7, T8, ED, HS>
where
    Self: Send + Sync,
    ED: Editor<T>,
    HS: EditHistory<T, ED::Log>,
{
    async fn redo(&self, component: &RootComponentClassHandle<T>) -> bool {
        if let Some(log) = self.edit_history.redo(component, None).await {
            self.editor.edit_by_log(&log).await;
            true
        } else {
            false
        }
    }

    async fn redo_instance(&self, root: &RootComponentClassHandle<T>, target: &ComponentInstanceId) -> bool {
        if let Some(log) = self.edit_history.redo(root, Some(target)).await {
            self.editor.edit_by_log(&log).await;
            true
        } else {
            false
        }
    }
}
