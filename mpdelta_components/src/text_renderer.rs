use crate::common::color;
use crate::text_renderer::rich_text::{RichTextParser, RichTextToken};
use crate::text_renderer::shaping::{GlyphData, ShapingBuilder, ShapingBuilderSegment};
use async_trait::async_trait;
use crossbeam_queue::SegQueue;
use font_kit::family_name::FamilyName;
use font_kit::handle::Handle;
use font_kit::properties::Properties;
use font_kit::source::SystemSource;
use glam::{Mat4, Vec4};
use lyon_tessellation::math::Point as LyonPoint;
use lyon_tessellation::path::{ControlPointId, EndpointId, IdEvent, PositionStore};
use lyon_tessellation::{BuffersBuilder, FillOptions, FillTessellator, FillVertex, FillVertexConstructor, LineJoin, StrokeOptions, StrokeTessellator, StrokeVertex, StrokeVertexConstructor, VertexBuffers};
use mpdelta_core::component::class::{ComponentClass, ComponentClassIdentifier};
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerTime};
use mpdelta_core::component::parameter::value::{DynEditableSelfValue, EasingValue, LinearEasing};
use mpdelta_core::component::parameter::{ImageRequiredParams, Parameter, ParameterNullableValue, ParameterSelect, ParameterType, ParameterValueRaw, ParameterValueType, VariableParameterPriority, VariableParameterValue};
use mpdelta_core::component::processor::{ComponentProcessor, ComponentProcessorNative, ComponentProcessorWrapper, NativeProcessorInput, NativeProcessorRequest};
use mpdelta_core::core::IdGenerator;
use mpdelta_core::ptr::StaticPointer;
use mpdelta_core::time::TimelineTime;
use mpdelta_core::time_split_value_persistent;
use mpdelta_core_vulkano::ImageType;
use rpds::Vector;
use shader_font_rendering::{Constant, FontVertex, GlyphStyle};
use smallvec::{smallvec, SmallVec};
use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::iter::Peekable;
use std::mem::offset_of;
use std::sync::Arc;
use std::{iter, mem, slice};
use swash::scale::ScaleContext;
use swash::zeno::{Point as ZenoPoint, Verb};
use swash::FontRef;
use tokio::sync::RwLock;
use vulkano::buffer::{Buffer, BufferCreateInfo, BufferUsage, Subbuffer};
use vulkano::command_buffer::allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo};
use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBufferUsage, PrimaryCommandBufferAbstract, RenderPassBeginInfo, SubpassBeginInfo, SubpassEndInfo};
use vulkano::descriptor_set::allocator::{StandardDescriptorSetAllocator, StandardDescriptorSetAllocatorCreateInfo};
use vulkano::descriptor_set::{PersistentDescriptorSet, WriteDescriptorSet};
use vulkano::device::{Device, Queue};
use vulkano::format::{ClearValue, Format};
use vulkano::image::view::{ImageView, ImageViewCreateInfo};
use vulkano::image::{Image, ImageAspects, ImageCreateInfo, ImageSubresourceRange, ImageUsage, SampleCount};
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryAllocator, MemoryTypeFilter, StandardMemoryAllocator};
use vulkano::pipeline::graphics::color_blend::{ColorBlendAttachmentState, ColorBlendState};
use vulkano::pipeline::graphics::input_assembly::InputAssemblyState;
use vulkano::pipeline::graphics::multisample::MultisampleState;
use vulkano::pipeline::graphics::rasterization::RasterizationState;
use vulkano::pipeline::graphics::subpass::PipelineSubpassType;
use vulkano::pipeline::graphics::vertex_input::{VertexInputAttributeDescription, VertexInputBindingDescription, VertexInputRate, VertexInputState};
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::pipeline::graphics::GraphicsPipelineCreateInfo;
use vulkano::pipeline::layout::PipelineDescriptorSetLayoutCreateInfo;
use vulkano::pipeline::{DynamicState, GraphicsPipeline, Pipeline, PipelineBindPoint, PipelineLayout, PipelineShaderStageCreateInfo};
use vulkano::render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass, Subpass};
use vulkano::shader::spirv::bytes_to_words;
use vulkano::shader::{ShaderModule, ShaderModuleCreateInfo};
use vulkano::single_pass_renderpass;
use vulkano::sync::{GpuFuture, HostAccessError};

mod rich_text;
mod shaping;

pub struct TextRendererClass<T: ParameterValueType> {
    processor: ComponentProcessorWrapper<T>,
}

impl<T> TextRendererClass<T>
where
    T: ParameterValueType<Image = ImageType>,
{
    pub fn new(device: &Arc<Device>, queue: &Arc<Queue>, memory_allocator: &Arc<StandardMemoryAllocator>) -> TextRendererClass<T> {
        TextRendererClass {
            processor: ComponentProcessorWrapper::Native(Arc::new(TextRenderer::new(device, queue, memory_allocator))),
        }
    }
}

#[async_trait]
impl<T> ComponentClass<T> for TextRendererClass<T>
where
    T: ParameterValueType<Image = ImageType>,
{
    fn identifier(&self) -> ComponentClassIdentifier {
        ComponentClassIdentifier {
            namespace: Cow::Borrowed("mpdelta"),
            name: Cow::Borrowed("TextRenderer"),
            inner_identifier: Default::default(),
        }
    }

    fn processor(&self) -> ComponentProcessorWrapper<T> {
        self.processor.clone()
    }

    async fn instantiate(&self, this: &StaticPointer<RwLock<dyn ComponentClass<T>>>, id: &dyn IdGenerator) -> ComponentInstance<T> {
        let left = MarkerPin::new(id.generate_new(), MarkerTime::ZERO);
        let right = MarkerPin::new_unlocked(id.generate_new());
        let image_required_params = ImageRequiredParams::new_default(left.id(), right.id());
        let string_param = time_split_value_persistent![*left.id(), Some(EasingValue::new(DynEditableSelfValue(String::new()), Arc::new(LinearEasing))), *right.id()];
        ComponentInstance::builder(this.clone(), left, right, Vec::new(), self.processor.clone())
            .image_required_params(image_required_params)
            .variable_parameters(
                vec![("text".to_owned(), Parameter::String(()))],
                [VariableParameterValue {
                    params: ParameterNullableValue::String(string_param),
                    components: Vector::new_sync(),
                    priority: VariableParameterPriority::PrioritizeManually,
                }]
                .into_iter()
                .collect(),
            )
            .build(id)
    }
}

struct TextRenderer {
    queue: Arc<Queue>,
    render_pass: Arc<RenderPass>,
    pipeline: Arc<GraphicsPipeline>,
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: StandardCommandBufferAllocator,
    descriptor_set_allocator: StandardDescriptorSetAllocator,
    vertex_buffer_queue: SegQueue<Subbuffer<[FontVertex]>>,
    index_buffer_queue: SegQueue<Subbuffer<[u32]>>,
    glyph_style_buffer_queue: SegQueue<Subbuffer<[GlyphStyle]>>,
}

const MULTISAMPLE: u32 = 4;

impl TextRenderer {
    fn new(device: &Arc<Device>, queue: &Arc<Queue>, memory_allocator: &Arc<StandardMemoryAllocator>) -> TextRenderer {
        let render_pass = single_pass_renderpass!(
            Arc::clone(device),
            attachments: {
                color: {
                    format: Format::R8G8B8A8_UNORM,
                    samples: MULTISAMPLE,
                    load_op: Clear,
                    store_op: DontCare,
                },
                color_resolve: {
                    format: Format::R8G8B8A8_UNORM,
                    samples: 1,
                    load_op: DontCare,
                    store_op: Store,
                },
            },
            pass: {
                color: [color],
                color_resolve: [color_resolve],
                depth_stencil: { },
            },
        )
        .unwrap();
        let subpass = Subpass::from(Arc::clone(&render_pass), 0).unwrap();
        let font_triangle_shader = unsafe { ShaderModule::new(Arc::clone(device), ShaderModuleCreateInfo::new(&bytes_to_words(include_bytes!(concat!(env!("OUT_DIR"), "/font_rendering.spv"))).unwrap())).unwrap() };
        let vertex_shader = font_triangle_shader.entry_point("shader::main_vs").unwrap();
        let fragment_shader = font_triangle_shader.entry_point("shader::main_fs").unwrap();
        let vertex_input_state = VertexInputState::new()
            .binding(
                0,
                VertexInputBindingDescription {
                    stride: mem::size_of::<FontVertex>() as u32,
                    input_rate: VertexInputRate::Vertex,
                },
            )
            .attributes([
                (
                    0,
                    VertexInputAttributeDescription {
                        binding: 0,
                        format: Format::R32_SFLOAT,
                        offset: offset_of!(FontVertex, x) as u32,
                    },
                ),
                (
                    1,
                    VertexInputAttributeDescription {
                        binding: 0,
                        format: Format::R32_SFLOAT,
                        offset: offset_of!(FontVertex, y) as u32,
                    },
                ),
                (
                    2,
                    VertexInputAttributeDescription {
                        binding: 0,
                        format: Format::R32_UINT,
                        offset: offset_of!(FontVertex, glyph) as u32,
                    },
                ),
            ]);
        let shader_stages = smallvec![PipelineShaderStageCreateInfo::new(vertex_shader), PipelineShaderStageCreateInfo::new(fragment_shader)];
        let pipeline_layout = PipelineLayout::new(Arc::clone(device), PipelineDescriptorSetLayoutCreateInfo::from_stages(&shader_stages).into_pipeline_layout_create_info(Arc::clone(device)).unwrap()).unwrap();
        let pipeline = GraphicsPipeline::new(
            Arc::clone(device),
            None,
            GraphicsPipelineCreateInfo {
                stages: shader_stages,
                vertex_input_state: Some(vertex_input_state),
                input_assembly_state: Some(InputAssemblyState::default()),
                viewport_state: Some(ViewportState::default()),
                rasterization_state: Some(RasterizationState::default()),
                multisample_state: Some(MultisampleState {
                    rasterization_samples: SampleCount::try_from(MULTISAMPLE).unwrap(),
                    ..MultisampleState::default()
                }),
                depth_stencil_state: None,
                color_blend_state: Some(ColorBlendState::with_attachment_states(1, ColorBlendAttachmentState::default())),
                dynamic_state: [DynamicState::Viewport].into_iter().collect(),
                subpass: Some(PipelineSubpassType::BeginRenderPass(subpass)),
                ..GraphicsPipelineCreateInfo::layout(pipeline_layout)
            },
        )
        .unwrap();
        let command_buffer_allocator = StandardCommandBufferAllocator::new(Arc::clone(device), StandardCommandBufferAllocatorCreateInfo::default());
        let descriptor_set_allocator = StandardDescriptorSetAllocator::new(Arc::clone(device), StandardDescriptorSetAllocatorCreateInfo::default());
        TextRenderer {
            queue: Arc::clone(queue),
            render_pass,
            pipeline,
            memory_allocator: Arc::clone(memory_allocator),
            command_buffer_allocator,
            descriptor_set_allocator,
            vertex_buffer_queue: SegQueue::new(),
            index_buffer_queue: SegQueue::new(),
            glyph_style_buffer_queue: SegQueue::new(),
        }
    }
}

#[async_trait]
impl<T> ComponentProcessor<T> for TextRenderer
where
    T: ParameterValueType<Image = ImageType>,
{
    async fn fixed_parameter_types(&self) -> &[(String, ParameterType)] {
        &[]
    }

    async fn update_variable_parameter(&self, _fixed_params: &[ParameterValueRaw<T::Image, T::Audio>], variable_parameters: &mut Vec<(String, ParameterType)>) {
        *variable_parameters = vec![("text".to_owned(), Parameter::String(()))];
    }
}

macro_rules! get_or_create_buffer {
    ($buffer:ident, $buffer_lock:ident, $queue:expr, $allocator:expr, $buffer_len:expr $(, $buffer_usage:expr $(,)?)?) => {
        let mut $buffer;
        let mut $buffer_lock = 'lock: {
            if let Some(b) = $queue.pop().and_then(|buffer| (buffer.len() >= $buffer_len).then_some(buffer)) {
                $buffer = b;
                match $buffer.write() {
                    Ok(buffer) => break 'lock buffer,
                    Err(HostAccessError::AccessConflict(_)) => {}
                    Err(err) => panic!("Unexpected error: {}", err),
                }
                $queue.push($buffer);
            }
            $buffer = Buffer::new_slice(
                Arc::clone(&$allocator) as Arc<_>,
                BufferCreateInfo {
                    usage: BufferUsage::TRANSFER_SRC $( | $buffer_usage )?,
                    ..BufferCreateInfo::default()
                },
                AllocationCreateInfo {
                    memory_type_filter: MemoryTypeFilter::HOST_RANDOM_ACCESS,
                    ..AllocationCreateInfo::default()
                },
                $buffer_len,
            )
            .unwrap();
            $buffer.write().unwrap()
        };
    };
}

#[async_trait]
impl<T> ComponentProcessorNative<T> for TextRenderer
where
    T: ParameterValueType<Image = ImageType>,
{
    type WholeComponentCacheKey = ();
    type WholeComponentCacheValue = ();
    type FramedCacheKey = ();
    type FramedCacheValue = ();

    fn whole_component_cache_key(&self, _fixed_parameters: &[ParameterValueRaw<T::Image, T::Audio>]) -> Option<Self::WholeComponentCacheKey> {
        None
    }

    fn framed_cache_key(&self, _parameters: NativeProcessorInput<'_, T>, _time: TimelineTime, _output_type: Parameter<ParameterSelect>) -> Option<Self::FramedCacheKey> {
        None
    }

    async fn natural_length(&self, _fixed_params: &[ParameterValueRaw<T::Image, T::Audio>], _cache: &mut Option<Arc<Self::WholeComponentCacheValue>>) -> Option<MarkerTime> {
        None
    }

    async fn supports_output_type(&self, _fixed_params: &[ParameterValueRaw<T::Image, T::Audio>], out: Parameter<ParameterSelect>, _cache: &mut Option<Arc<Self::WholeComponentCacheValue>>) -> bool {
        matches!(out, Parameter::Image(_))
    }

    async fn process(
        &self,
        parameters: NativeProcessorInput<'_, T>,
        _time: TimelineTime,
        output_type: Parameter<NativeProcessorRequest>,
        _whole_component_cache: &mut Option<Arc<Self::WholeComponentCacheValue>>,
        _framed_cache: &mut Option<Arc<Self::FramedCacheValue>>,
    ) -> ParameterValueRaw<T::Image, T::Audio> {
        let Parameter::Image((width, height)) = output_type else { panic!() };
        let [Parameter::String(text)] = parameters.variable_parameters else { panic!() };
        let (builder, font_list) = parse(text).await;
        let fonts = font_list.iter().map(|&(ref binary, index)| FontRef::from_index(binary, index as usize).unwrap()).collect::<Vec<_>>();
        let result = builder.shape(&fonts, width as f32);
        let mut fill = FillTessellator::new();
        let mut stroke = StrokeTessellator::new();
        let mut buffers = SmallVec::<[_; 4]>::new();
        let mut scaler_context = ScaleContext::new();
        let mut scaler = scaler_context.builder(fonts[0]).build();
        let mut scaler_font_id = 0;
        let mut units_per_em = fonts[0].metrics(&[]).units_per_em as f32;
        let mut glyph_style = Vec::new();
        for (GlyphData { x, y, font_id, font_size, glyph_id }, &TextData { color, ref outline }) in result.glyphs() {
            if buffers.len() < outline.len() + 2 {
                buffers.resize(outline.len() + 2, VertexBuffers::<_, u32>::new());
            }
            if scaler_font_id != font_id {
                scaler = scaler_context.builder(fonts[font_id]).build();
                scaler_font_id = font_id;
                units_per_em = fonts[font_id].metrics(&[]).units_per_em as f32;
            }
            let glyph_style_template = GlyphStyle {
                scale: font_size / units_per_em,
                offset_x: x,
                offset_y: y,
                color: 0,
            };
            if let Some(glyph_outline) = scaler.scale_outline(glyph_id) {
                if glyph_outline.verbs().is_empty() {
                    continue;
                }
                let one_px = units_per_em / font_size;
                let tolerance = one_px / 2.;
                fill.tessellate_with_ids(
                    IdEventIter::new(glyph_outline.verbs()),
                    &Points::new(glyph_outline.points()),
                    None,
                    &FillOptions::even_odd().with_tolerance(tolerance),
                    &mut BuffersBuilder::new(&mut buffers[0], VertexCtor::new(glyph_style.len() as u32)),
                )
                .unwrap();
                glyph_style.push(GlyphStyle {
                    color: u32::from_be_bytes(color),
                    ..glyph_style_template
                });
                // outlineを一つ増やしているのは、そうしないと一番外側の透過部分といっしょにresolveされる部分が透明な黒(#00000000)とブレンドされてくすんでしまうため
                // depth/stencilを上手く使えばもっと簡単に解決できる気がしている(TODO)
                let outline_iter = outline.iter().copied().chain(iter::once((2., outline.last().map_or([color[0], color[1], color[2], 0], |&(_, [r, g, b, _])| [r, g, b, 0])))).scan(0., |sum, (width, color)| {
                    *sum += width;
                    Some((*sum, color))
                });
                for (buffer, (outline_width, outline_color)) in buffers[1..].iter_mut().zip(outline_iter) {
                    stroke
                        .tessellate_with_ids(
                            IdEventIter::new(glyph_outline.verbs()),
                            &Points::new(glyph_outline.points()),
                            None,
                            &StrokeOptions::tolerance(tolerance).with_line_join(LineJoin::Round).with_line_width(outline_width * one_px),
                            &mut BuffersBuilder::new(buffer, VertexCtor::new(glyph_style.len() as u32)),
                        )
                        .unwrap();
                    glyph_style.push(GlyphStyle {
                        color: u32::from_be_bytes(outline_color),
                        ..glyph_style_template
                    });
                }
            }
        }

        let color_image = Image::new(
            Arc::clone(&self.memory_allocator) as Arc<dyn MemoryAllocator>,
            ImageCreateInfo {
                format: Format::R8G8B8A8_UNORM,
                view_formats: vec![Format::R8G8B8A8_UNORM],
                extent: [width, height, 1],
                usage: ImageUsage::COLOR_ATTACHMENT | ImageUsage::TRANSFER_SRC,
                samples: SampleCount::try_from(MULTISAMPLE).unwrap(),
                ..ImageCreateInfo::default()
            },
            AllocationCreateInfo::default(),
        )
        .unwrap();
        let color_resolve_image = Image::new(
            Arc::clone(&self.memory_allocator) as Arc<dyn MemoryAllocator>,
            ImageCreateInfo {
                format: Format::R8G8B8A8_UNORM,
                view_formats: vec![Format::R8G8B8A8_UNORM],
                extent: [width, height, 1],
                usage: ImageUsage::COLOR_ATTACHMENT | ImageUsage::TRANSFER_DST | ImageUsage::TRANSFER_SRC | ImageUsage::SAMPLED,
                ..ImageCreateInfo::default()
            },
            AllocationCreateInfo::default(),
        )
        .unwrap();

        let vertex_buffer_len = buffers.iter().map(|buffer| buffer.vertices.len()).sum::<usize>() as u64;
        let index_buffer_len = buffers.iter().map(|buffer| buffer.indices.len()).sum::<usize>() as u64;

        if index_buffer_len == 0 {
            return Parameter::Image(ImageType(color_resolve_image));
        }

        get_or_create_buffer!(vertex_buffer, vertex_buffer_lock, self.vertex_buffer_queue, self.memory_allocator, vertex_buffer_len, BufferUsage::VERTEX_BUFFER);
        get_or_create_buffer!(index_buffer, index_buffer_lock, self.index_buffer_queue, self.memory_allocator, index_buffer_len, BufferUsage::INDEX_BUFFER);
        let mut index_range = SmallVec::<[_; 4]>::with_capacity(buffers.len());
        let mut v = &mut vertex_buffer_lock[..];
        let mut i = &mut index_buffer_lock[..];
        let mut vertex_offset = 0;
        let mut index_offset = 0;
        for VertexBuffers { vertices, indices } in buffers.iter() {
            index_range.push((index_offset..index_offset + indices.len() as u32, vertex_offset));
            v[..vertices.len()].copy_from_slice(vertices);
            i[..indices.len()].copy_from_slice(indices);
            v = &mut v[vertices.len()..];
            i = &mut i[indices.len()..];
            vertex_offset += vertices.len() as u32;
            index_offset += indices.len() as u32;
        }

        get_or_create_buffer!(glyph_style_buffer, glyph_style_buffer_lock, self.glyph_style_buffer_queue, self.memory_allocator, glyph_style.len() as u64, BufferUsage::STORAGE_BUFFER);
        glyph_style_buffer_lock[..glyph_style.len()].copy_from_slice(&glyph_style);

        drop(vertex_buffer_lock);
        drop(index_buffer_lock);
        drop(glyph_style_buffer_lock);

        let command_buffer = {
            let mut builder = AutoCommandBufferBuilder::primary(&self.command_buffer_allocator, self.queue.queue_family_index(), CommandBufferUsage::OneTimeSubmit).unwrap();
            let color_image_view = ImageView::new(
                Arc::clone(&color_image),
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
            let color_resolve_image_view = ImageView::new(
                Arc::clone(&color_resolve_image),
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
                Arc::clone(&self.render_pass),
                FramebufferCreateInfo {
                    attachments: vec![color_image_view, color_resolve_image_view],
                    extent: [width, height],
                    ..FramebufferCreateInfo::default()
                },
            )
            .unwrap();
            builder
                .begin_render_pass(
                    RenderPassBeginInfo {
                        clear_values: vec![Some(ClearValue::Float([0.; 4])), None],
                        ..RenderPassBeginInfo::framebuffer(frame_buffer)
                    },
                    SubpassBeginInfo::default(),
                )
                .unwrap();
            builder.bind_pipeline_graphics(Arc::clone(&self.pipeline)).unwrap();
            builder
                .set_viewport(
                    0,
                    smallvec![Viewport {
                        offset: [0., 0.],
                        extent: [width as f32, height as f32],
                        depth_range: 0.0..=1.0,
                    }],
                )
                .unwrap();
            let set = PersistentDescriptorSet::new(
                &self.descriptor_set_allocator,
                Arc::clone(&self.pipeline.layout().set_layouts()[1]),
                [WriteDescriptorSet::buffer(0, glyph_style_buffer.clone().slice(..glyph_style.len() as u64))],
                [],
            )
            .unwrap();
            builder.bind_descriptor_sets(PipelineBindPoint::Graphics, Arc::clone(self.pipeline.layout()), 1, set).unwrap();
            builder.bind_vertex_buffers(0, vertex_buffer.clone().slice(..vertex_buffer_len)).unwrap();
            builder.bind_index_buffer(index_buffer.clone().slice(..index_buffer_len)).unwrap();
            let transform = Mat4::from_cols(Vec4::new(2. / width as f32, 0., 0., 0.), Vec4::new(0., 2. / height as f32, 0., 0.), Vec4::new(0., 0., 1., 0.), Vec4::new(-1., -1., 0., 1.));
            builder.push_constants(Arc::clone(self.pipeline.layout()), 0, Constant { transform }).unwrap();
            for (index_range, vertex_offset) in index_range.into_iter().rev() {
                builder.draw_indexed(index_range.end - index_range.start, 1, index_range.start, vertex_offset as i32, 0).unwrap();
            }
            builder.end_render_pass(SubpassEndInfo::default()).unwrap();
            builder.build().unwrap()
        };
        command_buffer.execute(Arc::clone(&self.queue)).unwrap().then_signal_fence_and_flush().unwrap().await.unwrap();

        self.vertex_buffer_queue.push(vertex_buffer);
        self.index_buffer_queue.push(index_buffer);
        self.glyph_style_buffer_queue.push(glyph_style_buffer);

        Parameter::Image(ImageType(color_resolve_image))
    }
}

struct VertexCtor {
    glyph_index: u32,
}

impl VertexCtor {
    fn new(glyph_index: u32) -> Self {
        Self { glyph_index }
    }
}

impl FillVertexConstructor<FontVertex> for VertexCtor {
    fn new_vertex(&mut self, vertex: FillVertex) -> FontVertex {
        let LyonPoint { x, y, .. } = vertex.position();
        FontVertex { x, y, glyph: self.glyph_index }
    }
}

impl StrokeVertexConstructor<FontVertex> for VertexCtor {
    fn new_vertex(&mut self, vertex: StrokeVertex) -> FontVertex {
        let LyonPoint { x, y, .. } = vertex.position();
        FontVertex { x, y, glyph: self.glyph_index }
    }
}

struct Points<'a> {
    points: &'a [ZenoPoint],
}

impl<'a> Points<'a> {
    fn new(points: &'a [ZenoPoint]) -> Self {
        Self { points }
    }
}

impl<'a> PositionStore for Points<'a> {
    fn get_endpoint(&self, EndpointId(id): EndpointId) -> LyonPoint {
        let ZenoPoint { x, y } = self.points[id as usize];
        LyonPoint::new(x, y)
    }

    fn get_control_point(&self, ControlPointId(id): ControlPointId) -> LyonPoint {
        let ZenoPoint { x, y } = self.points[id as usize];
        LyonPoint::new(x, y)
    }
}

struct IdEventIter<'a> {
    verbs: slice::Iter<'a, Verb>,
    point_index: u32,
    first: u32,
    end: bool,
}

impl<'a> IdEventIter<'a> {
    fn new(verbs: &'a [Verb]) -> Self {
        Self {
            verbs: verbs.iter(),
            point_index: 0,
            first: 0,
            end: false,
        }
    }
}

impl<'a> Iterator for IdEventIter<'a> {
    type Item = IdEvent;

    fn next(&mut self) -> Option<Self::Item> {
        let Some(verb) = self.verbs.next() else {
            if self.end {
                return None;
            }
            self.end = true;
            return Some(IdEvent::End {
                last: EndpointId(self.point_index - 1),
                first: EndpointId(self.first),
                close: false,
            });
        };
        self.end = false;
        match verb {
            Verb::MoveTo => {
                self.point_index += 1;
                Some(IdEvent::Begin { at: EndpointId(self.point_index - 1) })
            }
            Verb::LineTo => {
                self.point_index += 1;
                Some(IdEvent::Line {
                    from: EndpointId(self.point_index - 2),
                    to: EndpointId(self.point_index - 1),
                })
            }
            Verb::CurveTo => {
                self.point_index += 3;
                Some(IdEvent::Cubic {
                    from: EndpointId(self.point_index - 4),
                    ctrl1: ControlPointId(self.point_index - 3),
                    ctrl2: ControlPointId(self.point_index - 2),
                    to: EndpointId(self.point_index - 1),
                })
            }
            Verb::QuadTo => {
                self.point_index += 2;
                Some(IdEvent::Quadratic {
                    from: EndpointId(self.point_index - 3),
                    ctrl: ControlPointId(self.point_index - 2),
                    to: EndpointId(self.point_index - 1),
                })
            }
            Verb::Close => {
                self.end = true;
                let first = mem::replace(&mut self.first, self.point_index);
                Some(IdEvent::End {
                    last: EndpointId(self.point_index - 1),
                    first: EndpointId(first),
                    close: true,
                })
            }
        }
    }
}

async fn parse(text: &str) -> (ShapingBuilder<TextData>, Vec<(Arc<Vec<u8>>, u32)>) {
    thread_local!(static FONT_SOURCE: SystemSource = SystemSource::new());
    async fn inner(iter: &mut Peekable<RichTextParser<'_>>, mut builder: ShapingBuilderSegment<'_, TextData>, open_tag_name: Option<&str>, font_list: &mut Vec<(Arc<Vec<u8>>, u32)>, font_map: &mut HashMap<String, usize>) {
        while let Some(token) = iter.peek().cloned() {
            match token {
                RichTextToken::Text(text) => {
                    iter.next();
                    builder.push_str(text);
                }
                RichTextToken::TagOpen { raw, tag_name, value } => {
                    iter.next();
                    match tag_name {
                        "size" => {
                            let mut s;
                            let s = match value.as_slice() {
                                &[item] => item,
                                list => {
                                    s = String::new();
                                    for v in list {
                                        s.push_str(v);
                                    }
                                    &s
                                }
                            };
                            let Ok(size) = s.trim().parse() else {
                                builder.push_str(raw);
                                continue;
                            };
                            Box::pin(inner(iter, builder.font_size(size), Some(tag_name), font_list, font_map)).await;
                        }
                        "font" => {
                            let mut s;
                            let s = match value.as_slice() {
                                &[item] => item,
                                list => {
                                    s = String::new();
                                    for v in list {
                                        s.push_str(v);
                                    }
                                    &s
                                }
                            };
                            let mut fonts = Vec::new();
                            for font_name in s.split('/') {
                                match font_map.entry(font_name.to_owned()) {
                                    Entry::Occupied(entry) => {
                                        fonts.push(*entry.get());
                                    }
                                    Entry::Vacant(entry) => {
                                        let Ok(font) = FONT_SOURCE.with(|source| source.select_family_by_name(font_name)) else {
                                            continue;
                                        };
                                        let [font, ..] = font.fonts() else {
                                            continue;
                                        };
                                        let (data, font_index) = match *font {
                                            Handle::Path { ref path, font_index } => {
                                                let Ok(data) = tokio::fs::read(path).await else {
                                                    continue;
                                                };
                                                if FontRef::from_index(&data, font_index as usize).is_none() {
                                                    continue;
                                                }
                                                (Arc::new(data), font_index)
                                            }
                                            Handle::Memory { ref bytes, font_index } => {
                                                if FontRef::from_index(bytes, font_index as usize).is_none() {
                                                    continue;
                                                }
                                                (Arc::clone(bytes), font_index)
                                            }
                                        };
                                        fonts.push(font_list.len());
                                        entry.insert(font_list.len());
                                        font_list.push((data, font_index));
                                    }
                                }
                            }
                            Box::pin(inner(iter, builder.font(fonts), Some(tag_name), font_list, font_map)).await;
                        }
                        "color" => {
                            let mut s;
                            let s = match value.as_slice() {
                                &[item] => item,
                                list => {
                                    s = String::new();
                                    for v in list {
                                        s.push_str(v);
                                    }
                                    &s
                                }
                            };
                            let Some(color) = color::parse_color(s) else {
                                builder.push_str(raw);
                                continue;
                            };
                            Box::pin(inner(iter, builder.update_user_data(|user_data| user_data.clone_with_color(color)), Some(tag_name), font_list, font_map)).await;
                        }
                        "outline" => {
                            let mut s;
                            let s = match value.as_slice() {
                                &[item] => item,
                                list => {
                                    s = String::new();
                                    for v in list {
                                        s.push_str(v);
                                    }
                                    &s
                                }
                            };
                            let mut outline = Vec::new();
                            for item in s.split('/') {
                                let mut iter = item.splitn(2, ':');
                                let Some(color) = iter.next().and_then(color::parse_color) else {
                                    continue;
                                };
                                let width = iter.next().and_then(|w| w.parse().ok()).unwrap_or(5.);
                                outline.push((width, color));
                            }
                            Box::pin(inner(iter, builder.update_user_data(|user_data| user_data.clone_with_outline(outline)), Some(tag_name), font_list, font_map)).await;
                        }
                        _ => {
                            builder.push_str(raw);
                        }
                    }
                }
                RichTextToken::TagClose { raw, tag_name } => match (open_tag_name, tag_name) {
                    (Some(open_tag_name), Some(tag_name)) if open_tag_name == tag_name => {
                        iter.next();
                        return;
                    }
                    (Some(_), None) => {
                        return;
                    }
                    (Some(_), Some(_)) => return,
                    (None, _) => {
                        iter.next();
                        builder.push_str(raw);
                    }
                },
            }
        }
    }
    let mut iter = rich_text::parse(text).peekable();
    let mut builder = ShapingBuilder::new(TextData::default());
    let font = FONT_SOURCE.with(|source| source.select_best_match(&[FamilyName::Serif], &Properties::default())).unwrap();
    let font_data = match font {
        Handle::Path { path, font_index } => {
            let data = tokio::fs::read(path).await.unwrap();
            (Arc::new(data), font_index)
        }
        Handle::Memory { bytes, font_index } => (bytes, font_index),
    };
    let mut font_list = vec![font_data];
    let mut font_map = HashMap::new();
    inner(&mut iter, builder.font_size(50.), None, &mut font_list, &mut font_map).await;
    (builder, font_list)
}

#[derive(Clone)]
struct TextData {
    color: [u8; 4],
    outline: Vec<(f32, [u8; 4])>,
}

impl Default for TextData {
    fn default() -> Self {
        Self { color: [0, 0, 0, 255], outline: Vec::new() }
    }
}

impl TextData {
    fn clone_with_color(&self, color: [u8; 4]) -> Self {
        Self { color, outline: self.outline.clone() }
    }

    fn clone_with_outline(&self, outline: Vec<(f32, [u8; 4])>) -> Self {
        Self { outline, color: self.color }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;
    use std::path::Path;
    use vulkano::command_buffer::CopyImageToBufferInfo;
    use vulkano::device::Features;
    use vulkano_util::context::{VulkanoConfig, VulkanoContext};

    struct T;

    impl ParameterValueType for T {
        type Image = ImageType;
        type Audio = ();
        type Binary = ();
        type String = ();
        type Integer = ();
        type RealNumber = ();
        type Boolean = ();
        type Dictionary = ();
        type Array = ();
        type ComponentClass = ();
    }

    #[tokio::test]
    async fn test_render_text() {
        const TEST_OUTPUT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../test_output/", env!("CARGO_PKG_NAME"));
        let output_dir = Path::new(TEST_OUTPUT_DIR).join("text_renderer");
        tokio::fs::create_dir_all(&output_dir).await.unwrap();
        let context = VulkanoContext::new(VulkanoConfig {
            device_features: Features {
                occlusion_query_precise: true,
                pipeline_statistics_query: true,
                ..Features::default()
            },
            ..VulkanoConfig::default()
        });
        let renderer = TextRenderer::new(context.device(), context.graphics_queue(), context.memory_allocator());
        let Parameter::Image(ImageType(image)) = ComponentProcessorNative::<T>::process(
            &renderer,
            NativeProcessorInput {
                fixed_parameters: &[],
                variable_parameters: &[Parameter::String("<size=150><font=Times New Roman/Yu Gothic UI><color=#252525><outline=white:10>あのイーハトーヴォのすきとおった<color=whitesmoke><outline=lightskyblue:10>Wind</color>、<color=yellow><outline=orange:10>Summer</color>でも底に冷たさをもつ青い<color=whitesmoke><outline=cyan:10>Sky</color>、うつくしい<color=peru><outline=green:10>Forest</color>で飾られたモリーオ市、郊外のぎらぎらひかる<color=whitesmoke><outline=lime:10>Grass</color>の波。</></></>".to_owned())],
                variable_parameter_type: &[("text".to_owned(), Parameter::String(()))],
            },
            TimelineTime::ZERO,
            Parameter::Image((1920, 1080)),
            &mut None,
            &mut None,
        )
            .await
            else {
                panic!()
            };
        let buffer = Buffer::new_slice::<u8>(
            Arc::clone(context.memory_allocator()) as Arc<dyn MemoryAllocator>,
            BufferCreateInfo {
                usage: BufferUsage::TRANSFER_DST,
                ..BufferCreateInfo::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::HOST_RANDOM_ACCESS,
                ..AllocationCreateInfo::default()
            },
            1920 * 1080 * 4,
        )
        .unwrap();
        let command_buffer_allocator = StandardCommandBufferAllocator::new(Arc::clone(context.device()), StandardCommandBufferAllocatorCreateInfo::default());
        let command_buffer = {
            let mut builder = AutoCommandBufferBuilder::primary(&command_buffer_allocator, context.graphics_queue().queue_family_index(), CommandBufferUsage::OneTimeSubmit).unwrap();
            builder.copy_image_to_buffer(CopyImageToBufferInfo::image_buffer(image, buffer.clone())).unwrap();
            builder.build().unwrap()
        };
        command_buffer.execute(Arc::clone(context.graphics_queue())).unwrap().then_signal_fence_and_flush().unwrap().await.unwrap();
        let buffer = buffer.read().unwrap();
        RgbaImage::from_vec(1920, 1080, buffer.to_vec()).unwrap().save(output_dir.join("text.png")).unwrap();
    }
}
