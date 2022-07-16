use egui::{Context, Frame, ScrollArea};

pub trait Gui {
    fn ui(&mut self, ctx: &Context);
}

pub struct MPDeltaGUI {}

impl MPDeltaGUI {
    pub fn new() -> MPDeltaGUI {
        MPDeltaGUI {}
    }
}

impl Gui for MPDeltaGUI {
    fn ui(&mut self, ctx: &Context) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("File").clicked() {
                    println!("File-clicked");
                }
                if ui.button("Edit").clicked() {
                    println!("Edit-clicked");
                }
            });
        });

        egui::TopBottomPanel::bottom("bottom_info_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("mpdelta");
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
