use crate::timeline::view::range_max::RangeMax;
use crate::timeline::view::widgets::component_instance_block::{ComponentInstanceBlock, ComponentInstanceEditEvent};
use crate::timeline::viewmodel::{ComponentClassData, ComponentClassDataList, ComponentInstanceDataList, ComponentLinkDataList, TimelineViewModel};
use egui::style::ScrollStyle;
use egui::{PointerButton, ScrollArea, Sense, Stroke, Ui, Vec2};
use mpdelta_core::component::parameter::ParameterValueType;
use ordered_float::OrderedFloat;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

mod range_max;
mod widgets;

pub struct Timeline<K, T, VM> {
    view_model: Arc<VM>,
    size: Vec2,
    _phantom: PhantomData<(K, T)>,
}

impl<K: 'static, T: ParameterValueType, VM: TimelineViewModel<K, T>> Timeline<K, T, VM> {
    pub fn new(view_model: Arc<VM>) -> Timeline<K, T, VM> {
        Timeline { view_model, size: Vec2::ZERO, _phantom: PhantomData }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        ui.style_mut().spacing.scroll = ScrollStyle::solid();
        let output = ScrollArea::both().id_source("Timeline").show_viewport(ui, |ui, rect| {
            let available_size = ui.available_size();
            let response = ui.allocate_response(Vec2::new(self.size.x.max(available_size.x), available_size.y), Sense::click_and_drag());
            let time_to_point = |time: f64| time as f32 * 100. - rect.left();
            let point_to_time = |point: f32| (point + rect.left()) as f64 / 100.;
            if response.clicked_by(PointerButton::Primary) || response.dragged_by(PointerButton::Primary) {
                let time = point_to_time(response.interact_pointer_pos().unwrap().x);
                let frame = ((time * 60.).round() as usize).clamp(0, 599);
                self.view_model.set_seek(frame);
            }
            let top = response.rect.top();
            self.view_model.component_instances(|ComponentInstanceDataList { list: component_instances }| {
                let pin_position_map = component_instances
                    .iter()
                    .flat_map(|instance| [&instance.left_pin, &instance.right_pin].into_iter().chain(&instance.pins))
                    .map(|pin| (&pin.handle, pin.render_location.get()))
                    .collect::<HashMap<_, _>>();
                self.view_model.component_links(|ComponentLinkDataList { list }| {
                    list.iter().for_each(|link| {
                        let from = pin_position_map.get(&link.from_pin);
                        let to = pin_position_map.get(&link.to_pin);
                        if let (Some(from), Some(to)) = (from, to) {
                            ui.painter().line_segment([*from, *to], egui::Stroke::new(1., ui.visuals().text_color()));
                        }
                    });
                });
                let mut range_max = RangeMax::new();
                for instance_data in component_instances.iter() {
                    let top = range_max.get(&OrderedFloat(instance_data.start_time)..&OrderedFloat(instance_data.end_time)).copied().unwrap_or(top);
                    let block_bottom = ComponentInstanceBlock::new(instance_data, top, time_to_point, point_to_time, |event| match event {
                        ComponentInstanceEditEvent::Click => self.view_model.click_component_instance(&instance_data.handle),
                        ComponentInstanceEditEvent::Delete => self.view_model.delete_component_instance(&instance_data.handle),
                        ComponentInstanceEditEvent::MoveWholeBlockTemporary(to) | ComponentInstanceEditEvent::MoveWholeBlock(to) => self.view_model.move_component_instance(&instance_data.handle, to),
                        ComponentInstanceEditEvent::MovePinTemporary(pin, to) | ComponentInstanceEditEvent::MovePin(pin, to) => self.view_model.move_marker_pin(&instance_data.handle, pin, to),
                    })
                    .show(ui);
                    range_max.insert(OrderedFloat(instance_data.start_time)..OrderedFloat(instance_data.end_time), block_bottom);
                }
            });
            let seek = self.view_model.seek();
            let seek_line_position = time_to_point(seek as f64 / 60.);
            ui.painter().vline(seek_line_position, response.rect.y_range(), Stroke::new(1., egui::Color32::RED));
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
        });
        self.size = output.content_size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_window::viewmodel::ImageRequiredParamsForEdit;
    use crate::timeline::viewmodel::{ComponentInstanceData, ComponentLinkDataList, MarkerPinData};
    use egui::{Pos2, Visuals};
    use egui_image_renderer::FileFormat;
    use mpdelta_core::time::TimelineTime;
    use std::cell::Cell;
    use std::io::Cursor;
    use std::sync::Mutex;

    #[tokio::test]
    async fn view_timeline() {
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
        impl TimelineViewModel<K, T> for VM {
            fn seek(&self) -> usize {
                0
            }

            fn set_seek(&self, _seek: usize) {}

            type ComponentInstanceHandle = &'static str;

            type MarkerPinHandle = &'static str;

            fn component_instances<R>(&self, f: impl FnOnce(&ComponentInstanceDataList<Self::ComponentInstanceHandle, Self::MarkerPinHandle>) -> R) -> R {
                let list = ComponentInstanceDataList {
                    list: vec![
                        ComponentInstanceData {
                            handle: "ComponentInstance0",
                            name: "Component Instance 0".to_string(),
                            selected: false,
                            start_time: 0.0,
                            end_time: 1.0,
                            layer: 0.0,
                            left_pin: MarkerPinData {
                                handle: "0 - left",
                                at: 0.0,
                                render_location: Cell::new(Pos2::default()),
                            },
                            right_pin: MarkerPinData {
                                handle: "0 - right",
                                at: 1.0,
                                render_location: Cell::new(Pos2::default()),
                            },
                            pins: vec![MarkerPinData {
                                handle: "0 - pin 0",
                                at: 0.5,
                                render_location: Cell::new(Pos2::default()),
                            }],
                        },
                        ComponentInstanceData {
                            handle: "ComponentInstance1",
                            name: "Component Instance 1".to_string(),
                            selected: false,
                            start_time: 2.0,
                            end_time: 4.0,
                            layer: 0.0,
                            left_pin: MarkerPinData {
                                handle: "1 - left",
                                at: 2.0,
                                render_location: Cell::new(Pos2::default()),
                            },
                            right_pin: MarkerPinData {
                                handle: "1 - right",
                                at: 4.0,
                                render_location: Cell::new(Pos2::default()),
                            },
                            pins: vec![],
                        },
                        ComponentInstanceData {
                            handle: "ComponentInstance2",
                            name: "Component Instance 2".to_string(),
                            selected: true,
                            start_time: 0.5,
                            end_time: 1.8,
                            layer: 0.0,
                            left_pin: MarkerPinData {
                                handle: "2 - left",
                                at: 0.5,
                                render_location: Cell::new(Pos2::default()),
                            },
                            right_pin: MarkerPinData {
                                handle: "2 - right",
                                at: 1.8,
                                render_location: Cell::new(Pos2::default()),
                            },
                            pins: vec![
                                MarkerPinData {
                                    handle: "2 - pin 0",
                                    at: 0.6,
                                    render_location: Cell::new(Pos2::default()),
                                },
                                MarkerPinData {
                                    handle: "2 - pin 1",
                                    at: 1.5,
                                    render_location: Cell::new(Pos2::default()),
                                },
                            ],
                        },
                    ],
                };
                f(&list)
            }

            fn click_component_instance(&self, _handle: &Self::ComponentInstanceHandle) {}

            fn delete_component_instance(&self, _handle: &Self::ComponentInstanceHandle) {}

            fn move_component_instance(&self, _handle: &Self::ComponentInstanceHandle, _to: f64) {}

            fn move_marker_pin(&self, _instance_handle: &Self::ComponentInstanceHandle, _pin_handle: &Self::MarkerPinHandle, _to: f64) {}

            type ComponentLinkHandle = &'static str;

            fn component_links<R>(&self, f: impl FnOnce(&ComponentLinkDataList<Self::ComponentLinkHandle, Self::MarkerPinHandle, Self::ComponentInstanceHandle>) -> R) -> R {
                let list = ComponentLinkDataList { list: vec![] };
                f(&list)
            }

            fn edit_marker_link_length(&self, _link: &Self::ComponentLinkHandle, _value: f64) {}

            type ComponentClassHandle = &'static str;

            fn component_classes<R>(&self, f: impl FnOnce(&ComponentClassDataList<Self::ComponentClassHandle>) -> R) -> R {
                let list = ComponentClassDataList { list: vec![] };
                f(&list)
            }

            fn add_component_instance(&self, _class: Self::ComponentClassHandle) {}
        }
        let mut timeline = Timeline::new(Arc::new(VM { params: Mutex::new(None) }));
        let mut output = Cursor::new(Vec::new());
        egui_image_renderer::render_into_file(
            |ctx| {
                ctx.set_visuals(Visuals::light());
                egui::CentralPanel::default().show(ctx, |ui| timeline.ui(ui));
            },
            512,
            512,
            FileFormat::PNG,
            &mut output,
        )
        .await
        .unwrap();
        tokio::fs::write("timeline_light.png", output.into_inner()).await.unwrap();
        let mut output = Cursor::new(Vec::new());
        egui_image_renderer::render_into_file(
            |ctx| {
                ctx.set_visuals(Visuals::dark());
                egui::CentralPanel::default().show(ctx, |ui| timeline.ui(ui));
            },
            512,
            512,
            FileFormat::PNG,
            &mut output,
        )
        .await
        .unwrap();
        tokio::fs::write("timeline_dark.png", output.into_inner()).await.unwrap();
    }
}
