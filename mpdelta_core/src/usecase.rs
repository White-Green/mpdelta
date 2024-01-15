use crate::component::class::ComponentClass;
use crate::component::instance::ComponentInstanceHandle;
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
use tokio::sync::RwLock;

#[async_trait]
pub trait LoadProjectUsecase<K, T: ParameterValueType>: Send + Sync {
    type Err: Error + Send + 'static;
    async fn load_project(&self, path: impl AsRef<Path> + Send + Sync) -> Result<ProjectHandle<K, T>, Self::Err>;
}

#[async_trait]
impl<K, T, O> LoadProjectUsecase<K, T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: LoadProjectUsecase<K, T>,
{
    type Err = <O::Target as LoadProjectUsecase<K, T>>::Err;

    async fn load_project(&self, path: impl AsRef<Path> + Send + Sync) -> Result<ProjectHandle<K, T>, Self::Err> {
        self.deref().load_project(path).await
    }
}

#[async_trait]
pub trait WriteProjectUsecase<K, T: ParameterValueType>: Send + Sync {
    type Err: Error + Send + 'static;
    async fn write_project(&self, project: &ProjectHandle<K, T>, path: impl AsRef<Path> + Send + Sync) -> Result<(), Self::Err>;
}

#[async_trait]
impl<K, T, O> WriteProjectUsecase<K, T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: WriteProjectUsecase<K, T>,
{
    type Err = <O::Target as WriteProjectUsecase<K, T>>::Err;

    async fn write_project(&self, project: &ProjectHandle<K, T>, path: impl AsRef<Path> + Send + Sync) -> Result<(), Self::Err> {
        self.deref().write_project(project, path).await
    }
}

#[async_trait]
pub trait NewProjectUsecase<K, T: ParameterValueType>: Send + Sync {
    async fn new_project(&self) -> ProjectHandle<K, T>;
}

#[async_trait]
impl<K, T, O> NewProjectUsecase<K, T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: NewProjectUsecase<K, T>,
{
    async fn new_project(&self) -> ProjectHandle<K, T> {
        self.deref().new_project().await
    }
}

#[async_trait]
pub trait NewRootComponentClassUsecase<K, T: ParameterValueType>: Send + Sync {
    async fn new_root_component_class(&self) -> RootComponentClassHandle<K, T>;
}

#[async_trait]
impl<K, T, O> NewRootComponentClassUsecase<K, T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: NewRootComponentClassUsecase<K, T>,
{
    async fn new_root_component_class(&self) -> RootComponentClassHandle<K, T> {
        self.deref().new_root_component_class().await
    }
}

#[async_trait]
pub trait SetOwnerForRootComponentClassUsecase<K, T: ParameterValueType>: Send + Sync {
    async fn set_owner_for_root_component_class(&self, component: &RootComponentClassHandle<K, T>, owner: &ProjectHandle<K, T>);
}

#[async_trait]
impl<K, T, O> SetOwnerForRootComponentClassUsecase<K, T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: SetOwnerForRootComponentClassUsecase<K, T>,
{
    async fn set_owner_for_root_component_class(&self, component: &RootComponentClassHandle<K, T>, owner: &ProjectHandle<K, T>) {
        self.deref().set_owner_for_root_component_class(component, owner).await
    }
}

#[async_trait]
pub trait GetLoadedProjectsUsecase<K, T: ParameterValueType>: Send + Sync {
    async fn get_loaded_projects(&self) -> Cow<[ProjectHandle<K, T>]>;
}

#[async_trait]
impl<K, T, O> GetLoadedProjectsUsecase<K, T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: GetLoadedProjectsUsecase<K, T>,
{
    async fn get_loaded_projects(&self) -> Cow<[ProjectHandle<K, T>]> {
        self.deref().get_loaded_projects().await
    }
}

#[async_trait]
pub trait GetRootComponentClassesUsecase<K, T: ParameterValueType>: Send + Sync {
    async fn get_root_component_classes(&self, project: &ProjectHandle<K, T>) -> Cow<[RootComponentClassHandle<K, T>]>;
}

#[async_trait]
impl<K, T, O> GetRootComponentClassesUsecase<K, T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: GetRootComponentClassesUsecase<K, T>,
{
    async fn get_root_component_classes(&self, project: &ProjectHandle<K, T>) -> Cow<[RootComponentClassHandle<K, T>]> {
        self.deref().get_root_component_classes(project).await
    }
}

#[async_trait]
pub trait GetAvailableComponentClassesUsecase<K, T: ParameterValueType>: Send + Sync {
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<K, T>>>]>;
}

#[async_trait]
impl<K, T, O> GetAvailableComponentClassesUsecase<K, T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: GetAvailableComponentClassesUsecase<K, T>,
{
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<K, T>>>]> {
        self.deref().get_available_component_classes().await
    }
}

pub trait RealtimeComponentRenderer<T: ParameterValueType>: Send + Sync {
    type Err: Error + Send + 'static;
    fn get_frame_count(&self) -> usize;
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

    fn get_frame_count(&self) -> usize {
        self.deref().get_frame_count()
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
pub trait RealtimeRenderComponentUsecase<K, T: ParameterValueType>: Send + Sync {
    type Err: Error + Send + 'static;
    type Renderer: RealtimeComponentRenderer<T> + 'static;
    async fn render_component(&self, component: &ComponentInstanceHandle<K, T>) -> Result<Self::Renderer, Self::Err>;
}

#[async_trait]
impl<K, T, O> RealtimeRenderComponentUsecase<K, T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: RealtimeRenderComponentUsecase<K, T>,
{
    type Err = <O::Target as RealtimeRenderComponentUsecase<K, T>>::Err;
    type Renderer = <O::Target as RealtimeRenderComponentUsecase<K, T>>::Renderer;

    async fn render_component(&self, component: &ComponentInstanceHandle<K, T>) -> Result<Self::Renderer, Self::Err> {
        self.deref().render_component(component).await
    }
}

pub trait RenderWholeComponentUsecase<K, T: ParameterValueType, Encoder>: Send + Sync {
    type Err: Error + Send + 'static;
    fn render_and_encode<'life0, 'life1, 'async_trait>(&'life0 self, component: &'life1 ComponentInstanceHandle<K, T>, encoder: Encoder) -> impl Future<Output = Result<(), Self::Err>> + Send + 'async_trait
    where
        'life0: 'async_trait,
        'life1: 'async_trait;
}

impl<K, T, Encoder, O> RenderWholeComponentUsecase<K, T, Encoder> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: RenderWholeComponentUsecase<K, T, Encoder>,
{
    type Err = <O::Target as RenderWholeComponentUsecase<K, T, Encoder>>::Err;

    fn render_and_encode<'life0, 'life1, 'async_trait>(&'life0 self, component: &'life1 ComponentInstanceHandle<K, T>, encoder: Encoder) -> impl Future<Output = Result<(), Self::Err>> + Send + 'async_trait
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
    {
        self.deref().render_and_encode(component, encoder)
    }
}

#[async_trait]
pub trait EditUsecase<K, T: ParameterValueType>: Send + Sync {
    type Err: Error + Send + 'static;
    async fn edit(&self, target: &RootComponentClassHandle<K, T>, command: RootComponentEditCommand<K, T>) -> Result<(), Self::Err>;
    async fn edit_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>, command: InstanceEditCommand<K, T>) -> Result<(), Self::Err>;
}

#[async_trait]
impl<K, T, O> EditUsecase<K, T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: EditUsecase<K, T>,
{
    type Err = <O::Target as EditUsecase<K, T>>::Err;

    async fn edit(&self, target: &RootComponentClassHandle<K, T>, command: RootComponentEditCommand<K, T>) -> Result<(), Self::Err> {
        self.deref().edit(target, command).await
    }

    async fn edit_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>, command: InstanceEditCommand<K, T>) -> Result<(), Self::Err> {
        self.deref().edit_instance(root, target, command).await
    }
}

pub trait SubscribeEditEventUsecase<K, T: ParameterValueType>: Send + Sync {
    type EditEventListenerGuard: Send + Sync + 'static;
    fn add_edit_event_listener(&self, listener: impl EditEventListener<K, T> + 'static) -> Self::EditEventListenerGuard;
}

#[async_trait]
impl<K, T, O> SubscribeEditEventUsecase<K, T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: SubscribeEditEventUsecase<K, T>,
{
    type EditEventListenerGuard = <O::Target as SubscribeEditEventUsecase<K, T>>::EditEventListenerGuard;

    fn add_edit_event_listener(&self, listener: impl EditEventListener<K, T> + 'static) -> Self::EditEventListenerGuard {
        self.deref().add_edit_event_listener(listener)
    }
}

#[async_trait]
pub trait UndoUsecase<K, T: ParameterValueType>: Send + Sync {
    async fn undo(&self, component: &RootComponentClassHandle<K, T>) -> bool;
    async fn undo_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>) -> bool;
}

#[async_trait]
impl<K, T, O> UndoUsecase<K, T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: UndoUsecase<K, T>,
{
    async fn undo(&self, component: &RootComponentClassHandle<K, T>) -> bool {
        self.deref().undo(component).await
    }

    async fn undo_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>) -> bool {
        self.deref().undo_instance(root, target).await
    }
}

#[async_trait]
pub trait RedoUsecase<K, T: ParameterValueType>: Send + Sync {
    async fn redo(&self, component: &RootComponentClassHandle<K, T>) -> bool;
    async fn redo_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>) -> bool;
}

#[async_trait]
impl<K, T, O> RedoUsecase<K, T> for O
where
    T: ParameterValueType,
    O: Deref + Send + Sync,
    O::Target: RedoUsecase<K, T>,
{
    async fn redo(&self, component: &RootComponentClassHandle<K, T>) -> bool {
        self.deref().redo(component).await
    }

    async fn redo_instance(&self, root: &RootComponentClassHandle<K, T>, target: &ComponentInstanceHandle<K, T>) -> bool {
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
