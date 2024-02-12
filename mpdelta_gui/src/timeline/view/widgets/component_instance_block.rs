use crate::timeline::viewmodel::ComponentInstanceData;
use egui::{Id, PointerButton, Pos2, Rect, Sense, Shape, TextStyle, Ui, Vec2};
use mpdelta_core::time::TimelineTime;
use std::fmt::{Debug, Formatter};
use std::hash::Hash;

pub enum ComponentInstanceEditEvent<'a, PinHandle> {
    Click,
    Delete,
    MoveWholeBlockTemporary(TimelineTime),
    MoveWholeBlock(TimelineTime),
    MovePinTemporary(&'a PinHandle, TimelineTime),
    MovePin(&'a PinHandle, TimelineTime),
}

impl<'a, PinHandle> Debug for ComponentInstanceEditEvent<'a, PinHandle> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ComponentInstanceEditEvent::Click => write!(f, "Click"),
            ComponentInstanceEditEvent::Delete => write!(f, "Delete"),
            ComponentInstanceEditEvent::MoveWholeBlockTemporary(value) => f.debug_tuple("MoveWholeBlockTemporary").field(value).finish(),
            ComponentInstanceEditEvent::MoveWholeBlock(value) => f.debug_tuple("MoveWholeBlock").field(value).finish(),
            ComponentInstanceEditEvent::MovePinTemporary(_, value) => f.debug_tuple("MovePinTemporary").field(value).finish(),
            ComponentInstanceEditEvent::MovePin(_, value) => f.debug_tuple("MovePin").field(value).finish(),
        }
    }
}

pub struct ComponentInstanceBlock<'a, InstanceHandle, PinHandle, F1, F2, E> {
    instance: &'a ComponentInstanceData<InstanceHandle, PinHandle>,
    top: f32,
    time_to_point: F1,
    point_to_time: F2,
    edit: E,
}

impl<'a, InstanceHandle, PinHandle, F1, F2, E> ComponentInstanceBlock<'a, InstanceHandle, PinHandle, F1, F2, E>
where
    InstanceHandle: Clone + Hash,
    PinHandle: Clone + Hash,
    F1: Fn(TimelineTime) -> f32,
    F2: Fn(f32) -> TimelineTime,
    E: for<'b> FnMut(ComponentInstanceEditEvent<'b, PinHandle>),
{
    pub fn new(instance: &'a ComponentInstanceData<InstanceHandle, PinHandle>, top: f32, time_to_point: F1, point_to_time: F2, edit: E) -> ComponentInstanceBlock<'a, InstanceHandle, PinHandle, F1, F2, E> {
        ComponentInstanceBlock { instance, top, time_to_point, point_to_time, edit }
    }

    pub fn show(self, ui: &mut Ui) -> f32 {
        let ComponentInstanceBlock {
            instance:
                &ComponentInstanceData {
                    ref handle,
                    ref name,
                    selected,
                    start_time,
                    end_time,
                    layer: _,
                    ref left_pin,
                    ref right_pin,
                    ref pins,
                },
            top,
            time_to_point,
            point_to_time,
            mut edit,
        } = self;
        let left = time_to_point(start_time);
        let right = time_to_point(end_time);
        let clip_rect = Rect::from_x_y_ranges(left..=right, top..);
        let painter = ui.painter_at(clip_rect);
        let galley = painter.layout_no_wrap(name.clone(), ui.style().text_styles[&TextStyle::Body].clone(), ui.style().visuals.text_color());
        let text_height = galley.size().y;
        let pin_head_size = text_height / 2.;
        let padding = ui.style().visuals.widgets.active.bg_stroke.width * 2.;
        let block_height = text_height + padding * 2.;
        let block_rect = Rect::from_x_y_ranges(clip_rect.x_range(), clip_rect.top() + pin_head_size..=clip_rect.top() + pin_head_size + block_height);
        let widget_visuals = if selected { &ui.style().visuals.widgets.active } else { &ui.style().visuals.widgets.inactive };
        painter.rect(block_rect, 0., widget_visuals.bg_fill, widget_visuals.fg_stroke);
        left_pin.render_location.set(Pos2::new(left + pin_head_size / 4., block_rect.top() - pin_head_size * 2. / 3.));
        right_pin.render_location.set(Pos2::new(right - pin_head_size / 4., block_rect.top() - pin_head_size * 2. / 3.));
        let shapes = [
            Shape::convex_polygon(
                vec![
                    Pos2::new(left, block_rect.top()),
                    Pos2::new(left, block_rect.top() - pin_head_size),
                    Pos2::new(left + pin_head_size / 2., block_rect.top() - pin_head_size),
                    Pos2::new(left + pin_head_size / 2., block_rect.top() - pin_head_size / 3.),
                    Pos2::new(left, block_rect.top()),
                ],
                widget_visuals.bg_fill,
                widget_visuals.fg_stroke,
            ),
            Shape::convex_polygon(
                vec![
                    Pos2::new(right, block_rect.top()),
                    Pos2::new(right, block_rect.top() - pin_head_size),
                    Pos2::new(right - pin_head_size / 2., block_rect.top() - pin_head_size),
                    Pos2::new(right - pin_head_size / 2., block_rect.top() - pin_head_size / 3.),
                    Pos2::new(right, block_rect.top()),
                ],
                widget_visuals.bg_fill,
                widget_visuals.fg_stroke,
            ),
        ]
        .into_iter()
        .chain(pins.iter().flat_map(|pin| {
            let at = time_to_point(pin.at);
            pin.render_location.set(Pos2::new(at, block_rect.top() - pin_head_size * 2. / 3.));
            [
                Shape::convex_polygon(
                    vec![
                        Pos2::new(at, block_rect.top()),
                        Pos2::new(at - pin_head_size / 3., block_rect.top() - pin_head_size / 3.),
                        Pos2::new(at - pin_head_size / 3., block_rect.top() - pin_head_size),
                        Pos2::new(at + pin_head_size / 3., block_rect.top() - pin_head_size),
                        Pos2::new(at + pin_head_size / 3., block_rect.top() - pin_head_size),
                        Pos2::new(at + pin_head_size / 3., block_rect.top() - pin_head_size / 3.),
                        Pos2::new(at, block_rect.top()),
                    ],
                    widget_visuals.bg_fill,
                    widget_visuals.fg_stroke,
                ),
                Shape::vline(at, block_rect.y_range(), widget_visuals.fg_stroke),
            ]
        }));
        painter.extend(shapes);
        painter.galley(block_rect.min + Vec2::new(padding, padding), galley);
        let response = ui.allocate_rect(block_rect, Sense::click_and_drag());
        if response.clicked() {
            edit(ComponentInstanceEditEvent::Click);
        }
        if response.dragged_by(PointerButton::Primary) {
            let pointer_x = response.interact_pointer_pos().unwrap().x;
            let drag_started = response.drag_started_by(PointerButton::Primary);
            let drag_offset = ui.data_mut(|data| {
                let key = Id::new((handle, "drag_offset"));
                if drag_started {
                    let drag_offset = pointer_x - left;
                    data.insert_temp(key, drag_offset);
                    drag_offset
                } else {
                    data.get_temp(key).unwrap_or_default()
                }
            });
            let new_start_time = point_to_time(pointer_x - drag_offset);
            if response.drag_released_by(PointerButton::Primary) {
                edit(ComponentInstanceEditEvent::MoveWholeBlock(new_start_time));
            } else {
                edit(ComponentInstanceEditEvent::MoveWholeBlockTemporary(new_start_time));
            }
        }
        response.context_menu(|ui| {
            if ui.button("delete").clicked() {
                edit(ComponentInstanceEditEvent::Delete);
                ui.close_menu();
            }
        });
        let pin_head_y_range = block_rect.top() - pin_head_size..=block_rect.top();
        [
            (&left_pin.handle, ui.interact(Rect::from_x_y_ranges(left..=left + pin_head_size / 2., pin_head_y_range.clone()), Id::new((handle, &left_pin.handle)), Sense::click_and_drag())),
            (&right_pin.handle, ui.interact(Rect::from_x_y_ranges(right - pin_head_size / 2.0..=right, pin_head_y_range.clone()), Id::new((handle, &right_pin.handle)), Sense::click_and_drag())),
        ]
        .into_iter()
        .chain(pins.iter().map(|pin| {
            (
                &pin.handle,
                ui.interact(
                    Rect::from_x_y_ranges(time_to_point(pin.at) - pin_head_size / 2.0..=time_to_point(pin.at) + pin_head_size / 2.0, pin_head_y_range.clone()),
                    Id::new((handle, &pin.handle)),
                    Sense::click_and_drag(),
                ),
            )
        }))
        .for_each(|(handle, response)| {
            let make_event = if response.drag_released_by(PointerButton::Primary) {
                ComponentInstanceEditEvent::MovePin
            } else if response.dragged_by(PointerButton::Primary) {
                ComponentInstanceEditEvent::MovePinTemporary
            } else {
                return;
            };
            edit(make_event(handle, point_to_time(response.interact_pointer_pos().unwrap().x)));
        });
        block_rect.bottom() + padding * 2.
    }
}
