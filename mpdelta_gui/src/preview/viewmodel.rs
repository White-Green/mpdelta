use crate::global_ui_state::{GlobalUIEvent, GlobalUIEventHandler, GlobalUIState};
use crate::viewmodel::ViewModelParams;
use arc_swap::ArcSwapOption;
use mpdelta_async_runtime::{AsyncRuntime, JoinHandle};
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::component::instance::{ComponentInstanceHandle, ComponentInstanceHandleOwned};
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::EditEventListener;
use mpdelta_core::edit::{InstanceEditEvent, RootComponentEditEvent};
use mpdelta_core::project::RootComponentClassHandle;
use mpdelta_core::ptr::StaticPointerOwned;
use mpdelta_core::usecase::{RealtimeComponentRenderer, RealtimeRenderComponentUsecase, SubscribeEditEventUsecase};
use qcell::TCell;
use std::future;
use std::sync::{Arc, Mutex, OnceLock};

pub trait PreviewViewModel<K: 'static, T: ParameterValueType> {
    fn get_preview_image(&self) -> Option<T::Image>;
    fn playing(&self) -> bool;
    fn play(&self);
    fn pause(&self);
    fn seek(&self) -> usize;
    fn set_seek(&self, seek: usize);
}

pub struct PreviewViewModelImpl<K: 'static, T, GlobalUIState, RealtimeRenderComponent, R, G, Runtime, JoinHandle> {
    renderer: Arc<RealtimeRenderComponent>,
    real_time_renderer: Arc<ArcSwapOption<(R, ComponentInstanceHandleOwned<K, T>, RootComponentClassHandle<K, T>)>>,
    global_ui_state: Arc<GlobalUIState>,
    create_renderer: Mutex<JoinHandle>,
    handle: Runtime,
    guard: OnceLock<G>,
}

impl<K, T, S, R, G, Runtime> GlobalUIEventHandler<K, T> for PreviewViewModelImpl<K, T, S, R, R::Renderer, G, Runtime, Runtime::JoinHandle>
where
    K: Send + Sync + 'static,
    T: ParameterValueType,
    S: GlobalUIState<K, T>,
    R: RealtimeRenderComponentUsecase<K, T> + 'static,
    R::Renderer: 'static,
    G: Send + Sync + 'static,
    Runtime: AsyncRuntime<()> + Clone + 'static,
{
    fn handle(&self, event: GlobalUIEvent<K, T>) {
        match event {
            GlobalUIEvent::SelectRootComponentClass(Some(root_component_class)) => {
                self.create_real_time_renderer(root_component_class);
            }
            GlobalUIEvent::SelectRootComponentClass(None) => self.real_time_renderer.store(None),
            _ => {}
        }
    }
}

impl<K, T, S, R, G, Runtime> EditEventListener<K, T> for PreviewViewModelImpl<K, T, S, R, R::Renderer, G, Runtime, Runtime::JoinHandle>
where
    K: Send + Sync + 'static,
    T: ParameterValueType,
    S: GlobalUIState<K, T>,
    R: RealtimeRenderComponentUsecase<K, T> + 'static,
    R::Renderer: 'static,
    G: Send + Sync + 'static,
    Runtime: AsyncRuntime<()> + Clone + 'static,
{
    fn on_edit(&self, _: &RootComponentClassHandle<K, T>, _: RootComponentEditEvent<K, T>) {
        let real_time_renderer = self.real_time_renderer.load();
        let Some((_, _, component)) = real_time_renderer.as_deref() else {
            return;
        };

        self.create_real_time_renderer(component.clone());
    }

    fn on_edit_instance(&self, _: &RootComponentClassHandle<K, T>, _: &ComponentInstanceHandle<K, T>, _: InstanceEditEvent<K, T>) {
        let real_time_renderer = self.real_time_renderer.load();
        let Some((_, _, component)) = real_time_renderer.as_deref() else {
            return;
        };

        self.create_real_time_renderer(component.clone());
    }
}

impl<K: Send + Sync + 'static, T: ParameterValueType> PreviewViewModelImpl<K, T, (), (), (), (), (), ()> {
    pub fn new<S: GlobalUIState<K, T>, P: ViewModelParams<K, T>>(
        global_ui_state: &Arc<S>,
        params: &P,
    ) -> Arc<
        PreviewViewModelImpl<K, T, S, P::RealtimeRenderComponent, <P::RealtimeRenderComponent as RealtimeRenderComponentUsecase<K, T>>::Renderer, <P::SubscribeEditEvent as SubscribeEditEventUsecase<K, T>>::EditEventListenerGuard, P::AsyncRuntime, <P::AsyncRuntime as AsyncRuntime<()>>::JoinHandle>,
    > {
        let handle = params.runtime().clone();
        let arc = Arc::new(PreviewViewModelImpl {
            renderer: Arc::clone(params.realtime_render_component()),
            real_time_renderer: Arc::new(ArcSwapOption::empty()),
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

impl<K, T, S, R, G, Runtime> PreviewViewModelImpl<K, T, S, R, R::Renderer, G, Runtime, Runtime::JoinHandle>
where
    K: Send + Sync + 'static,
    T: ParameterValueType,
    S: GlobalUIState<K, T>,
    R: RealtimeRenderComponentUsecase<K, T> + 'static,
    R::Renderer: 'static,
    G: Send + Sync + 'static,
    Runtime: AsyncRuntime<()> + Clone + 'static,
{
    fn create_real_time_renderer(&self, root_component_class: RootComponentClassHandle<K, T>) {
        let mut create_renderer = self.create_renderer.lock().unwrap();
        create_renderer.abort();
        *create_renderer = self.handle.spawn(Self::create_real_time_renderer_inner(root_component_class, Arc::clone(&self.renderer), Arc::clone(&self.real_time_renderer)));
    }

    async fn create_real_time_renderer_inner(root_component_class: RootComponentClassHandle<K, T>, renderer: Arc<R>, real_time_renderer: Arc<ArcSwapOption<(R::Renderer, ComponentInstanceHandleOwned<K, T>, RootComponentClassHandle<K, T>)>>) {
        let new_renderer = 'renderer: {
            let Some(class) = root_component_class.upgrade() else {
                break 'renderer None;
            };
            let class = class.read().await;
            let instance = StaticPointerOwned::new(TCell::new(class.instantiate(&root_component_class.clone().map(|weak| weak as _)).await));
            match renderer.render_component(StaticPointerOwned::reference(&instance)).await {
                Ok(renderer) => Some(Arc::new((renderer, instance, root_component_class.clone()))),
                Err(err) => {
                    eprintln!("failed to create renderer by {err}");
                    None
                }
            }
        };
        real_time_renderer.store(new_renderer);
    }
}

impl<K, T, S, R, G, Runtime> PreviewViewModel<K, T> for PreviewViewModelImpl<K, T, S, R, R::Renderer, G, Runtime, Runtime::JoinHandle>
where
    K: 'static,
    T: ParameterValueType,
    S: GlobalUIState<K, T>,
    R: RealtimeRenderComponentUsecase<K, T>,
    Runtime: AsyncRuntime<()> + Clone,
{
    fn get_preview_image(&self) -> Option<T::Image> {
        self.real_time_renderer.load().as_deref().and_then(|(renderer, _, _)| renderer.render_frame(self.seek()).ok())
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

    fn seek(&self) -> usize {
        self.global_ui_state.seek()
    }

    fn set_seek(&self, seek: usize) {
        self.global_ui_state.set_seek(seek);
    }
}
