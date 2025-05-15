use crate::edit_funnel::EditFunnelImpl;
use crate::global_ui_state::GlobalUIStateImpl;
use crate::preview::view::Preview;
use crate::preview::viewmodel::{PreviewViewModel, PreviewViewModelImpl};
use crate::property_window::view::PropertyWindow;
use crate::property_window::viewmodel::{PropertyWindowViewModel, PropertyWindowViewModelImpl};
use crate::timeline::view::Timeline;
use crate::timeline::viewmodel::{TimelineViewModel, TimelineViewModelImpl};
use crate::viewmodel::{MainWindowViewModel, MainWindowViewModelImpl, ProjectData, ProjectDataList, RootComponentClassData, RootComponentClassDataList, ViewModelParams};
use crate::ImageRegister;
use egui::{Button, Context};
use mpdelta_core::component::parameter::ParameterValueType;
use std::sync::Arc;

pub trait Gui<T> {
    fn init(&mut self, ctx: &Context);
    fn ui(&mut self, ctx: &Context, image: &mut impl ImageRegister<T>);
}

pub struct MPDeltaGUI<T, VM, PreviewVM, TimelineVM, PropertyWindowVM>
where
    T: ParameterValueType,
    VM: MainWindowViewModel<T>,
    PreviewVM: PreviewViewModel<T>,
    TimelineVM: TimelineViewModel<T>,
    PropertyWindowVM: PropertyWindowViewModel<T>,
{
    view_model: Arc<VM>,
    preview: Preview<T, PreviewVM>,
    timeline: Timeline<T, TimelineVM>,
    property_window: PropertyWindow<T, PropertyWindowVM>,
}

pub fn new_gui<T, P>(view_model_params: P) -> MPDeltaGUI<T, impl MainWindowViewModel<T>, impl PreviewViewModel<T>, impl TimelineViewModel<T>, impl PropertyWindowViewModel<T>>
where
    T: ParameterValueType,
    P: ViewModelParams<T> + 'static,
{
    let global_ui_state = Arc::new(GlobalUIStateImpl::new(&view_model_params));
    let edit_funnel = EditFunnelImpl::new(view_model_params.runtime().clone(), Arc::clone(view_model_params.edit()));
    MPDeltaGUI {
        view_model: MainWindowViewModelImpl::new(&global_ui_state, &view_model_params),
        preview: Preview::new(PreviewViewModelImpl::new(&global_ui_state, &view_model_params)),
        timeline: Timeline::new(TimelineViewModelImpl::new(&global_ui_state, &edit_funnel, &view_model_params)),
        property_window: PropertyWindow::new(PropertyWindowViewModelImpl::new(&global_ui_state, &edit_funnel, &view_model_params)),
    }
}

impl<T, VM, TPreviewViewModel, TTimelineViewModel, TPropertyWindowViewModel> Gui<T::Image> for MPDeltaGUI<T, VM, TPreviewViewModel, TTimelineViewModel, TPropertyWindowViewModel>
where
    T: ParameterValueType,
    VM: MainWindowViewModel<T>,
    TPreviewViewModel: PreviewViewModel<T>,
    TTimelineViewModel: TimelineViewModel<T>,
    TPropertyWindowViewModel: PropertyWindowViewModel<T>,
{
    fn init(&mut self, ctx: &Context) {
        let mut fonts = egui::FontDefinitions::default();

        let font_name = "BIZUDPGothic".to_owned();
        fonts.font_data.insert(font_name.clone(), Arc::new(egui::FontData::from_static(include_bytes!("../fonts/BIZ_UDPGothic/BIZUDPGothic-Regular.ttf"))));
        fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, font_name);

        ctx.set_fonts(fonts);
    }

    fn ui(&mut self, ctx: &Context, image: &mut impl ImageRegister<T::Image>) {
        self.view_model.render_frame(|| {
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
                            self.view_model.open_project();
                            ui.close_menu();
                        }
                        if ui.button("Save").clicked() {
                            self.view_model.save_project();
                            ui.close_menu();
                        }
                        if ui.button("Encode").clicked() {
                            self.view_model.encode();
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
                    self.view_model.projects(|&ProjectDataList { ref list, selected }| {
                        for (i, ProjectData { handle, name }) in list.iter().enumerate() {
                            let button_color = if i == selected { ui.style().visuals.code_bg_color } else { ui.style().visuals.extreme_bg_color };
                            if ui.add(Button::new(name).fill(button_color)).clicked() {
                                self.view_model.select_project(handle);
                            }
                        }
                    });
                    if ui.button("+").clicked() {
                        self.view_model.new_project();
                    }
                });
            });

            egui::TopBottomPanel::top("root_component_tabs").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    self.view_model.root_component_classes(|&RootComponentClassDataList { ref list, selected }| {
                        for (i, RootComponentClassData { handle, name }) in list.iter().enumerate() {
                            let button_color = if i == selected { ui.style().visuals.code_bg_color } else { ui.style().visuals.extreme_bg_color };
                            if ui.add(Button::new(name).fill(button_color)).clicked() {
                                self.view_model.select_root_component_class(handle);
                            }
                        }
                    });
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
                self.timeline.ui(ui);
            });

            egui::SidePanel::left("property").resizable(true).show(ctx, |ui| {
                self.property_window.ui(ui);
            });

            egui::CentralPanel::default().show(ctx, |ui| {
                self.preview.ui(ui, image);
            });
        });
    }
}
