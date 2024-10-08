use crate::edit_funnel::EditFunnel;
use crate::global_ui_state::{GlobalUIEvent, GlobalUIEventHandler, GlobalUIState};
use crate::view_model_util::use_arc;
use crate::viewmodel::ViewModelParams;
use arc_swap::{ArcSwap, ArcSwapOption};
use mpdelta_async_runtime::AsyncRuntime;
use mpdelta_core::component::instance::ComponentInstanceId;
use mpdelta_core::component::marker_pin::MarkerPin;
use mpdelta_core::component::parameter::{ImageRequiredParams, ParameterNullableValue, ParameterValueFixed, ParameterValueType, VariableParameterValue};
use mpdelta_core::core::EditEventListener;
use mpdelta_core::edit::{InstanceEditCommand, InstanceEditEvent, RootComponentEditEvent};
use mpdelta_core::project::{RootComponentClassHandle, RootComponentClassItem, TimelineTimeOfPin};
use mpdelta_core::usecase::SubscribeEditEventUsecase;
use mpdelta_message_router::handler::{IntoAsyncFunctionHandler, IntoFunctionHandler, MessageHandlerBuilder};
use mpdelta_message_router::{MessageHandler, MessageRouter};
use std::iter;
use std::ops::Range;
use std::sync::atomic::AtomicBool;
use std::sync::{atomic, Arc, Mutex, OnceLock, RwLock as StdRwLock};

#[derive(Debug, Clone)]
pub struct WithName<T> {
    pub name: String,
    pub value: T,
}

impl<T> WithName<T> {
    pub fn new(name: String, value: T) -> WithName<T> {
        WithName { name, value }
    }
}

pub struct ParametersEditSet<T: ParameterValueType, P> {
    pub all_pins: Box<[MarkerPin]>,
    pub image_required_params: Option<ImageRequiredParams>,
    #[allow(clippy::type_complexity)]
    pub fixed_params: Box<[WithName<ParameterValueFixed<T::Image, T::Audio>>]>,
    #[allow(clippy::type_complexity)]
    pub variable_params: Box<[WithName<VariableParameterValue<ParameterNullableValue<T>>>]>,
    pub pin_times: Arc<P>,
}

pub trait PropertyWindowViewModel<T: ParameterValueType> {
    fn is_updated_now(&self) -> bool;
    fn selected_instance_at(&self) -> Range<f64>;
    type TimeMap: TimelineTimeOfPin;
    fn parameters<R>(&self, f: impl FnOnce(Option<&mut ParametersEditSet<T, Self::TimeMap>>) -> R) -> R;
    fn updated_image_required_params(&self, image_required_params: &ImageRequiredParams);
    fn updated_fixed_params(&self, fixed_params: &[WithName<ParameterValueFixed<T::Image, T::Audio>>]);
    fn updated_variable_params(&self, variable_params: &[WithName<VariableParameterValue<ParameterNullableValue<T>>>]);
}

struct SelectedItem<T: ParameterValueType> {
    root: Option<RootComponentClassHandle<T>>,
    instance: Option<ComponentInstanceId>,
}

impl<T: ParameterValueType> Default for SelectedItem<T> {
    fn default() -> Self {
        SelectedItem { root: None, instance: None }
    }
}

impl<T: ParameterValueType> Clone for SelectedItem<T> {
    fn clone(&self) -> Self {
        SelectedItem { root: self.root.clone(), instance: self.instance }
    }
}

type EditSet<T> = ParametersEditSet<T, RootComponentClassItem<T>>;

#[derive(Debug)]
pub enum Message<T>
where
    T: ParameterValueType,
{
    UpdatedImageRequiredParams(ImageRequiredParams),
    UpdatedFixedParams(Box<[ParameterValueFixed<T::Image, T::Audio>]>),
    UpdatedVariableParams(Vec<VariableParameterValue<ParameterNullableValue<T>>>),
    LoadParameters,
}

impl<T: ParameterValueType> Clone for Message<T> {
    fn clone(&self) -> Self {
        match self {
            Message::UpdatedImageRequiredParams(params) => Message::UpdatedImageRequiredParams(params.clone()),
            Message::UpdatedFixedParams(params) => Message::UpdatedFixedParams(params.clone()),
            Message::UpdatedVariableParams(params) => Message::UpdatedVariableParams(params.clone()),
            Message::LoadParameters => Message::LoadParameters,
        }
    }
}

pub struct PropertyWindowViewModelImpl<T: ParameterValueType, MessageHandler, Runtime, Guard> {
    updated_now: Arc<AtomicBool>,
    message_router: MessageRouter<MessageHandler, Runtime>,
    selected: Arc<StdRwLock<SelectedItem<T>>>,
    selected_instance_at: Arc<ArcSwap<Range<f64>>>,
    parameters: Arc<ArcSwapOption<Mutex<EditSet<T>>>>,
    guard: OnceLock<Guard>,
}

impl<T, M, Runtime, Guard> GlobalUIEventHandler<T> for PropertyWindowViewModelImpl<T, M, Runtime, Guard>
where
    T: ParameterValueType,
    M: MessageHandler<Message<T>, Runtime> + Send + Sync,
    Runtime: AsyncRuntime<()> + Clone,
{
    fn handle(&self, event: GlobalUIEvent<T>) {
        match event {
            GlobalUIEvent::SelectRootComponentClass(target) => {
                let mut selected = self.selected.write().unwrap();
                if selected.root != target {
                    *selected = SelectedItem { root: target, instance: None };
                }
            }
            GlobalUIEvent::SelectComponentInstance(instance) => {
                let mut selected = self.selected.write().unwrap();
                if selected.root.is_some() {
                    selected.instance = instance;
                } else {
                    return;
                }
                self.message_router.handle(Message::LoadParameters);
            }
            _ => {}
        }
    }
}

impl<T, M, Runtime, Guard> EditEventListener<T> for PropertyWindowViewModelImpl<T, M, Runtime, Guard>
where
    T: ParameterValueType,
    M: MessageHandler<Message<T>, Runtime> + Send + Sync,
    Runtime: AsyncRuntime<()> + Clone,
    Guard: Send + Sync,
{
    fn on_edit(&self, _: &RootComponentClassHandle<T>, _: RootComponentEditEvent) {
        self.updated_now.store(true, atomic::Ordering::Release);
    }

    fn on_edit_instance(&self, _: &RootComponentClassHandle<T>, _: &ComponentInstanceId, _: InstanceEditEvent<T>) {
        self.updated_now.store(true, atomic::Ordering::Release);
        self.message_router.handle(Message::LoadParameters);
    }
}

impl<T: ParameterValueType> PropertyWindowViewModelImpl<T, (), (), ()> {
    #[allow(clippy::type_complexity)]
    pub fn new<S: GlobalUIState<T>, Edit: EditFunnel<T> + 'static, P: ViewModelParams<T>>(
        global_ui_state: &Arc<S>,
        edit: &Arc<Edit>,
        params: &P,
    ) -> Arc<PropertyWindowViewModelImpl<T, impl MessageHandler<Message<T>, P::AsyncRuntime>, P::AsyncRuntime, <P::SubscribeEditEvent as SubscribeEditEventUsecase<T>>::EditEventListenerGuard>> {
        let updated_now = Arc::new(AtomicBool::new(false));
        let selected = Arc::new(StdRwLock::new(SelectedItem::default()));
        let selected_instance_at = Arc::new(ArcSwap::new(Arc::new(0.0..0.0)));
        let parameters = Arc::new(ArcSwapOption::empty());
        let message_router = MessageRouter::builder()
            .handle(|handler| {
                handler.filter_map(|message| if let Message::UpdatedImageRequiredParams(params) = message { Some(params) } else { None }).handle({
                    use_arc!(selected, edit);
                    move |params| {
                        let SelectedItem {
                            root: Some(root_component_class),
                            instance: Some(component_instance),
                        } = &*selected.read().unwrap()
                        else {
                            return;
                        };
                        edit.edit_instance(root_component_class, component_instance, InstanceEditCommand::UpdateImageRequiredParams(params));
                    }
                })
            })
            .handle(|handler| {
                handler.filter_map(|message| if let Message::UpdatedFixedParams(params) = message { Some(params) } else { None }).handle({
                    use_arc!(selected, edit);
                    move |params| {
                        let SelectedItem {
                            root: Some(root_component_class),
                            instance: Some(component_instance),
                        } = &*selected.read().unwrap()
                        else {
                            return;
                        };
                        edit.edit_instance(root_component_class, component_instance, InstanceEditCommand::UpdateFixedParams(params));
                    }
                })
            })
            .handle(|handler| {
                handler.filter_map(|message| if let Message::UpdatedVariableParams(params) = message { Some(params) } else { None }).handle({
                    use_arc!(selected, edit);
                    move |params| {
                        let SelectedItem {
                            root: Some(root_component_class),
                            instance: Some(component_instance),
                        } = &*selected.read().unwrap()
                        else {
                            return;
                        };
                        edit.edit_instance(root_component_class, component_instance, InstanceEditCommand::UpdateVariableParams(params));
                    }
                })
            })
            .handle(|handler| {
                handler.filter(|message| matches!(message, Message::LoadParameters)).handle_async({
                    use_arc!(selected, parameters, selected_instance_at, updated_now);
                    move |_| {
                        use_arc!(selected, parameters, selected_instance_at, updated_now);
                        async move {
                            let result = async {
                                let SelectedItem { root: Some(root), instance: Some(instance) } = selected.read().unwrap().clone() else {
                                    return None;
                                };
                                let root = root.upgrade()?;
                                let root = root.read().await;
                                let root = root.get();
                                let instance = root.component(&instance)?;
                                let all_pins = iter::once(instance.marker_left()).chain(instance.markers()).chain(iter::once(instance.marker_right())).cloned().collect();
                                selected_instance_at.store(Arc::new(root.time_of_pin(instance.marker_left().id())?.value().into_f64()..root.time_of_pin(instance.marker_right().id())?.value().into_f64()));
                                let variable_params = instance.variable_parameters();
                                let image_required_params = instance.image_required_params().cloned();
                                let new_fixed_params = instance.fixed_parameters();
                                let fixed_params_type = instance.fixed_parameters_type();
                                assert_eq!(new_fixed_params.len(), fixed_params_type.len());
                                let fixed_params = new_fixed_params.iter().zip(fixed_params_type.iter()).map(|(value, (name, _))| WithName::new(name.clone(), value.clone())).collect();
                                let variable_params_type = instance.variable_parameters_type();
                                assert_eq!(variable_params.len(), variable_params_type.len());
                                let variable_params = variable_params.iter().zip(variable_params_type).map(|(value, (name, _))| WithName::new(name.clone(), value.clone())).collect();
                                parameters.store(Some(Arc::new(Mutex::new(ParametersEditSet {
                                    all_pins,
                                    image_required_params,
                                    fixed_params,
                                    variable_params,
                                    pin_times: Arc::clone(&root),
                                }))));
                                updated_now.store(true, atomic::Ordering::Release);
                                Some(())
                            }
                            .await;
                            if result.is_none() {
                                parameters.store(None);
                            }
                        }
                    }
                })
            })
            .build(params.runtime().clone());
        let arc = Arc::new(PropertyWindowViewModelImpl {
            updated_now,
            message_router,
            selected,
            selected_instance_at,
            parameters,
            guard: OnceLock::new(),
        });
        global_ui_state.register_global_ui_event_handler(Arc::clone(&arc));
        let guard = params.subscribe_edit_event().add_edit_event_listener(Arc::clone(&arc));
        arc.guard.set(guard).unwrap_or_else(|_| unreachable!());
        arc
    }
}

impl<T, M, Runtime, Guard> PropertyWindowViewModel<T> for PropertyWindowViewModelImpl<T, M, Runtime, Guard>
where
    T: ParameterValueType,
    M: MessageHandler<Message<T>, Runtime> + Send + Sync,
    Runtime: AsyncRuntime<()> + Clone,
    Guard: Send + Sync,
{
    fn is_updated_now(&self) -> bool {
        self.updated_now.swap(false, atomic::Ordering::AcqRel)
    }

    fn selected_instance_at(&self) -> Range<f64> {
        (**self.selected_instance_at.load()).clone()
    }

    type TimeMap = RootComponentClassItem<T>;

    fn parameters<R>(&self, f: impl FnOnce(Option<&mut ParametersEditSet<T, Self::TimeMap>>) -> R) -> R {
        f(self.parameters.load().as_deref().map(|mutex| mutex.lock().unwrap()).as_deref_mut())
    }

    fn updated_image_required_params(&self, image_required_params: &ImageRequiredParams) {
        self.message_router.handle(Message::UpdatedImageRequiredParams(image_required_params.clone()));
    }

    fn updated_fixed_params(&self, fixed_params: &[WithName<ParameterValueFixed<T::Image, T::Audio>>]) {
        self.message_router.handle(Message::UpdatedFixedParams(fixed_params.iter().map(|WithName { value, .. }| value.clone()).collect()));
    }

    fn updated_variable_params(&self, variable_params: &[WithName<VariableParameterValue<ParameterNullableValue<T>>>]) {
        self.message_router.handle(Message::UpdatedVariableParams(variable_params.iter().map(|WithName { value, .. }| value.clone()).collect()));
    }
}
