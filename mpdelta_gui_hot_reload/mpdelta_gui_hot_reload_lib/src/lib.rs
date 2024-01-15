use crate::dyn_trait::AudioTypePlayerDyn;
use async_trait::async_trait;
use mpdelta_async_runtime::AsyncRuntimeDyn;
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::ComponentClassLoader;
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use mpdelta_core_audio::AudioType;
use mpdelta_core_vulkano::ImageType;
use mpdelta_dyn_usecase::{
    EditUsecaseDyn, GetAvailableComponentClassesUsecaseDyn, GetLoadedProjectsUsecaseDyn, GetRootComponentClassesUsecaseDyn, LoadProjectUsecaseDyn, NewProjectUsecaseDyn, NewRootComponentClassUsecaseDyn, RealtimeRenderComponentUsecaseDyn, RedoUsecaseDyn, RenderWholeComponentUsecaseDyn,
    SetOwnerForRootComponentClassUsecaseDyn, SubscribeEditEventUsecaseDyn, UndoUsecaseDyn, WriteProjectUsecaseDyn,
};
use mpdelta_gui::view::{Gui, MPDeltaGUI};
use mpdelta_gui::viewmodel::ViewModelParamsImpl;
use mpdelta_renderer::VideoEncoderBuilderDyn;
use std::borrow::Cow;
use std::sync::Arc;
use tokio::sync::RwLock;

pub mod dyn_trait;

pub struct ProjectKey;

pub struct ValueType;

impl ParameterValueType for ValueType {
    type Image = ImageType;
    type Audio = AudioType;
    type Binary = ();
    type String = ();
    type Integer = ();
    type RealNumber = ();
    type Boolean = ();
    type Dictionary = ();
    type Array = ();
    type ComponentClass = ();
}

#[derive(Default)]
pub struct ComponentClassList(Vec<StaticPointerOwned<RwLock<dyn ComponentClass<ProjectKey, ValueType>>>>, Vec<StaticPointer<RwLock<dyn ComponentClass<ProjectKey, ValueType>>>>);

impl ComponentClassList {
    pub fn new() -> ComponentClassList {
        Default::default()
    }

    pub fn add(&mut self, class: impl ComponentClass<ProjectKey, ValueType> + 'static) -> &mut Self {
        let class = StaticPointerOwned::new(RwLock::new(class)).map(|arc| arc as _, |weak| weak as _);
        let reference = StaticPointerOwned::reference(&class).clone();
        self.0.push(class);
        self.1.push(reference);
        self
    }
}

#[async_trait]
impl ComponentClassLoader<ProjectKey, ValueType> for ComponentClassList {
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<ProjectKey, ValueType>>>]> {
        Cow::Borrowed(&self.1)
    }
}

pub type Param = ViewModelParamsImpl<
    ProjectKey,
    Arc<dyn AsyncRuntimeDyn<()>>,
    Arc<dyn EditUsecaseDyn<ProjectKey, ValueType> + Send + Sync>,
    Arc<dyn SubscribeEditEventUsecaseDyn<ProjectKey, ValueType> + Send + Sync>,
    Arc<dyn GetAvailableComponentClassesUsecaseDyn<ProjectKey, ValueType> + Send + Sync>,
    Arc<dyn GetLoadedProjectsUsecaseDyn<ProjectKey, ValueType> + Send + Sync>,
    Arc<dyn GetRootComponentClassesUsecaseDyn<ProjectKey, ValueType> + Send + Sync>,
    Arc<dyn LoadProjectUsecaseDyn<ProjectKey, ValueType> + Send + Sync>,
    Arc<dyn NewProjectUsecaseDyn<ProjectKey, ValueType> + Send + Sync>,
    Arc<dyn NewRootComponentClassUsecaseDyn<ProjectKey, ValueType> + Send + Sync>,
    Arc<dyn RealtimeRenderComponentUsecaseDyn<ProjectKey, ValueType> + Send + Sync>,
    Arc<dyn RedoUsecaseDyn<ProjectKey, ValueType> + Send + Sync>,
    Arc<dyn SetOwnerForRootComponentClassUsecaseDyn<ProjectKey, ValueType> + Send + Sync>,
    Arc<dyn UndoUsecaseDyn<ProjectKey, ValueType> + Send + Sync>,
    Arc<dyn WriteProjectUsecaseDyn<ProjectKey, ValueType> + Send + Sync>,
    Arc<dyn AudioTypePlayerDyn<AudioType> + Send + Sync>,
    Box<dyn VideoEncoderBuilderDyn<ImageType, AudioType>>,
    Arc<dyn RenderWholeComponentUsecaseDyn<ProjectKey, ValueType, Box<dyn VideoEncoderBuilderDyn<ImageType, AudioType>>>>,
>;

#[no_mangle]
pub fn create_gui(params: Param) -> Box<dyn Gui<<ValueType as ParameterValueType>::Image> + Send + Sync> {
    Box::new(MPDeltaGUI::new(params))
}
