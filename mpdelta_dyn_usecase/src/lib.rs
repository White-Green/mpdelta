use async_trait::async_trait;
use futures::{FutureExt, TryFutureExt};
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::component::instance::ComponentInstanceHandle;
use mpdelta_core::component::parameter::{Parameter, ParameterSelect, ParameterValueType};
use mpdelta_core::core::EditEventListener;
use mpdelta_core::edit::{InstanceEditCommand, RootComponentEditCommand};
use mpdelta_core::project::{ProjectHandle, RootComponentClassHandle};
use mpdelta_core::ptr::StaticPointer;
use mpdelta_core::usecase::{
    EditUsecase, GetAvailableComponentClassesUsecase, GetLoadedProjectsUsecase, GetRootComponentClassesUsecase, LoadProjectUsecase, NewProjectUsecase, NewRootComponentClassUsecase, RealtimeComponentRenderer, RealtimeRenderComponentUsecase, RedoUsecase, RenderWholeComponentUsecase,
    SetOwnerForRootComponentClassUsecase, SubscribeEditEventUsecase, UndoUsecase, WriteProjectUsecase,
};
use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use tokio::sync::RwLock;

pub struct DynError(pub Box<dyn Error + Send + 'static>);

impl Debug for DynError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        Debug::fmt(&self.0, f)
    }
}

impl Display for DynError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        Display::fmt(&self.0, f)
    }
}

impl Error for DynError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Error::source(&*self.0)
    }
}

pub type DynFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait LoadProjectUsecaseDyn<K, T: ParameterValueType>: Send + Sync {
    fn load_project_dyn<'life0, 'life1, 'async_trait>(&'life0 self, path: &'life1 Path) -> DynFuture<'async_trait, Result<ProjectHandle<K, T>, Box<dyn Error + Send + 'static>>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait;
}

impl<K, T, O> LoadProjectUsecaseDyn<K, T> for O
where
    K: 'static,
    T: ParameterValueType,
    O: LoadProjectUsecase<K, T>,
{
    fn load_project_dyn<'life0, 'life1, 'async_trait>(&'life0 self, path: &'life1 Path) -> DynFuture<'async_trait, Result<ProjectHandle<K, T>, Box<dyn Error + Send + 'static>>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        Box::pin(async move { self.load_project(path).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>).await })
    }
}

#[async_trait]
impl<K, T> LoadProjectUsecase<K, T> for dyn LoadProjectUsecaseDyn<K, T> + Send + Sync
where
    K: 'static,
    T: ParameterValueType,
{
    type Err = DynError;

    async fn load_project(&self, path: impl AsRef<Path> + Send + Sync) -> Result<ProjectHandle<K, T>, Self::Err> {
        self.load_project_dyn(path.as_ref()).map_err(DynError).await
    }
}

pub trait WriteProjectUsecaseDyn<K, T: ParameterValueType>: Send + Sync {
    fn write_project_dyn<'life0, 'life1, 'life2, 'async_trait>(&'life0 self, project: &'life1 ProjectHandle<K, T>, path: &'life2 Path) -> DynFuture<'async_trait, Result<(), Box<dyn Error + Send + 'static>>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait;
}

impl<K, T, O> WriteProjectUsecaseDyn<K, T> for O
where
    K: 'static,
    T: ParameterValueType,
    O: WriteProjectUsecase<K, T>,
{
    fn write_project_dyn<'life0, 'life1, 'life2, 'async_trait>(&'life0 self, project: &'life1 ProjectHandle<K, T>, path: &'life2 Path) -> DynFuture<'async_trait, Result<(), Box<dyn Error + Send + 'static>>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
    {
        Box::pin(async move { self.write_project(project, path).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>).await })
    }
}

#[async_trait]
impl<K, T> WriteProjectUsecase<K, T> for dyn WriteProjectUsecaseDyn<K, T> + Send + Sync
where
    K: 'static,
    T: ParameterValueType,
{
    type Err = DynError;

    async fn write_project(&self, project: &ProjectHandle<K, T>, path: impl AsRef<Path> + Send + Sync) -> Result<(), Self::Err> {
        self.write_project_dyn(project, path.as_ref()).map_err(DynError).await
    }
}

pub trait NewProjectUsecaseDyn<K, T: ParameterValueType>: Send + Sync {
    fn new_project_dyn<'life0, 'async_trait>(&'life0 self) -> DynFuture<'async_trait, ProjectHandle<K, T>>
    where
        Self: 'async_trait,
        'life0: 'async_trait;
}

impl<K, T, O> NewProjectUsecaseDyn<K, T> for O
where
    K: 'static,
    T: ParameterValueType,
    O: NewProjectUsecase<K, T>,
{
    fn new_project_dyn<'life0, 'async_trait>(&'life0 self) -> DynFuture<'async_trait, ProjectHandle<K, T>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
    {
        Box::pin(async move { self.new_project().await })
    }
}

#[async_trait]
impl<K, T> NewProjectUsecase<K, T> for dyn NewProjectUsecaseDyn<K, T> + Send + Sync
where
    K: 'static,
    T: ParameterValueType,
{
    async fn new_project(&self) -> ProjectHandle<K, T> {
        self.new_project_dyn().await
    }
}

pub trait NewRootComponentClassUsecaseDyn<K, T: ParameterValueType>: Send + Sync {
    fn new_root_component_class_dyn<'life0, 'async_trait>(&'life0 self) -> DynFuture<'async_trait, RootComponentClassHandle<K, T>>
    where
        Self: 'async_trait,
        'life0: 'async_trait;
}

impl<K, T, O> NewRootComponentClassUsecaseDyn<K, T> for O
where
    K: 'static,
    T: ParameterValueType,
    O: NewRootComponentClassUsecase<K, T>,
{
    fn new_root_component_class_dyn<'life0, 'async_trait>(&'life0 self) -> DynFuture<'async_trait, RootComponentClassHandle<K, T>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
    {
        Box::pin(async move { self.new_root_component_class().await })
    }
}

#[async_trait]
impl<K, T> NewRootComponentClassUsecase<K, T> for dyn NewRootComponentClassUsecaseDyn<K, T> + Send + Sync
where
    K: 'static,
    T: ParameterValueType,
{
    async fn new_root_component_class(&self) -> RootComponentClassHandle<K, T> {
        self.new_root_component_class_dyn().await
    }
}

pub trait SetOwnerForRootComponentClassUsecaseDyn<K, T: ParameterValueType>: Send + Sync {
    fn set_owner_for_root_component_class_dyn<'life0, 'life1, 'life2, 'async_trait>(&'life0 self, component: &'life1 RootComponentClassHandle<K, T>, owner: &'life2 ProjectHandle<K, T>) -> DynFuture<'async_trait, ()>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait;
}

impl<K, T, O> SetOwnerForRootComponentClassUsecaseDyn<K, T> for O
where
    K: 'static,
    T: ParameterValueType,
    O: SetOwnerForRootComponentClassUsecase<K, T>,
{
    fn set_owner_for_root_component_class_dyn<'life0, 'life1, 'life2, 'async_trait>(&'life0 self, component: &'life1 RootComponentClassHandle<K, T>, owner: &'life2 ProjectHandle<K, T>) -> DynFuture<'async_trait, ()>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
    {
        Box::pin(async move { self.set_owner_for_root_component_class(component, owner).await })
    }
}

#[async_trait]
impl<K, T> SetOwnerForRootComponentClassUsecase<K, T> for dyn SetOwnerForRootComponentClassUsecaseDyn<K, T> + Send + Sync
where
    K: 'static,
    T: ParameterValueType,
{
    async fn set_owner_for_root_component_class(&self, component: &RootComponentClassHandle<K, T>, owner: &ProjectHandle<K, T>) {
        self.set_owner_for_root_component_class_dyn(component, owner).await
    }
}

pub trait GetLoadedProjectsUsecaseDyn<K, T: ParameterValueType>: Send + Sync {
    fn get_loaded_projects_dyn<'life0, 'async_trait>(&'life0 self) -> DynFuture<'async_trait, Cow<'_, [ProjectHandle<K, T>]>>
    where
        Self: 'async_trait,
        'life0: 'async_trait;
}

impl<K, T, O> GetLoadedProjectsUsecaseDyn<K, T> for O
where
    K: 'static,
    T: ParameterValueType,
    O: GetLoadedProjectsUsecase<K, T>,
{
    fn get_loaded_projects_dyn<'life0, 'async_trait>(&'life0 self) -> DynFuture<'async_trait, Cow<'_, [ProjectHandle<K, T>]>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
    {
        Box::pin(async move { self.get_loaded_projects().await })
    }
}

#[async_trait]
impl<K, T> GetLoadedProjectsUsecase<K, T> for dyn GetLoadedProjectsUsecaseDyn<K, T> + Send + Sync
where
    K: 'static,
    T: ParameterValueType,
{
    async fn get_loaded_projects(&self) -> Cow<[ProjectHandle<K, T>]> {
        self.get_loaded_projects_dyn().await
    }
}

pub trait GetRootComponentClassesUsecaseDyn<K, T: ParameterValueType>: Send + Sync {
    fn get_root_component_classes_dyn<'life0, 'life1, 'async_trait>(&'life0 self, project: &'life1 ProjectHandle<K, T>) -> DynFuture<'async_trait, Cow<'_, [RootComponentClassHandle<K, T>]>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait;
}

impl<K, T, O> GetRootComponentClassesUsecaseDyn<K, T> for O
where
    K: 'static,
    T: ParameterValueType,
    O: GetRootComponentClassesUsecase<K, T>,
{
    fn get_root_component_classes_dyn<'life0, 'life1, 'async_trait>(&'life0 self, project: &'life1 ProjectHandle<K, T>) -> DynFuture<'async_trait, Cow<'_, [RootComponentClassHandle<K, T>]>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        Box::pin(async move { self.get_root_component_classes(project).await })
    }
}

#[async_trait]
impl<K, T> GetRootComponentClassesUsecase<K, T> for dyn GetRootComponentClassesUsecaseDyn<K, T> + Send + Sync
where
    K: 'static,
    T: ParameterValueType,
{
    async fn get_root_component_classes(&self, project: &ProjectHandle<K, T>) -> Cow<[RootComponentClassHandle<K, T>]> {
        self.get_root_component_classes_dyn(project).await
    }
}

pub trait GetAvailableComponentClassesUsecaseDyn<K, T>: Send + Sync {
    fn get_available_component_classes_dyn<'life0, 'async_trait>(&'life0 self) -> DynFuture<'async_trait, Cow<'_, [StaticPointer<RwLock<dyn ComponentClass<K, T>>>]>>
    where
        Self: 'async_trait,
        'life0: 'async_trait;
}

impl<K, T, O> GetAvailableComponentClassesUsecaseDyn<K, T> for O
where
    K: 'static,
    T: ParameterValueType,
    O: GetAvailableComponentClassesUsecase<K, T>,
{
    fn get_available_component_classes_dyn<'life0, 'async_trait>(&'life0 self) -> DynFuture<'async_trait, Cow<'_, [StaticPointer<RwLock<dyn ComponentClass<K, T>>>]>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
    {
        Box::pin(async move { self.get_available_component_classes().await })
    }
}

#[async_trait]
impl<K, T> GetAvailableComponentClassesUsecase<K, T> for dyn GetAvailableComponentClassesUsecaseDyn<K, T> + Send + Sync
where
    K: 'static,
    T: ParameterValueType,
{
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<K, T>>>]> {
        self.get_available_component_classes_dyn().await
    }
}

pub trait RealtimeComponentRendererDyn<T: ParameterValueType>: Send + Sync {
    fn get_frame_count_dyn(&self) -> usize;
    fn render_frame_dyn(&self, frame: usize) -> Result<T::Image, Box<dyn Error + Send + 'static>>;
    fn sampling_rate_dyn(&self) -> u32;
    fn mix_audio_dyn(&self, offset: usize, length: usize) -> Pin<Box<dyn Future<Output = Result<T::Audio, Box<dyn Error + Send + 'static>>> + Send + '_>>;
    fn render_param_dyn(&self, param: Parameter<ParameterSelect>) -> Result<Parameter<T>, Box<dyn Error + Send + 'static>>;
}

impl<T, O> RealtimeComponentRendererDyn<T> for O
where
    T: ParameterValueType,
    O: RealtimeComponentRenderer<T>,
{
    fn get_frame_count_dyn(&self) -> usize {
        self.get_frame_count()
    }

    fn render_frame_dyn(&self, frame: usize) -> Result<T::Image, Box<dyn Error + Send + 'static>> {
        self.render_frame(frame).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>)
    }

    fn sampling_rate_dyn(&self) -> u32 {
        self.sampling_rate()
    }

    fn mix_audio_dyn(&self, offset: usize, length: usize) -> Pin<Box<dyn Future<Output = Result<T::Audio, Box<dyn Error + Send + 'static>>> + Send + '_>> {
        Box::pin(self.mix_audio(offset, length).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>))
    }

    fn render_param_dyn(&self, param: Parameter<ParameterSelect>) -> Result<Parameter<T>, Box<dyn Error + Send + 'static>> {
        self.render_param(param).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>)
    }
}

impl<T: ParameterValueType> RealtimeComponentRenderer<T> for dyn RealtimeComponentRendererDyn<T> + Send + Sync {
    type Err = DynError;

    fn get_frame_count(&self) -> usize {
        self.get_frame_count_dyn()
    }

    fn render_frame(&self, frame: usize) -> Result<T::Image, Self::Err> {
        self.render_frame_dyn(frame).map_err(DynError)
    }

    fn sampling_rate(&self) -> u32 {
        self.sampling_rate_dyn()
    }

    fn mix_audio(&self, offset: usize, length: usize) -> impl Future<Output = Result<T::Audio, Self::Err>> + Send + '_ {
        self.mix_audio_dyn(offset, length).map_err(DynError)
    }

    fn render_param(&self, param: Parameter<ParameterSelect>) -> Result<Parameter<T>, Self::Err> {
        self.render_param_dyn(param).map_err(DynError)
    }
}

pub trait RealtimeRenderComponentUsecaseDyn<K, T: ParameterValueType>: Send + Sync {
    fn render_component_dyn<'life0, 'life1, 'async_trait>(&'life0 self, component: &'life1 ComponentInstanceHandle<K, T>) -> DynFuture<'async_trait, Result<Box<dyn RealtimeComponentRendererDyn<T> + Send + Sync + 'static>, Box<dyn Error + Send + 'static>>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait;
}

impl<K, T, O> RealtimeRenderComponentUsecaseDyn<K, T> for O
where
    T: ParameterValueType,
    O: RealtimeRenderComponentUsecase<K, T>,
{
    fn render_component_dyn<'life0, 'life1, 'async_trait>(&'life0 self, component: &'life1 ComponentInstanceHandle<K, T>) -> DynFuture<'async_trait, Result<Box<dyn RealtimeComponentRendererDyn<T> + Send + Sync + 'static>, Box<dyn Error + Send + 'static>>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        Box::pin(async move {
            self.render_component(component)
                .map(|result| match result {
                    Ok(renderer) => Ok(Box::new(renderer) as Box<dyn RealtimeComponentRendererDyn<T> + Send + Sync + 'static>),
                    Err(err) => Err(Box::new(err) as Box<dyn Error + Send + 'static>),
                })
                .await
        })
    }
}

#[async_trait]
impl<K, T> RealtimeRenderComponentUsecase<K, T> for dyn RealtimeRenderComponentUsecaseDyn<K, T> + Send + Sync
where
    K: 'static,
    T: ParameterValueType,
{
    type Err = DynError;
    type Renderer = Box<dyn RealtimeComponentRendererDyn<T> + Send + Sync + 'static>;

    async fn render_component(&self, component: &ComponentInstanceHandle<K, T>) -> Result<Self::Renderer, Self::Err> {
        self.render_component_dyn(component).map_err(DynError).await
    }
}

pub trait RenderWholeComponentUsecaseDyn<K, T: ParameterValueType, Encoder>: Send + Sync {
    fn render_and_encode_dyn<'life0, 'life1, 'async_trait>(&'life0 self, component: &'life1 ComponentInstanceHandle<K, T>, encoder: Encoder) -> DynFuture<'async_trait, Result<(), Box<dyn Error + Send + 'static>>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait;
}

impl<K, T, Encoder, O> RenderWholeComponentUsecaseDyn<K, T, Encoder> for O
where
    T: ParameterValueType,
    O: RenderWholeComponentUsecase<K, T, Encoder>,
{
    fn render_and_encode_dyn<'life0, 'life1, 'async_trait>(&'life0 self, component: &'life1 ComponentInstanceHandle<K, T>, encoder: Encoder) -> DynFuture<'async_trait, Result<(), Box<dyn Error + Send + 'static>>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        Box::pin(self.render_and_encode(component, encoder).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>))
    }
}

impl<K, T, Encoder> RenderWholeComponentUsecase<K, T, Encoder> for dyn RenderWholeComponentUsecaseDyn<K, T, Encoder>
where
    T: ParameterValueType,
{
    type Err = DynError;

    fn render_and_encode<'life0, 'life1, 'async_trait>(&'life0 self, component: &'life1 ComponentInstanceHandle<K, T>, encoder: Encoder) -> impl Future<Output = Result<(), Self::Err>> + Send + 'async_trait
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        self.render_and_encode_dyn(component, encoder).map_err(DynError)
    }
}

pub trait EditUsecaseDyn<K, T: ParameterValueType>: Send + Sync {
    fn edit_dyn<'life0, 'life1, 'async_trait>(&'life0 self, target: &'life1 RootComponentClassHandle<K, T>, command: RootComponentEditCommand<K, T>) -> DynFuture<'async_trait, Result<(), Box<dyn Error + Send + 'static>>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait;
    fn edit_instance_dyn<'life0, 'life1, 'life2, 'async_trait>(&'life0 self, root: &'life1 RootComponentClassHandle<K, T>, target: &'life2 ComponentInstanceHandle<K, T>, command: InstanceEditCommand<K, T>) -> DynFuture<'async_trait, Result<(), Box<dyn Error + Send + 'static>>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait;
}

impl<K, T, O> EditUsecaseDyn<K, T> for O
where
    K: 'static,
    T: ParameterValueType,
    O: EditUsecase<K, T>,
{
    fn edit_dyn<'life0, 'life1, 'async_trait>(&'life0 self, target: &'life1 RootComponentClassHandle<K, T>, command: RootComponentEditCommand<K, T>) -> DynFuture<'async_trait, Result<(), Box<dyn Error + Send + 'static>>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        Box::pin(async move { self.edit(target, command).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>).await })
    }

    fn edit_instance_dyn<'life0, 'life1, 'life2, 'async_trait>(&'life0 self, root: &'life1 RootComponentClassHandle<K, T>, target: &'life2 ComponentInstanceHandle<K, T>, command: InstanceEditCommand<K, T>) -> DynFuture<'async_trait, Result<(), Box<dyn Error + Send + 'static>>>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
    {
        Box::pin(async move { self.edit_instance(root, target, command).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>).await })
    }
}

#[async_trait]
impl<K, T> EditUsecase<K, T> for dyn EditUsecaseDyn<K, T> + Send + Sync
where
    K: 'static,
    T: ParameterValueType,
{
    type Err = DynError;

    async fn edit(&self, target: &RootComponentClassHandle<K, T>, command: RootComponentEditCommand<K, T>) -> Result<(), Self::Err> {
        self.edit_dyn(target, command).map_err(DynError).await
    }

    async fn edit_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>, command: InstanceEditCommand<K, T>) -> Result<(), Self::Err> {
        self.edit_instance_dyn(root, target, command).map_err(DynError).await
    }
}

pub trait SubscribeEditEventUsecaseDyn<K, T>: Send + Sync {
    fn add_edit_event_listener_dyn(&self, listener: Box<dyn EditEventListener<K, T> + 'static>) -> Box<dyn Send + Sync>;
}

impl<K, T, O> SubscribeEditEventUsecaseDyn<K, T> for O
where
    K: 'static,
    T: ParameterValueType,
    O: SubscribeEditEventUsecase<K, T>,
{
    fn add_edit_event_listener_dyn(&self, listener: Box<dyn EditEventListener<K, T> + 'static>) -> Box<dyn Send + Sync> {
        Box::new(self.add_edit_event_listener(listener))
    }
}

impl<K, T> SubscribeEditEventUsecase<K, T> for dyn SubscribeEditEventUsecaseDyn<K, T> + Send + Sync
where
    K: 'static,
    T: ParameterValueType,
{
    type EditEventListenerGuard = Box<dyn Send + Sync>;

    fn add_edit_event_listener(&self, listener: impl EditEventListener<K, T> + 'static) -> Self::EditEventListenerGuard {
        self.add_edit_event_listener_dyn(Box::new(listener))
    }
}

pub trait UndoUsecaseDyn<K, T: ParameterValueType>: Send + Sync {
    fn undo_dyn<'life0, 'life1, 'async_trait>(&'life0 self, component: &'life1 RootComponentClassHandle<K, T>) -> DynFuture<'async_trait, bool>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait;
    fn undo_instance_dyn<'life0, 'life1, 'life2, 'async_trait>(&'life0 self, root: &'life1 RootComponentClassHandle<K, T>, target: &'life2 ComponentInstanceHandle<K, T>) -> DynFuture<'async_trait, bool>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait;
}

impl<K, T, O> UndoUsecaseDyn<K, T> for O
where
    K: 'static,
    T: ParameterValueType,
    O: UndoUsecase<K, T>,
{
    fn undo_dyn<'life0, 'life1, 'async_trait>(&'life0 self, component: &'life1 RootComponentClassHandle<K, T>) -> DynFuture<'async_trait, bool>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        Box::pin(async move { self.undo(component).await })
    }

    fn undo_instance_dyn<'life0, 'life1, 'life2, 'async_trait>(&'life0 self, root: &'life1 RootComponentClassHandle<K, T>, target: &'life2 ComponentInstanceHandle<K, T>) -> DynFuture<'async_trait, bool>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
    {
        Box::pin(async move { self.undo_instance(root, target).await })
    }
}

#[async_trait]
impl<K, T> UndoUsecase<K, T> for dyn UndoUsecaseDyn<K, T> + Send + Sync
where
    K: 'static,
    T: ParameterValueType,
{
    async fn undo(&self, component: &RootComponentClassHandle<K, T>) -> bool {
        self.undo_dyn(component).await
    }

    async fn undo_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>) -> bool {
        self.undo_instance_dyn(root, target).await
    }
}

pub trait RedoUsecaseDyn<K, T: ParameterValueType>: Send + Sync {
    fn redo_dyn<'life0, 'life1, 'async_trait>(&'life0 self, component: &'life1 RootComponentClassHandle<K, T>) -> DynFuture<'async_trait, bool>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait;
    fn redo_instance_dyn<'life0, 'life1, 'life2, 'async_trait>(&'life0 self, root: &'life1 RootComponentClassHandle<K, T>, target: &'life2 ComponentInstanceHandle<K, T>) -> DynFuture<'async_trait, bool>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait;
}

impl<K, T, O> RedoUsecaseDyn<K, T> for O
where
    K: 'static,
    T: ParameterValueType,
    O: RedoUsecase<K, T>,
{
    fn redo_dyn<'life0, 'life1, 'async_trait>(&'life0 self, component: &'life1 RootComponentClassHandle<K, T>) -> DynFuture<'async_trait, bool>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        Box::pin(async move { self.redo(component).await })
    }

    fn redo_instance_dyn<'life0, 'life1, 'life2, 'async_trait>(&'life0 self, root: &'life1 RootComponentClassHandle<K, T>, target: &'life2 ComponentInstanceHandle<K, T>) -> DynFuture<'async_trait, bool>
    where
        Self: 'async_trait,
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
    {
        Box::pin(async move { self.redo_instance(root, target).await })
    }
}

#[async_trait]
impl<K, T> RedoUsecase<K, T> for dyn RedoUsecaseDyn<K, T> + Send + Sync
where
    K: 'static,
    T: ParameterValueType,
{
    async fn redo(&self, component: &RootComponentClassHandle<K, T>) -> bool {
        self.redo_dyn(component).await
    }

    async fn redo_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>) -> bool {
        self.redo_instance_dyn(root, target).await
    }
}
