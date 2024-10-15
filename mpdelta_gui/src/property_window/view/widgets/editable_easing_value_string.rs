use egui::epaint::{PathShape, RectShape};
use egui::scroll_area::ScrollBarVisibility;
use egui::{CursorIcon, Id, Pos2, Rect, ScrollArea, Sense, Shape, TextEdit, Ui, Vec2};
use mpdelta_core::component::marker_pin::MarkerPin;
use mpdelta_core::component::parameter::value::{EasingValue, EasingValueEdit};
use mpdelta_core::component::parameter::PinSplitValue;
use mpdelta_core::project::TimelineTimeOfPin;
use std::hash::Hash;
use std::iter;
use std::ops::Range;

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

pub struct EasingValueEditorString<'a, P, H> {
    pub id: H,
    pub time_range: Range<f64>,
    pub all_pins: &'a [MarkerPin],
    pub times: &'a P,
    pub value: &'a mut PinSplitValue<Option<EasingValue<String>>>,
    pub point_per_second: f64,
    pub scroll_offset: &'a mut f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UpdateStatus {
    NotUpdated,
    Updated,
}

impl UpdateStatus {
    pub fn is_updated(&self) -> bool {
        matches!(self, UpdateStatus::Updated)
    }
}

impl<'a, P, H> EasingValueEditorString<'a, P, H>
where
    P: TimelineTimeOfPin,
    H: Hash,
{
    pub fn show(self, ui: &mut Ui) -> UpdateStatus {
        let EasingValueEditorString {
            id,
            time_range,
            all_pins,
            times,
            value,
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
            let plot_area_rect = whole_rect.with_min_y(whole_rect.top() + slider_width * 3.).with_max_y(whole_rect.bottom() - slider_width);
            let time_map = glam::Mat2::from_cols(glam::Vec2::new(time_range.start as f32, time_range.end as f32), glam::Vec2::new(1., 1.)).inverse() * glam::Vec2::new(plot_area_rect.left(), plot_area_rect.right());
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
                            closed: false,
                            fill: widget_visuals.bg_fill,
                            stroke: widget_visuals.fg_stroke.into(),
                        }),
                        Shape::Path(PathShape {
                            points: vec![
                                Pos2::new(base_position - slider_width, whole_rect.bottom() - slider_width),
                                Pos2::new(base_position, whole_rect.bottom()),
                                Pos2::new(base_position + slider_width, whole_rect.bottom() - slider_width),
                            ],
                            closed: false,
                            fill: widget_visuals.bg_fill,
                            stroke: widget_visuals.fg_stroke.into(),
                        }),
                    ]
                });

            let foreground_pin = (0..value.len_time()).map(|i| {
                let (left, time, right) = value.get_time(i).unwrap();
                let base_time_pixel = glam::Vec2::new(times.time_of_pin(time).unwrap().value().into_f64() as f32, 1.).dot(time_map);

                match (left.and_then(Option::as_ref), right.and_then(Option::as_ref)) {
                    (Some(_), Some(_)) => Shape::Path(PathShape {
                        points: vec![
                            Pos2::new(base_time_pixel + slider_width * 2., whole_rect.top()),
                            Pos2::new(base_time_pixel + slider_width * 2., whole_rect.top() + slider_width * 2.),
                            Pos2::new(base_time_pixel + slider_width, whole_rect.top() + slider_width * 3.),
                            Pos2::new(base_time_pixel + slider_width, whole_rect.bottom() - slider_width),
                            Pos2::new(base_time_pixel, whole_rect.bottom()),
                            Pos2::new(base_time_pixel - slider_width, whole_rect.bottom() - slider_width),
                            Pos2::new(base_time_pixel - slider_width, whole_rect.top() + slider_width * 3.),
                            Pos2::new(base_time_pixel - slider_width * 2., whole_rect.top() + slider_width * 2.),
                            Pos2::new(base_time_pixel - slider_width * 2., whole_rect.top()),
                        ],
                        closed: true,
                        fill: widget_visuals.bg_fill,
                        stroke: widget_visuals.fg_stroke.into(),
                    }),
                    (None, Some(_)) => Shape::Path(PathShape {
                        points: vec![
                            Pos2::new(base_time_pixel, whole_rect.top()),
                            Pos2::new(base_time_pixel + slider_width * 2., whole_rect.top()),
                            Pos2::new(base_time_pixel + slider_width * 2., whole_rect.top() + slider_width * 2.),
                            Pos2::new(base_time_pixel + slider_width, whole_rect.top() + slider_width * 3.),
                            Pos2::new(base_time_pixel + slider_width, whole_rect.bottom() - slider_width),
                            Pos2::new(base_time_pixel, whole_rect.bottom()),
                        ],
                        closed: true,
                        fill: widget_visuals.bg_fill,
                        stroke: widget_visuals.fg_stroke.into(),
                    }),
                    (Some(_), None) => Shape::Path(PathShape {
                        points: vec![
                            Pos2::new(base_time_pixel, whole_rect.top()),
                            Pos2::new(base_time_pixel - slider_width * 2., whole_rect.top()),
                            Pos2::new(base_time_pixel - slider_width * 2., whole_rect.top() + slider_width * 2.),
                            Pos2::new(base_time_pixel - slider_width, whole_rect.top() + slider_width * 3.),
                            Pos2::new(base_time_pixel - slider_width, whole_rect.bottom() - slider_width),
                            Pos2::new(base_time_pixel, whole_rect.bottom()),
                        ],
                        closed: true,
                        fill: widget_visuals.bg_fill,
                        stroke: widget_visuals.fg_stroke.into(),
                    }),
                    (None, None) => Shape::Noop,
                }
            });

            let shapes = iter::empty().chain(background_pin).chain(iter::once(Shape::Rect(RectShape::new(plot_area_rect, 0., widget_visuals.weak_bg_fill, widget_visuals.bg_stroke)))).chain(foreground_pin);
            painter.extend(shapes);

            {
                for i in 0..value.len_value() {
                    let (&left, _, &right) = value.get_value(i).unwrap();
                    let value = value.get_value_mut(i).unwrap();
                    let left = glam::Vec2::new(times.time_of_pin(&left).unwrap().value().into_f64() as f32, 1.).dot(time_map);
                    let right = glam::Vec2::new(times.time_of_pin(&right).unwrap().value().into_f64() as f32, 1.).dot(time_map);
                    if let Some(EasingValue { value, .. }) = value {
                        let result = value.edit_value(|s: &mut String| {
                            let rect = plot_area_rect.with_min_x(left).with_max_x(right);
                            let edit_output = ui.allocate_ui_at_rect(rect, |ui| TextEdit::multiline(s).min_size(rect.size()).show(ui).response.changed());
                            edit_output.inner
                        });
                        match result {
                            Ok(false) => {}
                            Ok(true) => updated = UpdateStatus::Updated,
                            Err(e) => eprintln!("{e}"),
                        }
                    }
                }
            }
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
    use mpdelta_core::component::parameter::value::{DynEditableSelfValue, LinearEasing};
    use mpdelta_core::core::IdGenerator;
    use mpdelta_core::time::TimelineTime;
    use mpdelta_core::time_split_value_persistent;
    use mpdelta_core_test_util::TestIdGenerator;
    use std::collections::HashMap;
    use std::io::Cursor;
    use std::path::Path;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_editable_easing_value_editor() {
        const TEST_OUTPUT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../test_output/", env!("CARGO_PKG_NAME"));
        let test_output_dir = Path::new(TEST_OUTPUT_DIR);
        tokio::fs::create_dir_all(test_output_dir).await.unwrap();
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
                    Some(EasingValue::new(DynEditableSelfValue("string1".to_owned()), Arc::new(LinearEasing))),
                    *all_pins[1].id(),
                    None,
                    *all_pins[2].id(),
                    Some(EasingValue::new(DynEditableSelfValue("string2".to_owned()), Arc::new(LinearEasing))),
                    *all_pins[4].id(),
                );
                let mut scroll_offset = 0.;
                let $editor = EasingValueEditorString {
                    id: "editor",
                    time_range: 1.0..5.0,
                    all_pins: &all_pins,
                    times: &HashMap::from_iter(all_pins.iter().enumerate().map(|(i, p)| (*p.id(), TimelineTime::new(MixedFraction::from_integer(i as i32))))),
                    value: &mut value,
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
        tokio::fs::write(test_output_dir.join("easing_value_string_light.png"), output.into_inner()).await.unwrap();

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
        tokio::fs::write(test_output_dir.join("easing_value_string_dark.png"), output.into_inner()).await.unwrap();
    }
}
