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
use mpdelta_core::component::parameter::{BlendMode, CompositeOperation, ImageRequiredParams, ImageRequiredParamsTransform, ParameterValueType, PinSplitValue, VariableParameterValue, Vector3Params};
use mpdelta_core::edit::InstanceEditCommand;
use mpdelta_core::project::RootComponentClassHandle;
use mpdelta_core::ptr::{StaticPointer, StaticPointerOwned};
use mpdelta_core::time::TimelineTime;
use qcell::TCellOwner;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::ops::{DerefMut, Range};
use std::sync::{Arc, Mutex, MutexGuard, RwLock as StdRwLock};
use std::{future, iter};
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct ImageRequiredParamsForEdit<K: 'static, T: ParameterValueType> {
    pub aspect_ratio: (u32, u32),
    pub transform: ImageRequiredParamsTransformForEdit<K, T>,
    pub background_color: [u8; 4],
    pub opacity: PinSplitValue<K, EasingValue<f64>>,
    pub blend_mode: PinSplitValue<K, BlendMode>,
    pub composite_operation: PinSplitValue<K, CompositeOperation>,
}

#[derive(Debug)]
pub enum ImageRequiredParamsTransformForEdit<K: 'static, T: ParameterValueType> {
    Params {
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
    pub fn from_image_required_params(value: ImageRequiredParams<K, T>, all_pins: impl IntoIterator<Item = MarkerPinHandle<K>>, key: &TCellOwner<K>) -> (ImageRequiredParamsForEdit<K, T>, Vec<f64>) {
        fn get_all_pins_iter<K, T>(values: &PinSplitValue<K, T>) -> impl Iterator<Item = MarkerPinHandle<K>> + '_ {
            (0..values.len_time()).map(|i| {
                let (_, pin, _) = values.get_time(i).unwrap();
                pin.clone()
            })
        }
        fn collect_into_pins_map<K>(pins_map: &mut HashMap<MarkerPinHandle<K>, TimelineTime>, iter: impl IntoIterator<Item = MarkerPinHandle<K>>, key: &TCellOwner<K>) {
            for pin in iter {
                if let Entry::Vacant(entry) = pins_map.entry(pin) {
                    if let Some(pin) = entry.key().upgrade() {
                        let time = pin.ro(key).cached_timeline_time();
                        entry.insert(time);
                    }
                }
            }
        }
        fn pins_map_into_required_structures<K>(pins_map: HashMap<MarkerPinHandle<K>, TimelineTime>) -> (HashMap<MarkerPinHandle<K>, usize>, Vec<f64>) {
            let mut pins = pins_map.into_iter().collect::<Vec<_>>();
            pins.sort_unstable_by_key(|&(_, time)| time);
            pins.into_iter().enumerate().map(|(i, (pin, time))| ((pin, i), time.value().into_f64())).unzip()
        }
        fn into_for_edit<K, T: ParameterValueType>(value: VariableParameterValue<K, T, PinSplitValue<K, Option<EasingValue<f64>>>>, pin_index: &HashMap<MarkerPinHandle<K>, usize>) -> ValueWithEditCopy<K, T> {
            let index_based = value.params.map_time_value_ref(|pin| *pin_index.get(pin).unwrap(), Clone::clone);
            ValueWithEditCopy { value, edit_copy: index_based }
        }
        let ImageRequiredParams {
            aspect_ratio,
            transform,
            background_color,
            opacity,
            blend_mode,
            composite_operation,
        } = value;
        let mut pins_map = all_pins
            .into_iter()
            .filter_map(|pin| {
                let time = pin.upgrade()?.ro(key).cached_timeline_time();
                Some((pin, time))
            })
            .collect::<HashMap<_, _>>();
        let iter = get_all_pins_iter(&opacity).chain(get_all_pins_iter(&blend_mode)).chain(get_all_pins_iter(&composite_operation));
        let (transform, times) = match transform {
            ImageRequiredParamsTransform::Params { scale, translate, rotate, scale_center, rotate_center } => {
                let iter = iter
                    .chain(AsRef::<[_; 3]>::as_ref(&scale).iter().flat_map(|value| get_all_pins_iter(&value.params)))
                    .chain(AsRef::<[_; 3]>::as_ref(&translate).iter().flat_map(|value| get_all_pins_iter(&value.params)))
                    .chain(get_all_pins_iter(&rotate))
                    .chain(AsRef::<[_; 3]>::as_ref(&scale_center).iter().flat_map(|value| get_all_pins_iter(&value.params)))
                    .chain(AsRef::<[_; 3]>::as_ref(&rotate_center).iter().flat_map(|value| get_all_pins_iter(&value.params)));
                collect_into_pins_map(&mut pins_map, iter, key);
                let (pin_index, times) = pins_map_into_required_structures(pins_map);
                let transform = ImageRequiredParamsTransformForEdit::Params {
                    scale: scale.map(|value| into_for_edit(value, &pin_index)),
                    translate: translate.map(|value| into_for_edit(value, &pin_index)),
                    rotate,
                    scale_center: scale_center.map(|value| into_for_edit(value, &pin_index)),
                    rotate_center: rotate_center.map(|value| into_for_edit(value, &pin_index)),
                };
                (transform, times)
            }
            ImageRequiredParamsTransform::Free { left_top, right_top, left_bottom, right_bottom } => {
                let iter = iter
                    .chain(AsRef::<[_; 3]>::as_ref(&left_top).iter().flat_map(|value| get_all_pins_iter(&value.params)))
                    .chain(AsRef::<[_; 3]>::as_ref(&right_top).iter().flat_map(|value| get_all_pins_iter(&value.params)))
                    .chain(AsRef::<[_; 3]>::as_ref(&left_bottom).iter().flat_map(|value| get_all_pins_iter(&value.params)))
                    .chain(AsRef::<[_; 3]>::as_ref(&right_bottom).iter().flat_map(|value| get_all_pins_iter(&value.params)));
                collect_into_pins_map(&mut pins_map, iter, key);
                let (pin_index, times) = pins_map_into_required_structures(pins_map);
                let transform = ImageRequiredParamsTransformForEdit::Free {
                    left_top: left_top.map(|value| into_for_edit(value, &pin_index)),
                    right_top: right_top.map(|value| into_for_edit(value, &pin_index)),
                    left_bottom: left_bottom.map(|value| into_for_edit(value, &pin_index)),
                    right_bottom: right_bottom.map(|value| into_for_edit(value, &pin_index)),
                };
                (transform, times)
            }
        };
        let value = ImageRequiredParamsForEdit {
            aspect_ratio,
            transform,
            background_color,
            opacity,
            blend_mode,
            composite_operation,
        };
        (value, times)
    }

    fn as_non_edit(&self) -> ImageRequiredParams<K, T> {
        let &ImageRequiredParamsForEdit {
            aspect_ratio,
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
            ImageRequiredParamsTransformForEdit::Params { scale, translate, rotate, scale_center, rotate_center } => ImageRequiredParamsTransform::Params {
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
            aspect_ratio,
            transform,
            background_color,
            opacity: opacity.clone(),
            blend_mode: blend_mode.clone(),
            composite_operation: composite_operation.clone(),
        }
    }
}

pub struct ImageRequiredParamsEditSet<K: 'static, T: ParameterValueType, Times> {
    pub params: ImageRequiredParamsForEdit<K, T>,
    pub pin_times: Times,
}

pub trait PropertyWindowViewModel<K: 'static, T: ParameterValueType> {
    type Times: AsRef<[f64]>;
    type ImageRequiredParams<'a>: DerefMut<Target = Option<ImageRequiredParamsEditSet<K, T, Self::Times>>> + 'a
    where
        Self: 'a;
    fn selected_instance_at(&self) -> Range<f64>;
    fn image_required_params(&self) -> Self::ImageRequiredParams<'_>;
    fn updated_image_required_params(&self, image_required_params: &ImageRequiredParamsForEdit<K, T>);
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

pub struct PropertyWindowViewModelImpl<K: 'static, T: ParameterValueType, GlobalUIState, Edit, Runtime, JoinHandle> {
    global_ui_state: Arc<GlobalUIState>,
    edit: Arc<Edit>,
    selected: Arc<StdRwLock<SelectedItem<K, T>>>,
    selected_instance_at: Arc<ArcSwap<Range<f64>>>,
    #[allow(clippy::type_complexity)]
    image_required_params: Arc<Mutex<Option<ImageRequiredParamsEditSet<K, T, Vec<f64>>>>>,
    image_required_params_update_task: Mutex<JoinHandleWrapper<JoinHandle>>,
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
                use_arc!(image_required_params = self.image_required_params, key = self.key, selected_instance_at = self.selected_instance_at);
                *task = self.runtime.spawn(async move {
                    let key = key.read().await;

                    let mut image_required_params = image_required_params.lock().unwrap();
                    *image_required_params = if let Some(instance) = instance.as_ref().and_then(StaticPointer::upgrade) {
                        let instance = instance.ro(&key);
                        if let (Some(left), Some(right)) = (instance.marker_left().upgrade(), instance.marker_right().upgrade()) {
                            selected_instance_at.store(Arc::new(left.ro(&key).cached_timeline_time().value().into_f64()..right.ro(&key).cached_timeline_time().value().into_f64()));
                        }
                        instance.image_required_params().cloned().map(|value| {
                            let markers = iter::once(instance.marker_left()).chain(instance.markers().iter().map(StaticPointerOwned::reference)).chain(iter::once(instance.marker_right())).cloned();
                            let (params, pin_times) = ImageRequiredParamsForEdit::from_image_required_params(value, markers, &key);
                            ImageRequiredParamsEditSet { params, pin_times }
                        })
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
    #[allow(clippy::type_complexity)]
    pub fn new<S: GlobalUIState<K, T>, Edit: EditFunnel<K, T> + 'static, P: ViewModelParams<K, T>>(global_ui_state: &Arc<S>, edit: &Arc<Edit>, params: &P) -> Arc<PropertyWindowViewModelImpl<K, T, S, Edit, P::AsyncRuntime, <P::AsyncRuntime as AsyncRuntime<()>>::JoinHandle>> {
        let runtime = params.runtime().clone();
        let arc = Arc::new(PropertyWindowViewModelImpl {
            global_ui_state: Arc::clone(global_ui_state),
            edit: Arc::clone(edit),
            selected: Arc::new(StdRwLock::new(SelectedItem::default())),
            selected_instance_at: Arc::new(ArcSwap::new(Arc::new(0.0..0.0))),
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
    type Times = Vec<f64>;
    type ImageRequiredParams<'a> = MutexGuard<'a, Option<ImageRequiredParamsEditSet<K, T, Self::Times>>> where Self: 'a;

    fn selected_instance_at(&self) -> Range<f64> {
        (**self.selected_instance_at.load()).clone()
    }

    fn image_required_params(&self) -> Self::ImageRequiredParams<'_> {
        self.image_required_params.lock().unwrap()
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
}
