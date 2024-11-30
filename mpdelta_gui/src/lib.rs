use egui::TextureId;
use mpdelta_core::time::TimelineTime;
use std::ops::Deref;

pub mod edit_funnel;
pub mod global_ui_state;
mod preview;
mod property_window;
mod timeline;
pub mod view;
pub(crate) mod view_model_util;
pub mod viewmodel;

pub use view::new_gui;

pub trait ImageRegister<T> {
    fn register_image(&mut self, texture: T) -> TextureId;
    fn unregister_image(&mut self, id: TextureId);
}

impl<I, T> ImageRegister<T> for &mut I
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

pub trait AudioTypePlayer<AudioType>: Send + Sync {
    fn set_audio(&self, audio: AudioType);
    fn seek(&self, time: TimelineTime);
    fn play(&self);
    fn pause(&self);
}

impl<AudioType, O> AudioTypePlayer<AudioType> for O
where
    O: Deref + Send + Sync,
    O::Target: AudioTypePlayer<AudioType>,
{
    fn set_audio(&self, audio: AudioType) {
        self.deref().set_audio(audio)
    }

    fn seek(&self, time: TimelineTime) {
        self.deref().seek(time)
    }

    fn play(&self) {
        self.deref().play()
    }

    fn pause(&self) {
        self.deref().pause()
    }
}
