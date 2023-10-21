use async_trait::async_trait;
use mpdelta_components::rectangle::RectangleClass;
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::component::parameter::{AudioRequiredParamsFixed, ParameterValueType};
use mpdelta_core::core::{ComponentClassLoader, MPDeltaCore};
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use mpdelta_core_vulkano::ImageType;
use mpdelta_gui::view::MPDeltaGUI;
use mpdelta_gui::viewmodel::ViewModelParamsImpl;
use mpdelta_gui_vulkano::MPDeltaGUIVulkano;
use mpdelta_renderer::{Combiner, CombinerBuilder, MPDeltaRendererBuilder};
use mpdelta_rendering_controller::LRUCacheRenderingControllerBuilder;
use mpdelta_services::history::InMemoryEditHistoryStore;
use mpdelta_services::id_generator::UniqueIdGenerator;
use mpdelta_services::project_editor::ProjectEditor;
use mpdelta_services::project_io::{TemporaryProjectLoader, TemporaryProjectWriter};
use mpdelta_services::project_store::InMemoryProjectStore;
use mpdelta_video_renderer_vulkano::ImageCombinerBuilder;
use qcell::TCellOwner;
use std::borrow::Cow;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use vulkano::command_buffer::allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo};
use vulkano::instance::InstanceCreateInfo;
use vulkano::Version;
use vulkano_util::context::{VulkanoConfig, VulkanoContext};
use vulkano_util::window::VulkanoWindows;
use winit::event_loop::EventLoop;

struct ProjectKey;

struct ValueType;

impl ParameterValueType for ValueType {
    type Image = ImageType;
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

struct TmpAudioCombiner;

impl CombinerBuilder<()> for TmpAudioCombiner {
    type Request = ();
    type Param = AudioRequiredParamsFixed;
    type Combiner = TmpAudioCombiner;

    fn new_combiner(&self, _request: Self::Request) -> Self::Combiner {
        TmpAudioCombiner
    }
}

impl Combiner<()> for TmpAudioCombiner {
    type Param = AudioRequiredParamsFixed;

    fn add(&mut self, _data: (), _param: Self::Param) {}

    fn collect(self) {}
}

#[derive(Default)]
struct ComponentClassList(Vec<StaticPointerOwned<RwLock<dyn ComponentClass<ProjectKey, ValueType>>>>, Vec<StaticPointer<RwLock<dyn ComponentClass<ProjectKey, ValueType>>>>);

impl ComponentClassList {
    fn new() -> ComponentClassList {
        Default::default()
    }

    fn add(&mut self, class: impl ComponentClass<ProjectKey, ValueType> + 'static) -> &mut Self {
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

fn main() {
    let context = Arc::new(VulkanoContext::new(VulkanoConfig {
        instance_create_info: InstanceCreateInfo {
            max_api_version: Some(Version::V1_2),
            ..InstanceCreateInfo::default()
        },
        ..VulkanoConfig::default()
    }));
    let event_loop = EventLoop::new();
    let windows = VulkanoWindows::default();
    let runtime = Runtime::new().unwrap();
    let id_generator = Arc::new(UniqueIdGenerator::new());
    let project_loader = Arc::new(TemporaryProjectLoader);
    let project_writer = Arc::new(TemporaryProjectWriter);
    let project_memory = Arc::new(InMemoryProjectStore::<ProjectKey, ValueType>::new());
    let mut component_class_loader = ComponentClassList::new();
    let command_buffer_allocator = StandardCommandBufferAllocator::new(Arc::clone(context.device()), StandardCommandBufferAllocatorCreateInfo::default());
    component_class_loader.add(RectangleClass::new(Arc::clone(context.graphics_queue()), context.memory_allocator(), &command_buffer_allocator));
    let component_class_loader = Arc::new(component_class_loader);
    let key = Arc::new(RwLock::new(TCellOwner::<ProjectKey>::new()));
    let component_renderer_builder = Arc::new(MPDeltaRendererBuilder::new(
        Arc::clone(&id_generator),
        Arc::new(ImageCombinerBuilder::new(Arc::clone(&context))),
        Arc::new(LRUCacheRenderingControllerBuilder::new()),
        Arc::new(TmpAudioCombiner),
        Arc::clone(&key),
        runtime.handle().clone(),
    ));
    let project_editor = Arc::new(ProjectEditor::new(Arc::clone(&key)));
    let edit_history = Arc::new(InMemoryEditHistoryStore::new(100));
    let core = Arc::new(MPDeltaCore::new(
        id_generator,
        project_loader,
        project_writer,
        Arc::clone(&project_memory),
        project_memory,
        component_class_loader,
        component_renderer_builder,
        project_editor,
        edit_history,
        Arc::clone(&key),
    ));
    let params = ViewModelParamsImpl::new(
        runtime.handle().clone(),
        Arc::clone(&core),
        Arc::clone(&core),
        Arc::clone(&core),
        Arc::clone(&core),
        Arc::clone(&core),
        Arc::clone(&core),
        Arc::clone(&core),
        Arc::clone(&core),
        Arc::clone(&core),
        Arc::clone(&core),
        Arc::clone(&core),
        Arc::clone(&core),
        Arc::clone(&core),
        Arc::clone(&key),
    );
    let gui = MPDeltaGUI::new(params);
    let gui = MPDeltaGUIVulkano::new(context, event_loop, windows, gui);
    gui.main();
    drop(core);
    drop(runtime);
}
