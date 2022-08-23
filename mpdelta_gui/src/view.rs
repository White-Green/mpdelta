use crate::viewmodel::{MPDeltaViewModel, ViewModelParams};
use egui::{Button, Context, Frame, ScrollArea, Style};
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::usecase::{
    EditUsecase, GetAvailableComponentClassesUsecase, GetLoadedProjectsUsecase, GetRootComponentClassesUsecase, LoadProjectUsecase, NewProjectUsecase, NewRootComponentClassUsecase, RealtimeRenderComponentUsecase, RedoUsecase, SetOwnerForRootComponentClassUsecase, UndoUsecase, WriteProjectUsecase,
};

pub trait Gui {
    fn ui(&mut self, ctx: &Context);
}

pub struct MPDeltaGUI<T> {
    view_model: MPDeltaViewModel<T>,
}

impl<T> MPDeltaGUI<T> {
    pub fn new<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>(
        view_model_params: ViewModelParams<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>,
    ) -> MPDeltaGUI<T>
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
        Redo: RedoUsecase<T> + Send + Sync + 'static,
        SetOwnerForRootComponentClass: SetOwnerForRootComponentClassUsecase<T> + Send + Sync + 'static,
        Undo: UndoUsecase<T> + Send + Sync + 'static,
        WriteProject: WriteProjectUsecase<T> + Send + Sync + 'static,
    {
        MPDeltaGUI {
            view_model: MPDeltaViewModel::new::<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>(view_model_params),
        }
    }
}

impl<T> Gui for MPDeltaGUI<T> {
    fn ui(&mut self, ctx: &Context) {
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
            ui.label("timeline");
        });

        egui::SidePanel::left("property").resizable(true).show(ctx, |ui| {
            ui.label("Component Properties");
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Preview");
        });
    }
}
