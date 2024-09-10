use crate::edit_funnel::EditFunnel;
use crate::global_ui_state::{GlobalUIEvent, GlobalUIEventHandler, GlobalUIState};
use crate::view_model_util::use_arc;
use crate::viewmodel::ViewModelParams;
use arc_swap::{ArcSwap, ArcSwapOption};
use mpdelta_async_runtime::{AsyncRuntime, JoinHandleWrapper};
use mpdelta_core::component::instance::ComponentInstanceId;
use mpdelta_core::component::marker_pin::MarkerPin;
use mpdelta_core::component::parameter::{ImageRequiredParams, ParameterNullableValue, ParameterValueFixed, ParameterValueType, VariableParameterValue};
use mpdelta_core::edit::InstanceEditCommand;
use mpdelta_core::project::{RootComponentClassHandle, RootComponentClassItem, TimelineTimeOfPin};
use std::ops::Range;
use std::sync::{Arc, Mutex, RwLock as StdRwLock};
use std::{future, iter};

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

type EditSet<T> = ParametersEditSet<T, RootComponentClassItem<T>>;

pub struct PropertyWindowViewModelImpl<T: ParameterValueType, Edit, Runtime, JoinHandle> {
    edit: Arc<Edit>,
    selected: Arc<StdRwLock<SelectedItem<T>>>,
    selected_instance_at: Arc<ArcSwap<Range<f64>>>,
    parameters: Arc<ArcSwapOption<Mutex<EditSet<T>>>>,
    image_required_params_update_task: Mutex<JoinHandleWrapper<JoinHandle>>,
    runtime: Runtime,
}

impl<T, Edit, Runtime> GlobalUIEventHandler<T> for PropertyWindowViewModelImpl<T, Edit, Runtime, Runtime::JoinHandle>
where
    T: ParameterValueType,
    Edit: EditFunnel<T>,
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
                let root = selected.root.clone().unwrap();
                drop(selected);
                let mut task = self.image_required_params_update_task.lock().unwrap();
                task.abort();
                use_arc!(parameters = self.parameters, selected_instance_at = self.selected_instance_at);
                *task = self.runtime.spawn(async move {
                    let result = async {
                        let instance = instance.as_ref()?;
                        let root = root.upgrade()?;
                        let root = root.read().await;
                        let root = root.get();
                        let instance = root.component(instance)?;
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
                        Some(())
                    }
                    .await;
                    if result.is_none() {
                        parameters.store(None);
                    }
                });
            }
            _ => {}
        }
    }
}

impl<T: ParameterValueType> PropertyWindowViewModelImpl<T, (), (), ()> {
    #[allow(clippy::type_complexity)]
    pub fn new<S: GlobalUIState<T>, Edit: EditFunnel<T> + 'static, P: ViewModelParams<T>>(global_ui_state: &Arc<S>, edit: &Arc<Edit>, params: &P) -> Arc<PropertyWindowViewModelImpl<T, Edit, P::AsyncRuntime, <P::AsyncRuntime as AsyncRuntime<()>>::JoinHandle>> {
        let runtime = params.runtime().clone();
        let arc = Arc::new(PropertyWindowViewModelImpl {
            edit: Arc::clone(edit),
            selected: Arc::new(StdRwLock::new(SelectedItem::default())),
            selected_instance_at: Arc::new(ArcSwap::new(Arc::new(0.0..0.0))),
            parameters: Arc::new(ArcSwapOption::empty()),
            image_required_params_update_task: Mutex::new(runtime.spawn(future::ready(()))),
            runtime,
        });
        global_ui_state.register_global_ui_event_handler(Arc::clone(&arc));
        arc
    }
}

impl<T, Edit, Runtime> PropertyWindowViewModel<T> for PropertyWindowViewModelImpl<T, Edit, Runtime, Runtime::JoinHandle>
where
    T: ParameterValueType,
    Edit: EditFunnel<T>,
    Runtime: AsyncRuntime<()> + Clone,
{
    fn selected_instance_at(&self) -> Range<f64> {
        (**self.selected_instance_at.load()).clone()
    }

    type TimeMap = RootComponentClassItem<T>;

    fn parameters<R>(&self, f: impl FnOnce(Option<&mut ParametersEditSet<T, Self::TimeMap>>) -> R) -> R {
        f(self.parameters.load().as_deref().map(|mutex| mutex.lock().unwrap()).as_deref_mut())
    }

    fn updated_image_required_params(&self, image_required_params: &ImageRequiredParams) {
        let SelectedItem {
            root: Some(root_component_class),
            instance: Some(component_instance),
        } = &*self.selected.read().unwrap()
        else {
            return;
        };
        self.edit.edit_instance(root_component_class, component_instance, InstanceEditCommand::UpdateImageRequiredParams(image_required_params.clone()));
    }

    fn updated_fixed_params(&self, fixed_params: &[WithName<ParameterValueFixed<T::Image, T::Audio>>]) {
        let SelectedItem {
            root: Some(root_component_class),
            instance: Some(component_instance),
        } = &*self.selected.read().unwrap()
        else {
            return;
        };
        self.edit.edit_instance(root_component_class, component_instance, InstanceEditCommand::UpdateFixedParams(fixed_params.iter().map(|WithName { value, .. }| value.clone()).collect()));
    }

    fn updated_variable_params(&self, variable_params: &[WithName<VariableParameterValue<ParameterNullableValue<T>>>]) {
        let SelectedItem {
            root: Some(root_component_class),
            instance: Some(component_instance),
        } = &*self.selected.read().unwrap()
        else {
            return;
        };
        self.edit
            .edit_instance(root_component_class, component_instance, InstanceEditCommand::UpdateVariableParams(variable_params.iter().map(|WithName { value, .. }| value.clone()).collect()));
    }
}
