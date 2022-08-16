use async_trait::async_trait;
use dashmap::DashMap;
use mpdelta_core::component::parameter::placeholder::{Placeholder, TagAudio, TagImage};
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_renderer::evaluate_component::{AudioNativeTreeNode, ImageNativeTreeNode, ReadonlySourceTree};
use mpdelta_renderer::{VideoRenderer, VideoRendererBuilder};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;
use vulkano::image::ImageAccess;

pub struct MPDeltaVideoRendererBuilder {
    default_image: Arc<dyn ImageAccess>,
}

#[async_trait]
impl<T: ParameterValueType<'static, Image = Arc<dyn ImageAccess>> + 'static> VideoRendererBuilder<T> for MPDeltaVideoRendererBuilder {
    type Renderer = MPDeltaVideoRenderer;

    async fn create_renderer(&self, param: Placeholder<TagImage>, frames_per_second: f64, image_source_tree: ReadonlySourceTree<TagImage, ImageNativeTreeNode<T>>) -> Self::Renderer {
        let cache = Arc::new(DashMap::new());
        let (request_sender, request_receiver) = mpsc::unbounded_channel();
        let notifier = Arc::new(Notify::new());
        let handle = tokio::spawn(render_loop(param, frames_per_second, image_source_tree, Arc::clone(&cache), request_receiver, Arc::clone(&notifier)));
        MPDeltaVideoRenderer {
            handle,
            cache,
            request_sender,
            notifier,
            last: Arc::clone(&self.default_image),
        }
    }
}

#[derive(Debug)]
enum RenderRequest {
    Render(usize),
    Shutdown,
}

pub struct MPDeltaVideoRenderer {
    handle: JoinHandle<()>,
    cache: Arc<DashMap<usize, Arc<dyn ImageAccess>>>,
    request_sender: UnboundedSender<RenderRequest>,
    notifier: Arc<Notify>,
    last: Arc<dyn ImageAccess>,
}

#[async_trait]
impl VideoRenderer<Arc<dyn ImageAccess>> for MPDeltaVideoRenderer {
    async fn render_frame(&mut self, frame: usize, timeout: Duration) -> Arc<dyn ImageAccess> {
        tokio::time::timeout(timeout, async {
            if let Some(frame) = self.cache.get(&frame) {
                return Arc::clone(&*frame);
            }
            let _ = self.request_sender.send(RenderRequest::Render(frame));
            loop {
                self.notifier.notified().await;
                if let Some(frame) = self.cache.get(&frame) {
                    return Arc::clone(&*frame);
                }
            }
        })
        .await
        .unwrap_or_else(|_| Arc::clone(&self.last))
    }
}

impl Drop for MPDeltaVideoRenderer {
    fn drop(&mut self) {
        let _ = self.request_sender.send(RenderRequest::Shutdown);
        self.request_sender.closed();
    }
}

async fn render_loop<T>(param: Placeholder<TagImage>, frames_per_second: f64, image_source_tree: ReadonlySourceTree<TagImage, ImageNativeTreeNode<T>>, cache: Arc<DashMap<usize, Arc<dyn ImageAccess>>>, mut request_receiver: UnboundedReceiver<RenderRequest>, notifier: Arc<Notify>) {
    let mut frame = 0;
    let image_source_tree = Arc::new(image_source_tree);
    loop {
        match request_receiver.try_recv() {
            Ok(RenderRequest::Render(f)) => frame = f,
            Err(TryRecvError::Empty) => {}
            Ok(RenderRequest::Shutdown) | Err(TryRecvError::Disconnected) => break,
        }
        cache.insert(frame, render(param, Arc::clone(&image_source_tree), frame as f64 / frames_per_second).await);
        notifier.notify_one();
        frame += 1;
    }
    while let Some(_) = request_receiver.recv().await {}
}

async fn render<T>(param: Placeholder<TagImage>, image_source_tree: Arc<ReadonlySourceTree<TagImage, ImageNativeTreeNode<T>>>, at: f64) -> Arc<dyn ImageAccess> {
    todo!()
}
