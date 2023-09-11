use egui::TextureId;

pub mod edit_funnel;
pub mod global_ui_state;
pub(crate) mod message_router;
mod preview;
mod property_window;
mod timeline;
pub mod view;
pub(crate) mod view_model_util;
pub mod viewmodel;

pub trait ImageRegister<T> {
    fn register_image(&mut self, texture: T) -> TextureId;
    fn unregister_image(&mut self, id: TextureId);
}

impl<'a, I, T> ImageRegister<T> for &'a mut I
where
    I: ImageRegister<T> + ?Sized,
{
    fn register_image(&mut self, texture: T) -> TextureId {
        I::register_image(self, texture)
    }

    fn unregister_image(&mut self, id: TextureId) {
        I::unregister_image(self, id)
    }
}
