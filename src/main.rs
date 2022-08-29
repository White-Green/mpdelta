use async_trait::async_trait;
use mpdelta_components::rectangle::RectangleClass;
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::{ComponentClassLoader, MPDeltaCore};
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use mpdelta_core_vulkano::ImageType;
use mpdelta_gui::view::MPDeltaGUI;
use mpdelta_gui::viewmodel::ViewModelParams;
use mpdelta_gui_vulkano::MPDeltaGUIVulkano;
use mpdelta_renderer::MPDeltaRendererBuilder;
use mpdelta_services::component_class_loader::TemporaryComponentClassLoader;
use mpdelta_services::history::InMemoryEditHistoryStore;
use mpdelta_services::id_generator::UniqueIdGenerator;
use mpdelta_services::project_editor::ProjectEditor;
use mpdelta_services::project_io::{TemporaryProjectLoader, TemporaryProjectWriter};
use mpdelta_services::project_store::InMemoryProjectStore;
use mpdelta_video_renderer_vulkano::MPDeltaVideoRendererBuilder;
use std::borrow::Cow;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use vulkano::device::physical::{PhysicalDevice, PhysicalDeviceType};
use vulkano::device::{Device, DeviceCreateInfo, Features, Queue, QueueCreateInfo};
use vulkano::instance::{Instance, InstanceCreateInfo};
use vulkano::swapchain::Surface;
use vulkano::Version;
use vulkano_win::VkSurfaceBuild;
use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};

struct ValueType;

impl<'a> ParameterValueType<'a> for ValueType {
    type Image = ImageType;
    type Audio = ();
    type Video = ();
    type File = ();
    type String = ();
    type Select = ();
    type Boolean = ();
    type Radio = ();
    type Integer = ();
    type RealNumber = ();
    type Vec2 = ();
    type Vec3 = ();
    type Dictionary = ();
    type ComponentClass = ();
}

#[derive(Default)]
struct ComponentClassList(Vec<StaticPointerOwned<RwLock<dyn ComponentClass<ValueType>>>>, Vec<StaticPointer<RwLock<dyn ComponentClass<ValueType>>>>);

impl ComponentClassList {
    fn new() -> ComponentClassList {
        Default::default()
    }

    fn add(&mut self, class: impl ComponentClass<ValueType> + 'static) -> &mut Self {
        let class = StaticPointerOwned::new(RwLock::new(class)).map(|arc| arc as _, |weak| weak as _);
        let reference = StaticPointerOwned::reference(&class).clone();
        self.0.push(class);
        self.1.push(reference);
        self
    }
}

#[async_trait]
impl ComponentClassLoader<ValueType> for ComponentClassList {
    async fn get_available_component_classes(&self) -> Cow<[StaticPointer<RwLock<dyn ComponentClass<ValueType>>>]> {
        Cow::Borrowed(&self.1)
    }
}

fn main() {
    let (instance, device, queue, event_loop, surface) = initialize_graphics();
    let runtime = Runtime::new().unwrap();
    let id_generator = Arc::new(UniqueIdGenerator::new());
    let project_loader = Arc::new(TemporaryProjectLoader);
    let project_writer = Arc::new(TemporaryProjectWriter);
    let project_memory = Arc::new(InMemoryProjectStore::<ValueType>::new());
    let mut component_class_loader = ComponentClassList::new();
    component_class_loader.add(RectangleClass::new(Arc::clone(&queue)));
    let component_class_loader = Arc::new(component_class_loader);
    let component_renderer_builder = Arc::new(MPDeltaRendererBuilder::new(Arc::clone(&id_generator), Arc::new(MPDeltaVideoRendererBuilder::new(Arc::clone(&device), Arc::clone(&queue))), Arc::new(())));
    let project_editor = Arc::new(ProjectEditor::new());
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
    ));
    let params = ViewModelParams::new(
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
    );
    let gui = MPDeltaGUI::new(params);
    let gui = MPDeltaGUIVulkano::new(device, queue, event_loop, surface, gui);
    gui.main();
    drop(core);
    drop(runtime);
}

fn initialize_graphics() -> (Arc<Instance>, Arc<Device>, Arc<Queue>, EventLoop<()>, Arc<Surface<Window>>) {
    let required_extensions = vulkano_win::required_extensions().union(&mpdelta_gui_vulkano::required_extensions());
    let instance = Instance::new(InstanceCreateInfo {
        enabled_extensions: required_extensions,
        max_api_version: Some(Version::V1_1),
        ..Default::default()
    })
    .unwrap();

    let event_loop = EventLoop::new();
    let surface = WindowBuilder::new().with_inner_size(PhysicalSize::new(1920, 1080)).with_title("mpdelta").build_vk_surface(&event_loop, Arc::clone(&instance)).unwrap();

    let device_extensions = mpdelta_gui_vulkano::device_extensions();

    let (physical_device, queue_family) = PhysicalDevice::enumerate(&instance)
        .filter(|&p| p.supported_extensions().is_superset_of(&device_extensions))
        .filter_map(|p| p.queue_families().find(|&q| q.supports_graphics() && q.supports_surface(&surface).unwrap_or(false)).map(|q| (p, q)))
        .min_by_key(|(p, _)| match p.properties().device_type {
            PhysicalDeviceType::DiscreteGpu => 0,
            PhysicalDeviceType::IntegratedGpu => 1,
            PhysicalDeviceType::VirtualGpu => 2,
            PhysicalDeviceType::Cpu => 3,
            PhysicalDeviceType::Other => 4,
        })
        .unwrap();

    let (device, mut queues) = Device::new(
        physical_device,
        DeviceCreateInfo {
            enabled_extensions: device_extensions,
            enabled_features: Features { ..Features::none() },
            queue_create_infos: vec![QueueCreateInfo::family(queue_family)],
            ..Default::default()
        },
    )
    .unwrap();

    let queue = queues.next().unwrap();
    (instance, device, queue, event_loop, surface)
}
