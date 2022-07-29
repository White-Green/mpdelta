use async_trait::async_trait;
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::parameter::{Parameter, ParameterSelect, ParameterValueType};
use mpdelta_core::core::ComponentRendererBuilder;
use mpdelta_core::ptr::StaticPointer;
use mpdelta_core::usecase::RealtimeComponentRenderer;
use tokio::sync::RwLock;

pub struct MPDeltaRendererBuilder {}

#[async_trait]
impl<T: ParameterValueType<'static>> ComponentRendererBuilder<T> for MPDeltaRendererBuilder {
    type Renderer = MPDeltaRenderer;

    async fn create_renderer(&self, component: &StaticPointer<RwLock<ComponentInstance<T>>>) -> Self::Renderer {
        todo!()
    }
}

async fn freeze_component<T>(component: &StaticPointer<RwLock<ComponentInstance<T>>>) -> () {}

pub struct MPDeltaRenderer {}

#[async_trait]
impl<T: ParameterValueType<'static>> RealtimeComponentRenderer<T> for MPDeltaRenderer {
    fn get_frame_count(&self) -> usize {
        todo!()
    }

    async fn render_frame(&mut self, frame: usize) -> T::Image {
        todo!()
    }

    fn sampling_rate(&self) -> u32 {
        todo!()
    }

    async fn mix_audio(&mut self, offset: usize, length: usize) -> T::Audio {
        todo!()
    }

    async fn render_param(&mut self, param: Parameter<'static, ParameterSelect>) -> Parameter<'static, T> {
        todo!()
    }
}
