use crate::preview::viewmodel::PreviewViewModel;
use crate::ImageRegister;
use egui::load::SizedTexture;
use egui::{Color32, Rect, Rounding, Slider, Stroke, TextureId, Ui, Vec2};
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::marker_pin::MarkerTime;
use mpdelta_core::component::parameter::ParameterValueType;
use std::marker::PhantomData;
use std::sync::Arc;

pub struct Preview<K, T, VM>
where
    K: 'static,
    T: ParameterValueType,
    VM: PreviewViewModel<K, T>,
{
    previous_instance: Option<VM::ComponentInstanceHandle>,
    previous_preview: Option<TextureId>,
    view_model: Arc<VM>,
    _phantom: PhantomData<(K, T)>,
}

impl<K: 'static, T: ParameterValueType, VM: PreviewViewModel<K, T>> Preview<K, T, VM> {
    pub fn new(view_model: Arc<VM>) -> Preview<K, T, VM> {
        Preview {
            previous_instance: None,
            previous_preview: None,
            view_model,
            _phantom: PhantomData,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, mut image_register: impl ImageRegister<T::Image>) {
        let preview_image = self.view_model.get_preview_image();
        if self.previous_instance != preview_image.instance {
            self.previous_instance = preview_image.instance;
            if let Some(previous_preview) = self.previous_preview.take() {
                image_register.unregister_image(previous_preview);
            }
        }
        if let Some(new_image) = preview_image.image {
            if let Some(previous_preview) = self.previous_preview.take() {
                image_register.unregister_image(previous_preview);
            }
            let texture_id = image_register.register_image(new_image);
            self.previous_preview = Some(texture_id);
        }
        if self.previous_instance.is_some() {
            let Vec2 { x: area_width, y: area_height } = ui.available_size();
            let area_height = area_height - 72.;
            let (image_width, image_height) = (area_width.min(area_height * 16. / 9.), area_height.min(area_width * 9. / 16.) + 66.);
            let base_pos = ui.cursor().min + Vec2::new(0., 72.);
            ui.allocate_ui_at_rect(Rect::from_min_size(base_pos + Vec2::new((area_width - image_width) / 2., (area_height - image_height) / 2.), Vec2::new(image_width, image_height)), |ui| {
                let image_size = Vec2 { x: image_width, y: image_height - 66. };
                ui.painter().rect(Rect::from_min_size(ui.cursor().min, image_size), Rounding::ZERO, Color32::BLACK, Stroke::default());
                if let Some(texture_id) = self.previous_preview {
                    ui.image(SizedTexture::new(texture_id, image_size));
                }
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
                        Slider::from_get_set(0.0..=self.view_model.component_length().map_or(10., |time| time.value().into_f64()), |value| {
                            if let Some(value) = value {
                                self.view_model.set_seek(MarkerTime::new(MixedFraction::from_f64(value)).unwrap());
                                value
                            } else {
                                self.view_model.seek().value().into_f64()
                            }
                        })
                        .show_value(false),
                    );
                });
            });
        }
    }
}
