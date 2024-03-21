use crate::property_window::view::widgets::editable_easing_value::{EasingValueEditEvent, EasingValueEditor, Side};
use crate::property_window::viewmodel::{ImageRequiredParamsEditSet, ImageRequiredParamsTransformForEdit, PropertyWindowViewModel, ValueWithEditCopy};
use cgmath::Vector3;
use egui::scroll_area::ScrollBarVisibility;
use egui::style::ScrollStyle;
use egui::{ScrollArea, Sense, Ui, Vec2};
use mpdelta_core::common::time_split_value::TimeSplitValue;
use mpdelta_core::component::parameter::value::{EasingValue, EasingValueEdit, SingleValueEdit};
use mpdelta_core::component::parameter::{ParameterValueFixed, ParameterValueType, PinSplitValue, VariableParameterValue};
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

fn edit_value_by_event<K>(translate_x: &mut PinSplitValue<K, Option<EasingValue<f64>>>, tx: &mut TimeSplitValue<usize, Option<EasingValue<f64>>>, edit_events: SmallVec<[EasingValueEditEvent; 1]>) -> bool {
    let mut edited = false;
    for edit in edit_events {
        let (slot, side, value) = match edit {
            EasingValueEditEvent::FlipPin(_) => {
                eprintln!("not supported");
                continue;
            }
            EasingValueEditEvent::MoveValueTemporary { value_index, side, value } => {
                let Some((_, Some(slot), _)) = tx.get_value_mut(value_index) else { unreachable!() };
                (slot, side, value)
            }
            EasingValueEditEvent::MoveValue { value_index, side, value } => {
                let Some((_, Some(slot), _)) = translate_x.get_value_mut(value_index) else { unreachable!() };
                edited = true;
                (slot, side, value)
            }
        };
        slot.value
            .edit_value::<f64, _>(|left, right| {
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
            ui.label("Component Properties");
            let mut fixed_params = self.view_model.fixed_params();
            if let Some(fixed_params) = &mut *fixed_params {
                let mut edited = false;
                for (name, value) in fixed_params.as_mut().iter_mut() {
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
            }
            drop(fixed_params);
            let image_required_params = &mut *self.view_model.image_required_params();
            let mut edited = false;
            if let Some(ImageRequiredParamsEditSet { params: image_required_params, pin_times }) = image_required_params {
                if let ImageRequiredParamsTransformForEdit::Params {
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
                    ScrollArea::vertical()
                        .max_height(ui.available_height() - (ScrollStyle::solid().bar_width + ScrollStyle::solid().bar_inner_margin * 2. + ScrollStyle::solid().bar_outer_margin * 2.))
                        .show(ui, |ui| {
                            ui.label("position - X");
                            let mut edit_events = SmallVec::<[_; 1]>::new();
                            EasingValueEditor {
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
                            edited |= edit_value_by_event(translate_x, tx, edit_events);
                            let mut edit_events = SmallVec::<[_; 1]>::new();
                            ui.label("position - Y");
                            EasingValueEditor {
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
                            edited |= edit_value_by_event(translate_y, ty, edit_events);
                            let mut edit_events = SmallVec::<[_; 1]>::new();
                            ui.label("scale - X");
                            EasingValueEditor {
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
                            edited |= edit_value_by_event(scale_x, sx, edit_events);
                            let mut edit_events = SmallVec::<[_; 1]>::new();
                            ui.label("scale - Y");
                            EasingValueEditor {
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
                            edited |= edit_value_by_event(scale_y, sy, edit_events);
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
            }
            if edited {
                self.view_model.updated_image_required_params(&image_required_params.as_ref().unwrap().params);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::property_window::view::PropertyWindow;
    use crate::property_window::viewmodel::{ImageRequiredParamsEditSet, ImageRequiredParamsForEdit, PropertyWindowViewModel};
    use egui::Visuals;
    use egui_image_renderer::FileFormat;
    use mpdelta_core::component::marker_pin::{MarkerPin, MarkerPinHandleOwned, MarkerTime};
    use mpdelta_core::component::parameter::value::{DynEditableSelfValue, DynEditableSingleValue};
    use mpdelta_core::component::parameter::{ImageRequiredParams, ParameterValueFixed, ParameterValueType};
    use mpdelta_core::mfrac;
    use mpdelta_core::ptr::StaticPointerOwned;
    use mpdelta_core::time::TimelineTime;
    use qcell::{TCell, TCellOwner};
    use std::io::Cursor;
    use std::iter;
    use std::ops::Range;
    use std::sync::{Arc, Mutex, MutexGuard};

    #[tokio::test]
    async fn view_property_window() {
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
            #[allow(clippy::type_complexity)]
            fixed_params: Mutex<Option<Vec<(String, ParameterValueFixed<(), ()>)>>>,
            params: Mutex<Option<ImageRequiredParamsEditSet<K, T, Vec<f64>>>>,
        }
        impl PropertyWindowViewModel<K, T> for VM {
            type Times = Vec<f64>;
            type FixedParams = Vec<(String, ParameterValueFixed<(), ()>)>;
            type FixedParamsLock<'a> = MutexGuard<'a, Option<Self::FixedParams>> where Self: 'a;

            fn fixed_params(&self) -> Self::FixedParamsLock<'_> {
                self.fixed_params.lock().unwrap()
            }

            fn updated_fixed_params(&self, _fixed_params: &Self::FixedParams) {}

            type ImageRequiredParams<'a> = MutexGuard<'a, Option<ImageRequiredParamsEditSet<K, T, Self::Times>>>;

            fn selected_instance_at(&self) -> Range<f64> {
                0.0..1.0
            }

            fn image_required_params(&self) -> Self::ImageRequiredParams<'_> {
                self.params.lock().unwrap()
            }

            fn updated_image_required_params(&self, _image_required_params: &ImageRequiredParamsForEdit<K, T>) {}
        }
        let owner = TCellOwner::new();
        let left = MarkerPinHandleOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(mfrac!(0)), MarkerTime::new(mfrac!(0)).unwrap())));
        let right = MarkerPinHandleOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(mfrac!(1)), MarkerTime::new(mfrac!(1)).unwrap())));
        let (params, pin_times) = ImageRequiredParamsForEdit::from_image_required_params(ImageRequiredParams::new_default(StaticPointerOwned::reference(&left), StaticPointerOwned::reference(&right)), iter::empty(), &owner);
        let mut window = PropertyWindow::new(Arc::new(VM {
            fixed_params: Mutex::new(Some(vec![("Param1".to_owned(), ParameterValueFixed::String(DynEditableSingleValue::new(DynEditableSelfValue("String Value".to_owned()))))])),
            params: Mutex::new(Some(ImageRequiredParamsEditSet { params, pin_times })),
        }));
        let mut output = Cursor::new(Vec::new());
        egui_image_renderer::render_into_file(
            |ctx| {
                ctx.set_visuals(Visuals::light());
                egui::CentralPanel::default().show(ctx, |ui| window.ui(ui));
            },
            512,
            512,
            FileFormat::PNG,
            &mut output,
        )
        .await
        .unwrap();
        tokio::fs::write("view_property_window_light.png", output.into_inner()).await.unwrap();
        let mut output = Cursor::new(Vec::new());
        egui_image_renderer::render_into_file(
            |ctx| {
                ctx.set_visuals(Visuals::dark());
                egui::CentralPanel::default().show(ctx, |ui| window.ui(ui));
            },
            512,
            512,
            FileFormat::PNG,
            &mut output,
        )
        .await
        .unwrap();
        tokio::fs::write("view_property_window_dark.png", output.into_inner()).await.unwrap();
    }
}
