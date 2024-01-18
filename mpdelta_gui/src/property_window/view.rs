use crate::property_window::view::widgets::editable_easing_value::{EasingValueEditEvent, EasingValueEditor, Side};
use crate::property_window::viewmodel::{ImageRequiredParamsTransformForEdit, PropertyWindowViewModel};
use cgmath::Vector3;
use egui::scroll_area::ScrollBarVisibility;
use egui::style::ScrollStyle;
use egui::{ScrollArea, Sense, Ui, Vec2};
use mpdelta_core::common::time_split_value::TimeSplitValue;
use mpdelta_core::component::parameter::value::{EasingValue, EasingValueEdit};
use mpdelta_core::component::parameter::{ParameterValueType, PinSplitValue, VariableParameterValue};
use smallvec::SmallVec;
use std::marker::PhantomData;
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
        let instance_length = (instance_range.end.value() - instance_range.start.value()) as f32;
        let point_per_second = 320.;
        let (rect, _) = ui.allocate_at_least(ui.available_size(), Sense::click());
        ui.allocate_ui_at_rect(rect, |ui| {
            ui.label("Component Properties");
            let image_required_params = &mut *self.view_model.image_required_params();
            let mut edited = false;
            if let Some((image_required_params, times)) = image_required_params {
                if let ImageRequiredParamsTransformForEdit::Params {
                    scale: Vector3 {
                        x: (VariableParameterValue { params: scale_x, .. }, sx),
                        y: (VariableParameterValue { params: scale_y, .. }, sy),
                        ..
                    },
                    translate: Vector3 {
                        x: (VariableParameterValue { params: translate_x, .. }, tx),
                        y: (VariableParameterValue { params: translate_y, .. }, ty),
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
                            EasingValueEditor::new("position - X", times.as_ref(), instance_range.clone(), tx, -3.0..3.0, point_per_second, &mut self.scroll_offset, extend_fn(&mut edit_events)).show(ui);
                            edited |= edit_value_by_event(translate_x, tx, edit_events);
                            let mut edit_events = SmallVec::<[_; 1]>::new();
                            ui.label("position - Y");
                            EasingValueEditor::new("position - Y", times.as_ref(), instance_range.clone(), ty, -3.0..3.0, point_per_second, &mut self.scroll_offset, extend_fn(&mut edit_events)).show(ui);
                            edited |= edit_value_by_event(translate_y, ty, edit_events);
                            let mut edit_events = SmallVec::<[_; 1]>::new();
                            ui.label("scale - X");
                            EasingValueEditor::new("scale - X", times.as_ref(), instance_range.clone(), sx, 0.0..2.0, point_per_second, &mut self.scroll_offset, extend_fn(&mut edit_events)).show(ui);
                            edited |= edit_value_by_event(scale_x, sx, edit_events);
                            let mut edit_events = SmallVec::<[_; 1]>::new();
                            ui.label("scale - Y");
                            EasingValueEditor::new("scale - Y", times.as_ref(), instance_range.clone(), sy, 0.0..2.0, point_per_second, &mut self.scroll_offset, extend_fn(&mut edit_events)).show(ui);
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
                self.view_model.updated_image_required_params(&image_required_params.as_ref().unwrap().0);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::property_window::view::PropertyWindow;
    use crate::property_window::viewmodel::{ImageRequiredParamsForEdit, PropertyWindowViewModel};
    use egui::Visuals;
    use egui_image_renderer::FileFormat;
    use mpdelta_core::component::marker_pin::{MarkerPin, MarkerPinHandleOwned, MarkerTime};
    use mpdelta_core::component::parameter::{ImageRequiredParams, ParameterValueType};
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
            params: Mutex<Option<(ImageRequiredParamsForEdit<K, T>, Vec<TimelineTime>)>>,
        }
        impl PropertyWindowViewModel<K, T> for VM {
            type Times = Vec<TimelineTime>;
            type ImageRequiredParams<'a> = MutexGuard<'a, Option<(ImageRequiredParamsForEdit<K, T>, Self::Times)>>;

            fn selected_instance_at(&self) -> Range<TimelineTime> {
                TimelineTime::new(0.0).unwrap()..TimelineTime::new(1.0).unwrap()
            }

            fn image_required_params(&self) -> Self::ImageRequiredParams<'_> {
                self.params.lock().unwrap()
            }

            fn updated_image_required_params(&self, _image_required_params: &ImageRequiredParamsForEdit<K, T>) {}
        }
        let owner = TCellOwner::new();
        let left = MarkerPinHandleOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(0.0).unwrap(), MarkerTime::new(0.0).unwrap())));
        let right = MarkerPinHandleOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(1.0).unwrap(), MarkerTime::new(1.0).unwrap())));
        let (params, _, times) = ImageRequiredParamsForEdit::from_image_required_params(ImageRequiredParams::new_default(StaticPointerOwned::reference(&left), StaticPointerOwned::reference(&right)), iter::empty(), &owner);
        let mut window = PropertyWindow::new(Arc::new(VM { params: Mutex::new(Some((params, times))) }));
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
