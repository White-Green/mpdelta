use egui::{Context, Frame, ScrollArea, Style};

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
