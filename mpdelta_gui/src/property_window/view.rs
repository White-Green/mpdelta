use crate::property_window::viewmodel::PropertyWindowViewModel;
use cgmath::Vector3;
use egui::{Sense, Slider, Ui};
use mpdelta_core::component::parameter::{ImageRequiredParamsTransform, ParameterValueType, VariableParameterValue};
use std::marker::PhantomData;
use std::sync::Arc;

pub struct PropertyWindow<K, T, VM> {
    view_model: Arc<VM>,
    _phantom: PhantomData<(K, T)>,
}

impl<K: 'static, T: ParameterValueType, VM: PropertyWindowViewModel<K, T>> PropertyWindow<K, T, VM> {
    pub fn new(view_model: Arc<VM>) -> PropertyWindow<K, T, VM> {
        PropertyWindow { view_model, _phantom: PhantomData }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let (rect, _) = ui.allocate_at_least(ui.available_size(), Sense::click());
        ui.allocate_ui_at_rect(rect, |ui| {
            ui.label("Component Properties");
            let image_required_params = &mut *self.view_model.image_required_params();
            let mut edited = false;
            if let Some(image_required_params) = image_required_params {
                if let ImageRequiredParamsTransform::Params {
                    scale: Vector3 {
                        x: VariableParameterValue::Manually(scale_x),
                        y: VariableParameterValue::Manually(scale_y),
                        ..
                    },
                    translate: Vector3 {
                        x: VariableParameterValue::Manually(translate_x),
                        y: VariableParameterValue::Manually(translate_y),
                        ..
                    },
                    ..
                } = &mut image_required_params.transform
                {
                    ui.label("position - X");
                    ui.add(Slider::from_get_set(-3.0..=3.0, |new_value| {
                        let current_value = translate_x.get_value_mut(0).unwrap().1;
                        if let Some(value) = new_value {
                            current_value.from = value;
                            current_value.to = value;
                            edited = true;
                            value
                        } else {
                            current_value.from
                        }
                    }));
                    ui.label("position - Y");
                    ui.add(Slider::from_get_set(-3.0..=3.0, |new_value| {
                        let current_value = translate_y.get_value_mut(0).unwrap().1;
                        if let Some(value) = new_value {
                            current_value.from = value;
                            current_value.to = value;
                            edited = true;
                            value
                        } else {
                            current_value.from
                        }
                    }));
                    ui.label("scale - X");
                    ui.add(Slider::from_get_set(0.0..=2.0, |new_value| {
                        let current_value = scale_x.get_value_mut(0).unwrap().1;
                        if let Some(value) = new_value {
                            current_value.from = value;
                            current_value.to = value;
                            edited = true;
                            value
                        } else {
                            current_value.from
                        }
                    }));
                    ui.label("scale - Y");
                    ui.add(Slider::from_get_set(0.0..=2.0, |new_value| {
                        let current_value = scale_y.get_value_mut(0).unwrap().1;
                        if let Some(value) = new_value {
                            current_value.from = value;
                            current_value.to = value;
                            edited = true;
                            value
                        } else {
                            current_value.from
                        }
                    }));
                }
            }
            if edited {
                self.view_model.updated_image_required_params(image_required_params.as_ref().unwrap());
            }
        });
    }
}
