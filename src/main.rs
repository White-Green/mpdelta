use ash::vk;
use async_trait::async_trait;
use cpal::traits::HostTrait;
use futures::{pin_mut, stream, FutureExt, StreamExt};
use mpdelta_audio_mixer::MPDeltaAudioMixerBuilder;
use mpdelta_components::multimedia_loader::FfmpegMultimediaLoaderClass;
use mpdelta_components::parameter::file_reader::FileReaderParamManager;
use mpdelta_components::rectangle::RectangleClass;
use mpdelta_components::sine_audio::SineAudio;
use mpdelta_components::text_renderer::TextRendererClass;
use mpdelta_core::component::class::{ComponentClass, ComponentClassIdentifier};
use mpdelta_core::component::parameter::value::{DynEditableLerpEasingValueManager, DynEditableSelfValueManager, LinearEasing};
use mpdelta_core::component::parameter::{AbstractFile, ParameterAllValues, ParameterValueRaw, ParameterValueType};
use mpdelta_core::core::{ComponentClassLoader, MPDeltaCore, MPDeltaCoreArgs, NewWithArgs};
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use mpdelta_core_audio::AudioType;
use mpdelta_core_vulkano::ImageType;
use mpdelta_gui::viewmodel::ViewModelParamsImpl;
use mpdelta_gui_audio_player_cpal::CpalAudioPlayer;
use mpdelta_gui_wgpu::MPDeltaGUIWgpu;
use mpdelta_multimedia_encoder_ffmpeg::{FfmpegEncodeSettings, FfmpegEncoderBuilder};
use mpdelta_project_serialize::MPDeltaProjectSerializer;
use mpdelta_renderer::MPDeltaRendererBuilder;
use mpdelta_rendering_controller::LookaheadRenderingControllerBuilder;
use mpdelta_services::easing_loader::InMemoryEasingLoader;
use mpdelta_services::history::InMemoryEditHistoryStore;
use mpdelta_services::id_generator::UniqueIdGenerator;
use mpdelta_services::project_editor::ProjectEditor;
use mpdelta_services::project_io::{LocalFSProjectLoader, LocalFSProjectWriter};
use mpdelta_services::project_store::InMemoryProjectStore;
use mpdelta_services::value_manager_loader::InMemoryValueManagerLoader;
use mpdelta_video_renderer_vulkano::ImageCombinerBuilder;
use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::CStr;
use std::fs::File;
use std::os::raw::c_char;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use vulkano::command_buffer::allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo};
use vulkano::device::physical::PhysicalDeviceType;
use vulkano::device::{Device, DeviceCreateInfo, DeviceExtensions, Features, QueueCreateInfo, QueueFlags};
use vulkano::instance::InstanceExtensions;
use vulkano::memory::allocator::StandardMemoryAllocator;
use vulkano::{VulkanLibrary, VulkanObject};

struct ValueType;

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

    async fn component_class_by_identifier(&self, identifier: ComponentClassIdentifier<'_>) -> Option<StaticPointer<RwLock<dyn ComponentClass<ValueType>>>> {
        let map = stream::iter(self.0.iter()).filter(|&class| class.read().map(|class| class.identifier() == identifier)).map(|class| StaticPointerOwned::reference(class).clone());
        pin_mut!(map);
        map.next().await
    }
}

struct ValueManagerLoaderTypes;

impl ParameterValueType for ValueManagerLoaderTypes {
    type Image = Arc<InMemoryValueManagerLoader<ImageType>>;
    type Audio = Arc<InMemoryValueManagerLoader<AudioType>>;
    type Binary = Arc<InMemoryValueManagerLoader<AbstractFile>>;
    type String = Arc<InMemoryValueManagerLoader<String>>;
    type Integer = Arc<InMemoryValueManagerLoader<i64>>;
    type RealNumber = Arc<InMemoryValueManagerLoader<f64>>;
    type Boolean = Arc<InMemoryValueManagerLoader<bool>>;
    type Dictionary = Arc<InMemoryValueManagerLoader<HashMap<String, ParameterValueRaw<ImageType, AudioType>>>>;
    type Array = Arc<InMemoryValueManagerLoader<Vec<ParameterValueRaw<ImageType, AudioType>>>>;
    type ComponentClass = Arc<InMemoryValueManagerLoader<()>>;
}

fn main() {
    let GpuHandle {
        vulkano_instance: _,
        vulkano_device,
        vulkano_queue,
        vulkano_memory_allocator,
        wgpu_instance,
        wgpu_adapter,
        wgpu_device,
        wgpu_queue,
    } = initialize_gpu();
    let runtime = Runtime::new().unwrap();
    let id_generator = Arc::new(UniqueIdGenerator::new());
    let project_loader = Arc::new(LocalFSProjectLoader);
    let project_writer = Arc::new(LocalFSProjectWriter);
    let project_memory = Arc::new(InMemoryProjectStore::<ValueType>::new());
    let mut component_class_loader = ComponentClassList::new();
    let command_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(Arc::clone(&vulkano_device), StandardCommandBufferAllocatorCreateInfo::default()));
    component_class_loader.add(RectangleClass::new(Arc::clone(&vulkano_queue), &vulkano_memory_allocator, &command_buffer_allocator));
    component_class_loader.add(SineAudio::new());
    component_class_loader.add(FfmpegMultimediaLoaderClass::new(&vulkano_queue, &vulkano_memory_allocator, &command_buffer_allocator));
    component_class_loader.add(TextRendererClass::new(&vulkano_device, &vulkano_queue, &vulkano_memory_allocator));
    let component_class_loader = Arc::new(component_class_loader);
    let value_managers = ParameterAllValues::<ValueManagerLoaderTypes> {
        image: Arc::new(InMemoryValueManagerLoader::from_iter([], [])),
        audio: Arc::new(InMemoryValueManagerLoader::from_iter([], [])),
        binary: Arc::new(InMemoryValueManagerLoader::from_iter([Arc::new(FileReaderParamManager) as _], [])),
        string: Arc::new(InMemoryValueManagerLoader::from_iter([Arc::new(DynEditableSelfValueManager::default()) as _], [Arc::new(DynEditableSelfValueManager::default()) as _])),
        integer: Arc::new(InMemoryValueManagerLoader::from_iter([Arc::new(DynEditableSelfValueManager::default()) as _], [Arc::new(DynEditableSelfValueManager::default()) as _])),
        real_number: Arc::new(InMemoryValueManagerLoader::from_iter(
            [Arc::new(DynEditableSelfValueManager::default()) as _],
            [Arc::new(DynEditableSelfValueManager::default()) as _, Arc::new(DynEditableLerpEasingValueManager::default()) as _],
        )),
        boolean: Arc::new(InMemoryValueManagerLoader::from_iter([Arc::new(DynEditableSelfValueManager::default()) as _], [Arc::new(DynEditableSelfValueManager::default()) as _])),
        dictionary: Arc::new(InMemoryValueManagerLoader::from_iter([], [])),
        array: Arc::new(InMemoryValueManagerLoader::from_iter([], [])),
        component_class: Arc::new(InMemoryValueManagerLoader::from_iter([], [])),
    };
    let quaternion_manager = Arc::new(InMemoryValueManagerLoader::from_iter(
        [Arc::new(DynEditableSelfValueManager::default()) as _],
        [Arc::new(DynEditableSelfValueManager::default()) as _, Arc::new(DynEditableLerpEasingValueManager::default()) as _],
    ));
    let easing_manager = Arc::new(InMemoryEasingLoader::from_iter([Arc::new(LinearEasing) as _]));
    let project_serializer = Arc::new(MPDeltaProjectSerializer::new(runtime.handle().clone(), Arc::clone(&id_generator), Arc::clone(&component_class_loader), value_managers, quaternion_manager, easing_manager));
    let component_renderer_builder = Arc::new(MPDeltaRendererBuilder::new(
        Arc::new(ImageCombinerBuilder::new(Arc::clone(&vulkano_device), Arc::clone(&vulkano_queue))),
        Arc::new(LookaheadRenderingControllerBuilder::new()),
        Arc::new(MPDeltaAudioMixerBuilder::new()),
        runtime.handle().clone(),
    ));
    let editor = Arc::new(ProjectEditor::new(Arc::clone(&id_generator)));
    let edit_history = Arc::new(InMemoryEditHistoryStore::new(100));
    let core = Arc::new(MPDeltaCore::new(MPDeltaCoreArgs {
        id_generator: Arc::clone(&id_generator),
        project_serializer,
        project_loader,
        project_writer,
        project_memory: Arc::clone(&project_memory),
        root_component_class_memory: project_memory,
        component_class_loader,
        component_renderer_builder: Arc::clone(&component_renderer_builder),
        video_encoder: component_renderer_builder,
        editor,
        edit_history,
    }));
    let audio_player = Arc::new(
        CpalAudioPlayer::new(
            || {
                let host = cpal::default_host();
                host.default_output_device().unwrap()
            },
            runtime.handle(),
        )
        .unwrap(),
    );
    let encoder_builder = Arc::new(FfmpegEncoderBuilder::new(Arc::clone(&vulkano_device), Arc::clone(&vulkano_queue), Arc::clone(&vulkano_memory_allocator)));
    let params = ViewModelParamsImpl {
        runtime: runtime.handle().clone(),
        id: Arc::clone(&id_generator),
        edit: Arc::clone(&core),
        subscribe_edit_event: Arc::clone(&core),
        get_available_component_classes: Arc::clone(&core),
        get_loaded_projects: Arc::clone(&core),
        get_root_component_classes: Arc::clone(&core),
        load_project: Arc::clone(&core),
        new_project: Arc::clone(&core),
        new_root_component_class: Arc::clone(&core),
        realtime_render_component: Arc::clone(&core),
        redo: Arc::clone(&core),
        set_owner_for_root_component_class: Arc::clone(&core),
        undo: Arc::clone(&core),
        write_project: Arc::clone(&core),
        audio_player,
        available_video_codec: encoder_builder.available_video_codec::<FfmpegEncodeSettings<File>>().into_iter().collect::<Vec<_>>().into(),
        available_audio_codec: encoder_builder.available_audio_codec().into_iter().collect::<Vec<_>>().into(),
        encode: Arc::clone(&core),
    };
    let gui = mpdelta_gui::new_gui(params);
    let gui = MPDeltaGUIWgpu::new(Arc::new(wgpu_instance), Arc::new(wgpu_adapter), Arc::new(wgpu_device), Arc::new(wgpu_queue), gui);
    gui.main();
    drop(core);
    drop(runtime);
}

#[allow(unused)]
struct GpuHandle {
    vulkano_instance: Arc<vulkano::instance::Instance>,
    vulkano_device: Arc<vulkano::device::Device>,
    vulkano_queue: Arc<vulkano::device::Queue>,
    vulkano_memory_allocator: Arc<vulkano::memory::allocator::StandardMemoryAllocator>,
    wgpu_instance: wgpu::Instance,
    wgpu_adapter: wgpu::Adapter,
    wgpu_device: wgpu::Device,
    wgpu_queue: wgpu::Queue,
}

fn initialize_gpu() -> GpuHandle {
    let entry = ash::Entry::linked();
    struct LibraryLoader(ash::Entry);
    unsafe impl vulkano::library::Loader for LibraryLoader {
        unsafe fn get_instance_proc_addr(&self, instance: vk::Instance, name: *const c_char) -> vk::PFN_vkVoidFunction {
            self.0.get_instance_proc_addr(instance, name)
        }
    }
    let vulkan_library = VulkanLibrary::with_loader(LibraryLoader(entry.clone())).unwrap();
    let instance_extensions = vulkan_library.supported_extensions().intersection(&InstanceExtensions {
        khr_surface: true,
        khr_xlib_surface: true,
        khr_xcb_surface: true,
        khr_wayland_surface: true,
        khr_android_surface: true,
        khr_win32_surface: true,
        mvk_ios_surface: true,
        mvk_macos_surface: true,
        ..InstanceExtensions::empty()
    });
    let vulkano_instance = vulkano::instance::Instance::new(
        vulkan_library,
        vulkano::instance::InstanceCreateInfo {
            enabled_extensions: instance_extensions,
            ..vulkano::instance::InstanceCreateInfo::default()
        },
    )
    .unwrap();
    let vulkano_physical_device = vulkano_instance
        .enumerate_physical_devices()
        .unwrap()
        .max_by_key(|physical_device| match physical_device.properties().device_type {
            PhysicalDeviceType::DiscreteGpu => 5,
            PhysicalDeviceType::IntegratedGpu => 4,
            PhysicalDeviceType::VirtualGpu => 3,
            PhysicalDeviceType::Cpu => 2,
            PhysicalDeviceType::Other => 1,
            _ => 0,
        })
        .unwrap();
    let queue_index = vulkano_physical_device.queue_family_properties().iter().position(|q| q.queue_flags.contains(QueueFlags::GRAPHICS | QueueFlags::COMPUTE)).unwrap();

    let queue_create_infos = vec![QueueCreateInfo {
        queue_family_index: queue_index as u32,
        queues: vec![0.5; 2],
        ..Default::default()
    }];

    let (vulkano_device, mut queues) = {
        Device::new(
            vulkano_physical_device.clone(),
            DeviceCreateInfo {
                queue_create_infos,
                enabled_extensions: DeviceExtensions { khr_swapchain: true, ..DeviceExtensions::empty() },
                enabled_features: Features::empty(),
                ..Default::default()
            },
        )
        .expect("failed to create device")
    };
    let vulkano_queue = queues.next().unwrap();

    let ash_instance = unsafe { ash::Instance::load(entry.static_fn(), vulkano_instance.handle()) };
    let wgpu_hal_instance = unsafe { wgpu::hal::vulkan::Instance::from_raw(entry, ash_instance, u32::try_from(vulkano_instance.api_version()).unwrap(), 0, None, instance_extension_into_vec(instance_extensions), wgpu::InstanceFlags::empty(), false, None).unwrap() };

    let wgpu_hal_adapter = wgpu_hal_instance.expose_adapter(vulkano_physical_device.handle()).unwrap();

    let queue_family_index = vulkano_physical_device.queue_family_properties().iter().position(|q| q.queue_flags.contains(QueueFlags::GRAPHICS | QueueFlags::COMPUTE)).unwrap();
    let extensions = wgpu_hal_adapter.adapter.required_device_extensions(wgpu_hal_adapter.features);
    let ash_device = unsafe { ash::Device::load(wgpu_hal_instance.shared_instance().raw_instance().fp_v1_0(), vulkano_device.handle()) };
    let wgpu_hal_device = unsafe { wgpu_hal_adapter.adapter.device_from_raw(ash_device, false, &extensions, wgpu_hal_adapter.features, queue_family_index as u32, 1).unwrap() };

    let wgpu_instance;
    let wgpu_adapter;
    let wgpu_device;
    let wgpu_queue;
    unsafe {
        wgpu_instance = wgpu::Instance::from_hal::<wgpu::hal::vulkan::Api>(wgpu_hal_instance);
        wgpu_adapter = wgpu_instance.create_adapter_from_hal::<wgpu::hal::vulkan::Api>(wgpu_hal_adapter);
        (wgpu_device, wgpu_queue) = wgpu_adapter
            .create_device_from_hal(
                wgpu_hal_device,
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: Default::default(),
                    required_limits: Default::default(),
                },
                None,
            )
            .unwrap();
    }

    let vulkano_memory_allocator = Arc::new(StandardMemoryAllocator::new_default(Arc::clone(&vulkano_device)));

    GpuHandle {
        vulkano_instance,
        vulkano_device,
        vulkano_queue,
        vulkano_memory_allocator,
        wgpu_instance,
        wgpu_adapter,
        wgpu_device,
        wgpu_queue,
    }
}

fn instance_extension_into_vec(instance_extensions: InstanceExtensions) -> Vec<&'static CStr> {
    #[allow(clippy::type_complexity)]
    const EXTENSIONS: &[(fn(&InstanceExtensions) -> bool, &CStr)] = &[
        (|e| e.khr_android_surface, c"VK_KHR_android_surface"),
        (|e| e.khr_device_group_creation, c"VK_KHR_device_group_creation"),
        (|e| e.khr_display, c"VK_KHR_display"),
        (|e| e.khr_external_fence_capabilities, c"VK_KHR_external_fence_capabilities"),
        (|e| e.khr_external_memory_capabilities, c"VK_KHR_external_memory_capabilities"),
        (|e| e.khr_external_semaphore_capabilities, c"VK_KHR_external_semaphore_capabilities"),
        (|e| e.khr_get_display_properties2, c"VK_KHR_get_display_properties2"),
        (|e| e.khr_get_physical_device_properties2, c"VK_KHR_get_physical_device_properties2"),
        (|e| e.khr_get_surface_capabilities2, c"VK_KHR_get_surface_capabilities2"),
        (|e| e.khr_portability_enumeration, c"VK_KHR_portability_enumeration"),
        (|e| e.khr_surface, c"VK_KHR_surface"),
        (|e| e.khr_surface_protected_capabilities, c"VK_KHR_surface_protected_capabilities"),
        (|e| e.khr_wayland_surface, c"VK_KHR_wayland_surface"),
        (|e| e.khr_win32_surface, c"VK_KHR_win32_surface"),
        (|e| e.khr_xcb_surface, c"VK_KHR_xcb_surface"),
        (|e| e.khr_xlib_surface, c"VK_KHR_xlib_surface"),
        (|e| e.ext_acquire_drm_display, c"VK_EXT_acquire_drm_display"),
        (|e| e.ext_acquire_xlib_display, c"VK_EXT_acquire_xlib_display"),
        (|e| e.ext_debug_report, c"VK_EXT_debug_report"),
        (|e| e.ext_debug_utils, c"VK_EXT_debug_utils"),
        (|e| e.ext_direct_mode_display, c"VK_EXT_direct_mode_display"),
        (|e| e.ext_directfb_surface, c"VK_EXT_directfb_surface"),
        (|e| e.ext_display_surface_counter, c"VK_EXT_display_surface_counter"),
        (|e| e.ext_headless_surface, c"VK_EXT_headless_surface"),
        (|e| e.ext_metal_surface, c"VK_EXT_metal_surface"),
        (|e| e.ext_surface_maintenance1, c"VK_EXT_surface_maintenance1"),
        (|e| e.ext_swapchain_colorspace, c"VK_EXT_swapchain_colorspace"),
        (|e| e.ext_validation_features, c"VK_EXT_validation_features"),
        (|e| e.ext_validation_flags, c"VK_EXT_validation_flags"),
        (|e| e.fuchsia_imagepipe_surface, c"VK_FUCHSIA_imagepipe_surface"),
        (|e| e.ggp_stream_descriptor_surface, c"VK_GGP_stream_descriptor_surface"),
        (|e| e.google_surfaceless_query, c"VK_GOOGLE_surfaceless_query"),
        (|e| e.lunarg_direct_driver_loading, c"VK_LUNARG_direct_driver_loading"),
        (|e| e.mvk_ios_surface, c"VK_MVK_ios_surface"),
        (|e| e.mvk_macos_surface, c"VK_MVK_macos_surface"),
        (|e| e.nn_vi_surface, c"VK_NN_vi_surface"),
        (|e| e.nv_external_memory_capabilities, c"VK_NV_external_memory_capabilities"),
        (|e| e.qnx_screen_surface, c"VK_QNX_screen_surface"),
    ];
    EXTENSIONS.iter().filter_map(|&(f, c)| f(&instance_extensions).then_some(c)).collect()
}
