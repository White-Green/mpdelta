use egui::epaint::{PathShape, RectShape};
use egui::scroll_area::ScrollBarVisibility;
use egui::{CursorIcon, Id, Pos2, Rect, ScrollArea, Sense, Shape, TextEdit, Ui, Vec2};
use mpdelta_core::common::time_split_value::TimeSplitValue;
use mpdelta_core::component::parameter::value::{EasingValue, EasingValueEdit};
use std::hash::Hash;
use std::iter;
use std::ops::{Bound, Range};

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

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum EasingValueStringEditEvent {
    FlipPin(usize),
    UpdateValue { value_index: usize, value: String },
}

pub struct EasingValueEditorString<'a, H, F> {
    pub id: H,
    pub time_range: Range<f64>,
    pub times: &'a [f64],
    pub value: &'a mut TimeSplitValue<usize, Option<EasingValue<String>>>,
    pub point_per_second: f64,
    pub scroll_offset: &'a mut f32,
    pub update: F,
}

impl<'a, H, F> EasingValueEditorString<'a, H, F>
where
    H: Hash,
    F: FnMut(EasingValueStringEditEvent) + 'a,
{
    pub fn show(self, ui: &mut Ui) {
        let EasingValueEditorString {
            id,
            time_range,
            times,
            value,
            point_per_second,
            scroll_offset,
            mut update,
        } = self;
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
                let &[left, ref center @ .., right] = times else {
                    unreachable!();
                };
                let left_position = glam::Vec2::new(left as f32, 1.).dot(time_map);
                let response = ui.interact(Rect::from_x_y_ranges(left_position..=left_position + slider_width * 2., whole_rect.top()..=whole_rect.top() + slider_width * 3.), id.with(("pin_head", 0usize)), Sense::click());
                if response.clicked() {
                    update(EasingValueStringEditEvent::FlipPin(0));
                }
                for (i, &time) in center.iter().enumerate() {
                    let x = glam::Vec2::new(time as f32, 1.).dot(time_map);
                    let response = ui.interact(Rect::from_x_y_ranges(x - slider_width * 2.0..=x + slider_width * 2., whole_rect.top()..=whole_rect.top() + slider_width * 3.), id.with(("pin_head", i + 1)), Sense::click());
                    if response.clicked() {
                        update(EasingValueStringEditEvent::FlipPin(i + 1));
                    }
                }
                let right_position = glam::Vec2::new(right as f32, 1.).dot(time_map);
                let response = ui.interact(
                    Rect::from_x_y_ranges(right_position - slider_width * 2.0..=right_position, whole_rect.top()..=whole_rect.top() + slider_width * 3.),
                    id.with(("pin_head", center.len() + 1)),
                    Sense::click(),
                );
                if response.clicked() {
                    update(EasingValueStringEditEvent::FlipPin(center.len() + 1));
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

            let background_pin = (0..value.len_value())
                .flat_map(|i| {
                    let (&left, _, &right) = value.get_value(i).unwrap();
                    times[(Bound::Excluded(left), Bound::Excluded(right))].iter().copied()
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
                            stroke: widget_visuals.fg_stroke,
                        }),
                        Shape::Path(PathShape {
                            points: vec![
                                Pos2::new(base_position - slider_width, whole_rect.bottom() - slider_width),
                                Pos2::new(base_position, whole_rect.bottom()),
                                Pos2::new(base_position + slider_width, whole_rect.bottom() - slider_width),
                            ],
                            closed: false,
                            fill: widget_visuals.bg_fill,
                            stroke: widget_visuals.fg_stroke,
                        }),
                    ]
                });

            let foreground_pin = (0..value.len_time()).map(|i| {
                let (left, &time, right) = value.get_time(i).unwrap();
                let base_time_pixel = glam::Vec2::new(times[time] as f32, 1.).dot(time_map);

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
                        stroke: widget_visuals.fg_stroke,
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
                        stroke: widget_visuals.fg_stroke,
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
                        stroke: widget_visuals.fg_stroke,
                    }),
                    (None, None) => Shape::Noop,
                }
            });

            let shapes = iter::empty().chain(background_pin).chain(iter::once(Shape::Rect(RectShape::new(plot_area_rect, 0., widget_visuals.weak_bg_fill, widget_visuals.bg_stroke)))).chain(foreground_pin);
            painter.extend(shapes);

            let update_state = {
                let mut update_state = None;
                for i in 0..value.len_value() {
                    let (&left, value, &right) = value.get_value_mut(i).unwrap();
                    let left = glam::Vec2::new(times[left] as f32, 1.).dot(time_map);
                    let right = glam::Vec2::new(times[right] as f32, 1.).dot(time_map);
                    if let Some(EasingValue { value, .. }) = value {
                        let result = value.edit_value(|s: &mut String| {
                            let s_clone = s.clone();
                            let rect = plot_area_rect.with_min_x(left).with_max_x(right);
                            ui.allocate_ui_at_rect(rect, |ui| {
                                TextEdit::multiline(s).min_size(rect.size()).show(ui);
                            });
                            (*s != s_clone).then(|| EasingValueStringEditEvent::UpdateValue { value_index: i, value: s.clone() })
                        });
                        match result {
                            Ok(None) => {}
                            Ok(Some(update)) => assert!(update_state.replace(update).is_none()),
                            Err(e) => eprintln!("{e}"),
                        }
                    }
                }
                update_state
            };

            if let Some(edit) = update_state {
                update(edit);
            }
        });
        *scroll_offset = scroll_area_output.state.offset.x;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Visuals;
    use egui_image_renderer::FileFormat;
    use mpdelta_core::component::parameter::value::{DynEditableSelfValue, LinearEasing};
    use mpdelta_core::time_split_value;
    use std::io::Cursor;
    use std::path::Path;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_editable_easing_value_editor() {
        const TEST_OUTPUT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../test_output/", env!("CARGO_PKG_NAME"));
        let test_output_dir = Path::new(TEST_OUTPUT_DIR);
        tokio::fs::create_dir_all(test_output_dir).await.unwrap();
        macro_rules! create_editor {
            ($editor:ident) => {
                let times = [1., 2., 3., 4., 5.];
                let mut value = time_split_value!(
                    0,
                    Some(EasingValue::new(DynEditableSelfValue("string1".to_owned()), Arc::new(LinearEasing))),
                    1,
                    None,
                    2,
                    Some(EasingValue::new(DynEditableSelfValue("string2".to_owned()), Arc::new(LinearEasing))),
                    4,
                );
                let mut scroll_offset = 0.;
                let $editor = EasingValueEditorString {
                    id: "editor",
                    time_range: 1.0..5.0,
                    times: &times,
                    value: &mut value,
                    point_per_second: 150.,
                    scroll_offset: &mut scroll_offset,
                    update: |_| {},
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
