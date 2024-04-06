use crate::edit_funnel::EditFunnel;
use crate::global_ui_state::{GlobalUIEvent, GlobalUIEventHandler, GlobalUIState};
use crate::view_model_util::use_arc;
use crate::viewmodel::ViewModelParams;
use arc_swap::ArcSwap;
use cgmath::{Quaternion, Vector3};
use mpdelta_async_runtime::{AsyncRuntime, JoinHandleWrapper};
use mpdelta_core::common::time_split_value::TimeSplitValue;
use mpdelta_core::component::instance::ComponentInstanceHandle;
use mpdelta_core::component::marker_pin::MarkerPinHandle;
use mpdelta_core::component::parameter::value::EasingValue;
use mpdelta_core::component::parameter::{AbstractFile, BlendMode, CompositeOperation, ImageRequiredParams, ImageRequiredParamsTransform, Never, Parameter, ParameterNullableValue, ParameterValueFixed, ParameterValueType, PinSplitValue, VariableParameterValue, Vector3Params};
use mpdelta_core::edit::InstanceEditCommand;
use mpdelta_core::project::RootComponentClassHandle;
use mpdelta_core::ptr::StaticPointer;
use mpdelta_core::time::TimelineTime;
use qcell::TCellOwner;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::future;
use std::marker::PhantomData;
use std::ops::{DerefMut, Range};
use std::sync::{Arc, Mutex, MutexGuard, RwLock as StdRwLock};
use tokio::sync::RwLock;

pub struct MarkerPinTimeMap<K> {
    pin_index_map: HashMap<MarkerPinHandle<K>, usize>,
    pub times: Vec<f64>,
}

impl<K> MarkerPinTimeMap<K> {
    pub fn builder(key: &TCellOwner<K>) -> MarkerPinTimeMapBuilder<K> {
        MarkerPinTimeMapBuilder { pin_index_map: HashMap::new(), key }
    }
}

pub struct MarkerPinTimeMapBuilder<'a, K: 'static> {
    pin_index_map: HashMap<MarkerPinHandle<K>, TimelineTime>,
    key: &'a TCellOwner<K>,
}

impl<'a, K> MarkerPinTimeMapBuilder<'a, K> {
    fn insert_inner<T>(&mut self, value: &PinSplitValue<K, T>) {
        for i in 0..value.len_time() {
            let (_, pin, _) = value.get_time(i).unwrap();
            if let Entry::Vacant(entry) = self.pin_index_map.entry(pin.clone()) {
                if let Some(pin) = pin.upgrade() {
                    let time = pin.ro(self.key).cached_timeline_time();
                    entry.insert(time);
                }
            }
        }
    }

    pub fn insert_by_image_required_params<T>(&mut self, image_required_params: &ImageRequiredParams<K, T>)
    where
        T: ParameterValueType,
    {
        let ImageRequiredParams {
            transform,
            background_color: _,
            opacity,
            blend_mode,
            composite_operation,
        } = image_required_params;
        self.insert_inner(opacity);
        self.insert_inner(blend_mode);
        self.insert_inner(composite_operation);
        match transform {
            ImageRequiredParamsTransform::Params {
                size,
                scale,
                translate,
                rotate,
                scale_center,
                rotate_center,
            } => {
                AsRef::<[_; 3]>::as_ref(size).iter().for_each(|value| self.insert_inner(&value.params));
                AsRef::<[_; 3]>::as_ref(scale).iter().for_each(|value| self.insert_inner(&value.params));
                AsRef::<[_; 3]>::as_ref(translate).iter().for_each(|value| self.insert_inner(&value.params));
                self.insert_inner(rotate);
                AsRef::<[_; 3]>::as_ref(scale_center).iter().for_each(|value| self.insert_inner(&value.params));
                AsRef::<[_; 3]>::as_ref(rotate_center).iter().for_each(|value| self.insert_inner(&value.params));
            }
            ImageRequiredParamsTransform::Free { left_top, right_top, left_bottom, right_bottom } => {
                AsRef::<[_; 3]>::as_ref(left_top).iter().for_each(|value| self.insert_inner(&value.params));
                AsRef::<[_; 3]>::as_ref(right_top).iter().for_each(|value| self.insert_inner(&value.params));
                AsRef::<[_; 3]>::as_ref(left_bottom).iter().for_each(|value| self.insert_inner(&value.params));
                AsRef::<[_; 3]>::as_ref(right_bottom).iter().for_each(|value| self.insert_inner(&value.params));
            }
        }
    }

    pub fn insert_variable_parameters<T>(&mut self, variable_params: &[VariableParameterValue<K, T, ParameterNullableValue<K, T>>])
    where
        T: ParameterValueType,
    {
        for value in variable_params {
            match &value.params {
                Parameter::None => {}
                Parameter::Image(value) => self.insert_inner(value),
                Parameter::Audio(value) => self.insert_inner(value),
                Parameter::Binary(value) => self.insert_inner(value),
                Parameter::String(value) => self.insert_inner(value),
                Parameter::Integer(value) => self.insert_inner(value),
                Parameter::RealNumber(value) => self.insert_inner(value),
                Parameter::Boolean(value) => self.insert_inner(value),
                Parameter::Dictionary(value) => {
                    let _: &Never = value;
                }
                Parameter::Array(value) => {
                    let _: &Never = value;
                }
                Parameter::ComponentClass(value) => {
                    let _: &Option<()> = value;
                }
            }
        }
    }

    pub fn build(self) -> MarkerPinTimeMap<K> {
        let mut pins = self.pin_index_map.into_iter().collect::<Vec<_>>();
        pins.sort_unstable_by_key(|&(_, time)| time);
        let (pin_index_map, times) = pins.into_iter().enumerate().map(|(i, (pin, time))| ((pin, i), time.value().into_f64())).unzip();
        MarkerPinTimeMap { pin_index_map, times }
    }
}

#[derive(Debug)]
pub struct ImageRequiredParamsForEdit<K: 'static, T: ParameterValueType> {
    pub transform: ImageRequiredParamsTransformForEdit<K, T>,
    pub background_color: [u8; 4],
    pub opacity: PinSplitValue<K, EasingValue<f64>>,
    pub blend_mode: PinSplitValue<K, BlendMode>,
    pub composite_operation: PinSplitValue<K, CompositeOperation>,
}

#[derive(Debug)]
pub enum ImageRequiredParamsTransformForEdit<K: 'static, T: ParameterValueType> {
    Params {
        size: Vector3ParamsForEdit<K, T>,
        scale: Vector3ParamsForEdit<K, T>,
        translate: Vector3ParamsForEdit<K, T>,
        rotate: TimeSplitValue<MarkerPinHandle<K>, EasingValue<Quaternion<f64>>>,
        scale_center: Vector3ParamsForEdit<K, T>,
        rotate_center: Vector3ParamsForEdit<K, T>,
    },
    Free {
        left_top: Vector3ParamsForEdit<K, T>,
        right_top: Vector3ParamsForEdit<K, T>,
        left_bottom: Vector3ParamsForEdit<K, T>,
        right_bottom: Vector3ParamsForEdit<K, T>,
    },
}

#[derive(Debug)]
pub struct ValueWithEditCopy<K: 'static, T: ParameterValueType> {
    pub value: VariableParameterValue<K, T, PinSplitValue<K, Option<EasingValue<f64>>>>,
    pub edit_copy: TimeSplitValue<usize, Option<EasingValue<f64>>>,
}

pub type Vector3ParamsForEdit<K, T> = Vector3<ValueWithEditCopy<K, T>>;

impl<K: 'static, T: ParameterValueType> ImageRequiredParamsForEdit<K, T> {
    pub fn from_image_required_params(value: ImageRequiredParams<K, T>, pin_time_map: &MarkerPinTimeMap<K>) -> ImageRequiredParamsForEdit<K, T> {
        fn into_for_edit<K, T: ParameterValueType>(value: VariableParameterValue<K, T, PinSplitValue<K, Option<EasingValue<f64>>>>, pin_index: &HashMap<MarkerPinHandle<K>, usize>) -> ValueWithEditCopy<K, T> {
            let index_based = value.params.map_time_value_ref(|pin| *pin_index.get(pin).unwrap(), Clone::clone);
            ValueWithEditCopy { value, edit_copy: index_based }
        }
        let ImageRequiredParams {
            transform,
            background_color,
            opacity,
            blend_mode,
            composite_operation,
        } = value;
        let transform = match transform {
            ImageRequiredParamsTransform::Params {
                size,
                scale,
                translate,
                rotate,
                scale_center,
                rotate_center,
            } => ImageRequiredParamsTransformForEdit::Params {
                size: size.map(|value| into_for_edit(value, &pin_time_map.pin_index_map)),
                scale: scale.map(|value| into_for_edit(value, &pin_time_map.pin_index_map)),
                translate: translate.map(|value| into_for_edit(value, &pin_time_map.pin_index_map)),
                rotate,
                scale_center: scale_center.map(|value| into_for_edit(value, &pin_time_map.pin_index_map)),
                rotate_center: rotate_center.map(|value| into_for_edit(value, &pin_time_map.pin_index_map)),
            },
            ImageRequiredParamsTransform::Free { left_top, right_top, left_bottom, right_bottom } => ImageRequiredParamsTransformForEdit::Free {
                left_top: left_top.map(|value| into_for_edit(value, &pin_time_map.pin_index_map)),
                right_top: right_top.map(|value| into_for_edit(value, &pin_time_map.pin_index_map)),
                left_bottom: left_bottom.map(|value| into_for_edit(value, &pin_time_map.pin_index_map)),
                right_bottom: right_bottom.map(|value| into_for_edit(value, &pin_time_map.pin_index_map)),
            },
        };

        ImageRequiredParamsForEdit {
            transform,
            background_color,
            opacity,
            blend_mode,
            composite_operation,
        }
    }

    fn as_non_edit(&self) -> ImageRequiredParams<K, T> {
        let &ImageRequiredParamsForEdit {
            ref transform,
            background_color,
            ref opacity,
            ref blend_mode,
            ref composite_operation,
        } = self;
        fn transform_vec3<K, T: ParameterValueType>(value: &Vector3ParamsForEdit<K, T>) -> Vector3Params<K, T> {
            let Vector3 {
                x: ValueWithEditCopy { value: x, .. },
                y: ValueWithEditCopy { value: y, .. },
                z: ValueWithEditCopy { value: z, .. },
            } = value;
            Vector3::new(x.clone(), y.clone(), z.clone())
        }
        let transform = match transform {
            ImageRequiredParamsTransformForEdit::Params {
                size,
                scale,
                translate,
                rotate,
                scale_center,
                rotate_center,
            } => ImageRequiredParamsTransform::Params {
                size: transform_vec3(size),
                scale: transform_vec3(scale),
                translate: transform_vec3(translate),
                rotate: rotate.clone(),
                scale_center: transform_vec3(scale_center),
                rotate_center: transform_vec3(rotate_center),
            },
            ImageRequiredParamsTransformForEdit::Free { left_top, right_top, left_bottom, right_bottom } => ImageRequiredParamsTransform::Free {
                left_top: transform_vec3(left_top),
                right_top: transform_vec3(right_top),
                left_bottom: transform_vec3(left_bottom),
                right_bottom: transform_vec3(right_bottom),
            },
        };
        ImageRequiredParams {
            transform,
            background_color,
            opacity: opacity.clone(),
            blend_mode: blend_mode.clone(),
            composite_operation: composite_operation.clone(),
        }
    }
}

pub struct TimeSplitValueEditCopy<K, T, V>
where
    K: 'static,
    T: ParameterValueType,
{
    pub value: VariableParameterValue<K, T, ParameterNullableValue<K, T>>,
    pub edit_copy: TimeSplitValue<usize, V>,
}

impl<K, T, V> Clone for TimeSplitValueEditCopy<K, T, V>
where
    T: ParameterValueType,
    V: Clone,
{
    fn clone(&self) -> Self {
        let TimeSplitValueEditCopy { value, edit_copy } = self;
        TimeSplitValueEditCopy { value: value.clone(), edit_copy: edit_copy.clone() }
    }
}

pub struct NullableValueForEdit<K, T>(PhantomData<(K, T)>);

unsafe impl<K, T> Send for NullableValueForEdit<K, T> {}

unsafe impl<K, T> Sync for NullableValueForEdit<K, T> {}

impl<K, T> ParameterValueType for NullableValueForEdit<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    type Image = TimeSplitValueEditCopy<K, T, Option<EasingValue<T::Image>>>;
    type Audio = TimeSplitValueEditCopy<K, T, Option<EasingValue<T::Audio>>>;
    type Binary = TimeSplitValueEditCopy<K, T, Option<EasingValue<AbstractFile>>>;
    type String = TimeSplitValueEditCopy<K, T, Option<EasingValue<String>>>;
    type Integer = TimeSplitValueEditCopy<K, T, Option<EasingValue<i64>>>;
    type RealNumber = TimeSplitValueEditCopy<K, T, Option<EasingValue<f64>>>;
    type Boolean = TimeSplitValueEditCopy<K, T, Option<EasingValue<bool>>>;
    type Dictionary = Never;
    type Array = Never;
    type ComponentClass = Option<()>;
}

impl<K, T> NullableValueForEdit<K, T>
where
    K: 'static,
    T: ParameterValueType,
{
    pub fn from_variable_parameter_value(value: VariableParameterValue<K, T, ParameterNullableValue<K, T>>, pin_time_map: &MarkerPinTimeMap<K>) -> Parameter<NullableValueForEdit<K, T>> {
        let pin_index = &pin_time_map.pin_index_map;
        match &value.params {
            Parameter::None => Parameter::None,
            Parameter::Image(value_inner) => {
                let edit_copy = value_inner.map_time_value_ref(|pin| pin_index[pin], Clone::clone);
                Parameter::Image(TimeSplitValueEditCopy { value, edit_copy })
            }
            Parameter::Audio(value_inner) => {
                let edit_copy = value_inner.map_time_value_ref(|pin| pin_index[pin], Clone::clone);
                Parameter::Audio(TimeSplitValueEditCopy { value, edit_copy })
            }
            Parameter::Binary(value_inner) => {
                let edit_copy = value_inner.map_time_value_ref(|pin| pin_index[pin], Clone::clone);
                Parameter::Binary(TimeSplitValueEditCopy { value, edit_copy })
            }
            Parameter::String(value_inner) => {
                let edit_copy = value_inner.map_time_value_ref(|pin| pin_index[pin], Clone::clone);
                Parameter::String(TimeSplitValueEditCopy { value, edit_copy })
            }
            Parameter::Integer(value_inner) => {
                let edit_copy = value_inner.map_time_value_ref(|pin| pin_index[pin], Clone::clone);
                Parameter::Integer(TimeSplitValueEditCopy { value, edit_copy })
            }
            Parameter::RealNumber(value_inner) => {
                let edit_copy = value_inner.map_time_value_ref(|pin| pin_index[pin], Clone::clone);
                Parameter::RealNumber(TimeSplitValueEditCopy { value, edit_copy })
            }
            Parameter::Boolean(value_inner) => {
                let edit_copy = value_inner.map_time_value_ref(|pin| pin_index[pin], Clone::clone);
                Parameter::Boolean(TimeSplitValueEditCopy { value, edit_copy })
            }
            Parameter::Dictionary(value_inner) => {
                let _: &Never = value_inner;
                unreachable!()
            }
            Parameter::Array(value_inner) => {
                let _: &Never = value_inner;
                unreachable!()
            }
            Parameter::ComponentClass(value_inner) => {
                let _: &Option<()> = value_inner;
                unreachable!()
            }
        }
    }
}

pub struct WithName<T> {
    pub name: String,
    pub value: T,
}

impl<T> WithName<T> {
    pub fn new(name: String, value: T) -> WithName<T> {
        WithName { name, value }
    }
}

pub struct ParametersEditSet<K: 'static, T: ParameterValueType> {
    pub image_required_params: Option<ImageRequiredParamsForEdit<K, T>>,
    #[allow(clippy::type_complexity)]
    pub fixed_params: Box<[WithName<ParameterValueFixed<T::Image, T::Audio>>]>,
    #[allow(clippy::type_complexity)]
    pub variable_params: Box<[WithName<Parameter<NullableValueForEdit<K, T>>>]>,
    pub pin_times: Box<[f64]>,
}

pub trait PropertyWindowViewModel<K: 'static, T: ParameterValueType> {
    type Parameters<'a>: DerefMut<Target = Option<ParametersEditSet<K, T>>> + 'a
    where
        Self: 'a;
    fn selected_instance_at(&self) -> Range<f64>;
    fn parameters(&self) -> Self::Parameters<'_>;
    fn updated_image_required_params(&self, image_required_params: &ImageRequiredParamsForEdit<K, T>);
    fn updated_fixed_params(&self, fixed_params: &[WithName<ParameterValueFixed<T::Image, T::Audio>>]);
    fn updated_variable_params(&self, variable_params: &[WithName<Parameter<NullableValueForEdit<K, T>>>]);
}

struct SelectedItem<K: 'static, T: ParameterValueType> {
    root: Option<RootComponentClassHandle<K, T>>,
    instance: Option<ComponentInstanceHandle<K, T>>,
}

impl<K: 'static, T: ParameterValueType> Default for SelectedItem<K, T> {
    fn default() -> Self {
        SelectedItem { root: None, instance: None }
    }
}

pub struct PropertyWindowViewModelImpl<K: 'static, T: ParameterValueType, Edit, Runtime, JoinHandle> {
    edit: Arc<Edit>,
    selected: Arc<StdRwLock<SelectedItem<K, T>>>,
    selected_instance_at: Arc<ArcSwap<Range<f64>>>,
    parameters: Arc<Mutex<Option<ParametersEditSet<K, T>>>>,
    image_required_params_update_task: Mutex<JoinHandleWrapper<JoinHandle>>,
    runtime: Runtime,
    key: Arc<RwLock<TCellOwner<K>>>,
}

impl<K, T, Edit, Runtime> GlobalUIEventHandler<K, T> for PropertyWindowViewModelImpl<K, T, Edit, Runtime, Runtime::JoinHandle>
where
    K: 'static,
    T: ParameterValueType,
    Edit: EditFunnel<K, T>,
    Runtime: AsyncRuntime<()> + Clone,
{
    fn handle(&self, event: GlobalUIEvent<K, T>) {
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
                    selected.instance = instance.clone();
                } else {
                    return;
                }
                drop(selected);
                let mut task = self.image_required_params_update_task.lock().unwrap();
                task.abort();
                use_arc!(parameters = self.parameters, key = self.key, selected_instance_at = self.selected_instance_at);
                *task = self.runtime.spawn(async move {
                    let key = key.read().await;

                    if let Some(instance) = instance.as_ref().and_then(StaticPointer::upgrade) {
                        let instance = instance.ro(&key);
                        selected_instance_at.store(Arc::new(instance.marker_left().ro(&key).cached_timeline_time().value().into_f64()..instance.marker_right().ro(&key).cached_timeline_time().value().into_f64()));
                        let mut pin_time_map = MarkerPinTimeMap::builder(&key);
                        if let Some(params) = instance.image_required_params() {
                            pin_time_map.insert_by_image_required_params(params);
                        }
                        let variable_params = instance.variable_parameters();
                        pin_time_map.insert_variable_parameters(variable_params);
                        let pin_time_map = pin_time_map.build();
                        let image_required_params = instance.image_required_params().cloned().map(|value| ImageRequiredParamsForEdit::from_image_required_params(value, &pin_time_map));
                        let new_fixed_params = instance.fixed_parameters();
                        let fixed_params_type = instance.fixed_parameters_type();
                        assert_eq!(new_fixed_params.len(), fixed_params_type.len());
                        let fixed_params = new_fixed_params.iter().zip(fixed_params_type).map(|(value, (name, _))| WithName::new(name.clone(), value.clone())).collect();
                        let variable_params_type = instance.variable_parameters_type();
                        assert_eq!(variable_params.len(), variable_params_type.len());
                        let variable_params = variable_params
                            .iter()
                            .zip(variable_params_type)
                            .map(|(value, (name, _))| WithName::new(name.clone(), NullableValueForEdit::from_variable_parameter_value(value.clone(), &pin_time_map)))
                            .collect();
                        *parameters.lock().unwrap() = Some(ParametersEditSet {
                            image_required_params,
                            fixed_params,
                            variable_params,
                            pin_times: pin_time_map.times.into_boxed_slice(),
                        });
                    } else {
                        *parameters.lock().unwrap() = None;
                    }
                });
            }
            _ => {}
        }
    }
}

impl<K: 'static, T: ParameterValueType> PropertyWindowViewModelImpl<K, T, (), (), ()> {
    #[allow(clippy::type_complexity)]
    pub fn new<S: GlobalUIState<K, T>, Edit: EditFunnel<K, T> + 'static, P: ViewModelParams<K, T>>(global_ui_state: &Arc<S>, edit: &Arc<Edit>, params: &P) -> Arc<PropertyWindowViewModelImpl<K, T, Edit, P::AsyncRuntime, <P::AsyncRuntime as AsyncRuntime<()>>::JoinHandle>> {
        let runtime = params.runtime().clone();
        let arc = Arc::new(PropertyWindowViewModelImpl {
            edit: Arc::clone(edit),
            selected: Arc::new(StdRwLock::new(SelectedItem::default())),
            selected_instance_at: Arc::new(ArcSwap::new(Arc::new(0.0..0.0))),
            parameters: Arc::new(Mutex::new(None)),
            image_required_params_update_task: Mutex::new(runtime.spawn(future::ready(()))),
            runtime,
            key: Arc::clone(params.key()),
        });
        global_ui_state.register_global_ui_event_handler(Arc::clone(&arc));
        arc
    }
}

impl<K, T, Edit, Runtime> PropertyWindowViewModel<K, T> for PropertyWindowViewModelImpl<K, T, Edit, Runtime, Runtime::JoinHandle>
where
    K: 'static,
    T: ParameterValueType,
    Edit: EditFunnel<K, T>,
    Runtime: AsyncRuntime<()> + Clone,
{
    type Parameters<'a> = MutexGuard<'a, Option<ParametersEditSet<K, T>>> where Self: 'a;

    fn selected_instance_at(&self) -> Range<f64> {
        (**self.selected_instance_at.load()).clone()
    }

    fn parameters(&self) -> Self::Parameters<'_> {
        self.parameters.lock().unwrap()
    }

    fn updated_image_required_params(&self, image_required_params: &ImageRequiredParamsForEdit<K, T>) {
        let SelectedItem {
            root: Some(root_component_class),
            instance: Some(component_instance),
        } = &*self.selected.read().unwrap()
        else {
            return;
        };
        self.edit.edit_instance(root_component_class, component_instance, InstanceEditCommand::UpdateImageRequiredParams(image_required_params.as_non_edit()));
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

    fn updated_variable_params(&self, variable_params: &[WithName<Parameter<NullableValueForEdit<K, T>>>]) {
        let SelectedItem {
            root: Some(root_component_class),
            instance: Some(component_instance),
        } = &*self.selected.read().unwrap()
        else {
            return;
        };
        self.edit.edit_instance(
            root_component_class,
            component_instance,
            InstanceEditCommand::UpdateVariableParams(
                variable_params
                    .iter()
                    .map(|WithName { value, .. }| match value {
                        Parameter::None => VariableParameterValue::new(Parameter::None),
                        Parameter::Image(value) => value.value.clone(),
                        Parameter::Audio(value) => value.value.clone(),
                        Parameter::Binary(value) => value.value.clone(),
                        Parameter::String(value) => value.value.clone(),
                        Parameter::Integer(value) => value.value.clone(),
                        Parameter::RealNumber(value) => value.value.clone(),
                        Parameter::Boolean(value) => value.value.clone(),
                        Parameter::Dictionary(value) => {
                            let _: &Never = value;
                            unreachable!()
                        }
                        Parameter::Array(value) => {
                            let _: &Never = value;
                            unreachable!()
                        }
                        Parameter::ComponentClass(value) => {
                            let _: &Option<()> = value;
                            unreachable!()
                        }
                    })
                    .collect(),
            ),
        );
    }
}
