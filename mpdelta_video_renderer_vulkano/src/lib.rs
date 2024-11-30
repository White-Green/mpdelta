use cgmath::{Matrix4, SquareMatrix, Vector3, Vector4};
use futures::future::FutureExt;
use glam::{Mat4, Vec4};
use mpdelta_core::component::parameter::{ImageRequiredParamsFixed, ImageRequiredParamsTransformFixed};
use mpdelta_core_vulkano::ImageType;
use mpdelta_renderer::{Combiner, CombinerBuilder, ImageCombinerParam, ImageCombinerRequest, ImageSizeRequest};
use shader_composite_operation::CompositeOperationConstant;
use shader_texture_drawing::TextureDrawingConstant;
use smallvec::smallvec;
use std::cmp::Ordering;
use std::future::Future;
use std::sync::Arc;
use vulkano::command_buffer::allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo};
use vulkano::command_buffer::{AutoCommandBufferBuilder, ClearColorImageInfo, CommandBufferUsage, PrimaryCommandBufferAbstract, RenderPassBeginInfo, SubpassBeginInfo, SubpassEndInfo};
use vulkano::descriptor_set::allocator::{StandardDescriptorSetAllocator, StandardDescriptorSetAllocatorCreateInfo};
use vulkano::descriptor_set::{PersistentDescriptorSet, WriteDescriptorSet};
use vulkano::device::{Device, Queue};
use vulkano::format::{ClearColorValue, ClearValue, Format};
use vulkano::image::sampler::{Sampler, SamplerCreateInfo};
use vulkano::image::view::{ImageView, ImageViewCreateInfo};
use vulkano::image::{Image, ImageAspects, ImageCreateInfo, ImageSubresourceRange, ImageUsage};
use vulkano::memory::allocator::{AllocationCreateInfo, FreeListAllocator, GenericMemoryAllocator, MemoryAllocator, StandardMemoryAllocator};
use vulkano::pipeline::compute::ComputePipelineCreateInfo;
use vulkano::pipeline::graphics::color_blend::{ColorBlendAttachmentState, ColorBlendState};
use vulkano::pipeline::graphics::input_assembly::{InputAssemblyState, PrimitiveTopology};
use vulkano::pipeline::graphics::multisample::MultisampleState;
use vulkano::pipeline::graphics::rasterization::RasterizationState;
use vulkano::pipeline::graphics::subpass::PipelineSubpassType;
use vulkano::pipeline::graphics::vertex_input::VertexInputState;
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::pipeline::graphics::GraphicsPipelineCreateInfo;
use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;
use vulkano::pipeline::{ComputePipeline, DynamicState, GraphicsPipeline, Pipeline, PipelineBindPoint, PipelineLayout, PipelineShaderStageCreateInfo};
use vulkano::render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass, Subpass};
use vulkano::shader::spirv::bytes_to_words;
use vulkano::shader::{ShaderModule, ShaderModuleCreateInfo};
use vulkano::single_pass_renderpass;
use vulkano::sync::GpuFuture;

struct SharedResource {
    device: Arc<Device>,
    graphics_queue: Arc<Queue>,
    render_pass: Arc<RenderPass>,
    texture_drawing_pipeline: Arc<GraphicsPipeline>,
    composite_operation_pipeline: Arc<ComputePipeline>,
    memory_allocator: Arc<GenericMemoryAllocator<FreeListAllocator>>,
    command_buffer_allocator: StandardCommandBufferAllocator,
    descriptor_set_allocator: StandardDescriptorSetAllocator,
}

impl SharedResource {
    fn new(device: Arc<Device>, graphics_queue: Arc<Queue>) -> SharedResource {
        let render_pass = single_pass_renderpass!(
            Arc::clone(&device),
            attachments: {
                color: {
                    format: Format::R8G8B8A8_UNORM,
                    samples: 1,
                    load_op: Clear,
                    store_op: Store,
                },
                stencil: {
                    format: Format::R32_UINT,
                    samples: 1,
                    load_op: Clear,
                    store_op: Store,
                }
            },
            pass: {
                color: [color, stencil],
                depth_stencil: {}
            }
        )
        .unwrap();
        let subpass = Subpass::from(render_pass.clone(), 0).unwrap();
        let texture_drawing_shader = unsafe { ShaderModule::new(Arc::clone(&device), ShaderModuleCreateInfo::new(&bytes_to_words(include_bytes!(concat!(env!("OUT_DIR"), "/texture_drawing.spv"))).unwrap())).unwrap() };
        let vertex_shader = texture_drawing_shader.entry_point("shader::main_vs").unwrap();
        let fragment_shader = texture_drawing_shader.entry_point("shader::main_fs").unwrap();
        let shader_stages = smallvec![PipelineShaderStageCreateInfo::new(vertex_shader), PipelineShaderStageCreateInfo::new(fragment_shader)];
        let pipeline_layout = PipelineLayout::new(Arc::clone(&device), PipelineDescriptorSetLayoutCreateInfo::from_stages(&shader_stages).into_pipeline_layout_create_info(Arc::clone(&device)).unwrap()).unwrap();
        let texture_drawing_pipeline = GraphicsPipeline::new(
            Arc::clone(&device),
            None,
            GraphicsPipelineCreateInfo {
                stages: shader_stages,
                vertex_input_state: Some(VertexInputState::new()),
                input_assembly_state: Some(InputAssemblyState {
                    topology: PrimitiveTopology::TriangleStrip,
                    primitive_restart_enable: false,
                    ..InputAssemblyState::default()
                }),
                viewport_state: Some(ViewportState::default()),
                rasterization_state: Some(RasterizationState::default()),
                multisample_state: Some(MultisampleState::default()),
                color_blend_state: Some(ColorBlendState::with_attachment_states(2, ColorBlendAttachmentState::default())),
                dynamic_state: [DynamicState::Viewport].into_iter().collect(),
                subpass: Some(PipelineSubpassType::BeginRenderPass(subpass)),
                ..GraphicsPipelineCreateInfo::layout(pipeline_layout)
            },
        )
        .unwrap();
        let composite_operation_shader = unsafe { ShaderModule::new(Arc::clone(&device), ShaderModuleCreateInfo::new(&bytes_to_words(include_bytes!(concat!(env!("OUT_DIR"), "/composite_operation.spv"))).unwrap())).unwrap() };
        let compute_shader = composite_operation_shader.entry_point("shader::main").unwrap();
        let composite_operation_pipeline = ComputePipeline::new(
            Arc::clone(&device),
            None,
            ComputePipelineCreateInfo::stage_layout(
                PipelineShaderStageCreateInfo::new(compute_shader.clone()),
                PipelineLayout::new(
                    Arc::clone(&device),
                    PipelineDescriptorSetLayoutCreateInfo::from_stages(&[PipelineShaderStageCreateInfo::new(compute_shader)]).into_pipeline_layout_create_info(Arc::clone(&device)).unwrap(),
                )
                .unwrap(),
            ),
        )
        .unwrap();
        let efficient_allocator = StandardMemoryAllocator::new_default(Arc::clone(&device));
        let command_buffer_allocator = StandardCommandBufferAllocator::new(Arc::clone(&device), StandardCommandBufferAllocatorCreateInfo::default());
        let descriptor_set_allocator = StandardDescriptorSetAllocator::new(Arc::clone(&device), StandardDescriptorSetAllocatorCreateInfo::default());
        SharedResource {
            device,
            graphics_queue,
            render_pass,
            texture_drawing_pipeline,
            composite_operation_pipeline,
            memory_allocator: Arc::new(efficient_allocator),
            command_buffer_allocator,
            descriptor_set_allocator,
        }
    }
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

pub struct ImageCombinerBuilder {
    shared: Arc<SharedResource>,
}

impl ImageCombinerBuilder {
    pub fn new(device: Arc<Device>, graphics_queue: Arc<Queue>) -> ImageCombinerBuilder {
        ImageCombinerBuilder {
            shared: Arc::new(SharedResource::new(device, graphics_queue)),
        }
    }
}

impl CombinerBuilder<ImageType> for ImageCombinerBuilder {
    type Request = ImageCombinerRequest;
    type Param = ImageCombinerParam;
    type Combiner = ImageCombiner;

    fn new_combiner(&self, request: Self::Request) -> Self::Combiner {
        ImageCombiner {
            shared: Arc::clone(&self.shared),
            image_size_request: request.size_request,
            buffer: Vec::new(),
        }
    }
}

pub struct ImageCombiner {
    shared: Arc<SharedResource>,
    image_size_request: ImageSizeRequest,
    buffer: Vec<(ImageType, ImageRequiredParamsFixed)>,
}

impl Combiner<ImageType> for ImageCombiner {
    type Param = ImageCombinerParam;

    fn add(&mut self, data: ImageType, param: Self::Param) {
        self.buffer.push((data, param))
    }

    fn collect<'async_trait>(self) -> impl Future<Output = ImageType> + Send + 'async_trait
    where
        Self: 'async_trait,
        ImageType: 'async_trait,
    {
        let ImageCombiner { shared: shared_resource, image_size_request, buffer } = self;
        let image_width = image_size_request.width.ceil() as u32;
        let image_height = image_size_request.height.ceil() as u32;
        let result_image = Image::new(
            Arc::clone(&shared_resource.memory_allocator) as Arc<dyn MemoryAllocator>,
            ImageCreateInfo {
                format: Format::R8G8B8A8_UNORM,
                view_formats: vec![Format::R8G8B8A8_UNORM],
                extent: [image_width, image_height, 1],
                usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_DST | ImageUsage::TRANSFER_SRC | ImageUsage::SAMPLED,
                ..ImageCreateInfo::default()
            },
            AllocationCreateInfo::default(),
        )
        .unwrap();
        let result_image_view = ImageView::new(
            Arc::clone(&result_image),
            ImageViewCreateInfo {
                format: Format::R8G8B8A8_UNORM,
                subresource_range: ImageSubresourceRange {
                    aspects: ImageAspects::COLOR,
                    mip_levels: 0..1,
                    array_layers: 0..1,
                },
                ..ImageViewCreateInfo::default()
            },
        )
        .unwrap();
        let mut builder = AutoCommandBufferBuilder::primary(&shared_resource.command_buffer_allocator, shared_resource.graphics_queue.queue_family_index(), CommandBufferUsage::OneTimeSubmit).unwrap();
        builder
            .clear_color_image(ClearColorImageInfo {
                clear_value: ClearColorValue::Float([0., 0., 0., 0.]),
                ..ClearColorImageInfo::image(Arc::clone(&result_image) as Arc<_>)
            })
            .unwrap();
        let buffer_image = Image::new(
            Arc::clone(&shared_resource.memory_allocator) as Arc<dyn MemoryAllocator>,
            ImageCreateInfo {
                format: Format::R8G8B8A8_UNORM,
                view_formats: vec![Format::R8G8B8A8_UNORM],
                extent: [image_width, image_height, 1],
                usage: ImageUsage::STORAGE | ImageUsage::COLOR_ATTACHMENT,
                ..ImageCreateInfo::default()
            },
            AllocationCreateInfo::default(),
        )
        .unwrap();
        let buffer_image_view = ImageView::new(
            Arc::clone(&buffer_image),
            ImageViewCreateInfo {
                format: Format::R8G8B8A8_UNORM,
                subresource_range: ImageSubresourceRange {
                    aspects: ImageAspects::COLOR,
                    mip_levels: 0..1,
                    array_layers: 0..1,
                },
                ..ImageViewCreateInfo::default()
            },
        )
        .unwrap();
        let depth = Image::new(
            Arc::clone(&shared_resource.memory_allocator) as Arc<dyn MemoryAllocator>,
            ImageCreateInfo {
                format: Format::R32_UINT,
                view_formats: vec![Format::R32_UINT],
                extent: [image_width, image_height, 1],
                usage: ImageUsage::STORAGE | ImageUsage::COLOR_ATTACHMENT,
                ..ImageCreateInfo::default()
            },
            AllocationCreateInfo::default(),
        )
        .unwrap();
        let depth_view = ImageView::new(
            Arc::clone(&depth),
            ImageViewCreateInfo {
                format: Format::R32_UINT,
                subresource_range: ImageSubresourceRange {
                    aspects: ImageAspects::COLOR,
                    mip_levels: 0..1,
                    array_layers: 0..1,
                },
                ..ImageViewCreateInfo::default()
            },
        )
        .unwrap();
        for (ImageType(image), image_param) in buffer {
            let transform_mat = match image_param.transform {
                ImageRequiredParamsTransformFixed::Params {
                    size,
                    scale,
                    translate,
                    rotate,
                    scale_center,
                    rotate_center,
                } => {
                    let image_native_size = match (image_width as f64 * size.x * image.extent()[1] as f64).partial_cmp(&(image_height as f64 * size.y * image.extent()[0] as f64)).unwrap() {
                        Ordering::Greater => (image_height as f64 * size.y * image.extent()[0] as f64 / image.extent()[1] as f64, image_height as f64 * size.y),
                        Ordering::Equal => (image_width as f64 * size.x, image_height as f64 * size.y),
                        Ordering::Less => (image_width as f64 * size.x, image_width as f64 * size.x * image.extent()[1] as f64 / image.extent()[0] as f64),
                    };
                    scale_mat(Vector3::new(image_native_size.0 / image_size_request.width as f64, image_native_size.1 / image_size_request.height as f64, 1.))
                        * move_mat(-scale_center)
                        * scale_mat(scale)
                        * move_mat(scale_center)
                        * move_mat(-rotate_center)
                        * Matrix4::from(rotate)
                        * move_mat(rotate_center)
                        * move_mat(translate)
                }
                ImageRequiredParamsTransformFixed::Free {
                    left_top: _,
                    right_top: _,
                    left_bottom: _,
                    right_bottom: _,
                } => todo!(),
            };
            let transform_matrix = mat4_into_glam(transform_mat);
            // imageを空間に貼る
            let image_view = ImageView::new(
                image,
                ImageViewCreateInfo {
                    format: Format::R8G8B8A8_UNORM,
                    subresource_range: ImageSubresourceRange {
                        aspects: ImageAspects::COLOR,
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
                    attachments: vec![Arc::clone(&buffer_image_view) as Arc<_>, Arc::clone(&depth_view) as Arc<_>],
                    extent: [image_width, image_height],
                    layers: 0,
                    ..FramebufferCreateInfo::default()
                },
            )
            .unwrap();
            builder
                .begin_render_pass(
                    RenderPassBeginInfo {
                        clear_values: vec![Some(ClearValue::Float(image_param.background_color.map(|i| i as f32 / 255.))), Some(ClearValue::Uint([0; 4]))],
                        ..RenderPassBeginInfo::framebuffer(frame_buffer)
                    },
                    SubpassBeginInfo::default(),
                )
                .unwrap();
            builder.bind_pipeline_graphics(Arc::clone(&shared_resource.texture_drawing_pipeline)).unwrap();
            builder
                .set_viewport(
                    0,
                    smallvec![Viewport {
                        offset: [0., 0.],
                        extent: [image_size_request.width, image_size_request.height],
                        depth_range: 0.0..=1.0,
                    }],
                )
                .unwrap();
            builder.push_constants(Arc::clone(shared_resource.texture_drawing_pipeline.layout()), 0, TextureDrawingConstant { transform_matrix }).unwrap();
            builder
                .bind_descriptor_sets(
                    PipelineBindPoint::Graphics,
                    Arc::clone(shared_resource.texture_drawing_pipeline.layout()),
                    0,
                    PersistentDescriptorSet::new(
                        &shared_resource.descriptor_set_allocator,
                        Arc::clone(&shared_resource.texture_drawing_pipeline.layout().set_layouts()[0]),
                        [
                            WriteDescriptorSet::image_view(0, image_view),
                            WriteDescriptorSet::sampler(1, Sampler::new(Arc::clone(&shared_resource.device), SamplerCreateInfo::simple_repeat_linear_no_mipmap()).unwrap()),
                        ],
                        [],
                    )
                    .unwrap(),
                )
                .unwrap();
            //StencilとTopologyはパイプラインで設定
            builder.draw(4, 1, 0, 0).unwrap();
            builder.end_render_pass(SubpassEndInfo::default()).unwrap();

            builder.bind_pipeline_compute(Arc::clone(&shared_resource.composite_operation_pipeline)).unwrap();
            builder
                .bind_descriptor_sets(
                    PipelineBindPoint::Compute,
                    Arc::clone(shared_resource.composite_operation_pipeline.layout()),
                    0,
                    PersistentDescriptorSet::new(
                        &shared_resource.descriptor_set_allocator,
                        Arc::clone(&shared_resource.composite_operation_pipeline.layout().set_layouts()[0]),
                        [WriteDescriptorSet::image_view(0, Arc::clone(&result_image_view) as Arc<_>)],
                        [],
                    )
                    .unwrap(),
                )
                .unwrap();
            builder
                .bind_descriptor_sets(
                    PipelineBindPoint::Compute,
                    Arc::clone(shared_resource.composite_operation_pipeline.layout()),
                    1,
                    PersistentDescriptorSet::new(
                        &shared_resource.descriptor_set_allocator,
                        Arc::clone(&shared_resource.composite_operation_pipeline.layout().set_layouts()[1]),
                        [WriteDescriptorSet::image_view(0, Arc::clone(&buffer_image_view) as Arc<_>), WriteDescriptorSet::image_view(1, Arc::clone(&depth_view) as Arc<_>)],
                        [],
                    )
                    .unwrap(),
                )
                .unwrap();
            builder
                .push_constants(
                    Arc::clone(shared_resource.composite_operation_pipeline.layout()),
                    0,
                    CompositeOperationConstant {
                        composite: image_param.composite_operation as u32,
                        blend: image_param.blend_mode as u32,
                        image_width,
                        image_height,
                    },
                )
                .unwrap();
            builder.dispatch([image_width.div_ceil(32), image_height.div_ceil(32), 1]).unwrap();
        }
        builder.build().unwrap().execute(Arc::clone(&shared_resource.graphics_queue)).unwrap().then_signal_fence_and_flush().unwrap().map(move |_| ImageType(result_image))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::{Quaternion, Zero};
    use vulkano::instance::InstanceCreateInfo;
    use vulkano::Version;
    use vulkano_util::context::{VulkanoConfig, VulkanoContext};

    #[tokio::test]
    async fn test_image_combiner() {
        let context = Arc::new(VulkanoContext::new(VulkanoConfig {
            instance_create_info: InstanceCreateInfo {
                max_api_version: Some(Version::V1_2),
                ..InstanceCreateInfo::default()
            },
            ..VulkanoConfig::default()
        }));
        let command_buffer_allocator = StandardCommandBufferAllocator::new(Arc::clone(context.device()), StandardCommandBufferAllocatorCreateInfo::default());
        let get_image = |color: [f32; 4]| {
            let image = Image::new(
                Arc::clone(context.memory_allocator()) as Arc<dyn MemoryAllocator>,
                ImageCreateInfo {
                    format: Format::R8G8B8A8_UNORM,
                    extent: [1, 1, 1],
                    usage: ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
                    ..ImageCreateInfo::default()
                },
                AllocationCreateInfo::default(),
            )
            .unwrap();
            let mut builder = AutoCommandBufferBuilder::primary(&command_buffer_allocator, context.graphics_queue().queue_family_index(), CommandBufferUsage::OneTimeSubmit).unwrap();
            builder
                .clear_color_image(ClearColorImageInfo {
                    clear_value: ClearColorValue::Float(color),
                    ..ClearColorImageInfo::image(Arc::clone(&image))
                })
                .unwrap();
            builder.build().unwrap().execute(Arc::clone(context.graphics_queue())).unwrap().then_signal_fence_and_flush().unwrap().wait(None).unwrap();
            image
        };

        let image_combiner_builder = ImageCombinerBuilder::new(Arc::clone(context.device()), Arc::clone(context.graphics_queue()));
        let image_combiner = image_combiner_builder.new_combiner(ImageCombinerRequest::from(ImageSizeRequest { width: 24., height: 24. }));
        image_combiner.collect().await;

        let mut image_combiner = image_combiner_builder.new_combiner(ImageCombinerRequest::from(ImageSizeRequest { width: 24., height: 24. }));
        image_combiner.add(
            ImageType(get_image([1., 0., 0., 1.])),
            ImageRequiredParamsFixed {
                transform: ImageRequiredParamsTransformFixed::Params {
                    size: Vector3::new(1., 1., 1.),
                    scale: Vector3::new(1. / 3., 1. / 3., 1.),
                    translate: Vector3::new(0., 0., 0.),
                    rotate: Quaternion::zero(),
                    scale_center: Vector3::new(0., 0., 0.),
                    rotate_center: Vector3::new(0., 0., 0.),
                },
                background_color: [0; 4],
                opacity: Default::default(),
                blend_mode: Default::default(),
                composite_operation: Default::default(),
            },
        );
        image_combiner.add(
            ImageType(get_image([0., 1., 0., 1.])),
            ImageRequiredParamsFixed {
                transform: ImageRequiredParamsTransformFixed::Params {
                    size: Vector3::new(1., 1., 1.),
                    scale: Vector3::new(1. / 3., 1. / 3., 1.),
                    translate: Vector3::new(0., 0., 0.),
                    rotate: Quaternion::zero(),
                    scale_center: Vector3::new(1., 1., 0.),
                    rotate_center: Vector3::new(0., 0., 0.),
                },
                background_color: [0; 4],
                opacity: Default::default(),
                blend_mode: Default::default(),
                composite_operation: Default::default(),
            },
        );
        image_combiner.collect().await;
    }
}
