use crate::viewmodel::{MPDeltaViewModel, ViewModelParams};
use crate::ImageRegister;
use cgmath::Vector3;
use egui::epaint::Shadow;
use egui::style::Margin;
use egui::{Area, Button, Color32, Context, Direction, Frame, Id, Label, Layout, Pos2, Rect, Rounding, ScrollArea, Sense, Slider, Stroke, Style, TextEdit, TextureId, Vec2, Visuals, Widget, Window};
use mpdelta_core::component::parameter::{ImageRequiredParamsTransform, Opacity, ParameterValueType, VariableParameterValue};
use mpdelta_core::time::TimelineTime;
use mpdelta_core::usecase::{
    EditUsecase, GetAvailableComponentClassesUsecase, GetLoadedProjectsUsecase, GetRootComponentClassesUsecase, LoadProjectUsecase, NewProjectUsecase, NewRootComponentClassUsecase, RealtimeComponentRenderer, RealtimeRenderComponentUsecase, RedoUsecase, SetOwnerForRootComponentClassUsecase,
    UndoUsecase, WriteProjectUsecase,
};
use once_cell::sync::Lazy;
use regex::Regex;
use std::hash::SipHasher;
use std::str::FromStr;
use std::sync::Arc;

pub trait Gui<T> {
    fn ui(&mut self, ctx: &Context, image: &mut impl ImageRegister<T>);
}

pub struct MPDeltaGUI<T, R> {
    view_model: MPDeltaViewModel<T, R>,
    previous_preview: Option<TextureId>,
}

impl<T> MPDeltaGUI<T, ()> {
    pub fn new<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>(
        view_model_params: ViewModelParams<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>,
    ) -> MPDeltaGUI<T, RealtimeRenderComponent::Renderer>
    where
        T: ParameterValueType<'static>,
        Edit: EditUsecase<T> + Send + Sync + 'static,
        GetAvailableComponentClasses: GetAvailableComponentClassesUsecase<T> + Send + Sync + 'static,
        GetLoadedProjects: GetLoadedProjectsUsecase<T> + Send + Sync + 'static,
        GetRootComponentClasses: GetRootComponentClassesUsecase<T> + Send + Sync + 'static,
        LoadProject: LoadProjectUsecase<T> + Send + Sync + 'static,
        NewProject: NewProjectUsecase<T> + Send + Sync + 'static,
        NewRootComponentClass: NewRootComponentClassUsecase<T> + Send + Sync + 'static,
        RealtimeRenderComponent: RealtimeRenderComponentUsecase<T> + Send + Sync + 'static,
        RealtimeRenderComponent::Renderer: Send + Sync + 'static,
        Redo: RedoUsecase<T> + Send + Sync + 'static,
        SetOwnerForRootComponentClass: SetOwnerForRootComponentClassUsecase<T> + Send + Sync + 'static,
        Undo: UndoUsecase<T> + Send + Sync + 'static,
        WriteProject: WriteProjectUsecase<T> + Send + Sync + 'static,
    {
        MPDeltaGUI {
            view_model: MPDeltaViewModel::new::<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>(view_model_params),
            previous_preview: None,
        }
    }
}

impl<T: ParameterValueType<'static>, R: RealtimeComponentRenderer<T>> Gui<T::Image> for MPDeltaGUI<T, R> {
    fn ui(&mut self, ctx: &Context, image: &mut impl ImageRegister<T::Image>) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Project").clicked() {
                        println!("File/New Project Clicked");
                        self.view_model.new_project();
                        ui.close_menu();
                    }
                    if ui.button("Open").clicked() {
                        println!("File/Open Clicked");
                        ui.close_menu();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Undo").clicked() {
                        println!("Edit/Undo Clicked");
                        ui.close_menu();
                    }
                    if ui.button("Redo").clicked() {
                        println!("Edit/Redo Clicked");
                        ui.close_menu();
                    }
                });
            });
        });

        egui::TopBottomPanel::top("project_tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let selected = self.view_model.selected_project();
                for (i, (_, name)) in self.view_model.projects().iter().enumerate() {
                    if ui.add(Button::new(name).fill(if selected == i { ui.style().visuals.code_bg_color } else { ui.style().visuals.extreme_bg_color })).clicked() {
                        self.view_model.select_project(i);
                    }
                }
                if ui.button("+").clicked() {
                    self.view_model.new_project();
                }
            });
        });

        egui::TopBottomPanel::top("root_component_tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let selected = self.view_model.selected_root_component_class();
                for (i, (_, name)) in self.view_model.root_component_classes().iter().enumerate() {
                    if ui.add(Button::new(name).fill(if selected == i { ui.style().visuals.code_bg_color } else { ui.style().visuals.extreme_bg_color })).clicked() {
                        self.view_model.select_root_component_class(i);
                    }
                }
                if ui.button("+").clicked() {
                    self.view_model.new_root_component_class();
                }
            });
        });

        egui::TopBottomPanel::bottom("bottom_info_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(concat!("mpdelta - ", env!("CARGO_PKG_VERSION")));
                egui::warn_if_debug_build(ui);
            })
        });

        egui::TopBottomPanel::bottom("timeline").resizable(true).show(ctx, |ui| {
            ScrollArea::both().show(ui, |ui| {
                let (rect, response) = ui.allocate_at_least(ui.available_size(), Sense::click());
                ui.allocate_ui_at_rect(rect, |ui| {
                    let base_point = ui.cursor().min;
                    let selected = self.view_model.selected_component_instance();
                    let selected = selected.as_deref();
                    for item in self.view_model.component_instances().iter() {
                        let (handle, rect) = item.pair();
                        let rectangle = Rect::from_min_size(Pos2::new(rect.time.start.value() as f32 * 100., rect.layer * 60.), Vec2::new((rect.time.end.value() - rect.time.start.value()) as f32 * 100., 50.));
                        ui.allocate_ui_at_rect(Rect::from_min_size(base_point + rectangle.min.to_vec2(), rectangle.size()), |ui| {
                            let frame = Frame::group(&Style::default()).inner_margin(Margin::default());
                            let frame = match selected {
                                Some(selected) if handle == selected => frame.shadow(Shadow::big_light()),
                                _ => frame,
                            };
                            frame.show(ui, |ui| {
                                let (rect, response) = ui.allocate_exact_size(rectangle.size(), Sense::drag());
                                ui.allocate_ui_at_rect(rect, |ui| {
                                    ui.label("Rectangle");
                                });
                                if response.clicked() {
                                    self.view_model.click_component_instance(handle);
                                }
                                let delta = response.drag_delta();
                                if delta != Vec2::default() {
                                    self.view_model.drag_component_instance(handle, Vec2::new(delta.x / 100., delta.y / 60.));
                                }
                            });
                        });
                    }
                    let pins = &*self.view_model.marker_pins();
                    for (link_ref, link, len_str) in self.view_model.component_links().iter() {
                        let from = if let Some(from) = pins.get(&link.from) {
                            from
                        } else {
                            continue;
                        };
                        let to = if let Some(to) = pins.get(&link.to) {
                            to
                        } else {
                            continue;
                        };
                        let from = &*from;
                        let to = &*to;
                        if selected.is_some() && (from.0.as_ref() == selected || to.0.as_ref() == selected) {
                            ui.painter()
                                .hline(base_point.x + (from.2.value() * 100.) as f32..=base_point.x + (to.2.value() * 100.) as f32, base_point.y + from.1.max(to.1) * 60. + 55., Stroke::new(1., ui.visuals().text_color()));
                            ui.allocate_ui_at_rect(Rect::from_min_size(base_point + Vec2::new((from.2.value() * 100.) as f32, to.1.max(to.1) * 60. + 57.), Vec2::new(20., 100.)), |ui| {
                                let len = &**len_str.load();
                                let mut s = len.clone();
                                ui.add(TextEdit::singleline(&mut s));
                                s.retain(|c| c.is_ascii_digit() || c == '.');
                                static REGEX: Lazy<Regex> = Lazy::new(|| Regex::new("^\\d+(?:\\.\\d+)?$").unwrap());
                                if s != *len {
                                    len_str.store(Arc::new(s.clone()));
                                    if REGEX.is_match(&s) {
                                        if let Ok(new_value) = f64::from_str(&s) {
                                            if let Some(new_value) = TimelineTime::new(new_value) {
                                                self.view_model.edit_marker_link_length(link_ref.clone(), new_value);
                                            }
                                        }
                                    }
                                }
                            });
                        }
                    }
                });
                response.context_menu(|ui| {
                    if ui.button("add").clicked() {
                        self.view_model.add_component_instance();
                        ui.close_menu();
                    }
                });
            });
        });

        egui::SidePanel::left("property").resizable(true).show(ctx, |ui| {
            ScrollArea::both().show(ui, |ui| {
                let (rect, _) = ui.allocate_at_least(ui.available_size(), Sense::click());
                ui.allocate_ui_at_rect(rect, |ui| {
                    ui.label("Component Properties");
                    let mut image_required_params = self.view_model.image_required_params().blocking_lock();
                    let mut edited = false;
                    if let Some(image_required_params) = image_required_params.as_mut() {
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
                            ui.label("opacity");
                            ui.add(Slider::from_get_set(0.0..=1.0, |new_value| {
                                let current_value = image_required_params.opacity.get_value_mut(0).unwrap().1;
                                if let Some(value) = new_value {
                                    let value = Opacity::new(value).unwrap_or(Opacity::OPAQUE);
                                    current_value.from = value;
                                    current_value.to = value;
                                    edited = true;
                                    value.value()
                                } else {
                                    current_value.from.value()
                                }
                            }));
                        }
                    }
                    self.view_model.updated_image_required_params();
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(img) = self.previous_preview {
                image.unregister_image(img);
            }
            if let Some(img) = self.view_model.get_preview_image() {
                let texture_id = image.register_image(img);
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
                        } else if ui.button("▶").clicked() {
                            self.view_model.play();
                        }
                        let button_width = ui.cursor().min.x - start;
                        ui.style_mut().spacing.slider_width = image_width - button_width;
                        ui.add_enabled(!self.view_model.playing(), Slider::new(self.view_model.seek(), 0..=599).show_value(false));
                    });
                });
                self.previous_preview = Some(texture_id);
            } else {
                if self.view_model.projects().get(self.view_model.selected_project()).is_none() {
                    if ui.button("create new project").clicked() {
                        self.view_model.new_project()
                    }
                } else if self.view_model.root_component_classes().get(self.view_model.selected_root_component_class()).is_none() {
                    if ui.button("add new root component class").clicked() {
                        self.view_model.new_root_component_class();
                    }
                }
                self.previous_preview = None;
            }
        });
    }
}
