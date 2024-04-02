use crate::property_window::view::widgets::editable_easing_value_f64::{EasingValueEditorF64, EasingValueF64EditEvent, Side};
use crate::property_window::view::widgets::editable_easing_value_string::{EasingValueEditorString, EasingValueStringEditEvent};
use crate::property_window::viewmodel::{ImageRequiredParamsTransformForEdit, ParametersEditSet, PropertyWindowViewModel, TimeSplitValueEditCopy, ValueWithEditCopy, WithName};
use cgmath::Vector3;
use egui::scroll_area::ScrollBarVisibility;
use egui::style::ScrollStyle;
use egui::{ScrollArea, Sense, Ui, Vec2};
use mpdelta_core::common::time_split_value::TimeSplitValue;
use mpdelta_core::component::parameter::value::{EasingValue, EasingValueEdit, SingleValueEdit};
use mpdelta_core::component::parameter::{Parameter, ParameterValueFixed, ParameterValueType, PinSplitValue, VariableParameterValue};
use smallvec::SmallVec;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::Arc;
use std::{iter, mem};

mod widgets;

pub struct PropertyWindow<K, T, VM> {
    view_model: Arc<VM>,
    scroll_offset: f32,
    _phantom: PhantomData<(K, T)>,
}

fn extend_fn<T, E: Extend<T>>(e: &mut E) -> impl FnMut(T) + '_ {
    move |t| e.extend(iter::once(t))
}

fn edit_value_f64_by_event<K>(translate_x: &mut PinSplitValue<K, Option<EasingValue<f64>>>, tx: &mut TimeSplitValue<usize, Option<EasingValue<f64>>>, edit_events: SmallVec<[EasingValueF64EditEvent; 1]>) -> bool {
    let mut edited = false;
    for edit in edit_events {
        let (slot, side, value) = match edit {
            EasingValueF64EditEvent::FlipPin(_) => {
                eprintln!("not supported");
                continue;
            }
            EasingValueF64EditEvent::MoveValueTemporary { value_index, side, value } => {
                let Some((_, Some(slot), _)) = tx.get_value_mut(value_index) else { unreachable!() };
                (slot, side, value)
            }
            EasingValueF64EditEvent::MoveValue { value_index, side, value } => {
                let Some((_, Some(slot), _)) = translate_x.get_value_mut(value_index) else { unreachable!() };
                edited = true;
                (slot, side, value)
            }
        };
        slot.value
            .edit_value::<(f64, f64), _>(|(left, right)| {
                let ptr = match side {
                    Side::Left => left,
                    Side::Right => right,
                };
                *ptr = value;
            })
            .unwrap();
    }
    edited
}

fn edit_value_string_by_event<K, T>(edit_copy: &mut TimeSplitValueEditCopy<K, T, Option<EasingValue<String>>>, edit_events: SmallVec<[EasingValueStringEditEvent; 1]>) -> bool
where
    T: ParameterValueType,
{
    let mut edited = false;
    for edit in edit_events {
        let (slot, value) = match edit {
            EasingValueStringEditEvent::FlipPin(_) => {
                eprintln!("not supported");
                continue;
            }
            EasingValueStringEditEvent::UpdateValue { value_index, value } => {
                let Parameter::String(slot) = &mut edit_copy.value.params else {
                    continue;
                };
                let Some((_, Some(slot), _)) = slot.get_value_mut(value_index) else { unreachable!() };
                edited = true;
                (slot, value)
            }
        };
        slot.value.edit_value::<String, _>(|slot| *slot = value).unwrap();
    }
    edited
}

impl<K: 'static, T: ParameterValueType, VM: PropertyWindowViewModel<K, T>> PropertyWindow<K, T, VM> {
    pub fn new(view_model: Arc<VM>) -> PropertyWindow<K, T, VM> {
        PropertyWindow { view_model, scroll_offset: 0., _phantom: PhantomData }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let id = ui.make_persistent_id("PropertyWindow");
        let instance_range = self.view_model.selected_instance_at();
        let instance_length = (instance_range.end - instance_range.start) as f32;
        let point_per_second = 320.;
        let (rect, _) = ui.allocate_at_least(ui.available_size(), Sense::click());
        ui.allocate_ui_at_rect(rect, |ui| {
            if let Some(ParametersEditSet {
                image_required_params,
                fixed_params,
                variable_params,
                pin_times,
            }) = &mut *self.view_model.parameters()
            {
                ui.label("Component Properties");
                ScrollArea::vertical()
                    .max_height(ui.available_height() - (ScrollStyle::solid().bar_width + ScrollStyle::solid().bar_inner_margin * 2. + ScrollStyle::solid().bar_outer_margin * 2.))
                    .show(ui, |ui| {
                        let mut edited = false;
                        if let Some(image_required_params) = image_required_params {
                            if let ImageRequiredParamsTransformForEdit::Params {
                                size:
                                    Vector3 {
                                        x: ValueWithEditCopy {
                                            value: VariableParameterValue { params: size_x, .. },
                                            edit_copy: zx,
                                        },
                                        y: ValueWithEditCopy {
                                            value: VariableParameterValue { params: size_y, .. },
                                            edit_copy: zy,
                                        },
                                        ..
                                    },
                                scale:
                                    Vector3 {
                                        x: ValueWithEditCopy {
                                            value: VariableParameterValue { params: scale_x, .. },
                                            edit_copy: sx,
                                        },
                                        y: ValueWithEditCopy {
                                            value: VariableParameterValue { params: scale_y, .. },
                                            edit_copy: sy,
                                        },
                                        ..
                                    },
                                translate:
                                    Vector3 {
                                        x: ValueWithEditCopy {
                                            value: VariableParameterValue { params: translate_x, .. },
                                            edit_copy: tx,
                                        },
                                        y: ValueWithEditCopy {
                                            value: VariableParameterValue { params: translate_y, .. },
                                            edit_copy: ty,
                                        },
                                        ..
                                    },
                                ..
                            } = &mut image_required_params.transform
                            {
                                ui.label("position - X");
                                let mut edit_events = SmallVec::<[_; 1]>::new();
                                EasingValueEditorF64 {
                                    id: "position - X",
                                    time_range: instance_range.clone(),
                                    times: pin_times.as_ref(),
                                    value: tx,
                                    value_range: -3.0..3.0,
                                    point_per_second,
                                    scroll_offset: &mut self.scroll_offset,
                                    update: extend_fn(&mut edit_events),
                                }
                                .show(ui);
                                edited |= edit_value_f64_by_event(translate_x, tx, edit_events);
                                let mut edit_events = SmallVec::<[_; 1]>::new();
                                ui.label("position - Y");
                                EasingValueEditorF64 {
                                    id: "position - Y",
                                    time_range: instance_range.clone(),
                                    times: pin_times.as_ref(),
                                    value: ty,
                                    value_range: -3.0..3.0,
                                    point_per_second,
                                    scroll_offset: &mut self.scroll_offset,
                                    update: extend_fn(&mut edit_events),
                                }
                                .show(ui);
                                edited |= edit_value_f64_by_event(translate_y, ty, edit_events);
                                let mut edit_events = SmallVec::<[_; 1]>::new();
                                ui.label("size - X");
                                EasingValueEditorF64 {
                                    id: "size - X",
                                    time_range: instance_range.clone(),
                                    times: pin_times.as_ref(),
                                    value: zx,
                                    value_range: 0.0..2.0,
                                    point_per_second,
                                    scroll_offset: &mut self.scroll_offset,
                                    update: extend_fn(&mut edit_events),
                                }
                                .show(ui);
                                edited |= edit_value_f64_by_event(size_x, zx, edit_events);
                                let mut edit_events = SmallVec::<[_; 1]>::new();
                                ui.label("size - Y");
                                EasingValueEditorF64 {
                                    id: "size - Y",
                                    time_range: instance_range.clone(),
                                    times: pin_times.as_ref(),
                                    value: zy,
                                    value_range: 0.0..2.0,
                                    point_per_second,
                                    scroll_offset: &mut self.scroll_offset,
                                    update: extend_fn(&mut edit_events),
                                }
                                .show(ui);
                                edited |= edit_value_f64_by_event(size_y, zy, edit_events);
                                let mut edit_events = SmallVec::<[_; 1]>::new();
                                ui.label("scale - X");
                                EasingValueEditorF64 {
                                    id: "scale - X",
                                    time_range: instance_range.clone(),
                                    times: pin_times.as_ref(),
                                    value: sx,
                                    value_range: 0.0..2.0,
                                    point_per_second,
                                    scroll_offset: &mut self.scroll_offset,
                                    update: extend_fn(&mut edit_events),
                                }
                                .show(ui);
                                edited |= edit_value_f64_by_event(scale_x, sx, edit_events);
                                let mut edit_events = SmallVec::<[_; 1]>::new();
                                ui.label("scale - Y");
                                EasingValueEditorF64 {
                                    id: "scale - Y",
                                    time_range: instance_range.clone(),
                                    times: pin_times.as_ref(),
                                    value: sy,
                                    value_range: 0.0..2.0,
                                    point_per_second,
                                    scroll_offset: &mut self.scroll_offset,
                                    update: extend_fn(&mut edit_events),
                                }
                                .show(ui);
                                edited |= edit_value_f64_by_event(scale_y, sy, edit_events);
                            }
                            if edited {
                                self.view_model.updated_image_required_params(image_required_params);
                            }
                        }

                        let mut edited = false;
                        for WithName { name, value } in fixed_params.as_mut().iter_mut() {
                            ui.label(name.clone());
                            match value {
                                ParameterValueFixed::None => {}
                                ParameterValueFixed::Image(_value) => {}
                                ParameterValueFixed::Audio(_value) => {}
                                ParameterValueFixed::Binary(value) => {
                                    let edit_as_path = value.edit_value(|path: &mut PathBuf| {
                                        let before = path.to_str().unwrap_or("").to_owned();
                                        let mut edit = before.clone();
                                        ui.text_edit_singleline(&mut edit);
                                        if before != edit {
                                            *path = PathBuf::from(edit);
                                            true
                                        } else {
                                            false
                                        }
                                    });
                                    if let Ok(edit) = edit_as_path {
                                        edited |= edit;
                                        continue;
                                    }
                                }
                                ParameterValueFixed::String(value) => {
                                    let edit_as_string = value.edit_value(|s: &mut String| {
                                        let mut edit = s.clone();
                                        ui.text_edit_singleline(&mut edit);
                                        if s != &edit {
                                            *s = edit;
                                            true
                                        } else {
                                            false
                                        }
                                    });
                                    if let Ok(edit) = edit_as_string {
                                        edited |= edit;
                                        continue;
                                    }
                                }
                                ParameterValueFixed::Integer(_value) => {}
                                ParameterValueFixed::RealNumber(_value) => {}
                                ParameterValueFixed::Boolean(_value) => {}
                                ParameterValueFixed::Dictionary(_value) => {}
                                ParameterValueFixed::Array(_value) => {}
                                ParameterValueFixed::ComponentClass(()) => {}
                            }
                            ui.label("Unknown ParameterValueFixed");
                        }
                        if edited {
                            self.view_model.updated_fixed_params(fixed_params);
                        }

                        let mut edited = false;
                        for WithName { name, value } in variable_params.as_mut().iter_mut() {
                            ui.label(name.clone());
                            match value {
                                Parameter::None => {}
                                Parameter::Image(_value) => {}
                                Parameter::Audio(_value) => {}
                                Parameter::Binary(_value) => {}
                                Parameter::String(value) => {
                                    let mut edit_events = SmallVec::<[_; 1]>::new();
                                    EasingValueEditorString {
                                        id: name,
                                        time_range: instance_range.clone(),
                                        times: pin_times.as_ref(),
                                        value: &mut value.edit_copy,
                                        point_per_second,
                                        scroll_offset: &mut self.scroll_offset,
                                        update: extend_fn(&mut edit_events),
                                    }
                                    .show(ui);
                                    edited |= edit_value_string_by_event(value, edit_events);
                                    continue;
                                }
                                Parameter::Integer(_value) => {}
                                Parameter::RealNumber(_value) => {}
                                Parameter::Boolean(_value) => {}
                                Parameter::Dictionary(_value) => {}
                                Parameter::Array(_value) => {}
                                Parameter::ComponentClass(_) => {}
                            }
                            ui.label("Unknown VariableParameter");
                        }
                        if edited {
                            self.view_model.updated_variable_params(variable_params);
                        }
                    });
                let old_scroll_style = mem::replace(&mut ui.style_mut().spacing.scroll, ScrollStyle::solid());
                let scroll_output = ScrollArea::horizontal()
                    .horizontal_scroll_offset(self.scroll_offset)
                    .id_source(id.with("scroll_bar"))
                    .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                    .show(ui, |ui| ui.allocate_space(Vec2::new(instance_length * point_per_second as f32, 0.)));
                self.scroll_offset = scroll_output.state.offset.x;
                ui.style_mut().spacing.scroll = old_scroll_style;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::property_window::view::PropertyWindow;
    use crate::property_window::viewmodel::{ImageRequiredParamsForEdit, MarkerPinTimeMap, NullableValueForEdit, ParametersEditSet, PropertyWindowViewModel, WithName};
    use egui::Visuals;
    use egui_image_renderer::FileFormat;
    use mpdelta_core::component::marker_pin::{MarkerPin, MarkerPinHandleOwned, MarkerTime};
    use mpdelta_core::component::parameter::value::{DynEditableEasingValue, DynEditableSelfValue, DynEditableSingleValue, EasingValue, LinearEasing};
    use mpdelta_core::component::parameter::{ImageRequiredParams, Parameter, ParameterNullableValue, ParameterValueFixed, ParameterValueType, VariableParameterPriority, VariableParameterValue};
    use mpdelta_core::ptr::StaticPointerOwned;
    use mpdelta_core::time::TimelineTime;
    use mpdelta_core::{mfrac, time_split_value};
    use qcell::{TCell, TCellOwner};
    use std::io::Cursor;
    use std::ops::Range;
    use std::path::Path;
    use std::sync::{Arc, Mutex, MutexGuard};

    #[tokio::test]
    async fn view_property_window() {
        const TEST_OUTPUT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../test_output/", env!("CARGO_PKG_NAME"));
        let test_output_dir = Path::new(TEST_OUTPUT_DIR);
        tokio::fs::create_dir_all(test_output_dir).await.unwrap();
        struct K;
        struct T;
        impl ParameterValueType for T {
            type Image = ();
            type Audio = ();
            type Binary = ();
            type String = ();
            type Integer = ();
            type RealNumber = ();
            type Boolean = ();
            type Dictionary = ();
            type Array = ();
            type ComponentClass = ();
        }
        struct VM {
            params: Mutex<Option<ParametersEditSet<K, T>>>,
        }
        impl PropertyWindowViewModel<K, T> for VM {
            type Parameters<'a> = MutexGuard<'a, Option<ParametersEditSet<K, T>>>;

            fn selected_instance_at(&self) -> Range<f64> {
                0.0..1.0
            }

            fn parameters(&self) -> Self::Parameters<'_> {
                self.params.lock().unwrap()
            }

            fn updated_image_required_params(&self, _image_required_params: &ImageRequiredParamsForEdit<K, T>) {}
            fn updated_fixed_params(&self, _fixed_params: &[WithName<ParameterValueFixed<(), ()>>]) {}
            fn updated_variable_params(&self, _variable_params: &[WithName<Parameter<NullableValueForEdit<K, T>>>]) {}
        }
        let owner = TCellOwner::new();
        let left = MarkerPinHandleOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(mfrac!(0)), MarkerTime::new(mfrac!(0)).unwrap())));
        let right = MarkerPinHandleOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(mfrac!(1)), MarkerTime::new(mfrac!(1)).unwrap())));
        let image_required_params = ImageRequiredParams::new_default(StaticPointerOwned::reference(&left), StaticPointerOwned::reference(&right));
        let variable_params = [(
            "VariableParam1",
            VariableParameterValue {
                params: ParameterNullableValue::String(time_split_value![
                    StaticPointerOwned::reference(&left).clone(),
                    Some(EasingValue::new(DynEditableEasingValue::new(DynEditableSelfValue("String Value".to_owned())), Arc::new(LinearEasing))),
                    StaticPointerOwned::reference(&right).clone()
                ]),
                components: vec![],
                priority: VariableParameterPriority::PrioritizeManually,
            },
        )];
        let mut builder = MarkerPinTimeMap::builder(&owner);
        builder.insert_by_image_required_params(&image_required_params);
        let pin_time_map = builder.build();
        let image_required_params = ImageRequiredParamsForEdit::from_image_required_params(image_required_params, &pin_time_map);
        let variable_params = variable_params.into_iter().map(|(s, v)| WithName::new(s.to_owned(), NullableValueForEdit::from_variable_parameter_value(v, &pin_time_map))).collect::<Box<[_]>>();
        let mut window = PropertyWindow::new(Arc::new(VM {
            params: Mutex::new(Some(ParametersEditSet {
                image_required_params: Some(image_required_params),
                fixed_params: Box::new([WithName::new("FixedParam1".to_owned(), ParameterValueFixed::String(DynEditableSingleValue::new(DynEditableSelfValue("String Value".to_owned()))))]),
                variable_params,
                pin_times: pin_time_map.times.into_boxed_slice(),
            })),
        }));
        let mut output = Cursor::new(Vec::new());
        egui_image_renderer::render_into_file(
            |ctx| {
                ctx.set_visuals(Visuals::light());
                egui::CentralPanel::default().show(ctx, |ui| window.ui(ui));
            },
            512,
            768,
            FileFormat::PNG,
            &mut output,
        )
        .await
        .unwrap();
        tokio::fs::write(test_output_dir.join("view_property_window_light.png"), output.into_inner()).await.unwrap();
        let mut output = Cursor::new(Vec::new());
        egui_image_renderer::render_into_file(
            |ctx| {
                ctx.set_visuals(Visuals::dark());
                egui::CentralPanel::default().show(ctx, |ui| window.ui(ui));
            },
            512,
            768,
            FileFormat::PNG,
            &mut output,
        )
        .await
        .unwrap();
        tokio::fs::write(test_output_dir.join("view_property_window_dark.png"), output.into_inner()).await.unwrap();
    }
}
