use async_trait::async_trait;
use mpdelta_core::component::parameter::placeholder::{Placeholder, TagAudio, TagImage};
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_renderer::evaluate_component::{AudioNativeTreeNode, ImageNativeTreeNode, ReadonlySourceTree};
use mpdelta_renderer::{VideoRenderer, VideoRendererBuilder};
use std::sync::Arc;
use std::time::Duration;
use vulkano::image::ImageAccess;

pub struct MPDeltaVideoRendererBuilder {}

#[async_trait]
impl<T: ParameterValueType<'static, Image = Arc<dyn ImageAccess>> + 'static> VideoRendererBuilder<T> for MPDeltaVideoRendererBuilder {
    type Renderer = MPDeltaVideoRenderer;

    async fn create_renderer(&self, param: Placeholder<TagImage>, frames_per_second: f64, image_source_tree: ReadonlySourceTree<TagImage, ImageNativeTreeNode<T>>, audio_source_tree: ReadonlySourceTree<TagAudio, AudioNativeTreeNode<T>>) -> Self::Renderer {
        todo!()
    }
}

pub struct MPDeltaVideoRenderer {}

#[async_trait]
impl VideoRenderer<Arc<dyn ImageAccess>> for MPDeltaVideoRenderer {
    async fn render_frame(&mut self, frame: usize, timeout: Duration) -> Arc<dyn ImageAccess> {
        todo!()
    }
}
