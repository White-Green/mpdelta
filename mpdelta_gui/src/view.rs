use egui::{Context, ScrollArea};

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
        egui::SidePanel::right("egui_demo_panel").min_width(150.0).default_width(180.0).show(ctx, |ui| {
            egui::trace!(ui);
            ui.vertical_centered(|ui| {
                ui.heading("✒ egui demos");
            });

            ui.separator();

            ScrollArea::vertical().show(ui, |ui| {
                use egui::special_emojis::{GITHUB, OS_APPLE, OS_LINUX, OS_WINDOWS, TWITTER};

                ui.label("egui is an immediate mode GUI library written in Rust.");

                ui.label(format!("egui runs on the web, or natively on {}{}{}", OS_APPLE, OS_LINUX, OS_WINDOWS,));

                ui.hyperlink_to(format!("{} egui on GitHub", GITHUB), "https://github.com/emilk/egui");

                ui.hyperlink_to(format!("{} @ernerfeldt", TWITTER), "https://twitter.com/ernerfeldt");

                ui.separator();
                ui.separator();
                ui.separator();

                ui.vertical_centered(|ui| {
                    if ui.button("Organize windows").clicked() {
                        ui.ctx().memory().reset_areas();
                    }
                });
            });
        });

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {});

        egui::CentralPanel::default().show(ctx, |_ui| {});
    }
}
