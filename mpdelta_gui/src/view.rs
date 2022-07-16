use egui::Context;

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
    fn ui(&mut self, ctx: &Context) {}
}
