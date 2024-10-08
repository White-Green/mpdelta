use crate::property_window::view::widgets::editable_easing_value_f64::EasingValueEditorF64;
use crate::property_window::view::widgets::editable_easing_value_string::EasingValueEditorString;
use crate::property_window::viewmodel::{ParametersEditSet, PropertyWindowViewModel, WithName};
use cgmath::Vector3;
use egui::scroll_area::ScrollBarVisibility;
use egui::style::ScrollStyle;
use egui::{ScrollArea, Sense, Ui, Vec2};
use mpdelta_core::component::parameter::value::SingleValueEdit;
use mpdelta_core::component::parameter::{ImageRequiredParamsTransform, Parameter, ParameterValueFixed, ParameterValueType, VariableParameterValue};
use std::marker::PhantomData;
use std::mem;
use std::path::PathBuf;
use std::sync::Arc;

mod widgets;

pub struct PropertyWindow<T, VM> {
    view_model: Arc<VM>,
    scroll_offset: f32,
    _phantom: PhantomData<T>,
}

impl<T: ParameterValueType, VM: PropertyWindowViewModel<T>> PropertyWindow<T, VM> {
    pub fn new(view_model: Arc<VM>) -> PropertyWindow<T, VM> {
        PropertyWindow { view_model, scroll_offset: 0., _phantom: PhantomData }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let id = ui.make_persistent_id("PropertyWindow");
        let updated_now = self.view_model.is_updated_now();
        let instance_range = self.view_model.selected_instance_at();
        let instance_length = (instance_range.end - instance_range.start) as f32;
        let point_per_second = 320.;
        let (rect, _) = ui.allocate_at_least(ui.available_size(), Sense::click());
        ui.allocate_ui_at_rect(rect, |ui| {
            self.view_model.parameters(|parameters| {
                if let Some(ParametersEditSet {
                    all_pins,
                    image_required_params,
                    fixed_params,
                    variable_params,
                    pin_times,
                }) = parameters
                {
                    ui.label("Component Properties");
                    ScrollArea::vertical()
                        .max_height(ui.available_height() - (ScrollStyle::solid().bar_width + ScrollStyle::solid().bar_inner_margin * 2. + ScrollStyle::solid().bar_outer_margin * 2.))
                        .show(ui, |ui| {
                            let mut edited = false;
                            if let Some(image_required_params) = image_required_params {
                                if let ImageRequiredParamsTransform::Params { size, scale, translate, .. } = Arc::make_mut(&mut image_required_params.transform) {
                                    let Vector3 {
                                        x: VariableParameterValue { params: size_x, .. },
                                        y: VariableParameterValue { params: size_y, .. },
                                        ..
                                    } = Arc::make_mut(size);
                                    let Vector3 {
                                        x: VariableParameterValue { params: scale_x, .. },
                                        y: VariableParameterValue { params: scale_y, .. },
                                        ..
                                    } = Arc::make_mut(scale);
                                    let Vector3 {
                                        x: VariableParameterValue { params: translate_x, .. },
                                        y: VariableParameterValue { params: translate_y, .. },
                                        ..
                                    } = Arc::make_mut(translate);
                                    ui.label("position - X");
                                    edited |= EasingValueEditorF64 {
                                        id: "position - X",
                                        reset: updated_now,
                                        time_range: instance_range.clone(),
                                        all_pins,
                                        times: pin_times.as_ref(),
                                        value: translate_x,
                                        value_range: -3.0..3.0,
                                        point_per_second,
                                        scroll_offset: &mut self.scroll_offset,
                                    }
                                    .show(ui)
                                    .is_updated();
                                    ui.label("position - Y");
                                    edited |= EasingValueEditorF64 {
                                        id: "position - Y",
                                        reset: updated_now,
                                        time_range: instance_range.clone(),
                                        all_pins,
                                        times: pin_times.as_ref(),
                                        value: translate_y,
                                        value_range: -3.0..3.0,
                                        point_per_second,
                                        scroll_offset: &mut self.scroll_offset,
                                    }
                                    .show(ui)
                                    .is_updated();
                                    ui.label("size - X");
                                    edited |= EasingValueEditorF64 {
                                        id: "size - X",
                                        reset: updated_now,
                                        time_range: instance_range.clone(),
                                        all_pins,
                                        times: pin_times.as_ref(),
                                        value: size_x,
                                        value_range: 0.0..2.0,
                                        point_per_second,
                                        scroll_offset: &mut self.scroll_offset,
                                    }
                                    .show(ui)
                                    .is_updated();
                                    ui.label("size - Y");
                                    edited |= EasingValueEditorF64 {
                                        id: "size - Y",
                                        reset: updated_now,
                                        time_range: instance_range.clone(),
                                        all_pins,
                                        times: pin_times.as_ref(),
                                        value: size_y,
                                        value_range: 0.0..2.0,
                                        point_per_second,
                                        scroll_offset: &mut self.scroll_offset,
                                    }
                                    .show(ui)
                                    .is_updated();
                                    ui.label("scale - X");
                                    edited |= EasingValueEditorF64 {
                                        id: "scale - X",
                                        reset: updated_now,
                                        time_range: instance_range.clone(),
                                        all_pins,
                                        times: pin_times.as_ref(),
                                        value: scale_x,
                                        value_range: 0.0..2.0,
                                        point_per_second,
                                        scroll_offset: &mut self.scroll_offset,
                                    }
                                    .show(ui)
                                    .is_updated();
                                    ui.label("scale - Y");
                                    edited |= EasingValueEditorF64 {
                                        id: "scale - Y",
                                        reset: updated_now,
                                        time_range: instance_range.clone(),
                                        all_pins,
                                        times: pin_times.as_ref(),
                                        value: scale_y,
                                        value_range: 0.0..2.0,
                                        point_per_second,
                                        scroll_offset: &mut self.scroll_offset,
                                    }
                                    .show(ui)
                                    .is_updated();
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
                                match &mut value.params {
                                    Parameter::None => {}
                                    Parameter::Image(_value) => {}
                                    Parameter::Audio(_value) => {}
                                    Parameter::Binary(_value) => {}
                                    Parameter::String(value) => {
                                        edited |= EasingValueEditorString {
                                            id: name,
                                            time_range: instance_range.clone(),
                                            all_pins,
                                            times: pin_times.as_ref(),
                                            value,
                                            point_per_second,
                                            scroll_offset: &mut self.scroll_offset,
                                        }
                                        .show(ui)
                                        .is_updated();
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
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::property_window::view::PropertyWindow;
    use crate::property_window::viewmodel::{ParametersEditSet, PropertyWindowViewModel, WithName};
    use egui::Visuals;
    use egui_image_renderer::FileFormat;
    use mpdelta_core::component::marker_pin::{MarkerPin, MarkerPinId, MarkerTime};
    use mpdelta_core::component::parameter::value::{DynEditableEasingValue, DynEditableSelfValue, DynEditableSingleValue, EasingValue, LinearEasing};
    use mpdelta_core::component::parameter::{ImageRequiredParams, ParameterNullableValue, ParameterValueFixed, ParameterValueType, VariableParameterPriority, VariableParameterValue};
    use mpdelta_core::core::IdGenerator;
    use mpdelta_core::time::TimelineTime;
    use mpdelta_core::{mfrac, time_split_value_persistent};
    use mpdelta_core_test_util::TestIdGenerator;
    use rpds::Vector;
    use std::collections::HashMap;
    use std::io::Cursor;
    use std::ops::Range;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn view_property_window() {
        const TEST_OUTPUT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../test_output/", env!("CARGO_PKG_NAME"));
        let test_output_dir = Path::new(TEST_OUTPUT_DIR);
        tokio::fs::create_dir_all(test_output_dir).await.unwrap();
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
            params: Mutex<Option<ParametersEditSet<T, HashMap<MarkerPinId, TimelineTime>>>>,
        }
        impl PropertyWindowViewModel<T> for VM {
            fn is_updated_now(&self) -> bool {
                false
            }

            fn selected_instance_at(&self) -> Range<f64> {
                0.0..1.0
            }

            type TimeMap = HashMap<MarkerPinId, TimelineTime>;

            fn parameters<R>(&self, f: impl FnOnce(Option<&mut ParametersEditSet<T, Self::TimeMap>>) -> R) -> R {
                f(self.params.lock().unwrap().as_mut())
            }

            fn updated_image_required_params(&self, _image_required_params: &ImageRequiredParams) {}
            fn updated_fixed_params(&self, _fixed_params: &[WithName<ParameterValueFixed<(), ()>>]) {}
            fn updated_variable_params(&self, _variable_params: &[WithName<VariableParameterValue<ParameterNullableValue<T>>>]) {}
        }
        let id = TestIdGenerator::new();
        let left = MarkerPin::new(id.generate_new(), MarkerTime::new(mfrac!(0)).unwrap());
        let right = MarkerPin::new(id.generate_new(), MarkerTime::new(mfrac!(1)).unwrap());
        let image_required_params = ImageRequiredParams::new_default(left.id(), right.id());
        let variable_params = [(
            "VariableParam1",
            VariableParameterValue {
                params: ParameterNullableValue::String(time_split_value_persistent![*left.id(), Some(EasingValue::new(DynEditableEasingValue::new(DynEditableSelfValue("String Value".to_owned())), Arc::new(LinearEasing))), *right.id()]),
                components: Vector::new_sync(),
                priority: VariableParameterPriority::PrioritizeManually,
            },
        )];
        let variable_params = variable_params.into_iter().map(|(s, v)| WithName::new(s.to_owned(), v)).collect::<Box<[_]>>();
        let mut window = PropertyWindow::new(Arc::new(VM {
            params: Mutex::new(Some(ParametersEditSet {
                all_pins: Box::new([left.clone(), right.clone()]),
                image_required_params: Some(image_required_params),
                fixed_params: Box::new([WithName::new("FixedParam1".to_owned(), ParameterValueFixed::String(DynEditableSingleValue::new(DynEditableSelfValue("String Value".to_owned()))))]),
                variable_params,
                pin_times: Arc::new(HashMap::from([(*left.id(), TimelineTime::new(mfrac!(0))), (*right.id(), TimelineTime::new(mfrac!(1)))])),
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
