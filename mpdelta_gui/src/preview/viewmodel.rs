use crate::global_ui_state::{GlobalUIEvent, GlobalUIEventHandler, GlobalUIState};
use crate::viewmodel::ViewModelParams;
use crate::AudioTypePlayer;
use arc_swap::ArcSwapOption;
use mpdelta_async_runtime::{AsyncRuntime, JoinHandleWrapper};
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::component::instance::ComponentInstanceId;
use mpdelta_core::component::marker_pin::MarkerTime;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::{EditEventListener, IdGenerator};
use mpdelta_core::edit::{InstanceEditEvent, RootComponentEditEvent};
use mpdelta_core::project::RootComponentClassHandle;
use mpdelta_core::time::TimelineTime;
use mpdelta_core::usecase::{RealtimeComponentRenderer, RealtimeRenderComponentUsecase, SubscribeEditEventUsecase};
use std::future;
use std::hash::Hash;
use std::sync::{Arc, Mutex, OnceLock};

pub struct PreviewImage<Handle, Image> {
    pub instance: Option<Handle>,
    pub image: Option<Image>,
}

impl<Handle, Image> Default for PreviewImage<Handle, Image> {
    fn default() -> Self {
        PreviewImage { instance: None, image: None }
    }
}

pub trait PreviewViewModel<T: ParameterValueType> {
    type ComponentInstanceHandle: 'static + Send + Sync + Eq + Hash;
    fn get_preview_image(&self) -> PreviewImage<Self::ComponentInstanceHandle, T::Image>;
    fn playing(&self) -> bool;
    fn play(&self);
    fn pause(&self);
    fn component_length(&self) -> Option<MarkerTime>;
    fn seek(&self) -> MarkerTime;
    fn set_seek(&self, seek: MarkerTime);
}

struct RealTimeRendererHandle<R, T: ParameterValueType> {
    renderer: R,
    render_target_instance: ComponentInstanceId,
    render_target_component_class: RootComponentClassHandle<T>,
}

pub struct PreviewViewModelImpl<T: ParameterValueType, Id, GlobalUIState, RealtimeRenderComponent, R, AudioPlayer, G, Runtime, JoinHandle> {
    id_generator: Arc<Id>,
    renderer: Arc<RealtimeRenderComponent>,
    real_time_renderer: Arc<ArcSwapOption<RealTimeRendererHandle<R, T>>>,
    audio_player: Arc<AudioPlayer>,
    global_ui_state: Arc<GlobalUIState>,
    create_renderer: Mutex<JoinHandleWrapper<JoinHandle>>,
    handle: Runtime,
    guard: OnceLock<G>,
}

impl<T, Id, S, R, A, G, Runtime> GlobalUIEventHandler<T> for PreviewViewModelImpl<T, Id, S, R, R::Renderer, A, G, Runtime, Runtime::JoinHandle>
where
    T: ParameterValueType,
    Id: IdGenerator + 'static,
    S: GlobalUIState<T>,
    R: RealtimeRenderComponentUsecase<T> + 'static,
    R::Renderer: 'static,
    A: AudioTypePlayer<T::Audio> + 'static,
    G: Send + Sync + 'static,
    Runtime: AsyncRuntime<()> + Clone + 'static,
{
    fn handle(&self, event: GlobalUIEvent<T>) {
        match event {
            GlobalUIEvent::SelectRootComponentClass(Some(root_component_class)) => {
                self.create_real_time_renderer(root_component_class);
            }
            GlobalUIEvent::SelectRootComponentClass(None) => self.real_time_renderer.store(None),
            _ => {}
        }
    }
}

impl<T, Id, S, R, A, G, Runtime> EditEventListener<T> for PreviewViewModelImpl<T, Id, S, R, R::Renderer, A, G, Runtime, Runtime::JoinHandle>
where
    T: ParameterValueType,
    Id: IdGenerator + 'static,
    S: GlobalUIState<T>,
    R: RealtimeRenderComponentUsecase<T> + 'static,
    R::Renderer: 'static,
    A: AudioTypePlayer<T::Audio> + 'static,
    G: Send + Sync + 'static,
    Runtime: AsyncRuntime<()> + Clone + 'static,
{
    fn on_edit(&self, _: &RootComponentClassHandle<T>, _: RootComponentEditEvent) {
        let real_time_renderer = self.real_time_renderer.load();
        let Some(RealTimeRendererHandle { render_target_component_class, .. }) = real_time_renderer.as_deref() else {
            return;
        };

        self.create_real_time_renderer(render_target_component_class.clone());
    }

    fn on_edit_instance(&self, _: &RootComponentClassHandle<T>, _: &ComponentInstanceId, _: InstanceEditEvent<T>) {
        let real_time_renderer = self.real_time_renderer.load();
        let Some(RealTimeRendererHandle { render_target_component_class, .. }) = real_time_renderer.as_deref() else {
            return;
        };

        self.create_real_time_renderer(render_target_component_class.clone());
    }
}

impl<T: ParameterValueType> PreviewViewModelImpl<T, (), (), (), (), (), (), (), ()> {
    #[allow(clippy::type_complexity)]
    pub fn new<S: GlobalUIState<T>, P: ViewModelParams<T>>(
        global_ui_state: &Arc<S>,
        params: &P,
    ) -> Arc<
        PreviewViewModelImpl<
            T,
            P::IdGenerator,
            S,
            P::RealtimeRenderComponent,
            <P::RealtimeRenderComponent as RealtimeRenderComponentUsecase<T>>::Renderer,
            P::AudioPlayer,
            <P::SubscribeEditEvent as SubscribeEditEventUsecase<T>>::EditEventListenerGuard,
            P::AsyncRuntime,
            <P::AsyncRuntime as AsyncRuntime<()>>::JoinHandle,
        >,
    > {
        let handle = params.runtime().clone();
        let arc = Arc::new(PreviewViewModelImpl {
            id_generator: Arc::clone(params.id_generator()),
            renderer: Arc::clone(params.realtime_render_component()),
            real_time_renderer: Arc::new(ArcSwapOption::empty()),
            audio_player: Arc::clone(params.audio_player()),
            global_ui_state: Arc::clone(global_ui_state),
            create_renderer: Mutex::new(handle.spawn(future::ready(()))),
            handle,
            guard: OnceLock::new(),
        });
        global_ui_state.register_global_ui_event_handler(Arc::clone(&arc));
        let guard = params.subscribe_edit_event().add_edit_event_listener(Arc::clone(&arc));
        arc.guard.set(guard).unwrap_or_else(|_| unreachable!());
        arc
    }
}

impl<T, Id, S, R, A, G, Runtime> PreviewViewModelImpl<T, Id, S, R, R::Renderer, A, G, Runtime, Runtime::JoinHandle>
where
    T: ParameterValueType,
    Id: IdGenerator + 'static,
    S: GlobalUIState<T>,
    R: RealtimeRenderComponentUsecase<T> + 'static,
    R::Renderer: 'static,
    A: AudioTypePlayer<T::Audio> + 'static,
    G: Send + Sync + 'static,
    Runtime: AsyncRuntime<()> + Clone + 'static,
{
    fn create_real_time_renderer(&self, root_component_class: RootComponentClassHandle<T>) {
        let mut create_renderer = self.create_renderer.lock().unwrap();
        create_renderer.abort();
        *create_renderer = self.handle.spawn(Self::create_real_time_renderer_inner(
            root_component_class,
            Arc::clone(&self.id_generator),
            Arc::clone(&self.renderer),
            Arc::clone(&self.real_time_renderer),
            Arc::clone(&self.audio_player),
            TimelineTime::new(self.global_ui_state.seek().value()),
        ));
    }

    async fn create_real_time_renderer_inner(root_component_class: RootComponentClassHandle<T>, id: Arc<Id>, renderer: Arc<R>, real_time_renderer: Arc<ArcSwapOption<RealTimeRendererHandle<R::Renderer, T>>>, audio_player: Arc<A>, current_time: TimelineTime) {
        let new_renderer = 'renderer: {
            let Some(class) = root_component_class.upgrade() else {
                break 'renderer None;
            };
            let class = class.read().await;
            let instance = class.instantiate(&root_component_class.clone().map(|weak| weak as _), &id).await;
            let instance_id = *instance.id();
            match renderer.render_component(Arc::new(instance)).await {
                Ok(renderer) => {
                    let audio = match renderer.mix_audio(0, 0).await {
                        Ok(audio) => audio,
                        Err(err) => {
                            eprintln!("failed to mix audio by {err}");
                            break 'renderer None;
                        }
                    };
                    audio_player.set_audio(audio);
                    audio_player.seek(current_time);
                    Some(Arc::new(RealTimeRendererHandle {
                        renderer,
                        render_target_instance: instance_id,
                        render_target_component_class: root_component_class.clone(),
                    }))
                }
                Err(err) => {
                    eprintln!("failed to create renderer by {err}");
                    None
                }
            }
        };
        real_time_renderer.store(new_renderer);
    }
}

impl<T, Id, S, R, A, G, Runtime> PreviewViewModel<T> for PreviewViewModelImpl<T, Id, S, R, R::Renderer, A, G, Runtime, Runtime::JoinHandle>
where
    T: ParameterValueType,
    Id: IdGenerator + 'static,
    S: GlobalUIState<T>,
    R: RealtimeRenderComponentUsecase<T>,
    A: AudioTypePlayer<T::Audio>,
    Runtime: AsyncRuntime<()> + Clone,
{
    type ComponentInstanceHandle = ComponentInstanceId;
    fn get_preview_image(&self) -> PreviewImage<Self::ComponentInstanceHandle, T::Image> {
        self.real_time_renderer.load().as_deref().map_or_else(PreviewImage::default, |RealTimeRendererHandle { renderer, render_target_instance, .. }| {
            let len = renderer.get_component_length();
            if self.global_ui_state.component_length() != len {
                if let Some(len) = len {
                    self.global_ui_state.set_component_length(len);
                }
            }
            let seek = self.seek();
            let (i, n) = seek.value().deconstruct_with_round(60);
            let seek = i as usize * 60 + n as usize;
            PreviewImage {
                instance: Some(*render_target_instance),
                image: renderer.render_frame(seek).ok(),
            }
        })
    }

    fn playing(&self) -> bool {
        self.global_ui_state.playing()
    }

    fn play(&self) {
        self.global_ui_state.play();
    }

    fn pause(&self) {
        self.global_ui_state.pause();
    }

    fn component_length(&self) -> Option<MarkerTime> {
        self.global_ui_state.component_length()
    }

    fn seek(&self) -> MarkerTime {
        self.global_ui_state.seek()
    }

    fn set_seek(&self, seek: MarkerTime) {
        let seek = seek.min(self.global_ui_state.component_length().unwrap_or_else(|| MarkerTime::new(MixedFraction::from_integer(10)).unwrap()));
        self.global_ui_state.set_seek(seek);
    }
}
