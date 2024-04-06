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
use mpdelta_gui_vulkano::MPDeltaGUIVulkano;
use mpdelta_multimedia_encoder_ffmpeg::{FfmpegEncodeSettings, FfmpegEncoderBuilder};
use mpdelta_project_serialize::MPDeltaProjectSerializer;
use mpdelta_renderer::MPDeltaRendererBuilder;
use mpdelta_rendering_controller::LRUCacheRenderingControllerBuilder;
use mpdelta_services::easing_loader::InMemoryEasingLoader;
use mpdelta_services::history::InMemoryEditHistoryStore;
use mpdelta_services::id_generator::UniqueIdGenerator;
use mpdelta_services::project_editor::ProjectEditor;
use mpdelta_services::project_io::{LocalFSProjectLoader, LocalFSProjectWriter};
use mpdelta_services::project_store::InMemoryProjectStore;
use mpdelta_services::value_manager_loader::InMemoryValueManagerLoader;
use mpdelta_video_renderer_vulkano::ImageCombinerBuilder;
use qcell::TCellOwner;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
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

    async fn component_class_by_identifier(&self, identifier: ComponentClassIdentifier<'_>) -> Option<StaticPointer<RwLock<dyn ComponentClass<ProjectKey, ValueType>>>> {
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
    let project_loader = Arc::new(LocalFSProjectLoader);
    let project_writer = Arc::new(LocalFSProjectWriter);
    let project_memory = Arc::new(InMemoryProjectStore::<ProjectKey, ValueType>::new());
    let mut component_class_loader = ComponentClassList::new();
    let command_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(Arc::clone(context.device()), StandardCommandBufferAllocatorCreateInfo::default()));
    component_class_loader.add(RectangleClass::new(Arc::clone(context.graphics_queue()), context.memory_allocator(), &command_buffer_allocator));
    component_class_loader.add(SineAudio::new());
    component_class_loader.add(FfmpegMultimediaLoaderClass::new(context.graphics_queue(), context.memory_allocator(), &command_buffer_allocator));
    component_class_loader.add(TextRendererClass::new(context.device(), context.graphics_queue(), context.memory_allocator()));
    let component_class_loader = Arc::new(component_class_loader);
    let key = Arc::new(RwLock::new(TCellOwner::<ProjectKey>::new()));
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
    let project_serializer = Arc::new(MPDeltaProjectSerializer::new(Arc::clone(&key), runtime.handle().clone(), Arc::clone(&component_class_loader), value_managers, quaternion_manager, easing_manager));
    let component_renderer_builder = Arc::new(MPDeltaRendererBuilder::new(
        Arc::new(ImageCombinerBuilder::new(Arc::clone(&context))),
        Arc::new(LRUCacheRenderingControllerBuilder::new()),
        Arc::new(MPDeltaAudioMixerBuilder::new()),
        Arc::clone(&key),
        runtime.handle().clone(),
    ));
    let editor = Arc::new(ProjectEditor::new(Arc::clone(&key)));
    let edit_history = Arc::new(InMemoryEditHistoryStore::new(100));
    let core = Arc::new(MPDeltaCore::new(MPDeltaCoreArgs {
        id_generator,
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
    let encoder_builder = Arc::new(FfmpegEncoderBuilder::new(Arc::clone(&context)));
    let params = ViewModelParamsImpl {
        runtime: runtime.handle().clone(),
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
        key: Arc::clone(&key),
        audio_player,
        available_video_codec: encoder_builder.available_video_codec::<FfmpegEncodeSettings<File>>().into_iter().collect::<Vec<_>>().into(),
        available_audio_codec: encoder_builder.available_audio_codec().into_iter().collect::<Vec<_>>().into(),
        encode: Arc::clone(&core),
    };
    let gui = mpdelta_gui::new_gui(params);
    let gui = MPDeltaGUIVulkano::new(context, event_loop, windows, gui);
    gui.main();
    drop(core);
    drop(runtime);
}
