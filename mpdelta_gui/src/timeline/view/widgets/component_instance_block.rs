use crate::timeline::viewmodel::{ComponentInstanceData, MarkerPinData};
use egui::{Id, PointerButton, Pos2, Rect, Sense, Shape, StrokeKind, TextStyle, Ui, Vec2};
use std::fmt::{Debug, Formatter};
use std::hash::Hash;

pub enum ComponentInstanceEditEvent<'a, PinHandle> {
    Click,
    Delete,
    MoveWholeBlockTemporary { time: f64, top: f32 },
    MoveWholeBlock { time: f64, top: f32 },
    MovePinTemporary(&'a PinHandle, f64),
    MovePin(&'a PinHandle, f64),
    PullLinkReleased(&'a PinHandle, Pos2),
    PullLink(&'a PinHandle, Pos2),
    UpdateContextMenuOpenedPos(f64, f32),
    AddMarkerPin,
    DeletePin(&'a PinHandle),
    UnlockPin(&'a PinHandle),
    LockPin(&'a PinHandle),
    SplitComponentAtPin(&'a PinHandle),
}

impl<PinHandle> Debug for ComponentInstanceEditEvent<'_, PinHandle> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ComponentInstanceEditEvent::Click => write!(f, "Click"),
            ComponentInstanceEditEvent::Delete => write!(f, "Delete"),
            ComponentInstanceEditEvent::MoveWholeBlockTemporary { time, top } => f.debug_struct("MoveWholeBlockTemporary").field("time", time).field("top", top).finish(),
            ComponentInstanceEditEvent::MoveWholeBlock { time, top } => f.debug_struct("MoveWholeBlock").field("time", time).field("top", top).finish(),
            ComponentInstanceEditEvent::MovePinTemporary(_, value) => f.debug_tuple("MovePinTemporary").field(value).finish(),
            ComponentInstanceEditEvent::MovePin(_, value) => f.debug_tuple("MovePin").field(value).finish(),
            ComponentInstanceEditEvent::PullLinkReleased(_, value) => f.debug_tuple("PullLinkReleased").field(value).finish(),
            ComponentInstanceEditEvent::PullLink(_, value) => f.debug_tuple("PullLink").field(value).finish(),
            ComponentInstanceEditEvent::UpdateContextMenuOpenedPos(time, y) => f.debug_tuple("UpdateContextMenuOpenedPos").field(time).field(y).finish(),
            ComponentInstanceEditEvent::AddMarkerPin => write!(f, "AddMarkerPin"),
            ComponentInstanceEditEvent::DeletePin(_) => f.debug_tuple("DeletePin").finish(),
            ComponentInstanceEditEvent::UnlockPin(_) => f.debug_tuple("UnlockPin").finish(),
            ComponentInstanceEditEvent::LockPin(_) => f.debug_tuple("LockPin").finish(),
            ComponentInstanceEditEvent::SplitComponentAtPin(_) => f.debug_tuple("SplitComponentAtPin").finish(),
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
    F1: Fn(f64) -> f32,
    F2: Fn(f32) -> f64,
    E: for<'b> FnMut(ComponentInstanceEditEvent<'b, PinHandle>),
{
    pub fn new(instance: &'a ComponentInstanceData<InstanceHandle, PinHandle>, top: f32, time_to_point: F1, point_to_time: F2, edit: E) -> ComponentInstanceBlock<'a, InstanceHandle, PinHandle, F1, F2, E> {
        ComponentInstanceBlock { instance, top, time_to_point, point_to_time, edit }
    }

    pub fn show(self, ui: &mut Ui) -> Rect {
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
        let block_showing_rect = Rect::from_x_y_ranges(clip_rect.x_range(), clip_rect.top()..=clip_rect.top() + block_height);
        let block_rect = Rect::from_x_y_ranges(clip_rect.x_range(), clip_rect.top() + pin_head_size..=clip_rect.top() + pin_head_size + block_height);
        let widget_visuals = if selected { &ui.style().visuals.widgets.active } else { &ui.style().visuals.widgets.inactive };
        painter.rect(block_rect, 0., widget_visuals.bg_fill, widget_visuals.fg_stroke, StrokeKind::Inside);
        left_pin.render_location.store(Pos2::new(left + pin_head_size / 4., block_rect.top() - pin_head_size * 2. / 3.));
        right_pin.render_location.store(Pos2::new(right - pin_head_size / 4., block_rect.top() - pin_head_size * 2. / 3.));
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
            pin.render_location.store(Pos2::new(at, block_rect.top() - pin_head_size * 2. / 3.));
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
        painter.galley(block_rect.min + Vec2::new(padding, padding), galley, widget_visuals.fg_stroke.color);
        let response = ui.allocate_rect(block_rect, Sense::click_and_drag());
        if response.clicked() {
            edit(ComponentInstanceEditEvent::Click);
        }
        if response.dragged_by(PointerButton::Primary) || response.drag_stopped_by(PointerButton::Primary) {
            let pointer_pos = response.interact_pointer_pos().unwrap();
            let drag_started = response.drag_started_by(PointerButton::Primary);
            let drag_offset = ui.data_mut(|data| {
                let key = Id::new((handle, "drag_offset"));
                if drag_started {
                    let drag_offset = pointer_pos - Vec2::new(left, top);
                    data.insert_temp(key, drag_offset);
                    drag_offset
                } else {
                    data.get_temp(key).unwrap_or_default()
                }
            });
            let drag_delta = pointer_pos - drag_offset;
            let new_start_time = point_to_time(drag_delta.x);
            if response.drag_stopped_by(PointerButton::Primary) {
                edit(ComponentInstanceEditEvent::MoveWholeBlock { time: new_start_time, top: drag_delta.y });
            } else {
                edit(ComponentInstanceEditEvent::MoveWholeBlockTemporary { time: new_start_time, top: drag_delta.y });
            }
        }
        let pointer_pos = response.interact_pointer_pos();
        response.context_menu(|ui| {
            if let Some(pointer_pos) = pointer_pos {
                edit(ComponentInstanceEditEvent::UpdateContextMenuOpenedPos(point_to_time(pointer_pos.x), pointer_pos.y));
            }
            if ui.button("add pin").clicked() {
                edit(ComponentInstanceEditEvent::AddMarkerPin);
                ui.close_menu();
            }
            if ui.button("delete component").clicked() {
                edit(ComponentInstanceEditEvent::Delete);
                ui.close_menu();
            }
        });
        let pin_head_y_range = block_rect.top() - pin_head_size..=block_rect.top();
        [
            (left_pin, ui.interact(Rect::from_x_y_ranges(left..=left + pin_head_size / 2., pin_head_y_range.clone()), Id::new((handle, &left_pin.handle)), Sense::click_and_drag())),
            (right_pin, ui.interact(Rect::from_x_y_ranges(right - pin_head_size / 2.0..=right, pin_head_y_range.clone()), Id::new((handle, &right_pin.handle)), Sense::click_and_drag())),
        ]
        .into_iter()
        .chain(pins.iter().map(|pin| {
            (
                pin,
                ui.interact(
                    Rect::from_x_y_ranges(time_to_point(pin.at) - pin_head_size / 2.0..=time_to_point(pin.at) + pin_head_size / 2.0, pin_head_y_range.clone()),
                    Id::new((handle, &pin.handle)),
                    Sense::click_and_drag(),
                ),
            )
        }))
        .for_each(|(&MarkerPinData { ref handle, locked, .. }, response)| {
            let make_event = if response.drag_stopped_by(PointerButton::Primary) {
                ComponentInstanceEditEvent::MovePin(handle, point_to_time(response.interact_pointer_pos().unwrap().x))
            } else if response.dragged_by(PointerButton::Primary) {
                ComponentInstanceEditEvent::MovePinTemporary(handle, point_to_time(response.interact_pointer_pos().unwrap().x))
            } else if response.drag_stopped_by(PointerButton::Middle) {
                ComponentInstanceEditEvent::PullLinkReleased(handle, response.interact_pointer_pos().unwrap())
            } else if response.dragged_by(PointerButton::Middle) {
                ComponentInstanceEditEvent::PullLink(handle, response.interact_pointer_pos().unwrap())
            } else {
                let pointer_pos = response.interact_pointer_pos();
                response.context_menu(|ui| {
                    if let Some(pointer_pos) = pointer_pos {
                        edit(ComponentInstanceEditEvent::UpdateContextMenuOpenedPos(point_to_time(pointer_pos.x), pointer_pos.y));
                    }
                    if ui.button("delete pin").clicked() {
                        edit(ComponentInstanceEditEvent::DeletePin(handle));
                        ui.close_menu();
                    }
                    if locked {
                        if ui.button("unlock pin").clicked() {
                            edit(ComponentInstanceEditEvent::UnlockPin(handle));
                            ui.close_menu();
                        }
                        if ui.button("split component at pin").clicked() {
                            edit(ComponentInstanceEditEvent::SplitComponentAtPin(handle));
                            ui.close_menu();
                        }
                    } else {
                        #[allow(clippy::collapsible_else_if)]
                        if ui.button("lock pin").clicked() {
                            edit(ComponentInstanceEditEvent::LockPin(handle));
                            ui.close_menu();
                        }
                    }
                });
                return;
            };
            edit(make_event);
        });
        block_showing_rect.with_max_y(block_rect.bottom() + padding * 2.)
    }
}
