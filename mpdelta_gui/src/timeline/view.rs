use crate::timeline::view::range_max::RangeMax;
use crate::timeline::view::widgets::component_instance_block::{ComponentInstanceBlock, ComponentInstanceEditEvent};
use crate::timeline::viewmodel::{ComponentClassData, ComponentClassDataList, ComponentInstanceDataList, MarkerLinkDataList, TimelineViewModel};
use egui::style::ScrollStyle;
use egui::{Color32, PointerButton, Pos2, Rect, ScrollArea, Sense, Stroke, Ui, Vec2};
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::marker_pin::MarkerTime;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::time::TimelineTime;
use ordered_float::OrderedFloat;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;
use std::{convert, mem};

mod range_max;
mod widgets;

pub struct Timeline<T, VM>
where
    T: ParameterValueType,
    VM: TimelineViewModel<T>,
{
    view_model: Arc<VM>,
    timeline_rect: Rect,
    scroll_offset: Vec2,
    component_top: Vec<RangeMax<OrderedFloat<f64>, f32>>,
    component_top_buf: Vec<RangeMax<OrderedFloat<f64>, f32>>,
    pulling_pin: Option<(VM::MarkerPinHandle, Pos2)>,
    context_menu_opened_pos: (f64, f32),
    _phantom: PhantomData<T>,
}

impl<T: ParameterValueType, VM: TimelineViewModel<T>> Timeline<T, VM> {
    pub fn new(view_model: Arc<VM>) -> Timeline<T, VM> {
        Timeline {
            view_model,
            timeline_rect: Rect::from_x_y_ranges(0.0..=0.0, 0.0..=30.0),
            scroll_offset: Vec2::ZERO,
            component_top: Vec::new(),
            component_top_buf: Vec::new(),
            pulling_pin: None,
            context_menu_opened_pos: (0., 0.),
            _phantom: PhantomData,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let mut next_timeline_rect = Rect::from_x_y_ranges(0.0..=0.0, 0.0..=30.0);
        let mut now_dragging = false;
        ui.style_mut().spacing.scroll = ScrollStyle::solid();
        let output = ScrollArea::horizontal().id_salt("Timeline").show(ui, |ui| {
            let time_to_point = |time: f64| time as f32 * 100. - self.scroll_offset.x - self.timeline_rect.left();
            let point_to_time = |point: f32| (point + self.scroll_offset.x + self.timeline_rect.left()) as f64 / 100.;
            let available_size = ui.available_size();
            let (response, painter) = ui.allocate_painter(Vec2::new(self.timeline_rect.width().max(available_size.x), 10.), Sense::click_and_drag());
            if response.clicked_by(PointerButton::Primary) || response.dragged_by(PointerButton::Primary) {
                now_dragging = true;
                let pointer_x = response.interact_pointer_pos().unwrap().x;
                let time = point_to_time(pointer_x);
                self.view_model.edit_component_length(MarkerTime::new(MixedFraction::from_f64(time.max(0.))).unwrap());
            }
            let length = self.view_model.component_length().map_or(10., |time| time.value().into_f64());
            painter.vline(time_to_point(length), response.rect.y_range(), Stroke::new(1., Color32::LIGHT_BLUE));
            next_timeline_rect.extend_with_x(time_to_point(length) + self.scroll_offset.x + self.timeline_rect.left());
            let output = ScrollArea::vertical().id_salt("Timeline-Vertical").show(ui, |ui| {
                let available_size = ui.available_size();
                let response = ui.allocate_response(Vec2::new(self.timeline_rect.width().max(available_size.x), self.timeline_rect.height().max(available_size.y)), Sense::click_and_drag());
                if response.clicked_by(PointerButton::Primary) || response.dragged_by(PointerButton::Primary) {
                    let pointer_x = response.interact_pointer_pos().unwrap().x;
                    now_dragging = true;
                    let time = point_to_time(pointer_x);
                    let limit = self.view_model.component_length().map_or(10., |time| time.value().into_f64());
                    let time = time.clamp(0., limit);
                    self.view_model.set_seek(MarkerTime::new(MixedFraction::from_f64(time)).unwrap());
                }
                let top = response.rect.top();
                self.view_model.component_instances(|ComponentInstanceDataList { list: component_instances }| {
                    let pin_position_map = component_instances
                        .iter()
                        .flat_map(|instance| [&instance.left_pin, &instance.right_pin].into_iter().chain(&instance.pins))
                        .map(|pin| (&pin.handle, pin.render_location.load()))
                        .collect::<HashMap<_, _>>();
                    let pull_link_pointer = self.pulling_pin.as_ref().map_or(Pos2::new(f32::INFINITY, f32::INFINITY), |(_, pos)| *pos);
                    let mut pull_link_target_pin = None;
                    self.view_model.marker_links(|MarkerLinkDataList { list }| {
                        list.iter().for_each(|link| {
                            let from = pin_position_map.get(&link.from_pin);
                            let to = pin_position_map.get(&link.to_pin);
                            if let (Some(from), Some(to)) = (from, to) {
                                const EPS_SQUARED: f32 = 25.;
                                if (*from - pull_link_pointer).length_sq() < EPS_SQUARED {
                                    pull_link_target_pin = Some(link.from_pin.clone());
                                }
                                if (*to - pull_link_pointer).length_sq() < EPS_SQUARED {
                                    pull_link_target_pin = Some(link.to_pin.clone());
                                }
                                ui.painter().line_segment([*from, *to], egui::Stroke::new(1., ui.visuals().text_color()));
                            }
                        });
                    });
                    if let Some((pin, pointer)) = &self.pulling_pin {
                        let from = pin_position_map.get(pin).unwrap();
                        ui.painter().line_segment([*from, *pointer], egui::Stroke::new(1., Color32::GREEN));
                    }

                    self.component_top_buf.clear();
                    let mut range_max = RangeMax::new();
                    for instance_data in component_instances.iter() {
                        self.component_top_buf.push(range_max.clone());
                        let range = &OrderedFloat(instance_data.start_time)..&OrderedFloat(instance_data.end_time);
                        let block_top = range_max.get(range.clone()).copied().unwrap_or(top);
                        let block = ComponentInstanceBlock::new(instance_data, block_top, time_to_point, point_to_time, |event| match event {
                            ComponentInstanceEditEvent::Click => self.view_model.click_component_instance(&instance_data.handle),
                            ComponentInstanceEditEvent::Delete => self.view_model.delete_component_instance(&instance_data.handle),
                            ComponentInstanceEditEvent::MoveWholeBlockTemporary { time, .. } => {
                                now_dragging = true;
                                self.view_model.move_component_instance(&instance_data.handle, time);
                            }
                            ComponentInstanceEditEvent::MoveWholeBlock { time, top: move_target_top } => {
                                now_dragging = true;
                                self.view_model.move_component_instance(&instance_data.handle, time);
                                let index = self
                                    .component_top_buf
                                    .binary_search_by_key(&OrderedFloat(move_target_top), |range_max| OrderedFloat(range_max.get(range.clone()).copied().unwrap_or(block_top)))
                                    .unwrap_or_else(convert::identity);
                                self.view_model.insert_component_instance_to(&instance_data.handle, index);
                            }
                            ComponentInstanceEditEvent::MovePinTemporary(pin, to) | ComponentInstanceEditEvent::MovePin(pin, to) => {
                                now_dragging = true;
                                self.view_model.move_marker_pin(&instance_data.handle, pin, to);
                            }
                            ComponentInstanceEditEvent::PullLinkReleased(handle, _pos) => {
                                now_dragging = true;
                                if let Some(target_pin) = &pull_link_target_pin {
                                    if handle != target_pin {
                                        self.view_model.connect_marker_pins(handle, target_pin);
                                    }
                                }
                                self.pulling_pin = None;
                            }
                            ComponentInstanceEditEvent::PullLink(handle, pos) => {
                                now_dragging = true;
                                self.pulling_pin = Some((handle.clone(), pos));
                            }
                            ComponentInstanceEditEvent::UpdateContextMenuOpenedPos(time, y) => {
                                now_dragging = true;
                                self.context_menu_opened_pos = (time, y);
                            }
                            ComponentInstanceEditEvent::AddMarkerPin => self.view_model.add_marker_pin(&instance_data.handle, TimelineTime::new(MixedFraction::from_f64(self.context_menu_opened_pos.0))),
                            ComponentInstanceEditEvent::DeletePin(handle) => self.view_model.delete_marker_pin(&instance_data.handle, handle),
                            ComponentInstanceEditEvent::UnlockPin(handle) => self.view_model.unlock_marker_pin(&instance_data.handle, handle),
                            ComponentInstanceEditEvent::LockPin(handle) => self.view_model.lock_marker_pin(&instance_data.handle, handle),
                            ComponentInstanceEditEvent::SplitComponentAtPin(handle) => self.view_model.split_component_at_pin(&instance_data.handle, handle),
                        })
                        .show(ui);
                        range_max = range_max.insert(OrderedFloat(instance_data.start_time)..OrderedFloat(instance_data.end_time), block.bottom());
                        next_timeline_rect.extend_with_x(block.left() + self.scroll_offset.x + self.timeline_rect.left());
                        next_timeline_rect.extend_with_x(block.right() + self.scroll_offset.x + self.timeline_rect.left());
                        next_timeline_rect.extend_with_y(block.bottom() - top);
                    }
                    mem::swap(&mut self.component_top, &mut self.component_top_buf);
                });
                let seek = self.view_model.seek();
                let seek_line_position = time_to_point(seek.value().into_f64());
                ui.painter().vline(seek_line_position, response.rect.y_range(), Stroke::new(1., egui::Color32::RED));
                next_timeline_rect.extend_with_x(seek_line_position + self.scroll_offset.x + self.timeline_rect.left());
                let pointer_pos = response.interact_pointer_pos();
                response.context_menu(|ui| {
                    if let Some(pointer_pos) = pointer_pos {
                        self.context_menu_opened_pos = (point_to_time(pointer_pos.x), pointer_pos.y);
                    }
                    self.view_model.component_classes(|ComponentClassDataList { list }| {
                        for ComponentClassData { name, handle } in list {
                            if ui.button(format!("add {name}")).clicked() {
                                self.view_model.add_component_instance(handle.clone());
                                ui.close_menu();
                            }
                        }
                    });
                });
            });
            (output.state.offset.y, output.content_size.y)
        });
        self.scroll_offset = Vec2::new(output.state.offset.x, output.inner.0);
        if now_dragging {
            next_timeline_rect.extend_with_x(self.timeline_rect.left());
            next_timeline_rect.extend_with_x(self.timeline_rect.right());
        }
        self.timeline_rect = next_timeline_rect;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::viewmodel::{ComponentInstanceData, MarkerPinData};
    use crossbeam_utils::atomic::AtomicCell;
    use egui::{Pos2, Visuals};
    use egui_image_renderer::FileFormat;
    use mpdelta_core::component::marker_pin::MarkerTime;
    use std::io::Cursor;
    use std::path::Path;

    #[tokio::test]
    async fn view_timeline() {
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
        struct VM;
        impl TimelineViewModel<T> for VM {
            fn component_length(&self) -> Option<MarkerTime> {
                None
            }

            fn seek(&self) -> MarkerTime {
                MarkerTime::ZERO
            }

            fn set_seek(&self, _seek: MarkerTime) {}

            fn edit_component_length(&self, _length: MarkerTime) {}

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
                                locked: true,
                                render_location: AtomicCell::new(Pos2::default()),
                            },
                            right_pin: MarkerPinData {
                                handle: "0 - right",
                                at: 1.0,
                                locked: true,
                                render_location: AtomicCell::new(Pos2::default()),
                            },
                            pins: vec![MarkerPinData {
                                handle: "0 - pin 0",
                                at: 0.5,
                                locked: true,
                                render_location: AtomicCell::new(Pos2::default()),
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
                                locked: true,
                                render_location: AtomicCell::new(Pos2::default()),
                            },
                            right_pin: MarkerPinData {
                                handle: "1 - right",
                                at: 4.0,
                                locked: true,
                                render_location: AtomicCell::new(Pos2::default()),
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
                                locked: true,
                                render_location: AtomicCell::new(Pos2::default()),
                            },
                            right_pin: MarkerPinData {
                                handle: "2 - right",
                                at: 1.8,
                                locked: true,
                                render_location: AtomicCell::new(Pos2::default()),
                            },
                            pins: vec![
                                MarkerPinData {
                                    handle: "2 - pin 0",
                                    at: 0.6,
                                    locked: true,
                                    render_location: AtomicCell::new(Pos2::default()),
                                },
                                MarkerPinData {
                                    handle: "2 - pin 1",
                                    at: 1.5,
                                    locked: true,
                                    render_location: AtomicCell::new(Pos2::default()),
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

            fn insert_component_instance_to(&self, _handle: &Self::ComponentInstanceHandle, _index: usize) {}

            fn move_marker_pin(&self, _instance_handle: &Self::ComponentInstanceHandle, _pin_handle: &Self::MarkerPinHandle, _to: f64) {}

            fn connect_marker_pins(&self, _from: &Self::MarkerPinHandle, _to: &Self::MarkerPinHandle) {}

            fn add_marker_pin(&self, _instance: &Self::ComponentInstanceHandle, _at: TimelineTime) {}

            fn delete_marker_pin(&self, _instance: &Self::ComponentInstanceHandle, _pin: &Self::MarkerPinHandle) {}

            fn lock_marker_pin(&self, _instance: &Self::ComponentInstanceHandle, _pin: &Self::MarkerPinHandle) {}

            fn unlock_marker_pin(&self, _instance: &Self::ComponentInstanceHandle, _pin: &Self::MarkerPinHandle) {}

            fn split_component_at_pin(&self, _instance: &Self::ComponentInstanceHandle, _pin: &Self::MarkerPinHandle) {}

            type MarkerLinkHandle = &'static str;

            fn marker_links<R>(&self, f: impl FnOnce(&MarkerLinkDataList<Self::MarkerLinkHandle, Self::MarkerPinHandle, Self::ComponentInstanceHandle>) -> R) -> R {
                let list = MarkerLinkDataList { list: vec![] };
                f(&list)
            }

            fn edit_marker_link_length(&self, _link: &Self::MarkerLinkHandle, _value: f64) {}

            type ComponentClassHandle = &'static str;

            fn component_classes<R>(&self, f: impl FnOnce(&ComponentClassDataList<Self::ComponentClassHandle>) -> R) -> R {
                let list = ComponentClassDataList { list: vec![] };
                f(&list)
            }

            fn add_component_instance(&self, _class: Self::ComponentClassHandle) {}
        }
        let mut timeline = Timeline::new(Arc::new(VM));
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
        tokio::fs::write(test_output_dir.join("timeline_light.png"), output.into_inner()).await.unwrap();
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
        tokio::fs::write(test_output_dir.join("timeline_dark.png"), output.into_inner()).await.unwrap();
    }
}
