use crate::edit_funnel::EditFunnel;
use crate::global_ui_state::{GlobalUIEvent, GlobalUIEventHandler, GlobalUIState};
use crate::view_model_util::use_arc;
use crate::viewmodel::ViewModelParams;
use mpdelta_async_runtime::{AsyncRuntime, JoinHandle};
use mpdelta_core::component::instance::ComponentInstanceHandle;
use mpdelta_core::component::parameter::{ImageRequiredParams, ParameterValueType};
use mpdelta_core::edit::InstanceEditCommand;
use mpdelta_core::project::RootComponentClassHandle;
use mpdelta_core::ptr::StaticPointer;
use qcell::TCellOwner;
use std::future;
use std::ops::DerefMut;
use std::sync::{Arc, Mutex, MutexGuard, RwLock as StdRwLock};
use tokio::sync::RwLock;

pub trait PropertyWindowViewModel<K: 'static, T: ParameterValueType> {
    type ImageRequiredParams<'a>: DerefMut<Target = Option<ImageRequiredParams<K, T>>> + 'a
    where
        Self: 'a;
    fn image_required_params(&self) -> Self::ImageRequiredParams<'_>;
    fn updated_image_required_params(&self, image_required_params: &ImageRequiredParams<K, T>);
}

pub struct PropertyWindowViewModelImpl<K: 'static, T: ParameterValueType, GlobalUIState, Edit, Runtime, JoinHandle> {
    global_ui_state: Arc<GlobalUIState>,
    edit: Arc<Edit>,
    selected: Arc<StdRwLock<(Option<RootComponentClassHandle<K, T>>, Option<ComponentInstanceHandle<K, T>>)>>,
    image_required_params: Arc<Mutex<Option<ImageRequiredParams<K, T>>>>,
    image_required_params_update_task: Mutex<JoinHandle>,
    runtime: Runtime,
    key: Arc<RwLock<TCellOwner<K>>>,
}

impl<K, T, S, Edit, Runtime> GlobalUIEventHandler<K, T> for PropertyWindowViewModelImpl<K, T, S, Edit, Runtime, Runtime::JoinHandle>
where
    K: 'static,
    T: ParameterValueType,
    S: GlobalUIState<K, T>,
    Edit: EditFunnel<K, T>,
    Runtime: AsyncRuntime<()> + Clone,
{
    fn handle(&self, event: GlobalUIEvent<K, T>) {
        match event {
            GlobalUIEvent::SelectRootComponentClass(target) => {
                let mut selected = self.selected.write().unwrap();
                if selected.0 != target {
                    *selected = (target, None);
                }
            }
            GlobalUIEvent::SelectComponentInstance(instance) => {
                let mut selected = self.selected.write().unwrap();
                if selected.0.is_some() {
                    selected.1 = instance.clone();
                } else {
                    return;
                }
                drop(selected);
                let mut task = self.image_required_params_update_task.lock().unwrap();
                task.abort();
                use_arc!(image_required_params = self.image_required_params, key = self.key);
                *task = self.runtime.spawn(async move {
                    let key = key.read().await;
                    let mut image_required_params = image_required_params.lock().unwrap();
                    *image_required_params = if let Some(instance) = instance.as_ref().and_then(StaticPointer::upgrade) {
                        let instance = instance.ro(&key);
                        instance.image_required_params().cloned()
                    } else {
                        None
                    };
                });
            }
            _ => {}
        }
    }
}

impl<K: 'static, T: ParameterValueType> PropertyWindowViewModelImpl<K, T, (), (), (), ()> {
    pub fn new<S: GlobalUIState<K, T>, Edit: EditFunnel<K, T> + 'static, P: ViewModelParams<K, T>>(global_ui_state: &Arc<S>, edit: &Arc<Edit>, params: &P) -> Arc<PropertyWindowViewModelImpl<K, T, S, Edit, P::AsyncRuntime, <P::AsyncRuntime as AsyncRuntime<()>>::JoinHandle>> {
        let runtime = params.runtime().clone();
        let arc = Arc::new(PropertyWindowViewModelImpl {
            global_ui_state: Arc::clone(global_ui_state),
            edit: Arc::clone(edit),
            selected: Arc::new(StdRwLock::new((None, None))),
            image_required_params: Arc::new(Mutex::new(None)),
            image_required_params_update_task: Mutex::new(runtime.spawn(future::ready(()))),
            runtime,
            key: Arc::clone(params.key()),
        });
        global_ui_state.register_global_ui_event_handler(Arc::clone(&arc));
        arc
    }
}

impl<K, T, S, Edit, Runtime> PropertyWindowViewModel<K, T> for PropertyWindowViewModelImpl<K, T, S, Edit, Runtime, Runtime::JoinHandle>
where
    K: 'static,
    T: ParameterValueType,
    S: GlobalUIState<K, T>,
    Edit: EditFunnel<K, T>,
    Runtime: AsyncRuntime<()> + Clone,
{
    type ImageRequiredParams<'a> = MutexGuard<'a, Option<ImageRequiredParams<K, T>>> where Self: 'a;

    fn image_required_params(&self) -> Self::ImageRequiredParams<'_> {
        self.image_required_params.lock().unwrap()
    }

    fn updated_image_required_params(&self, image_required_params: &ImageRequiredParams<K, T>) {
        let (Some(root_component_class), Some(component_instance)) = &*self.selected.read().unwrap() else {
            return;
        };
        self.edit.edit_instance(root_component_class, component_instance, InstanceEditCommand::UpdateImageRequiredParams(image_required_params.clone()));
    }
}
