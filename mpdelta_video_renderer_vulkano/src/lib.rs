use cgmath::{Matrix4, SquareMatrix, Vector3, Vector4};
use glam::{Mat4, Vec4};
use mpdelta_core::component::parameter::{ImageRequiredParamsFixed, ImageRequiredParamsTransformFixed};
use mpdelta_core_vulkano::ImageType;
use mpdelta_renderer::{Combiner, CombinerBuilder, ImageSizeRequest};
use shader_composite_operation::CompositeOperationConstant;
use shader_texture_drawing::TextureDrawingConstant;
use std::cmp::Ordering;
use std::sync::Arc;
use vulkano::command_buffer::allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo};
use vulkano::command_buffer::{AutoCommandBufferBuilder, ClearColorImageInfo, CommandBufferUsage, PrimaryCommandBufferAbstract, RenderPassBeginInfo, SubpassContents};
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator;
use vulkano::descriptor_set::{PersistentDescriptorSet, WriteDescriptorSet};
use vulkano::format::Format;
use vulkano::format::{ClearColorValue, ClearValue};
use vulkano::image::view::{ImageView, ImageViewCreateInfo};
use vulkano::image::{AttachmentImage, ImageAspects, ImageDimensions, ImageSubresourceRange, ImageUsage, StorageImage};
use vulkano::memory::allocator::{FreeListAllocator, GenericMemoryAllocator, StandardMemoryAllocator};
use vulkano::pipeline::graphics::input_assembly::{InputAssemblyState, PrimitiveTopology};
use vulkano::pipeline::graphics::vertex_input::VertexInputState;
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::pipeline::{ComputePipeline, GraphicsPipeline, PartialStateMode, Pipeline, PipelineBindPoint, StateMode};
use vulkano::render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass, Subpass};
use vulkano::sampler::{Sampler, SamplerCreateInfo};
use vulkano::shader::ShaderModule;
use vulkano::single_pass_renderpass;
use vulkano::sync::GpuFuture;
use vulkano_util::context::VulkanoContext;

struct SharedResource {
    context: Arc<VulkanoContext>,
    render_pass: Arc<RenderPass>,
    texture_drawing_pipeline: Arc<GraphicsPipeline>,
    composite_operation_pipeline: Arc<ComputePipeline>,
    memory_allocator: GenericMemoryAllocator<Arc<FreeListAllocator>>,
    command_buffer_allocator: StandardCommandBufferAllocator,
    descriptor_set_allocator: StandardDescriptorSetAllocator,
}

impl SharedResource {
    fn new(context: Arc<VulkanoContext>) -> SharedResource {
        let render_pass = single_pass_renderpass!(
            Arc::clone(context.device()),
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
                color: [color, stencil],
                depth_stencil: {}
            }
        )
        .unwrap();
        let texture_drawing_shader = unsafe { ShaderModule::from_bytes(Arc::clone(context.device()), include_bytes!(concat!(env!("OUT_DIR"), "/texture_drawing.spv"))).unwrap() };
        let subpass = Subpass::from(render_pass.clone(), 0).unwrap();
        let texture_drawing_pipeline = GraphicsPipeline::start()
            .render_pass(subpass)
            .vertex_shader(texture_drawing_shader.entry_point("shader::main_vs").unwrap(), ())
            .fragment_shader(texture_drawing_shader.entry_point("shader::main_fs").unwrap(), ())
            .input_assembly_state(InputAssemblyState {
                topology: PartialStateMode::Fixed(PrimitiveTopology::TriangleStrip),
                primitive_restart_enable: StateMode::Fixed(false),
            })
            .viewport_state(ViewportState::viewport_dynamic_scissor_irrelevant())
            .vertex_input_state(VertexInputState::new())
            .build(Arc::clone(context.device()))
            .unwrap();
        let composite_operation_shader = unsafe { ShaderModule::from_bytes(Arc::clone(context.device()), include_bytes!(concat!(env!("OUT_DIR"), "/composite_operation.spv"))).unwrap() };

        let composite_operation_pipeline = ComputePipeline::new(Arc::clone(context.device()), composite_operation_shader.entry_point_with_execution("shader::main", vulkano::shader::spirv::ExecutionModel::GLCompute).unwrap(), &(), None, |_| {}).unwrap();
        let efficient_allocator = StandardMemoryAllocator::new_default(Arc::clone(context.device()));
        let command_buffer_allocator = StandardCommandBufferAllocator::new(Arc::clone(context.device()), StandardCommandBufferAllocatorCreateInfo::default());
        let descriptor_set_allocator = StandardDescriptorSetAllocator::new(Arc::clone(context.device()));
        SharedResource {
            context,
            render_pass,
            texture_drawing_pipeline,
            composite_operation_pipeline,
            memory_allocator: efficient_allocator,
            command_buffer_allocator,
            descriptor_set_allocator,
        }
    }
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

pub struct ImageCombinerBuilder {
    shared: Arc<SharedResource>,
}

impl ImageCombinerBuilder {
    pub fn new(context: Arc<VulkanoContext>) -> ImageCombinerBuilder {
        ImageCombinerBuilder { shared: Arc::new(SharedResource::new(context)) }
    }
}

impl CombinerBuilder<ImageType> for ImageCombinerBuilder {
    type Request = ImageSizeRequest;
    type Param = ImageRequiredParamsFixed;
    type Combiner = ImageCombiner;

    fn new_combiner(&self, request: Self::Request) -> Self::Combiner {
        ImageCombiner {
            shared: Arc::clone(&self.shared),
            image_size_request: request,
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
    type Param = ImageRequiredParamsFixed;

    fn add(&mut self, data: ImageType, param: Self::Param) {
        self.buffer.push((data, param))
    }

    fn collect(self) -> ImageType {
        let ImageCombiner { shared: shared_resource, image_size_request, buffer } = self;
        let image_width = image_size_request.width.ceil() as u32;
        let image_height = image_size_request.height.ceil() as u32;
        let result_image = StorageImage::new(&shared_resource.memory_allocator, ImageDimensions::Dim2d { width: image_width, height: image_height, array_layers: 1 }, Format::R8G8B8A8_UNORM, [shared_resource.context.graphics_queue().queue_family_index()]).unwrap();
        let result_image_view = ImageView::new(
            Arc::clone(&result_image),
            ImageViewCreateInfo {
                format: Some(Format::R8G8B8A8_UNORM),
                subresource_range: ImageSubresourceRange {
                    aspects: ImageAspects::COLOR,
                    mip_levels: 0..1,
                    array_layers: 0..1,
                },
                ..ImageViewCreateInfo::default()
            },
        )
        .unwrap();
        let mut builder = AutoCommandBufferBuilder::primary(&shared_resource.command_buffer_allocator, shared_resource.context.graphics_queue().queue_family_index(), CommandBufferUsage::OneTimeSubmit).unwrap();
        builder
            .clear_color_image(ClearColorImageInfo {
                clear_value: ClearColorValue::Float([0., 0., 0., 0.]),
                ..ClearColorImageInfo::image(Arc::clone(&result_image) as Arc<_>)
            })
            .unwrap();
        let buffer_image = AttachmentImage::with_usage(&shared_resource.memory_allocator, [image_width, image_height], Format::R8G8B8A8_UNORM, ImageUsage::STORAGE).unwrap();
        let buffer_image_view = ImageView::new(
            Arc::clone(&buffer_image),
            ImageViewCreateInfo {
                format: Some(Format::R8G8B8A8_UNORM),
                subresource_range: ImageSubresourceRange {
                    aspects: ImageAspects::COLOR,
                    mip_levels: 0..1,
                    array_layers: 0..1,
                },
                ..ImageViewCreateInfo::default()
            },
        )
        .unwrap();
        let depth = AttachmentImage::with_usage(&shared_resource.memory_allocator, [image_width, image_height], Format::R32_UINT, ImageUsage::STORAGE).unwrap();
        let depth_view = ImageView::new(
            Arc::clone(&depth),
            ImageViewCreateInfo {
                format: Some(Format::R32_UINT),
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
            let image_native_size = match (image_width * image_param.aspect_ratio.1).cmp(&(image_height * image_param.aspect_ratio.0)) {
                Ordering::Greater => (div_ceil(image_param.aspect_ratio.0 * image_height, image_param.aspect_ratio.1), image_height),
                Ordering::Equal => (image_width, image_height),
                Ordering::Less => (image_width, div_ceil(image_param.aspect_ratio.1 * image_width, image_param.aspect_ratio.0)),
            };
            let transform_mat = match image_param.transform {
                ImageRequiredParamsTransformFixed::Params { scale, translate, rotate, scale_center, rotate_center } => {
                    scale_mat(Vector3::new(image_native_size.0 as f64 / image_size_request.width as f64, image_native_size.1 as f64 / image_size_request.height as f64, 1.)) * move_mat(-scale_center) * scale_mat(scale) * move_mat(scale_center) * move_mat(-rotate_center) * Matrix4::from(rotate) * move_mat(rotate_center) * move_mat(translate)
                }
                ImageRequiredParamsTransformFixed::Free { left_top: _, right_top: _, left_bottom: _, right_bottom: _ } => todo!(),
            };
            let transform_matrix = mat4_into_glam(transform_mat);
            // imageを空間に貼る
            let image_view = ImageView::new(
                image,
                ImageViewCreateInfo {
                    format: Some(Format::R8G8B8A8_UNORM),
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
                    SubpassContents::Inline,
                )
                .unwrap();
            builder.bind_pipeline_graphics(Arc::clone(&shared_resource.texture_drawing_pipeline));
            builder.set_viewport(
                0,
                [Viewport {
                    origin: [0., 0.],
                    dimensions: [image_size_request.width, image_size_request.height],
                    depth_range: 0.0..1.0,
                }],
            );
            builder.push_constants(Arc::clone(shared_resource.texture_drawing_pipeline.layout()), 0, TextureDrawingConstant { transform_matrix });
            builder.bind_descriptor_sets(
                PipelineBindPoint::Graphics,
                Arc::clone(shared_resource.texture_drawing_pipeline.layout()),
                0,
                PersistentDescriptorSet::new(
                    &shared_resource.descriptor_set_allocator,
                    Arc::clone(&shared_resource.texture_drawing_pipeline.layout().set_layouts()[0]),
                    [WriteDescriptorSet::image_view(0, image_view), WriteDescriptorSet::sampler(1, Sampler::new(Arc::clone(shared_resource.context.device()), SamplerCreateInfo::simple_repeat_linear_no_mipmap()).unwrap())],
                )
                .unwrap(),
            );
            //StencilとTopologyはパイプラインで設定
            builder.draw(4, 1, 0, 0).unwrap();
            builder.end_render_pass().unwrap();

            builder.bind_pipeline_compute(Arc::clone(&shared_resource.composite_operation_pipeline));
            builder.bind_descriptor_sets(
                PipelineBindPoint::Compute,
                Arc::clone(shared_resource.composite_operation_pipeline.layout()),
                0,
                PersistentDescriptorSet::new(&shared_resource.descriptor_set_allocator, Arc::clone(&shared_resource.composite_operation_pipeline.layout().set_layouts()[0]), [WriteDescriptorSet::image_view(0, Arc::clone(&result_image_view) as Arc<_>)]).unwrap(),
            );
            builder.bind_descriptor_sets(
                PipelineBindPoint::Compute,
                Arc::clone(shared_resource.composite_operation_pipeline.layout()),
                1,
                PersistentDescriptorSet::new(
                    &shared_resource.descriptor_set_allocator,
                    Arc::clone(&shared_resource.composite_operation_pipeline.layout().set_layouts()[1]),
                    [WriteDescriptorSet::image_view(0, Arc::clone(&buffer_image_view) as Arc<_>), WriteDescriptorSet::image_view(1, Arc::clone(&depth_view) as Arc<_>)],
                )
                .unwrap(),
            );
            builder.push_constants(
                Arc::clone(shared_resource.composite_operation_pipeline.layout()),
                0,
                CompositeOperationConstant {
                    composite: image_param.composite_operation as u32,
                    blend: image_param.blend_mode as u32,
                    image_width,
                    image_height,
                },
            );
            builder.dispatch([div_ceil(image_width, 32), div_ceil(image_height, 32), 1]).unwrap();
        }
        builder.build().unwrap().execute(Arc::clone(shared_resource.context.graphics_queue())).unwrap().then_signal_fence_and_flush().unwrap().wait(None).unwrap();
        ImageType(result_image)
    }
}
