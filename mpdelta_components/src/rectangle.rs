use async_trait::async_trait;
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::class::{ComponentClass, ComponentClassIdentifier};
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerTime};
use mpdelta_core::component::parameter::{ImageRequiredParams, Parameter, ParameterSelect, ParameterType, ParameterTypeExceptComponentClass, ParameterValueRaw, ParameterValueType};
use mpdelta_core::component::processor::{ComponentProcessor, ComponentProcessorNative, ComponentProcessorWrapper};
use mpdelta_core::native::processor::{NativeProcessor, ParameterNativeProcessorInputFixed};
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
        ComponentProcessorWrapper::Native(Arc::clone(&self.0) as Arc<dyn ComponentProcessorNative<K, T>>)
    }

    async fn instantiate(&self, this: &StaticPointer<RwLock<dyn ComponentClass<K, T>>>) -> ComponentInstance<K, T> {
        let left = StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::ZERO, MarkerTime::ZERO)));
        let right = StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::new(MixedFraction::from_integer(1)), MarkerTime::new(MixedFraction::from_integer(1)).unwrap())));
        let image_required_params = ImageRequiredParams::new_default(StaticPointerOwned::reference(&left), StaticPointerOwned::reference(&right));
        ComponentInstance::builder(this.clone(), StaticPointerCow::Owned(left), StaticPointerCow::Owned(right), Vec::new(), Arc::clone(&self.0) as Arc<dyn ComponentProcessorNative<K, T>>)
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

    async fn natural_length(&self, _: &[ParameterValueRaw<T::Image, T::Audio>]) -> MarkerTime {
        MarkerTime::new(MixedFraction::ONE).unwrap()
    }
}

#[async_trait]
impl<K, T: ParameterValueType<Image = ImageType>> ComponentProcessorNative<K, T> for Rectangle {
    fn supports_output_type(&self, out: Parameter<ParameterSelect>) -> bool {
        matches!(out, Parameter::Image(_))
    }

    async fn process(
        &self,
        _fixed_parameters: &[ParameterValueRaw<T::Image, T::Audio>],
        _variable_parameters: &[ParameterValueRaw<T::Image, T::Audio>],
        _variable_parameter_type: &[(String, ParameterType)],
        _time: TimelineTime,
        _output_type: Parameter<ParameterSelect>,
    ) -> ParameterValueRaw<T::Image, T::Audio> {
        ParameterValueRaw::Image(ImageType(Arc::clone(&self.0)))
    }
}

impl<T: ParameterValueType<Image = ImageType>> NativeProcessor<T> for Rectangle {
    fn parameter_type(&self) -> &[ParameterTypeExceptComponentClass] {
        &[]
    }

    fn return_type(&self) -> &ParameterTypeExceptComponentClass {
        const TYPE: &ParameterTypeExceptComponentClass = &ParameterTypeExceptComponentClass::Image(());
        TYPE
    }

    fn has_same_output(&self, _: TimelineTime, _: TimelineTime, _: &[ParameterNativeProcessorInputFixed<ImageType, T::Audio>]) -> bool {
        true
    }

    fn process(&self, _: TimelineTime, _: &[ParameterNativeProcessorInputFixed<ImageType, T::Audio>]) -> ParameterNativeProcessorInputFixed<ImageType, T::Audio> {
        ParameterNativeProcessorInputFixed::Image(ImageType(Arc::clone(&self.0)))
    }
}
