use crate::viewmodel::{MPDeltaViewModel, ViewModelParams};
use crate::ImageRegister;
use egui::style::Margin;
use egui::{Area, Button, Context, Frame, Id, Label, Pos2, Rect, ScrollArea, Sense, Style, TextureId, Vec2, Visuals, Widget, Window};
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::usecase::{
    EditUsecase, GetAvailableComponentClassesUsecase, GetLoadedProjectsUsecase, GetRootComponentClassesUsecase, LoadProjectUsecase, NewProjectUsecase, NewRootComponentClassUsecase, RealtimeComponentRenderer, RealtimeRenderComponentUsecase, RedoUsecase, SetOwnerForRootComponentClassUsecase,
    UndoUsecase, WriteProjectUsecase,
};
use std::hash::SipHasher;

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
                    for item in self.view_model.component_instances().iter() {
                        let (handle, rect) = item.pair();
                        let rectangle = Rect::from_min_size(Pos2::new(rect.time.start * 100., rect.layer * 60.), Vec2::new((rect.time.end - rect.time.start) * 100., 50.));
                        ui.allocate_ui_at_rect(Rect::from_min_size(base_point + rectangle.min.to_vec2(), rectangle.size()), |ui| {
                            Frame::group(&Style::default()).inner_margin(Margin::default()).show(ui, |ui| {
                                let (rect, response) = ui.allocate_exact_size(rectangle.size(), Sense::drag());
                                ui.allocate_ui_at_rect(rect, |ui| {
                                    ui.label("timeline");
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
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(img) = self.previous_preview {
                image.unregister_image(img);
            }
            if let Some(img) = self.view_model.get_preview_image() {
                let texture_id = image.register_image(img);
                let Vec2 { x, y } = ui.available_size();
                let (x, y) = (x.min(y * 16. / 9.), y.min(x * 9. / 16.));
                ui.image(texture_id, Vec2 { x, y });
                self.previous_preview = Some(texture_id);
            } else {
                self.previous_preview = None;
            }
        });
    }
}
