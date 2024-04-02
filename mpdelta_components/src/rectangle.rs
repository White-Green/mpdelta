use async_trait::async_trait;
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::class::{ComponentClass, ComponentClassIdentifier};
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerTime};
use mpdelta_core::component::parameter::{ImageRequiredParams, Parameter, ParameterSelect, ParameterType, ParameterValueRaw, ParameterValueType};
use mpdelta_core::component::processor::{ComponentProcessor, ComponentProcessorNative, ComponentProcessorNativeDyn, ComponentProcessorWrapper, NativeProcessorInput, NativeProcessorRequest};
use mpdelta_core::ptr::{StaticPointer, StaticPointerCow, StaticPointerOwned};
use mpdelta_core::time::TimelineTime;
use mpdelta_core_vulkano::ImageType;
use qcell::TCell;
use std::borrow::Cow;
use std::sync::Arc;
use tokio::sync::RwLock;
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::command_buffer::{AutoCommandBufferBuilder, ClearColorImageInfo, CommandBufferUsage, PrimaryCommandBufferAbstract};
use vulkano::device::Queue;
use vulkano::format::{ClearColorValue, Format};
use vulkano::image::{Image, ImageCreateInfo, ImageUsage};
use vulkano::memory::allocator::{AllocationCreateInfo, FreeListAllocator, GenericMemoryAllocator, MemoryAllocator};
use vulkano::sync::GpuFuture;

#[derive(Debug, Clone)]
pub struct RectangleClass(Arc<Rectangle>);

#[derive(Debug, Clone)]
pub struct Rectangle(Arc<Image>);

impl RectangleClass {
    pub fn new(queue: Arc<Queue>, allocator: &Arc<GenericMemoryAllocator<FreeListAllocator>>, command_buffer_allocator: &StandardCommandBufferAllocator) -> RectangleClass {
        RectangleClass(Arc::new(Rectangle::new(queue, allocator, command_buffer_allocator)))
    }
}

impl Rectangle {
    pub fn new(queue: Arc<Queue>, allocator: &Arc<GenericMemoryAllocator<FreeListAllocator>>, command_buffer_allocator: &StandardCommandBufferAllocator) -> Rectangle {
        let image = Image::new(
            Arc::clone(allocator) as Arc<dyn MemoryAllocator>,
            ImageCreateInfo {
                format: Format::R8G8B8A8_UNORM,
                extent: [1, 1, 1],
                usage: ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
                ..ImageCreateInfo::default()
            },
            AllocationCreateInfo::default(),
        )
        .unwrap();
        let mut builder = AutoCommandBufferBuilder::primary(command_buffer_allocator, queue.queue_family_index(), CommandBufferUsage::OneTimeSubmit).unwrap();
        builder
            .clear_color_image(ClearColorImageInfo {
                clear_value: ClearColorValue::Float([1.0; 4]),
                ..ClearColorImageInfo::image(Arc::clone(&image))
            })
            .unwrap();
        builder.build().unwrap().execute(queue).unwrap().then_signal_fence_and_flush().unwrap().wait(None).unwrap();
        Rectangle(image)
    }
}

#[async_trait]
impl<K, T: ParameterValueType<Image = ImageType>> ComponentClass<K, T> for RectangleClass {
    fn identifier(&self) -> ComponentClassIdentifier {
        ComponentClassIdentifier {
            namespace: Cow::Borrowed("mpdelta"),
            name: Cow::Borrowed("Rectangle"),
            inner_identifier: Default::default(),
        }
    }

    fn processor(&self) -> ComponentProcessorWrapper<K, T> {
        ComponentProcessorWrapper::Native(Arc::clone(&self.0) as Arc<dyn ComponentProcessorNativeDyn<K, T>>)
    }

    async fn instantiate(&self, this: &StaticPointer<RwLock<dyn ComponentClass<K, T>>>) -> ComponentInstance<K, T> {
        let left = StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::ZERO, MarkerTime::ZERO)));
        let right = StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(MixedFraction::from_integer(1)), MarkerTime::new(MixedFraction::from_integer(1)).unwrap())));
        let image_required_params = ImageRequiredParams::new_default(StaticPointerOwned::reference(&left), StaticPointerOwned::reference(&right));
        ComponentInstance::builder(this.clone(), StaticPointerCow::Owned(left), StaticPointerCow::Owned(right), Vec::new(), Arc::clone(&self.0) as Arc<dyn ComponentProcessorNativeDyn<K, T>>)
            .image_required_params(image_required_params)
            .build()
    }
}

#[async_trait]
impl<K, T: ParameterValueType<Image = ImageType>> ComponentProcessor<K, T> for Rectangle {
    async fn fixed_parameter_types(&self) -> &[(String, ParameterType)] {
        &[]
    }

    async fn update_variable_parameter(&self, _: &[ParameterValueRaw<T::Image, T::Audio>], variable_parameters: &mut Vec<(String, ParameterType)>) {
        variable_parameters.clear();
    }
}

#[async_trait]
impl<K, T: ParameterValueType<Image = ImageType>> ComponentProcessorNative<K, T> for Rectangle {
    type WholeComponentCacheKey = ();
    type WholeComponentCacheValue = ();
    type FramedCacheKey = ();
    type FramedCacheValue = ();

    fn whole_component_cache_key(&self, _fixed_parameters: &[ParameterValueRaw<T::Image, T::Audio>]) -> Option<Self::WholeComponentCacheKey> {
        None
    }

    fn framed_cache_key(&self, _: NativeProcessorInput<'_, T>, _: TimelineTime, _: Parameter<ParameterSelect>) -> Option<Self::WholeComponentCacheKey> {
        None
    }

    async fn natural_length(&self, _: &[ParameterValueRaw<T::Image, T::Audio>], _cache: &mut Option<Arc<Self::WholeComponentCacheValue>>) -> Option<MarkerTime> {
        None
    }

    async fn supports_output_type(&self, _fixed_params: &[ParameterValueRaw<T::Image, T::Audio>], out: Parameter<ParameterSelect>, _cache: &mut Option<Arc<Self::WholeComponentCacheValue>>) -> bool {
        matches!(out, Parameter::Image(_))
    }

    async fn process(
        &self,
        _parameters: NativeProcessorInput<'_, T>,
        _time: TimelineTime,
        _output_type: Parameter<NativeProcessorRequest>,
        _whole_component_cache: &mut Option<Arc<Self::WholeComponentCacheValue>>,
        _framed_cache: &mut Option<Arc<Self::FramedCacheValue>>,
    ) -> ParameterValueRaw<T::Image, T::Audio> {
        ParameterValueRaw::Image(ImageType(Arc::clone(&self.0)))
    }
}
