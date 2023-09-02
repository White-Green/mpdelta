use crate::preview::viewmodel::PreviewViewModel;
use crate::ImageRegister;
use egui::{Color32, Rect, Rounding, Slider, Stroke, TextureId, Ui, Vec2};
use mpdelta_core::component::parameter::ParameterValueType;
use std::marker::PhantomData;
use std::sync::Arc;

pub struct Preview<K, T, VM> {
    previous_preview: Option<TextureId>,
    view_model: Arc<VM>,
    _phantom: PhantomData<(K, T)>,
}

impl<K: 'static, T: ParameterValueType, VM: PreviewViewModel<K, T>> Preview<K, T, VM> {
    pub fn new(view_model: Arc<VM>) -> Preview<K, T, VM> {
        Preview {
            previous_preview: None,
            view_model,
            _phantom: PhantomData,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, mut image_register: impl ImageRegister<T::Image>) {
        if let Some(img) = self.previous_preview {
            image_register.unregister_image(img);
        }
        if let Some(img) = self.view_model.get_preview_image() {
            let texture_id = image_register.register_image(img);
            let Vec2 { x: area_width, y: area_height } = ui.available_size();
            let area_height = area_height - 72.;
            let (image_width, image_height) = (area_width.min(area_height * 16. / 9.), area_height.min(area_width * 9. / 16.) + 66.);
            let base_pos = ui.cursor().min + Vec2::new(0., 72.);
            ui.allocate_ui_at_rect(Rect::from_min_size(base_pos + Vec2::new((area_width - image_width) / 2., (area_height - image_height) / 2.), Vec2::new(image_width, image_height)), |ui| {
                let image_size = Vec2 { x: image_width, y: image_height - 66. };
                ui.painter().rect(Rect::from_min_size(ui.cursor().min, image_size), Rounding::none(), Color32::BLACK, Stroke::default());
                ui.image(texture_id, image_size);
                ui.horizontal(|ui| {
                    let start = ui.cursor().min.x;
                    if self.view_model.playing() {
                        if ui.button("⏸").clicked() {
                            self.view_model.pause();
                        }
                    } else {
                        #[allow(clippy::collapsible_if)]
                        if ui.button("▶").clicked() {
                            self.view_model.play();
                        }
                    }
                    let button_width = ui.cursor().min.x - start;
                    ui.style_mut().spacing.slider_width = image_width - button_width;
                    ui.add_enabled(
                        !self.view_model.playing(),
                        Slider::from_get_set(0.0..=599., |value| {
                            if let Some(value) = value.map(|value| value.round() as usize) {
                                self.view_model.set_seek(value);
                                value as f64
                            } else {
                                self.view_model.seek() as f64
                            }
                        })
                        .show_value(false),
                    );
                });
            });
            self.previous_preview = Some(texture_id);
        }
    }
}
