use crate::timeline::viewmodel::{ComponentClassData, ComponentClassDataList, ComponentInstanceData, ComponentInstanceDataList, ComponentLinkData, ComponentLinkDataList, TimelineViewModel};
use egui::epaint::Shadow;
use egui::{Color32, Frame, Margin, Pos2, Rect, Sense, Stroke, Style, TextEdit, Ui, Vec2};
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::time::TimelineTime;
use once_cell::sync::Lazy;
use regex::Regex;
use std::marker::PhantomData;
use std::str::FromStr;
use std::sync::Arc;

pub struct Timeline<K, T, VM> {
    view_model: Arc<VM>,
    _phantom: PhantomData<(K, T)>,
}

impl<K: 'static, T: ParameterValueType, VM: TimelineViewModel<K, T>> Timeline<K, T, VM> {
    pub fn new(view_model: Arc<VM>) -> Timeline<K, T, VM> {
        Timeline { view_model, _phantom: PhantomData }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let (rect, response) = ui.allocate_at_least(ui.available_size(), Sense::click());
        ui.allocate_ui_at_rect(rect, |ui| {
            let base_point = ui.cursor().min;
            let frame = self.view_model.seek();
            ui.painter().vline(base_point.x + frame as f32 / 60. * 100., ui.cursor().min.y..=ui.cursor().min.y + ui.ctx().available_rect().height(), Stroke::new(1., Color32::RED));
            self.view_model.component_instances(|ComponentInstanceDataList { list: component_instances }| {
                for ComponentInstanceData { handle, selected, start_time, end_time, layer } in component_instances {
                    let rectangle = Rect::from_min_size(Pos2::new(start_time.value() as f32 * 100., layer * 60.), Vec2::new((end_time.value() - start_time.value()) as f32 * 100., 50.));
                    ui.allocate_ui_at_rect(Rect::from_min_size(base_point + rectangle.min.to_vec2(), rectangle.size()), |ui| {
                        let frame = Frame::group(&Style::default()).inner_margin(Margin::default());
                        let frame = if *selected { frame.shadow(Shadow::big_light()) } else { frame };
                        frame.show(ui, |ui| {
                            let (rect, response) = ui.allocate_exact_size(rectangle.size(), Sense::drag());
                            ui.allocate_ui_at_rect(rect, |ui| {
                                ui.label("Rectangle");
                            });
                            if response.clicked() {
                                self.view_model.click_component_instance(handle);
                            }
                            let delta = response.drag_delta();
                            if delta != Vec2::default() {
                                self.view_model.drag_component_instance(handle, delta.x / 100., delta.y / 60.);
                            }
                        });
                    });
                }
            });
            self.view_model.component_links(|ComponentLinkDataList { list: component_links }| {
                for ComponentLinkData {
                    handle,
                    len: _,
                    len_str,
                    from_component,
                    to_component,
                    from_layer,
                    to_layer,
                    from_time,
                    to_time,
                } in component_links
                {
                    if !from_component.as_ref().is_some_and(|from_component| self.view_model.is_component_instance_selected(from_component)) && !to_component.as_ref().is_some_and(|to_component| self.view_model.is_component_instance_selected(to_component)) {
                        continue;
                    }
                    ui.painter().hline(
                        base_point.x + (from_time.value() * 100.) as f32..=base_point.x + (to_time.value() * 100.) as f32,
                        base_point.y + from_layer.max(*to_layer) * 60. + 55.,
                        Stroke::new(1., ui.visuals().text_color()),
                    );
                    ui.allocate_ui_at_rect(Rect::from_min_size(base_point + Vec2::new((from_time.value() * 100.) as f32, from_layer.max(*to_layer) * 60. + 57.), Vec2::new(20., 100.)), |ui| {
                        let mut len = len_str.lock().unwrap();
                        let mut s = String::clone(&len);
                        ui.add(TextEdit::singleline(&mut s));
                        s.retain(|c| c.is_ascii_digit() || c == '.');
                        static REGEX: Lazy<Regex> = Lazy::new(|| Regex::new("^\\d+(?:\\.\\d+)?$").unwrap());
                        if s != *len {
                            *len = s;
                            if REGEX.is_match(&len) {
                                if let Ok(new_value) = f64::from_str(&len) {
                                    if let Some(new_value) = TimelineTime::new(new_value) {
                                        self.view_model.edit_marker_link_length(handle, new_value);
                                    }
                                }
                            }
                        }
                    });
                }
            });
        });
        response.context_menu(|ui| {
            self.view_model.component_classes(|ComponentClassDataList { list }| {
                for ComponentClassData { handle } in list {
                    if ui.button("add").clicked() {
                        self.view_model.add_component_instance(handle.clone());
                        ui.close_menu();
                    }
                }
            });
        });
    }
}
