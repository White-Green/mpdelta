use cpal::traits::HostTrait;
use egui::Context;
use mpdelta_async_runtime::AsyncRuntimeDyn;
use mpdelta_audio_mixer::MPDeltaAudioMixerBuilder;
use mpdelta_components::rectangle::RectangleClass;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::MPDeltaCore;
use mpdelta_core_audio::AudioType;
use mpdelta_dyn_usecase::{
    EditUsecaseDyn, GetAvailableComponentClassesUsecaseDyn, GetLoadedProjectsUsecaseDyn, GetRootComponentClassesUsecaseDyn, LoadProjectUsecaseDyn, NewProjectUsecaseDyn, NewRootComponentClassUsecaseDyn, RealtimeRenderComponentUsecaseDyn, RedoUsecaseDyn, SetOwnerForRootComponentClassUsecaseDyn,
    SubscribeEditEventUsecaseDyn, UndoUsecaseDyn, WriteProjectUsecaseDyn,
};
use mpdelta_gui::view::Gui;
use mpdelta_gui::viewmodel::ViewModelParamsImpl;
use mpdelta_gui::ImageRegister;
use mpdelta_gui_audio_player_cpal::CpalAudioPlayer;
use mpdelta_gui_hot_reload_lib::dyn_trait::AudioTypePlayerDyn;
use mpdelta_gui_hot_reload_lib::{ComponentClassList, ProjectKey, ValueType};
use mpdelta_gui_vulkano::MPDeltaGUIVulkano;
use mpdelta_renderer::MPDeltaRendererBuilder;
use mpdelta_rendering_controller::LRUCacheRenderingControllerBuilder;
use mpdelta_services::history::InMemoryEditHistoryStore;
use mpdelta_services::id_generator::UniqueIdGenerator;
use mpdelta_services::project_editor::ProjectEditor;
use mpdelta_services::project_io::{TemporaryProjectLoader, TemporaryProjectWriter};
use mpdelta_services::project_store::InMemoryProjectStore;
use mpdelta_video_renderer_vulkano::ImageCombinerBuilder;
use notify::{RecursiveMode, Watcher};
use qcell::TCellOwner;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::{atomic, Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use std::{process, thread};
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use vulkano::command_buffer::allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo};
use vulkano::instance::InstanceCreateInfo;
use vulkano::Version;
use vulkano_util::context::{VulkanoConfig, VulkanoContext};
use vulkano_util::window::VulkanoWindows;
use winit::event_loop::EventLoop;

#[hot_lib_reloader::hot_module(
dylib = "mpdelta_gui_hot_reload_lib",
lib_dir = if cfg ! (debug_assertions) { "target/debug" } else { "target/release" }
)]
mod lib {
    use super::*;
    use mpdelta_gui::view::Gui;
    use mpdelta_gui_hot_reload_lib::Param;

    hot_functions_from_file!("mpdelta_gui_hot_reload/mpdelta_gui_hot_reload_lib/src/lib.rs");

    #[lib_change_subscription]
    pub fn subscribe() -> hot_lib_reloader::LibReloadObserver {}
}

type DynGui = dyn Gui<<ValueType as ParameterValueType>::Image> + Send + Sync;

#[derive(Clone)]
struct SharedDynGui(Arc<Mutex<Option<Box<DynGui>>>>);

struct SharedDynGuiSlot<'a>(MutexGuard<'a, Option<Box<dyn Gui<<ValueType as ParameterValueType>::Image> + Send + Sync>>>);

impl Gui<<ValueType as ParameterValueType>::Image> for SharedDynGui {
    fn ui(&mut self, ctx: &Context, image: &mut impl ImageRegister<<ValueType as ParameterValueType>::Image>)
    where
        Self: Sized,
    {
        if let Some(gui) = self.0.lock().unwrap_or_else(PoisonError::into_inner).as_mut() {
            gui.ui_dyn(ctx, image)
        }
    }

    fn ui_dyn(&mut self, ctx: &Context, image: &mut dyn ImageRegister<<ValueType as ParameterValueType>::Image>) {
        if let Some(gui) = self.0.lock().unwrap_or_else(PoisonError::into_inner).as_mut() {
            gui.ui_dyn(ctx, image)
        }
    }
}

impl SharedDynGui {
    fn new(gui: Box<dyn Gui<<ValueType as ParameterValueType>::Image> + Send + Sync>) -> SharedDynGui {
        SharedDynGui(Arc::new(Mutex::new(Some(gui))))
    }

    fn unload(&self) -> SharedDynGuiSlot {
        let mut guard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        *guard = None;
        SharedDynGuiSlot(guard)
    }
}

impl<'a> SharedDynGuiSlot<'a> {
    fn load(mut self, gui: Box<dyn Gui<<ValueType as ParameterValueType>::Image> + Send + Sync>) {
        *self.0 = Some(gui);
    }
}

fn main() {
    dbg!(env!("CARGO"));
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
        Arc::new(MPDeltaAudioMixerBuilder::new()),
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
    let params = ViewModelParamsImpl::new(
        Arc::new(runtime.handle().clone()) as Arc<dyn AsyncRuntimeDyn<()>>,
        Arc::new(Arc::clone(&core) as Arc<dyn EditUsecaseDyn<ProjectKey, ValueType> + Send + Sync>),
        Arc::new(Arc::clone(&core) as Arc<dyn SubscribeEditEventUsecaseDyn<ProjectKey, ValueType> + Send + Sync>),
        Arc::new(Arc::clone(&core) as Arc<dyn GetAvailableComponentClassesUsecaseDyn<ProjectKey, ValueType> + Send + Sync>),
        Arc::new(Arc::clone(&core) as Arc<dyn GetLoadedProjectsUsecaseDyn<ProjectKey, ValueType> + Send + Sync>),
        Arc::new(Arc::clone(&core) as Arc<dyn GetRootComponentClassesUsecaseDyn<ProjectKey, ValueType> + Send + Sync>),
        Arc::new(Arc::clone(&core) as Arc<dyn LoadProjectUsecaseDyn<ProjectKey, ValueType> + Send + Sync>),
        Arc::new(Arc::clone(&core) as Arc<dyn NewProjectUsecaseDyn<ProjectKey, ValueType> + Send + Sync>),
        Arc::new(Arc::clone(&core) as Arc<dyn NewRootComponentClassUsecaseDyn<ProjectKey, ValueType> + Send + Sync>),
        Arc::new(Arc::clone(&core) as Arc<dyn RealtimeRenderComponentUsecaseDyn<ProjectKey, ValueType> + Send + Sync>),
        Arc::new(Arc::clone(&core) as Arc<dyn RedoUsecaseDyn<ProjectKey, ValueType> + Send + Sync>),
        Arc::new(Arc::clone(&core) as Arc<dyn SetOwnerForRootComponentClassUsecaseDyn<ProjectKey, ValueType> + Send + Sync>),
        Arc::new(Arc::clone(&core) as Arc<dyn UndoUsecaseDyn<ProjectKey, ValueType> + Send + Sync>),
        Arc::new(Arc::clone(&core) as Arc<dyn WriteProjectUsecaseDyn<ProjectKey, ValueType> + Send + Sync>),
        Arc::clone(&key),
        Arc::new(audio_player as Arc<dyn AudioTypePlayerDyn<AudioType> + Send + Sync>),
    );
    let gui = lib::create_gui(params.clone());
    let gui = SharedDynGui::new(gui);
    let stop_signal = Arc::new(AtomicBool::new(false));
    let reload_thread = thread::spawn({
        let gui = gui.clone();
        let stop_signal = Arc::clone(&stop_signal);
        move || loop {
            let Some(guard) = lib::subscribe().wait_for_about_to_reload_timeout(Duration::from_millis(500)) else {
                if stop_signal.load(atomic::Ordering::Acquire) {
                    break;
                }
                continue;
            };
            let slot = gui.unload();
            drop(guard);
            lib::subscribe().wait_for_reload();
            slot.load(lib::create_gui(params.clone()));
        }
    });
    let mut process = None::<process::Child>;
    let mut watcher = notify::recommended_watcher(move |res| match res {
        Ok(event) => {
            println!("event: {:?}", event);
            let result = process::Command::new(env!("CARGO"))
                .arg("build")
                .args(["--package", "mpdelta_gui_hot_reload_lib"])
                .args::<&[&str], _>(if cfg!(debug_assertions) { &[] } else { &["--release"] })
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn();
            let child_process = match result {
                Ok(child_process) => child_process,
                Err(err) => {
                    println!("failed to spawn child process: {err}");
                    return;
                }
            };
            if let Some(Err(error)) = process.as_mut().map(process::Child::kill) {
                println!("{error}");
            }
            process = Some(child_process);
        }
        Err(e) => println!("watch error: {:?}", e),
    })
    .unwrap();
    let watch_path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../mpdelta_gui"));
    watcher.watch(watch_path, RecursiveMode::Recursive).unwrap();
    let gui = MPDeltaGUIVulkano::new(context, event_loop, windows, gui);
    gui.main();
    stop_signal.store(true, atomic::Ordering::Release);
    watcher.unwatch(watch_path).unwrap();
    reload_thread.join().unwrap();
    drop(core);
    drop(runtime);
}
