use egui::epaint::{PathShape, PathStroke, RectShape};
use egui::scroll_area::ScrollBarVisibility;
use egui::{Color32, CursorIcon, Id, PointerButton, Pos2, Rect, ScrollArea, Sense, Shape, Ui, Vec2};
use mpdelta_core::component::marker_pin::MarkerPin;
use mpdelta_core::component::parameter::value::{EasingValue, EasingValueEdit};
use mpdelta_core::component::parameter::PinSplitValue;
use mpdelta_core::project::TimelineTimeOfPin;
use std::hash::Hash;
use std::iter;
use std::ops::Range;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
struct InnerState {
    height: f32,
}

impl Default for InnerState {
    fn default() -> Self {
        InnerState { height: 96. }
    }
}

#[derive(Debug)]
struct InnerStateEdit {
    inner_state: InnerState,
    updated: bool,
}

impl InnerStateEdit {
    fn height(&self) -> f32 {
        self.inner_state.height
    }

    fn height_mut(&mut self) -> &mut f32 {
        self.updated = true;
        &mut self.inner_state.height
    }

    fn updated(&self) -> bool {
        self.updated
    }
}

impl From<InnerState> for InnerStateEdit {
    fn from(value: InnerState) -> Self {
        Self { inner_state: value, updated: false }
    }
}

impl From<InnerStateEdit> for InnerState {
    fn from(value: InnerStateEdit) -> Self {
        value.inner_state
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Hash)]
pub enum Side {
    Left,
    Right,
}

pub struct EasingValueEditorF64<'a, P, H> {
    pub id: H,
    pub reset: bool,
    pub time_range: Range<f64>,
    pub all_pins: &'a [MarkerPin],
    pub times: &'a P,
    pub value: &'a mut PinSplitValue<Option<EasingValue<f64>>>,
    pub value_range: Range<f64>,
    pub point_per_second: f64,
    pub scroll_offset: &'a mut f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UpdateStatus {
    NotUpdated,
    TemporaryUpdated,
    Updated,
}

impl UpdateStatus {
    pub fn is_updated(&self) -> bool {
        !matches!(self, UpdateStatus::NotUpdated)
    }
}

impl<P, H> EasingValueEditorF64<'_, P, H>
where
    P: TimelineTimeOfPin,
    H: Hash,
{
    pub fn show(self, ui: &mut Ui) -> UpdateStatus {
        let EasingValueEditorF64 {
            id,
            reset,
            time_range,
            all_pins,
            times,
            value,
            value_range,
            point_per_second,
            scroll_offset,
        } = self;
        let mut updated = UpdateStatus::NotUpdated;
        let id = Id::new(id);
        let scroll_area_output = ScrollArea::horizontal().id_source(id).scroll_offset(Vec2::new(*scroll_offset, 0.)).scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
            let id = ui.make_persistent_id(id);
            let mut state: InnerStateEdit = ui.data(|data| data.get_temp::<InnerState>(id).unwrap_or_default().into());
            let width = ((time_range.end - time_range.start) * point_per_second) as f32;
            let (response, mut painter) = ui.allocate_painter(Vec2::new(width, state.height()), Sense::hover());
            let whole_rect = response.rect;
            painter.set_clip_rect(painter.clip_rect().intersect(whole_rect));

            let widget_visuals = ui.visuals().widgets.inactive;
            let slider_width = painter.round_to_pixel(ui.spacing().interact_size.y / 6.);
            let text_box_height = 8.;
            let plot_area_rect = whole_rect.with_min_y(whole_rect.top() + slider_width * 3.).with_max_y(whole_rect.bottom() - slider_width - text_box_height);
            let time_map = glam::Mat2::from_cols(glam::Vec2::new(time_range.start as f32, time_range.end as f32), glam::Vec2::new(1., 1.)).inverse() * glam::Vec2::new(plot_area_rect.left(), plot_area_rect.right());
            let value_map = glam::Mat2::from_cols(glam::Vec2::new(value_range.start as f32, value_range.end as f32), glam::Vec2::new(1., 1.)).inverse() * glam::Vec2::new(plot_area_rect.bottom(), plot_area_rect.top());
            {
                let [left_pin, mid @ .., right_pin] = all_pins else { panic!() };
                let left = times.time_of_pin(left_pin.id()).unwrap().value().into_f64() as f32;
                let left_position = glam::Vec2::new(left, 1.).dot(time_map);
                let right = times.time_of_pin(right_pin.id()).unwrap().value().into_f64() as f32;
                let right_position = glam::Vec2::new(right, 1.).dot(time_map);
                let iter = iter::once((left_pin, left_position..=left_position + slider_width * 2.))
                    .chain(mid.iter().map(|pin| {
                        let time = times.time_of_pin(pin.id()).unwrap().value().into_f64() as f32;
                        let x = glam::Vec2::new(time, 1.).dot(time_map);
                        (pin, x - slider_width * 2.0..=x + slider_width * 2.)
                    }))
                    .chain(iter::once((right_pin, right_position - slider_width * 2.0..=right_position)));
                let mut value_time_index = 0;
                for (i, (pin, x_range)) in iter.enumerate() {
                    let response = ui.interact(Rect::from_x_y_ranges(x_range, whole_rect.top()..=whole_rect.top() + slider_width * 3.), id.with(("pin_head", i)), Sense::click());
                    if value.get_time(value_time_index).unwrap().1 != pin.id() {
                        if response.clicked() {
                            value.split_value_by_clone(value_time_index - 1, *pin.id());
                            updated = UpdateStatus::Updated;
                        }
                    } else if response.clicked() && value.merge_two_values_by_left(value_time_index).is_ok() {
                        updated = UpdateStatus::Updated;
                    } else {
                        value_time_index += 1;
                    }
                }
            }
            {
                let response = ui.interact(Rect::from_x_y_ranges(whole_rect.x_range(), whole_rect.bottom() - slider_width..=whole_rect.bottom()), id.with("bottom_resize"), Sense::drag());
                let response = response.on_hover_and_drag_cursor(CursorIcon::ResizeNorth);

                if let Some(Pos2 { y, .. }) = response.interact_pointer_pos() {
                    let height = state.height_mut();
                    *height = (y - whole_rect.top()).max(32.);
                }
            }
            if state.updated() {
                ui.data_mut(|data| data.insert_temp::<InnerState>(id, state.into()));
            }
            {
                let cursor_y_range = Range {
                    start: glam::Vec2::new(value_range.end as f32, 1.).dot(value_map),
                    end: glam::Vec2::new(value_range.start as f32, 1.).dot(value_map),
                };
                for i in 0..value.len_value() {
                    let (left, _, right) = value.get_value(i).unwrap();
                    let left_time_position = glam::Vec2::new(times.time_of_pin(left).unwrap().value().into_f64() as f32, 1.).dot(time_map);
                    let right_time_position = glam::Vec2::new(times.time_of_pin(right).unwrap().value().into_f64() as f32, 1.).dot(time_map);

                    let response = ui.interact(Rect::from_x_y_ranges(left_time_position..=left_time_position + slider_width * 2., plot_area_rect.y_range()), id.with(("pin_slider", i, Side::Left)), Sense::click_and_drag());
                    let interact_pointer_pos = response.interact_pointer_pos().map(|Pos2 { y, .. }| y.clamp(cursor_y_range.start, cursor_y_range.end));
                    let update_value = interact_pointer_pos.map(|y| ((y - value_map.y) / value_map.x) as f64);
                    if response.clicked_by(PointerButton::Primary) || response.drag_stopped_by(PointerButton::Primary) {
                        if let Some(easing_value) = value.get_value_mut(i).unwrap() {
                            easing_value.value.edit_value::<(f64, f64), _>(|(left, _)| *left = update_value.unwrap()).expect("downcast error");
                            updated = UpdateStatus::Updated;
                        }
                    } else if response.dragged_by(PointerButton::Primary) {
                        if let Some(easing_value) = value.get_value_mut(i).unwrap() {
                            easing_value.value.edit_value::<(f64, f64), _>(|(left, _)| *left = update_value.unwrap()).expect("downcast error");
                            updated = UpdateStatus::TemporaryUpdated;
                        }
                    }
                    let response = ui.interact(Rect::from_x_y_ranges(right_time_position - slider_width * 2.0..=right_time_position, plot_area_rect.y_range()), id.with(("pin_slider", i, Side::Right)), Sense::click_and_drag());
                    let interact_pointer_pos = response.interact_pointer_pos().map(|Pos2 { y, .. }| y.clamp(cursor_y_range.start, cursor_y_range.end));
                    let update_value = interact_pointer_pos.map(|y| ((y - value_map.y) / value_map.x) as f64);
                    if response.clicked_by(PointerButton::Primary) || response.drag_stopped_by(PointerButton::Primary) {
                        if let Some(easing_value) = value.get_value_mut(i).unwrap() {
                            easing_value.value.edit_value::<(f64, f64), _>(|(_, right)| *right = update_value.unwrap()).expect("downcast error");
                            updated = UpdateStatus::Updated;
                        }
                    } else if response.dragged_by(PointerButton::Primary) {
                        if let Some(easing_value) = value.get_value_mut(i).unwrap() {
                            easing_value.value.edit_value::<(f64, f64), _>(|(_, right)| *right = update_value.unwrap()).expect("downcast error");
                            updated = UpdateStatus::TemporaryUpdated;
                        }
                    }
                }
            }

            let mut pins = all_pins;
            let background_pin = (0..value.len_value())
                .flat_map(|i| {
                    let (_, _, right) = value.get_value(i).unwrap();
                    let right = pins.iter().position(|p| p.id() == right).unwrap();
                    let (head, tail) = pins.split_at(right);
                    pins = tail;
                    head[1..].iter().map(|p| times.time_of_pin(p.id()).unwrap().value().into_f64())
                })
                .flat_map(|time| {
                    let base_position = glam::Vec2::new(time as f32, 1.).dot(time_map);
                    [
                        Shape::Path(PathShape {
                            points: vec![
                                Pos2::new(base_position - slider_width, whole_rect.top() + slider_width * 3.),
                                Pos2::new(base_position - slider_width * 2., whole_rect.top() + slider_width * 2.),
                                Pos2::new(base_position - slider_width * 2., whole_rect.top()),
                                Pos2::new(base_position + slider_width * 2., whole_rect.top()),
                                Pos2::new(base_position + slider_width * 2., whole_rect.top() + slider_width * 2.),
                                Pos2::new(base_position + slider_width, whole_rect.top() + slider_width * 3.),
                            ],
                            closed: true,
                            fill: widget_visuals.bg_fill,
                            stroke: PathStroke::NONE,
                        }),
                        Shape::Path(PathShape {
                            points: vec![
                                Pos2::new(base_position - slider_width, whole_rect.top() + slider_width * 3.),
                                Pos2::new(base_position - slider_width * 2., whole_rect.top() + slider_width * 2.),
                                Pos2::new(base_position - slider_width * 2., whole_rect.top()),
                                Pos2::new(base_position + slider_width * 2., whole_rect.top()),
                                Pos2::new(base_position + slider_width * 2., whole_rect.top() + slider_width * 2.),
                                Pos2::new(base_position + slider_width, whole_rect.top() + slider_width * 3.),
                            ],
                            closed: false,
                            fill: Color32::TRANSPARENT,
                            stroke: widget_visuals.fg_stroke.into(),
                        }),
                        Shape::Path(PathShape {
                            points: vec![
                                Pos2::new(base_position - slider_width, plot_area_rect.bottom()),
                                Pos2::new(base_position, plot_area_rect.bottom() + slider_width),
                                Pos2::new(base_position + slider_width, plot_area_rect.bottom()),
                            ],
                            closed: true,
                            fill: widget_visuals.bg_fill,
                            stroke: PathStroke::NONE,
                        }),
                        Shape::Path(PathShape {
                            points: vec![
                                Pos2::new(base_position - slider_width, plot_area_rect.bottom()),
                                Pos2::new(base_position, plot_area_rect.bottom() + slider_width),
                                Pos2::new(base_position + slider_width, plot_area_rect.bottom()),
                            ],
                            closed: false,
                            fill: Color32::TRANSPARENT,
                            stroke: widget_visuals.fg_stroke.into(),
                        }),
                    ]
                });

            let foreground_pin = (0..value.len_time()).flat_map(|i| {
                let (left, time, right) = value.get_time(i).unwrap();
                let base_time_pixel = glam::Vec2::new(times.time_of_pin(time).unwrap().value().into_f64() as f32, 1.).dot(time_map);
                let visuals = ui.visuals();
                let slider_visuals_pin_right = &visuals.widgets.inactive;
                let slider_visuals_pin_left = &visuals.widgets.inactive;

                let get_pin_right_position = |value: &EasingValue<f64>| glam::Vec2::new(value.get_value(0.) as f32, 1.).dot(value_map);
                let get_pin_left_position = |value: &EasingValue<f64>| glam::Vec2::new(value.get_value(1.) as f32, 1.).dot(value_map);

                let pin_shape = match (left.and_then(Option::as_ref), right.and_then(Option::as_ref)) {
                    (Some(left_value), Some(right_value)) => {
                        let pin_left_position = get_pin_left_position(left_value);
                        let pin_right_position = get_pin_right_position(right_value);
                        let closed = (pin_left_position - pin_right_position).abs() > widget_visuals.fg_stroke.width;
                        [
                            Shape::Path(PathShape {
                                points: vec![
                                    Pos2::new(base_time_pixel + slider_width * 2., whole_rect.top()),
                                    Pos2::new(base_time_pixel + slider_width * 2., whole_rect.top() + slider_width * 2.),
                                    Pos2::new(base_time_pixel + slider_width, whole_rect.top() + slider_width * 3.),
                                    Pos2::new(base_time_pixel + slider_width, plot_area_rect.bottom()),
                                    Pos2::new(base_time_pixel, plot_area_rect.bottom() + slider_width),
                                    Pos2::new(base_time_pixel - slider_width, plot_area_rect.bottom()),
                                    Pos2::new(base_time_pixel - slider_width, whole_rect.top() + slider_width * 3.),
                                    Pos2::new(base_time_pixel - slider_width * 2., whole_rect.top() + slider_width * 2.),
                                    Pos2::new(base_time_pixel - slider_width * 2., whole_rect.top()),
                                ],
                                closed: true,
                                fill: widget_visuals.bg_fill,
                                stroke: widget_visuals.fg_stroke.into(),
                            }),
                            Shape::Path(PathShape {
                                points: vec![
                                    Pos2::new(base_time_pixel, pin_left_position - slider_width * 2.),
                                    Pos2::new(base_time_pixel - slider_width * 2., pin_left_position - slider_width),
                                    Pos2::new(base_time_pixel - slider_width * 2., pin_left_position + slider_width),
                                    Pos2::new(base_time_pixel, pin_left_position + slider_width * 2.),
                                ],
                                closed,
                                fill: slider_visuals_pin_left.bg_fill,
                                stroke: slider_visuals_pin_left.fg_stroke.into(),
                            }),
                            Shape::Path(PathShape {
                                points: vec![
                                    Pos2::new(base_time_pixel, pin_right_position - slider_width * 2.),
                                    Pos2::new(base_time_pixel + slider_width * 2., pin_right_position - slider_width),
                                    Pos2::new(base_time_pixel + slider_width * 2., pin_right_position + slider_width),
                                    Pos2::new(base_time_pixel, pin_right_position + slider_width * 2.),
                                ],
                                closed,
                                fill: slider_visuals_pin_right.bg_fill,
                                stroke: slider_visuals_pin_right.fg_stroke.into(),
                            }),
                        ]
                    }
                    (None, Some(right_value)) => {
                        let pin_right_position = get_pin_right_position(right_value);
                        [
                            Shape::Path(PathShape {
                                points: vec![
                                    Pos2::new(base_time_pixel, whole_rect.top()),
                                    Pos2::new(base_time_pixel + slider_width * 2., whole_rect.top()),
                                    Pos2::new(base_time_pixel + slider_width * 2., whole_rect.top() + slider_width * 2.),
                                    Pos2::new(base_time_pixel + slider_width, whole_rect.top() + slider_width * 3.),
                                    Pos2::new(base_time_pixel + slider_width, plot_area_rect.bottom()),
                                    Pos2::new(base_time_pixel, plot_area_rect.bottom() + slider_width),
                                ],
                                closed: true,
                                fill: widget_visuals.bg_fill,
                                stroke: widget_visuals.fg_stroke.into(),
                            }),
                            Shape::Noop,
                            Shape::Path(PathShape {
                                points: vec![
                                    Pos2::new(base_time_pixel, pin_right_position - slider_width * 2.),
                                    Pos2::new(base_time_pixel + slider_width * 2., pin_right_position - slider_width),
                                    Pos2::new(base_time_pixel + slider_width * 2., pin_right_position + slider_width),
                                    Pos2::new(base_time_pixel, pin_right_position + slider_width * 2.),
                                ],
                                closed: true,
                                fill: slider_visuals_pin_right.bg_fill,
                                stroke: slider_visuals_pin_right.fg_stroke.into(),
                            }),
                        ]
                    }
                    (Some(left_value), None) => {
                        let pin_left_position = get_pin_left_position(left_value);
                        [
                            Shape::Path(PathShape {
                                points: vec![
                                    Pos2::new(base_time_pixel, whole_rect.top()),
                                    Pos2::new(base_time_pixel - slider_width * 2., whole_rect.top()),
                                    Pos2::new(base_time_pixel - slider_width * 2., whole_rect.top() + slider_width * 2.),
                                    Pos2::new(base_time_pixel - slider_width, whole_rect.top() + slider_width * 3.),
                                    Pos2::new(base_time_pixel - slider_width, plot_area_rect.bottom()),
                                    Pos2::new(base_time_pixel, plot_area_rect.bottom() + slider_width),
                                ],
                                closed: true,
                                fill: widget_visuals.bg_fill,
                                stroke: widget_visuals.fg_stroke.into(),
                            }),
                            Shape::Path(PathShape {
                                points: vec![
                                    Pos2::new(base_time_pixel, pin_left_position - slider_width * 2.),
                                    Pos2::new(base_time_pixel - slider_width * 2., pin_left_position - slider_width),
                                    Pos2::new(base_time_pixel - slider_width * 2., pin_left_position + slider_width),
                                    Pos2::new(base_time_pixel, pin_left_position + slider_width * 2.),
                                ],
                                closed: true,
                                fill: slider_visuals_pin_left.bg_fill,
                                stroke: slider_visuals_pin_left.fg_stroke.into(),
                            }),
                            Shape::Noop,
                        ]
                    }
                    (None, None) => [Shape::Noop, Shape::Noop, Shape::Noop],
                };
                pin_shape.into_iter().filter(|shape| shape != &Shape::Noop)
            });

            let segment_lines = (0..value.len_value()).filter_map(|i| {
                let (left, value, right) = value.get_value(i).unwrap();
                let value = value.as_ref()?;
                let left = times.time_of_pin(left).unwrap().value().into_f64();
                let right = times.time_of_pin(right).unwrap().value().into_f64();
                let points = ((left * 60.).floor() as i32..=(right * 60.).ceil() as i32)
                    .map(|t| t as f64 / 60.)
                    .map(move |t| {
                        let t = t.clamp(left, right);
                        let p = (t - left) / (right - left);
                        let value = value.get_value(p);
                        Pos2::new(t as f32, value as f32)
                    })
                    .map(move |Pos2 { x, y }| Pos2::new(glam::Vec2::new(x, 1.).dot(time_map), glam::Vec2::new(y, 1.).dot(value_map)))
                    .collect();
                Some(Shape::line(points, widget_visuals.fg_stroke))
            });
            let shapes = iter::empty()
                .chain(background_pin)
                .chain(iter::once(Shape::Rect(RectShape::new(plot_area_rect, 0., widget_visuals.weak_bg_fill, widget_visuals.bg_stroke))))
                .chain(segment_lines)
                .chain(foreground_pin);
            painter.extend(shapes);

            let value_string_buffer_id = id.with("value_string_buffer");
            let value_string_buffer = ui.data(|data| {
                data.get_temp::<Arc<Mutex<Box<[String]>>>>(value_string_buffer_id).filter(|buffers| !reset && buffers.lock().unwrap().len() == value.len_value() * 2).unwrap_or_else(|| {
                    Arc::new(Mutex::new(
                        (0..value.len_value())
                            .flat_map(|i| {
                                if let (_, Some(value), _) = value.get_value(i).unwrap() {
                                    [value.get_value(0.).to_string(), value.get_value(1.).to_string()]
                                } else {
                                    [String::new(), String::new()]
                                }
                            })
                            .collect(),
                    ))
                })
            });
            {
                let mut value_string_buffer = value_string_buffer.lock().unwrap();
                for (value_index, buffer) in (0..value.len_value()).zip(value_string_buffer.chunks_mut(2)) {
                    let (left_pin, Some(_), right_pin) = value.get_value(value_index).unwrap() else {
                        continue;
                    };
                    let left_time = times.time_of_pin(left_pin).unwrap().value().into_f64();
                    let right_time = times.time_of_pin(right_pin).unwrap().value().into_f64();
                    let left_pixel = glam::Vec2::new(left_time as f32, 1.).dot(time_map);
                    let right_pixel = glam::Vec2::new(right_time as f32, 1.).dot(time_map);
                    let [left_buffer, right_buffer] = buffer else { unreachable!() };
                    let text_box_width = 32.;
                    ui.allocate_ui_at_rect(Rect::from_min_max(Pos2::new(left_pixel, plot_area_rect.bottom() + slider_width), Pos2::new(left_pixel + text_box_width, plot_area_rect.bottom() + text_box_height)), |ui| {
                        if ui.text_edit_singleline(left_buffer).changed() {
                            let Ok(update_value) = left_buffer.parse() else {
                                return;
                            };
                            if let Some(easing_value) = value.get_value_mut(value_index).unwrap() {
                                easing_value.value.edit_value::<(f64, f64), _>(|(left, _)| *left = update_value).expect("downcast error");
                                updated = UpdateStatus::Updated;
                            }
                        };
                    });
                    ui.allocate_ui_at_rect(Rect::from_min_max(Pos2::new(right_pixel - text_box_width, plot_area_rect.bottom() + slider_width), Pos2::new(right_pixel, plot_area_rect.bottom() + text_box_height)), |ui| {
                        if ui.text_edit_singleline(right_buffer).changed() {
                            let Ok(update_value) = right_buffer.parse() else {
                                return;
                            };
                            if let Some(easing_value) = value.get_value_mut(value_index).unwrap() {
                                easing_value.value.edit_value::<(f64, f64), _>(|(_, right)| *right = update_value).expect("downcast error");
                                updated = UpdateStatus::Updated;
                            }
                        }
                    });
                }
            }
            ui.data_mut(|data| data.insert_temp::<Arc<Mutex<Box<[String]>>>>(value_string_buffer_id, value_string_buffer));
        });
        *scroll_offset = scroll_area_output.state.offset.x;
        updated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Visuals;
    use egui_image_renderer::FileFormat;
    use mpdelta_core::common::mixed_fraction::MixedFraction;
    use mpdelta_core::component::parameter::value::{DynEditableEasingValueManager, DynEditableEasingValueMarker, Easing, EasingIdentifier, EasingInput, NamedAny};
    use mpdelta_core::core::IdGenerator;
    use mpdelta_core::time::TimelineTime;
    use mpdelta_core::time_split_value_persistent;
    use mpdelta_core_test_util::TestIdGenerator;
    use serde::Serialize;
    use std::collections::HashMap;
    use std::io::Cursor;
    use std::path::Path;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_editable_easing_value_editor() {
        const TEST_OUTPUT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../test_output/", env!("CARGO_PKG_NAME"));
        let test_output_dir = Path::new(TEST_OUTPUT_DIR);
        tokio::fs::create_dir_all(test_output_dir).await.unwrap();
        #[derive(Clone, Serialize)]
        struct LinearEasingF64 {
            start: f64,
            end: f64,
        }
        impl DynEditableEasingValueMarker for LinearEasingF64 {
            type Out = f64;
            fn manager(&self) -> &dyn DynEditableEasingValueManager<Self::Out> {
                todo!()
            }

            fn get_raw_value_mut(&mut self) -> &mut dyn NamedAny {
                unimplemented!()
            }

            fn get_value(&self, easing: f64) -> Self::Out {
                let LinearEasingF64 { start, end } = self;
                start + (end - start) * easing
            }
        }

        struct Easing1;
        impl Easing for Easing1 {
            fn identifier(&self) -> EasingIdentifier {
                todo!()
            }
            fn easing(&self, from: EasingInput) -> f64 {
                let x = 1. - from.value();
                1. - x * x
            }
        }
        struct Easing2;
        impl Easing for Easing2 {
            fn identifier(&self) -> EasingIdentifier {
                todo!()
            }
            fn easing(&self, from: EasingInput) -> f64 {
                let x = from.value();
                1. - (x * std::f64::consts::PI / 2.).cos()
            }
        }
        let id = TestIdGenerator::new();
        macro_rules! create_editor {
            ($editor:ident) => {
                let all_pins = [
                    MarkerPin::new_unlocked(id.generate_new()),
                    MarkerPin::new_unlocked(id.generate_new()),
                    MarkerPin::new_unlocked(id.generate_new()),
                    MarkerPin::new_unlocked(id.generate_new()),
                    MarkerPin::new_unlocked(id.generate_new()),
                ];
                let mut value = time_split_value_persistent!(
                    *all_pins[0].id(),
                    Some(EasingValue::new(LinearEasingF64 { start: 0., end: 1. }, Arc::new(Easing1))),
                    *all_pins[1].id(),
                    None,
                    *all_pins[2].id(),
                    Some(EasingValue::new(LinearEasingF64 { start: 1., end: 0.5 }, Arc::new(Easing2))),
                    *all_pins[4].id(),
                );
                let mut scroll_offset = 0.;
                let $editor = EasingValueEditorF64 {
                    id: "editor",
                    reset: false,
                    time_range: 1.0..5.0,
                    all_pins: &all_pins,
                    times: &HashMap::from_iter(all_pins.iter().enumerate().map(|(i, p)| (*p.id(), TimelineTime::new(MixedFraction::from_integer(i as i32))))),
                    value: &mut value,
                    value_range: -0.5..1.5,
                    point_per_second: 150.,
                    scroll_offset: &mut scroll_offset,
                };
            };
        }
        let width = 512;
        let height = 128;
        create_editor!(editor);
        let mut output = Cursor::new(Vec::new());
        egui_image_renderer::render_into_file(
            |ctx| {
                ctx.set_visuals(Visuals::light());
                egui::CentralPanel::default().show(ctx, |ui| editor.show(ui));
            },
            width,
            height,
            FileFormat::PNG,
            &mut output,
        )
        .await
        .unwrap();
        tokio::fs::write(test_output_dir.join("easing_value_editor_light.png"), output.into_inner()).await.unwrap();

        create_editor!(editor);
        let mut output = Cursor::new(Vec::new());
        egui_image_renderer::render_into_file(
            |ctx| {
                ctx.set_visuals(Visuals::dark());
                egui::CentralPanel::default().show(ctx, |ui| editor.show(ui));
            },
            width,
            height,
            FileFormat::PNG,
            &mut output,
        )
        .await
        .unwrap();
        tokio::fs::write(test_output_dir.join("easing_value_editor_dark.png"), output.into_inner()).await.unwrap();
    }
}
