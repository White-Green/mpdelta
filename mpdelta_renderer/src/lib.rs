use crate::evaluate_component::{evaluate_component, AudioNativeTreeNode, ImageNativeTreeNode, ReadonlySourceTree, RendererError, SourceTree};

use async_trait::async_trait;

use futures::prelude::future::FutureExt;
use futures::prelude::stream::{self, StreamExt};

use mpdelta_core::component::instance::ComponentInstance;

use mpdelta_core::component::parameter::placeholder::{Placeholder, TagAudio, TagImage};

use mpdelta_core::component::parameter::{Parameter, ParameterSelect, ParameterType, ParameterValueType};

use mpdelta_core::core::{ComponentRendererBuilder, IdGenerator};

use mpdelta_core::ptr::StaticPointer;
use mpdelta_core::time::TimelineTime;
use mpdelta_core::usecase::RealtimeComponentRenderer;

use std::sync::Arc;
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::sync::RwLock;

pub mod evaluate_component;

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

#[async_trait]
impl<T: ParameterValueType<'static> + 'static, ID: IdGenerator + 'static, Video: VideoRendererBuilder<T> + Send + Sync + 'static, Audio: AudioMixerBuilder<T> + Send + Sync + 'static> ComponentRendererBuilder<T> for MPDeltaRendererBuilder<ID, Video, Audio> {
    type Err = RendererError;
    type Renderer = MPDeltaRenderer<Video::Renderer, Audio::Mixer>;

    async fn create_renderer(&self, component: &StaticPointer<RwLock<ComponentInstance<T>>>) -> Result<Self::Renderer, Self::Err> {
        let image_evaluate = tokio::spawn({
            let component = component.clone();
            let image_source_tree = Arc::new(SourceTree::new(Arc::clone(&self.id)));
            let audio_source_tree = Arc::new(SourceTree::new(Arc::clone(&self.id)));
            let video_renderer_builder = Arc::clone(&self.video_renderer_builder);
            evaluate_component(component, ParameterType::Image(()), Arc::clone(&image_source_tree), Arc::clone(&audio_source_tree), Box::new((0u64..).map(|t| TimelineTime::new(t as f64 / 60.).unwrap()))).then(|result| async move {
                let (left, result, right) = result?;
                let left = left.upgrade().unwrap();
                let right = right.upgrade().unwrap();
                let (left, right) = futures::join!(left.read(), right.read());
                let length_secs = right.cached_timeline_time().value() - left.cached_timeline_time().value();
                Ok::<_, RendererError>((video_renderer_builder.create_renderer(result.into_image().unwrap(), 60., Arc::try_unwrap(image_source_tree).unwrap_or_else(|_| unreachable!()).into_readonly()).await, length_secs))
            })
        });
        let audio_evaluate = tokio::spawn({
            let component = component.clone();
            let image_source_tree = Arc::new(SourceTree::new(Arc::clone(&self.id)));
            let audio_source_tree = Arc::new(SourceTree::new(Arc::clone(&self.id)));
            let audio_mixer_builder = Arc::clone(&self.audio_mixer_builder);
            evaluate_component(component, ParameterType::Audio(()), Arc::clone(&image_source_tree), Arc::clone(&audio_source_tree), Box::new((0u64..).map(|t| TimelineTime::new(t as f64 / 60.).unwrap())))
                .then(|result| async move { Ok::<_, RendererError>(audio_mixer_builder.create_mixer(result?.1.into_audio().unwrap(), 48_000, Arc::try_unwrap(audio_source_tree).unwrap_or_else(|_| unreachable!()).into_readonly()).await) })
        });
        let (video_renderer, audio_mixer) = futures::join!(image_evaluate.map(|join_result| join_result.unwrap()), audio_evaluate.map(|join_result| join_result.unwrap()));
        let (video_renderer, length_secs) = video_renderer?;
        let audio_mixer = audio_mixer?;
        // TODO: ここマジックナンバー
        Ok(MPDeltaRenderer {
            runtime: Handle::current(),
            frame_count: (length_secs * 60.) as usize,
            video_renderer: RwLock::new(video_renderer),
            sampling_rate: 48_000,
            audio_mixer: RwLock::new(audio_mixer),
        })
    }
}

#[async_trait]
pub trait VideoRendererBuilder<T: ParameterValueType<'static>> {
    type Renderer: VideoRenderer<T::Image> + Send + Sync;
    async fn create_renderer(&self, param: Placeholder<TagImage>, frames_per_second: f64, image_source_tree: ReadonlySourceTree<TagImage, ImageNativeTreeNode<T>>) -> Self::Renderer;
}

#[async_trait]
pub trait AudioMixerBuilder<T: ParameterValueType<'static>> {
    type Mixer: AudioMixer<T::Audio> + Send + Sync;
    async fn create_mixer(&self, param: Placeholder<TagAudio>, sampling_rate: u32, audio_source_tree: ReadonlySourceTree<TagAudio, AudioNativeTreeNode<T>>) -> Self::Mixer;
}

#[async_trait]
impl<T: ParameterValueType<'static>> AudioMixerBuilder<T> for () {
    type Mixer = ();

    async fn create_mixer(&self, _: Placeholder<TagAudio>, _: u32, _: ReadonlySourceTree<TagAudio, AudioNativeTreeNode<T>>) -> Self::Mixer {
        ()
    }
}

#[async_trait]
impl<T> AudioMixer<T> for () {
    async fn mix_audio(&mut self, _: usize, _: usize, _: Duration) -> T {
        unimplemented!()
    }
}

pub struct MPDeltaRenderer<Video, Audio> {
    runtime: Handle,
    frame_count: usize,
    video_renderer: RwLock<Video>,
    sampling_rate: u32,
    audio_mixer: RwLock<Audio>,
}

impl<T: ParameterValueType<'static>, Video: VideoRenderer<T::Image>, Audio: AudioMixer<T::Audio>> RealtimeComponentRenderer<T> for MPDeltaRenderer<Video, Audio> {
    fn get_frame_count(&self) -> usize {
        self.frame_count
    }

    fn render_frame(&self, frame: usize) -> T::Image {
        self.runtime.block_on(async { self.video_renderer.write().await.render_frame(frame, Duration::MAX).await })
    }

    fn sampling_rate(&self) -> u32 {
        self.sampling_rate
    }

    fn mix_audio(&self, offset: usize, length: usize) -> T::Audio {
        self.runtime.block_on(async { self.audio_mixer.write().await.mix_audio(offset, length, Duration::MAX).await })
    }

    fn render_param(&self, _param: Parameter<'static, ParameterSelect>) -> Parameter<'static, T> {
        todo!()
    }
}

#[async_trait]
pub trait VideoRenderer<Image> {
    async fn render_frame(&mut self, frame: usize, timeout: Duration) -> Image;
}

#[async_trait]
pub trait AudioMixer<Audio> {
    async fn mix_audio(&mut self, offset: usize, length: usize, timeout: Duration) -> Audio;
}
