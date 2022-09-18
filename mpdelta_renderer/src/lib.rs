use async_recursion::async_recursion;
use async_trait::async_trait;
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::parameter::{Parameter, ParameterSelect, ParameterType, ParameterValueType};
use mpdelta_core::core::{ComponentRendererBuilder, IdGenerator};
use mpdelta_core::ptr::StaticPointer;
use mpdelta_core::time::TimelineTime;
use mpdelta_core::usecase::RealtimeComponentRenderer;
use std::error::Error;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

pub struct MPDeltaRendererBuilder<ID, Video, Audio> {
    id: Arc<ID>,
    video_renderer_builder: Arc<Video>,
    audio_mixer_builder: Arc<Audio>,
}

impl<ID, Video, Audio> MPDeltaRendererBuilder<ID, Video, Audio> {
    pub fn new(id: Arc<ID>, video_renderer_builder: Arc<Video>, audio_mixer_builder: Arc<Audio>) -> MPDeltaRendererBuilder<ID, Video, Audio> {
        MPDeltaRendererBuilder { id, video_renderer_builder, audio_mixer_builder }
    }
}

#[derive(Debug, Error)]
pub enum TmpError {}

pub struct TmpRenderer;

impl<T: ParameterValueType<'static>> RealtimeComponentRenderer<T> for TmpRenderer {
    fn get_frame_count(&self) -> usize {
        todo!()
    }

    fn render_frame(&self, frame: usize) -> T::Image {
        todo!()
    }

    fn sampling_rate(&self) -> u32 {
        todo!()
    }

    fn mix_audio(&self, offset: usize, length: usize) -> T::Audio {
        todo!()
    }

    fn render_param(&self, param: Parameter<'static, ParameterSelect>) -> Parameter<'static, T> {
        todo!()
    }
}

#[async_trait]
impl<T: ParameterValueType<'static> + 'static, ID: IdGenerator + 'static, Video: Send + Sync + 'static, Audio: Send + Sync + 'static> ComponentRendererBuilder<T> for MPDeltaRendererBuilder<ID, Video, Audio> {
    type Err = TmpError;
    type Renderer = TmpRenderer;

    async fn create_renderer(&self, component: &StaticPointer<RwLock<ComponentInstance<T>>>) -> Result<Self::Renderer, Self::Err> {
        todo!()
    }
}

struct ComponentCache {}

#[async_recursion]
async fn evaluate_component<T>(cache_context: Arc<ComponentCache>, component: StaticPointer<RwLock<ComponentInstance<T>>>, ty: ParameterType, time: TimelineTime) -> Option<() /* 時間毎の値ぜんぶまとめたやつと注目してる時間の値だけのやつがある */> {
    todo!()
}
