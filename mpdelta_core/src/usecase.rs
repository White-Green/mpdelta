use crate::component::class::ComponentClass;
use crate::component::instance::{ComponentInstance, ComponentInstanceId};
use crate::component::marker_pin::MarkerTime;
use crate::component::parameter::{Parameter, ParameterSelect, ParameterValueType};
use crate::core::EditEventListener;
use crate::edit::{InstanceEditCommand, RootComponentEditCommand};
use crate::project::{ProjectHandle, RootComponentClassHandle};
use crate::ptr::StaticPointer;
use async_trait::async_trait;
use std::borrow::Cow;
use std::error::Error;
use std::future::Future;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

#[async_trait]
pub trait LoadProjectUsecase<T: ParameterValueType>: Send + Sync {
    type Err: Error + Send + 'static;
    async fn load_project(&self, path: impl AsRef<Path> + Send + Sync) -> Result<ProjectHandle<T>, Self::Err>;
}

#[async_trait]
impl<T, O> LoadProjectUsecase<T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: LoadProjectUsecase<T>,
{
    type Err = <O::Target as LoadProjectUsecase<T>>::Err;

    async fn load_project(&self, path: impl AsRef<Path> + Send + Sync) -> Result<ProjectHandle<T>, Self::Err> {
        self.deref().load_project(path).await
    }
}

#[async_trait]
pub trait WriteProjectUsecase<T: ParameterValueType>: Send + Sync {
    type Err: Error + Send + 'static;
    async fn write_project(&self, project: &ProjectHandle<T>, path: impl AsRef<Path> + Send + Sync) -> Result<(), Self::Err>;
}

#[async_trait]
impl<T, O> WriteProjectUsecase<T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: WriteProjectUsecase<T>,
{
    type Err = <O::Target as WriteProjectUsecase<T>>::Err;

    async fn write_project(&self, project: &ProjectHandle<T>, path: impl AsRef<Path> + Send + Sync) -> Result<(), Self::Err> {
        self.deref().write_project(project, path).await
    }
}

#[async_trait]
pub trait NewProjectUsecase<T: ParameterValueType>: Send + Sync {
    async fn new_project(&self) -> ProjectHandle<T>;
}

#[async_trait]
impl<T, O> NewProjectUsecase<T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: NewProjectUsecase<T>,
{
    async fn new_project(&self) -> ProjectHandle<T> {
        self.deref().new_project().await
    }
}

#[async_trait]
pub trait NewRootComponentClassUsecase<T: ParameterValueType>: Send + Sync {
    async fn new_root_component_class(&self) -> RootComponentClassHandle<T>;
}

#[async_trait]
impl<T, O> NewRootComponentClassUsecase<T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: NewRootComponentClassUsecase<T>,
{
    async fn new_root_component_class(&self) -> RootComponentClassHandle<T> {
        self.deref().new_root_component_class().await
    }
}

#[async_trait]
pub trait SetOwnerForRootComponentClassUsecase<T: ParameterValueType>: Send + Sync {
    async fn set_owner_for_root_component_class(&self, component: &RootComponentClassHandle<T>, owner: &ProjectHandle<T>);
}

#[async_trait]
impl<T, O> SetOwnerForRootComponentClassUsecase<T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: SetOwnerForRootComponentClassUsecase<T>,
{
    async fn set_owner_for_root_component_class(&self, component: &RootComponentClassHandle<T>, owner: &ProjectHandle<T>) {
        self.deref().set_owner_for_root_component_class(component, owner).await
    }
}

#[async_trait]
pub trait GetLoadedProjectsUsecase<T: ParameterValueType>: Send + Sync {
    async fn get_loaded_projects(&self) -> Cow<[ProjectHandle<T>]>;
}

#[async_trait]
impl<T, O> GetLoadedProjectsUsecase<T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: GetLoadedProjectsUsecase<T>,
{
    async fn get_loaded_projects(&self) -> Cow<[ProjectHandle<T>]> {
        self.deref().get_loaded_projects().await
    }
}

#[async_trait]
pub trait GetRootComponentClassesUsecase<T: ParameterValueType>: Send + Sync {
    async fn get_root_component_classes(&self, project: &ProjectHandle<T>) -> Cow<[RootComponentClassHandle<T>]>;
}

#[async_trait]
impl<T, O> GetRootComponentClassesUsecase<T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: GetRootComponentClassesUsecase<T>,
{
    async fn get_root_component_classes(&self, project: &ProjectHandle<T>) -> Cow<[RootComponentClassHandle<T>]> {
        self.deref().get_root_component_classes(project).await
    }
}

#[async_trait]
pub trait GetAvailableComponentClassesUsecase<T: ParameterValueType>: Send + Sync {
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<T>>>]>;
}

#[async_trait]
impl<T, O> GetAvailableComponentClassesUsecase<T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: GetAvailableComponentClassesUsecase<T>,
{
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<T>>>]> {
        self.deref().get_available_component_classes().await
    }
}

pub trait RealtimeComponentRenderer<T: ParameterValueType>: Send + Sync {
    type Err: Error + Send + 'static;
    fn get_component_length(&self) -> Option<MarkerTime>;
    fn render_frame(&self, frame: usize) -> Result<T::Image, Self::Err>;
    fn sampling_rate(&self) -> u32;
    fn mix_audio(&self, offset: usize, length: usize) -> impl Future<Output = Result<T::Audio, Self::Err>> + Send + '_;
    fn render_param(&self, param: Parameter<ParameterSelect>) -> Result<Parameter<T>, Self::Err>;
}

#[async_trait]
impl<T, O> RealtimeComponentRenderer<T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: RealtimeComponentRenderer<T>,
{
    type Err = <O::Target as RealtimeComponentRenderer<T>>::Err;

    fn get_component_length(&self) -> Option<MarkerTime> {
        self.deref().get_component_length()
    }

    fn render_frame(&self, frame: usize) -> Result<T::Image, Self::Err> {
        self.deref().render_frame(frame)
    }

    fn sampling_rate(&self) -> u32 {
        self.deref().sampling_rate()
    }

    fn mix_audio(&self, offset: usize, length: usize) -> impl Future<Output = Result<T::Audio, Self::Err>> + Send + '_ {
        self.deref().mix_audio(offset, length)
    }

    fn render_param(&self, param: Parameter<ParameterSelect>) -> Result<Parameter<T>, Self::Err> {
        self.deref().render_param(param)
    }
}

#[async_trait]
pub trait RealtimeRenderComponentUsecase<T: ParameterValueType>: Send + Sync {
    type Err: Error + Send + 'static;
    type Renderer: RealtimeComponentRenderer<T> + 'static;
    async fn render_component(&self, component: Arc<ComponentInstance<T>>) -> Result<Self::Renderer, Self::Err>;
}

#[async_trait]
impl<T, O> RealtimeRenderComponentUsecase<T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: RealtimeRenderComponentUsecase<T>,
{
    type Err = <O::Target as RealtimeRenderComponentUsecase<T>>::Err;
    type Renderer = <O::Target as RealtimeRenderComponentUsecase<T>>::Renderer;

    async fn render_component(&self, component: Arc<ComponentInstance<T>>) -> Result<Self::Renderer, Self::Err> {
        self.deref().render_component(component).await
    }
}

pub trait RenderWholeComponentUsecase<T: ParameterValueType, Encoder>: Send + Sync {
    type Err: Error + Send + 'static;
    fn render_and_encode<'life0, 'async_trait>(&'life0 self, component: Arc<ComponentInstance<T>>, encoder: Encoder) -> impl Future<Output = Result<(), Self::Err>> + Send + 'async_trait
    where
        'life0: 'async_trait;
}

impl<T, Encoder, O> RenderWholeComponentUsecase<T, Encoder> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: RenderWholeComponentUsecase<T, Encoder>,
{
    type Err = <O::Target as RenderWholeComponentUsecase<T, Encoder>>::Err;

    fn render_and_encode<'life0, 'async_trait>(&'life0 self, component: Arc<ComponentInstance<T>>, encoder: Encoder) -> impl Future<Output = Result<(), Self::Err>> + Send + 'async_trait
    where
        'life0: 'async_trait,
    {
        self.deref().render_and_encode(component, encoder)
    }
}

#[async_trait]
pub trait EditUsecase<T: ParameterValueType>: Send + Sync {
    type Err: Error + Send + 'static;
    async fn edit(&self, target: &RootComponentClassHandle<T>, command: RootComponentEditCommand<T>) -> Result<(), Self::Err>;
    async fn edit_instance(&self, root: &RootComponentClassHandle<T>, target: &ComponentInstanceId, command: InstanceEditCommand<T>) -> Result<(), Self::Err>;
}

#[async_trait]
impl<T, O> EditUsecase<T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: EditUsecase<T>,
{
    type Err = <O::Target as EditUsecase<T>>::Err;

    async fn edit(&self, target: &RootComponentClassHandle<T>, command: RootComponentEditCommand<T>) -> Result<(), Self::Err> {
        self.deref().edit(target, command).await
    }

    async fn edit_instance(&self, root: &RootComponentClassHandle<T>, target: &ComponentInstanceId, command: InstanceEditCommand<T>) -> Result<(), Self::Err> {
        self.deref().edit_instance(root, target, command).await
    }
}

pub trait SubscribeEditEventUsecase<T: ParameterValueType>: Send + Sync {
    type EditEventListenerGuard: Send + Sync + 'static;
    fn add_edit_event_listener(&self, listener: impl EditEventListener<T> + 'static) -> Self::EditEventListenerGuard;
}

#[async_trait]
impl<T, O> SubscribeEditEventUsecase<T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: SubscribeEditEventUsecase<T>,
{
    type EditEventListenerGuard = <O::Target as SubscribeEditEventUsecase<T>>::EditEventListenerGuard;

    fn add_edit_event_listener(&self, listener: impl EditEventListener<T> + 'static) -> Self::EditEventListenerGuard {
        self.deref().add_edit_event_listener(listener)
    }
}

#[async_trait]
pub trait UndoUsecase<T: ParameterValueType>: Send + Sync {
    async fn undo(&self, component: &RootComponentClassHandle<T>) -> bool;
    async fn undo_instance(&self, root: &RootComponentClassHandle<T>, target: &ComponentInstanceId) -> bool;
}

#[async_trait]
impl<T, O> UndoUsecase<T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: UndoUsecase<T>,
{
    async fn undo(&self, component: &RootComponentClassHandle<T>) -> bool {
        self.deref().undo(component).await
    }

    async fn undo_instance(&self, root: &RootComponentClassHandle<T>, target: &ComponentInstanceId) -> bool {
        self.deref().undo_instance(root, target).await
    }
}

#[async_trait]
pub trait RedoUsecase<T: ParameterValueType>: Send + Sync {
    async fn redo(&self, component: &RootComponentClassHandle<T>) -> bool;
    async fn redo_instance(&self, root: &RootComponentClassHandle<T>, target: &ComponentInstanceId) -> bool;
}

#[async_trait]
impl<T, O> RedoUsecase<T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: RedoUsecase<T>,
{
    async fn redo(&self, component: &RootComponentClassHandle<T>) -> bool {
        self.deref().redo(component).await
    }

    async fn redo_instance(&self, root: &RootComponentClassHandle<T>, target: &ComponentInstanceId) -> bool {
        self.deref().redo_instance(root, target).await
    }
}

// 必須じゃないから後で
// #[async_trait]
// pub trait LoadSettingsUsecase {
//     async fn load_settings(&self);
// }
//
// #[async_trait]
// pub trait WriteSettingsUsecase {
//     async fn write_settings(&self);
// }
