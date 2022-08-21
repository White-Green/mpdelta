use crate::viewmodel::{MPDeltaViewModel, ViewModelParams};
use egui::{Context, Frame, ScrollArea, Style};
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::usecase::{
    EditUsecase, GetAvailableComponentClassesUsecase, GetLoadedProjectsUsecase, GetRootComponentClassesUsecase, LoadProjectUsecase, NewProjectUsecase, NewRootComponentClassUsecase, RealtimeRenderComponentUsecase, RedoUsecase, SetOwnerForRootComponentClassUsecase, UndoUsecase, WriteProjectUsecase,
};

pub trait Gui {
    fn ui(&mut self, ctx: &Context);
}

pub struct MPDeltaGUI {
    view_model: MPDeltaViewModel,
}

impl MPDeltaGUI {
    pub fn new<T, Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>(
        view_model_params: ViewModelParams<Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>,
    ) -> MPDeltaGUI
    where
        T: ParameterValueType<'static>,
        Edit: EditUsecase<T>,
        GetAvailableComponentClasses: GetAvailableComponentClassesUsecase<T>,
        GetLoadedProjects: GetLoadedProjectsUsecase<T>,
        GetRootComponentClasses: GetRootComponentClassesUsecase<T>,
        LoadProject: LoadProjectUsecase<T>,
        NewProject: NewProjectUsecase<T>,
        NewRootComponentClass: NewRootComponentClassUsecase<T>,
        RealtimeRenderComponent: RealtimeRenderComponentUsecase<T>,
        Redo: RedoUsecase<T>,
        SetOwnerForRootComponentClass: SetOwnerForRootComponentClassUsecase<T>,
        Undo: UndoUsecase<T>,
        WriteProject: WriteProjectUsecase<T>,
    {
        MPDeltaGUI {
            view_model: MPDeltaViewModel::new::<T, Edit, GetAvailableComponentClasses, GetLoadedProjects, GetRootComponentClasses, LoadProject, NewProject, NewRootComponentClass, RealtimeRenderComponent, Redo, SetOwnerForRootComponentClass, Undo, WriteProject>(view_model_params),
        }
    }
}

impl Gui for MPDeltaGUI {
    fn ui(&mut self, ctx: &Context) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open").clicked() {
                        println!("File/Open Clicked");
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Undo").clicked() {
                        println!("Edit/Undo Clicked");
                    }
                    if ui.button("Redo").clicked() {
                        println!("Edit/Redo Clicked");
                    }
                });
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
