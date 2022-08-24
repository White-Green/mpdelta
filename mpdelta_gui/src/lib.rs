use egui::TextureId;

pub mod view;
pub mod viewmodel;

pub trait ImageRegister<T> {
    fn register_image(&mut self, texture: T) -> TextureId;
    fn unregister_image(&mut self, id: TextureId);
}
