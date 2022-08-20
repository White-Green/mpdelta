use async_recursion::async_recursion;
use async_trait::async_trait;
use cgmath::{Matrix4, SquareMatrix, Vector3, Vector4};
use composite_operation_shader::CompositeOperationConstant;
use dashmap::DashMap;
use either::Either;
use futures::future::BoxFuture;
use futures::stream::{self, StreamExt};
use futures::FutureExt;
use glam::{Mat4, Vec4};
use mpdelta_core::component::marker_pin::MarkerTime;
use mpdelta_core::component::parameter::placeholder::{Placeholder, TagAudio, TagImage};
use mpdelta_core::component::parameter::{BlendMode, CompositeOperation, ImageRequiredParams, ImageRequiredParamsTransformFixed, Parameter, ParameterValueType};
use mpdelta_core::component::processor::{NativeProcessorExecutable, NativeProcessorInput};
use mpdelta_core::native::processor::ParameterNativeProcessorInputFixed;
use mpdelta_core::time::TimelineTime;
use mpdelta_core_vulkano::ImageType;
use mpdelta_renderer::evaluate_component::{AudioNativeTreeNode, ImageNativeTreeNode, ReadonlySourceTree};
use mpdelta_renderer::{VideoRenderer, VideoRendererBuilder};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;
use texture_drawing_shader::TextureDrawingConstant;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;
use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBufferUsage, PrimaryCommandBuffer, RenderPassBeginInfo, SubpassContents};
use vulkano::descriptor_set::layout::DescriptorSetLayout;
use vulkano::descriptor_set::{PersistentDescriptorSet, WriteDescriptorSet};
use vulkano::device::{Device, Queue};
use vulkano::format::ClearValue;
use vulkano::format::Format;
use vulkano::image::view::{ImageView, ImageViewCreateInfo};
use vulkano::image::{AttachmentImage, ImageAccess, ImageAspects, ImageDimensions, ImageSubresourceRange, ImmutableImage, MipmapsCount, StorageImage};
use vulkano::pipeline::graphics::depth_stencil::{CompareOp, DepthStencilState, StencilOp, StencilOpState, StencilOps, StencilState};
use vulkano::pipeline::graphics::input_assembly::{InputAssemblyState, PrimitiveTopology};
use vulkano::pipeline::{ComputePipeline, GraphicsPipeline, PartialStateMode, Pipeline, PipelineBindPoint, StateMode};
use vulkano::render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass};
use vulkano::sampler::{Sampler, SamplerCreateInfo};
use vulkano::shader::ShaderModule;
use vulkano::single_pass_renderpass;
use vulkano::sync::GpuFuture;

#[derive(Debug, Clone)]
struct SharedResource {
    device: Arc<Device>,
    queue: Arc<Queue>,
    render_pass: Arc<RenderPass>,
    texture_drawing_pipeline: Arc<GraphicsPipeline>,
    composite_operation_pipeline: Arc<ComputePipeline>,
}

pub struct MPDeltaVideoRendererBuilder {
    default_image: ImageType,
    shared_resource: Arc<SharedResource>,
}

impl MPDeltaVideoRendererBuilder {
    pub fn new(device: Arc<Device>, queue: Arc<Queue>) -> MPDeltaVideoRendererBuilder {
        let (default_image, image_creation_future) = ImmutableImage::from_iter([0u32], ImageDimensions::Dim2d { width: 1, height: 1, array_layers: 0 }, MipmapsCount::One, Format::R8G8B8A8_UNORM, Arc::clone(&queue)).unwrap();
        let render_pass = single_pass_renderpass!(
            Arc::clone(&device),
            attachments: {
                color: {
                    load: Clear,
                    store: Store,
                    format: Format::R8G8B8A8_UNORM,
                    samples: 1,
                },
                stencil: {
                    load: Clear,
                    store: Store,
                    format: Format::R32_UINT,
                    samples: 1,
                }
            },
            pass: {
                color: [color],
                depth_stencil: {stencil}
            }
        )
        .unwrap();
        let texture_drawing_shader = unsafe { ShaderModule::from_bytes(Arc::clone(&device), include_bytes!(concat!(env!("OUT_DIR"), "/texture_drawing_shader.spv"))).unwrap() };
        let texture_drawing_pipeline = GraphicsPipeline::start()
            .vertex_shader(texture_drawing_shader.entry_point("shader::main_vs").unwrap(), ())
            .fragment_shader(texture_drawing_shader.entry_point("shader::main_fs").unwrap(), ()) //StencilとTopology
            .depth_stencil_state(DepthStencilState {
                depth: None,
                depth_bounds: None,
                stencil: Some(StencilState {
                    enable_dynamic: false,
                    front: StencilOpState {
                        ops: StateMode::Fixed(StencilOps {
                            fail_op: StencilOp::Keep,
                            pass_op: StencilOp::IncrementAndClamp,
                            depth_fail_op: StencilOp::Keep,
                            compare_op: CompareOp::Always,
                        }),
                        compare_mask: StateMode::Fixed(1),
                        write_mask: StateMode::Fixed(1),
                        reference: StateMode::Fixed(1),
                    },
                    back: StencilOpState::default(),
                }),
            })
            .input_assembly_state(InputAssemblyState {
                topology: PartialStateMode::Fixed(PrimitiveTopology::TriangleStrip),
                primitive_restart_enable: StateMode::Fixed(false),
            })
            .build(Arc::clone(&device))
            .unwrap();
        let composite_operation_shader = unsafe { ShaderModule::from_bytes(Arc::clone(&device), include_bytes!(concat!(env!("OUT_DIR"), "/composite_operation_shader.spv"))).unwrap() };
        let composite_operation_pipeline = ComputePipeline::new(Arc::clone(&device), composite_operation_shader.entry_point("shader::main").unwrap(), &(), None, |_| {}).unwrap();
        image_creation_future.then_signal_fence().wait(None).unwrap();
        MPDeltaVideoRendererBuilder {
            default_image: ImageType(default_image),
            shared_resource: Arc::new(SharedResource {
                device,
                queue,
                render_pass,
                texture_drawing_pipeline,
                composite_operation_pipeline,
            }),
        }
    }
}

#[async_trait]
impl<T: ParameterValueType<'static, Image = ImageType> + 'static> VideoRendererBuilder<T> for MPDeltaVideoRendererBuilder {
    type Renderer = MPDeltaVideoRenderer;

    async fn create_renderer(&self, param: Placeholder<TagImage>, frames_per_second: f64, image_source_tree: ReadonlySourceTree<TagImage, ImageNativeTreeNode<T>>) -> Self::Renderer {
        let cache = Arc::new(DashMap::new());
        let (request_sender, request_receiver) = mpsc::unbounded_channel();
        let notifier = Arc::new(Notify::new());
        let handle = tokio::spawn(render_loop(Arc::clone(&self.shared_resource), param, frames_per_second, image_source_tree, Arc::clone(&cache), request_receiver, Arc::clone(&notifier)));
        MPDeltaVideoRenderer {
            handle,
            cache,
            request_sender,
            notifier,
            last: self.default_image.clone(),
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
    cache: Arc<DashMap<usize, ImageType>>,
    request_sender: UnboundedSender<RenderRequest>,
    notifier: Arc<Notify>,
    last: ImageType,
}

#[async_trait]
impl VideoRenderer<ImageType> for MPDeltaVideoRenderer {
    async fn render_frame(&mut self, frame: usize, timeout: Duration) -> ImageType {
        tokio::time::timeout(timeout, async {
            if let Some(frame) = self.cache.get(&frame) {
                return frame.clone();
            }
            let _ = self.request_sender.send(RenderRequest::Render(frame));
            loop {
                self.notifier.notified().await;
                if let Some(frame) = self.cache.get(&frame) {
                    return frame.clone();
                }
            }
        })
        .await
        .unwrap_or_else(|_| self.last.clone())
    }
}

impl Drop for MPDeltaVideoRenderer {
    fn drop(&mut self) {
        let _ = self.request_sender.send(RenderRequest::Shutdown);
    }
}

async fn render_loop<T: ParameterValueType<'static, Image = ImageType> + 'static>(
    shared_resource: Arc<SharedResource>,
    param: Placeholder<TagImage>,
    frames_per_second: f64,
    image_source_tree: ReadonlySourceTree<TagImage, ImageNativeTreeNode<T>>,
    cache: Arc<DashMap<usize, ImageType>>,
    mut request_receiver: UnboundedReceiver<RenderRequest>,
    notifier: Arc<Notify>,
) {
    let mut frame = 0;
    let image_source_tree = Arc::new(image_source_tree);
    loop {
        match request_receiver.try_recv() {
            Ok(RenderRequest::Render(f)) => frame = f,
            Err(TryRecvError::Empty) => {}
            Ok(RenderRequest::Shutdown) | Err(TryRecvError::Disconnected) => break,
        }
        cache.insert(frame, render(Arc::clone(&shared_resource), (1920, 1080), param, Arc::clone(&image_source_tree), TimelineTime::new(frame as f64 / frames_per_second).unwrap()).await.0);
        notifier.notify_one();
        frame += 1;
    }
    while let Some(_) = request_receiver.recv().await {}
}

fn div_ceil(a: u32, b: u32) -> u32 {
    (a + b - 1) / b
}

fn move_mat(pos: Vector3<f64>) -> Matrix4<f64> {
    Matrix4::from_cols(Vector4::unit_x(), Vector4::unit_y(), Vector4::unit_z(), pos.extend(1.))
}

fn scale_mat(scale: Vector3<f64>) -> Matrix4<f64> {
    Matrix4::from_diagonal(scale.extend(1.))
}

fn vec4_into_glam(vec: Vector4<f64>) -> Vec4 {
    Vec4::new(vec.x as f32, vec.y as f32, vec.z as f32, vec.w as f32)
}

fn mat4_into_glam(mat: Matrix4<f64>) -> Mat4 {
    Mat4::from_cols(vec4_into_glam(mat.x), vec4_into_glam(mat.y), vec4_into_glam(mat.z), vec4_into_glam(mat.w))
}

fn render<Audio: Clone + Send + Sync + 'static, T: ParameterValueType<'static, Image = ImageType, Audio = Audio> + 'static>(
    shared_resource: Arc<SharedResource>,
    required_size: (u32, u32),
    param: Placeholder<TagImage>,
    image_source_tree: Arc<ReadonlySourceTree<TagImage, ImageNativeTreeNode<T>>>,
    at: TimelineTime,
) -> BoxFuture<'static, (ImageType, Option<(Arc<AttachmentImage>, BlendMode, CompositeOperation)>)> {
    async move {
        match &*image_source_tree.get(param).unwrap() {
            Either::Right((image_params, native_processor)) => {
                let image_param = image_params.get(at);
                let image_native_size = match (required_size.0 * image_param.aspect_ratio.1).cmp(&(required_size.1 * image_param.aspect_ratio.0)) {
                    Ordering::Less => (div_ceil(image_param.aspect_ratio.0 * required_size.1, image_params.aspect_ratio.1), required_size.1),
                    Ordering::Equal => required_size,
                    Ordering::Greater => (required_size.0, div_ceil(image_param.aspect_ratio.1 * required_size.0, image_params.aspect_ratio.0)),
                };
                let tasks = (0..native_processor.parameter.len())
                    .map(|i| {
                        let params = Arc::clone(&native_processor.parameter);
                        let shared_resource = Arc::clone(&shared_resource);
                        let image_source_tree = Arc::clone(&image_source_tree);
                        tokio::spawn(get_param(params, i, shared_resource, image_native_size, image_source_tree, at))
                    })
                    .collect::<Vec<_>>();
                let transform_mat = match image_param.transform {
                    ImageRequiredParamsTransformFixed::Params { scale, translate, rotate, scale_center, rotate_center } => {
                        scale_mat(Vector3::new(image_native_size.0 as f64 / required_size.0 as f64, image_native_size.1 as f64 / required_size.1 as f64, 1.))
                            * move_mat(-scale_center)
                            * scale_mat(scale)
                            * move_mat(scale_center)
                            * move_mat(-rotate_center)
                            * Matrix4::from(rotate)
                            * move_mat(rotate_center)
                            * move_mat(translate)
                    }
                    ImageRequiredParamsTransformFixed::Free { left_top, right_top, left_bottom, right_bottom } => todo!(),
                };
                let transform_matrix = mat4_into_glam(transform_mat);
                let parameters = stream::iter(tasks).then(|param| async move { param.await.unwrap() }).collect::<Vec<_>>().await;
                let ImageType(image) = native_processor.processor.process(&parameters).into_image().unwrap();
                // imageを空間に貼る
                let mut builder = AutoCommandBufferBuilder::primary(Arc::clone(&shared_resource.device), shared_resource.queue.family(), CommandBufferUsage::OneTimeSubmit).unwrap();
                let image_view = ImageView::new(
                    image,
                    ImageViewCreateInfo {
                        format: Some(Format::R8G8B8A8_UNORM),
                        subresource_range: ImageSubresourceRange {
                            aspects: ImageAspects { color: true, ..ImageAspects::default() },
                            mip_levels: 0..1,
                            array_layers: 0..1,
                        },
                        ..ImageViewCreateInfo::default()
                    },
                )
                .unwrap();
                let result_image = AttachmentImage::new(Arc::clone(&shared_resource.device), [required_size.0, required_size.1], Format::R8G8B8A8_UNORM).unwrap();
                let result_image_view = ImageView::new(
                    Arc::clone(&result_image),
                    ImageViewCreateInfo {
                        format: Some(Format::R8G8B8A8_UNORM),
                        subresource_range: ImageSubresourceRange {
                            aspects: ImageAspects { color: true, ..ImageAspects::default() },
                            mip_levels: 0..1,
                            array_layers: 0..1,
                        },
                        ..ImageViewCreateInfo::default()
                    },
                )
                .unwrap();
                let depth = AttachmentImage::new(Arc::clone(&shared_resource.device), [required_size.0, required_size.1], Format::R32_SFLOAT).unwrap();
                let depth_view = ImageView::new(
                    Arc::clone(&depth),
                    ImageViewCreateInfo {
                        format: Some(Format::R32_UINT),
                        subresource_range: ImageSubresourceRange {
                            aspects: ImageAspects { depth: true, ..ImageAspects::default() },
                            mip_levels: 0..1,
                            array_layers: 0..1,
                        },
                        ..ImageViewCreateInfo::default()
                    },
                )
                .unwrap();
                let frame_buffer = Framebuffer::new(
                    Arc::clone(&shared_resource.render_pass),
                    FramebufferCreateInfo {
                        attachments: vec![result_image_view, depth_view],
                        extent: [required_size.0, required_size.1],
                        layers: 0,
                        ..FramebufferCreateInfo::default()
                    },
                )
                .unwrap();
                builder
                    .begin_render_pass(
                        RenderPassBeginInfo {
                            clear_values: vec![Some(ClearValue::Int([0; 4])), Some(ClearValue::Stencil(0))],
                            ..RenderPassBeginInfo::framebuffer(frame_buffer)
                        },
                        SubpassContents::Inline,
                    )
                    .unwrap();
                builder.bind_pipeline_graphics(Arc::clone(&shared_resource.texture_drawing_pipeline));
                builder.push_constants(Arc::clone(shared_resource.texture_drawing_pipeline.layout()), 0, TextureDrawingConstant { transform_matrix });
                builder.bind_descriptor_sets(
                    PipelineBindPoint::Graphics,
                    Arc::clone(shared_resource.texture_drawing_pipeline.layout()),
                    0,
                    PersistentDescriptorSet::new(
                        Arc::clone(&shared_resource.texture_drawing_pipeline.layout().set_layouts()[0]),
                        [
                            WriteDescriptorSet::image_view(0, image_view),
                            WriteDescriptorSet::sampler(1, Sampler::new(Arc::clone(&shared_resource.device), SamplerCreateInfo::simple_repeat_linear_no_mipmap()).unwrap()),
                        ],
                    )
                    .unwrap(),
                );
                //StencilとTopologyはパイプラインで設定
                builder.draw(4, 1, 0, 0).unwrap();
                builder.end_render_pass().unwrap();
                let command_buffer = builder.build().unwrap();
                let future = command_buffer.execute(Arc::clone(&shared_resource.queue)).unwrap();
                future.then_signal_fence().wait(None).unwrap();
                (ImageType(result_image), Some((depth, image_param.blend_mode, image_param.composite_operation)))
            }
            Either::Left((images, time_shift)) => {
                let at = time_shift.as_ref().map_or(at, |map| {
                    if let Some((timeline_time_range, marker_time_range)) = map.iter().find(|(_, Range { start, end })| start.value() <= at.value() && at.value() < end.value()) {
                        if (marker_time_range.end.value() - marker_time_range.start.value()).abs() < f64::EPSILON {
                            timeline_time_range.start
                        } else {
                            let p = (marker_time_range.start.value() - at.value()) / (marker_time_range.end.value() - marker_time_range.start.value());
                            TimelineTime::new(timeline_time_range.start.value() * p + timeline_time_range.end.value() * (1. - p)).unwrap()
                        }
                    } else {
                        at
                    }
                });
                let tasks = images.iter().map(|&image| tokio::spawn(render(Arc::clone(&shared_resource), required_size, image, Arc::clone(&image_source_tree), at))).collect::<Vec<_>>();
                let result_image = AttachmentImage::new(Arc::clone(&shared_resource.device), [required_size.0, required_size.1], Format::R8G8B8A8_UNORM).unwrap();
                let result_image_view = ImageView::new(
                    Arc::clone(&result_image),
                    ImageViewCreateInfo {
                        format: Some(Format::R8G8B8A8_UNORM),
                        subresource_range: ImageSubresourceRange {
                            aspects: ImageAspects { color: true, ..ImageAspects::default() },
                            mip_levels: 0..1,
                            array_layers: 0..1,
                        },
                        ..ImageViewCreateInfo::default()
                    },
                )
                .unwrap();
                let images = stream::iter(tasks).then(|task| async move { task.await.unwrap() }).collect::<Vec<_>>().await;
                let mut builder = AutoCommandBufferBuilder::primary(Arc::clone(&shared_resource.device), shared_resource.queue.family(), CommandBufferUsage::OneTimeSubmit).unwrap();
                builder.bind_pipeline_compute(Arc::clone(&shared_resource.composite_operation_pipeline));
                builder.bind_descriptor_sets(
                    PipelineBindPoint::Compute,
                    Arc::clone(shared_resource.composite_operation_pipeline.layout()),
                    0,
                    PersistentDescriptorSet::new(Arc::clone(&shared_resource.composite_operation_pipeline.layout().set_layouts()[0]), [WriteDescriptorSet::image_view(0, result_image_view)]).unwrap(),
                );
                for image in images {
                    let (ImageType(image), (stencil, blend, composite)) = (image.0, image.1.unwrap());
                    let image_view = ImageView::new(
                        image,
                        ImageViewCreateInfo {
                            format: Some(Format::R8G8B8A8_UNORM),
                            subresource_range: ImageSubresourceRange {
                                aspects: ImageAspects { color: true, ..ImageAspects::default() },
                                mip_levels: 0..1,
                                array_layers: 0..1,
                            },
                            ..ImageViewCreateInfo::default()
                        },
                    )
                    .unwrap();
                    let stencil_view = ImageView::new(
                        stencil,
                        ImageViewCreateInfo {
                            format: Some(Format::R32_UINT),
                            subresource_range: ImageSubresourceRange {
                                aspects: ImageAspects { stencil: true, ..ImageAspects::default() },
                                mip_levels: 0..1,
                                array_layers: 0..1,
                            },
                            ..ImageViewCreateInfo::default()
                        },
                    )
                    .unwrap();
                    builder.bind_descriptor_sets(
                        PipelineBindPoint::Compute,
                        Arc::clone(shared_resource.composite_operation_pipeline.layout()),
                        1,
                        PersistentDescriptorSet::new(
                            Arc::clone(&shared_resource.composite_operation_pipeline.layout().set_layouts()[1]),
                            [WriteDescriptorSet::image_view(0, image_view), WriteDescriptorSet::image_view(1, stencil_view)],
                        )
                        .unwrap(),
                    );
                    builder.dispatch([div_ceil(required_size.0, 32), div_ceil(required_size.1, 32), 1]).unwrap();
                    builder.push_constants(
                        Arc::clone(shared_resource.composite_operation_pipeline.layout()),
                        0,
                        CompositeOperationConstant {
                            composite: composite as u32,
                            blend: blend as u32,
                            image_width: required_size.0,
                            image_height: required_size.1,
                        },
                    );
                }
                builder.build().unwrap();
                // imagesをぜんぶ合成する
                (ImageType(result_image), None)
            }
        }
    }
    .boxed()
}

async fn get_param<T: ParameterValueType<'static, Image = ImageType> + 'static>(
    params: Arc<[Parameter<'static, NativeProcessorInput>]>,
    index: usize,
    shared_resource: Arc<SharedResource>,
    required_size: (u32, u32),
    image_source_tree: Arc<ReadonlySourceTree<TagImage, ImageNativeTreeNode<T>>>,
    at: TimelineTime,
) -> ParameterNativeProcessorInputFixed<ImageType, T::Audio> {
    match &params[index] {
        Parameter::None => Parameter::None,
        Parameter::Image(image) => Parameter::Image(render(shared_resource, required_size, *image, image_source_tree, at).await.0),
        Parameter::Audio(_) => todo!(),
        Parameter::Video(_) => todo!(),
        Parameter::File(value) => Parameter::File(value.get(at).unwrap().clone()),
        Parameter::String(value) => Parameter::String(value.get(at).unwrap().clone()),
        Parameter::Select(value) => Parameter::Select(value.get(at).unwrap().clone()),
        Parameter::Boolean(value) => Parameter::Boolean(value.get(at).unwrap().clone()),
        Parameter::Radio(value) => Parameter::Radio(value.get(at).unwrap().clone()),
        Parameter::Integer(value) => Parameter::Integer(value.get(at).unwrap().clone()),
        Parameter::RealNumber(value) => Parameter::RealNumber(value.get(at).unwrap().clone()),
        Parameter::Vec2(value) => Parameter::Vec2(value.get(at).unwrap().clone()),
        Parameter::Vec3(value) => Parameter::Vec3(value.get(at).unwrap().clone()),
        Parameter::Dictionary(_) => todo!(),
        Parameter::ComponentClass(_) => unreachable!(),
    }
}
