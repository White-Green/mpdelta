use async_trait::async_trait;
use mpdelta_core::component::class::ComponentClass;
use mpdelta_core::component::instance::ComponentInstance;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerTime};
use mpdelta_core::component::parameter::{ImageRequiredParams, ParameterType, ParameterTypeExceptComponentClass, ParameterValueFixed, ParameterValueType};
use mpdelta_core::component::processor::{ComponentProcessor, ComponentProcessorBody, NativeProcessorExecutable};
use mpdelta_core::native::processor::{NativeProcessor, ParameterNativeProcessorInputFixed};
use mpdelta_core::ptr::{StaticPointer, StaticPointerCow, StaticPointerOwned};
use mpdelta_core::time::TimelineTime;
use mpdelta_core_vulkano::ImageType;
use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use vulkano::device::Queue;
use vulkano::format::Format;
use vulkano::image::{ImageAccess, ImageDimensions, ImmutableImage, MipmapsCount};
use vulkano::sync::GpuFuture;

#[derive(Debug, Clone)]
pub struct RectangleClass(Arc<Rectangle>);

#[derive(Debug, Clone)]
pub struct Rectangle(Arc<dyn ImageAccess + 'static>);

impl RectangleClass {
    pub fn new(queue: Arc<Queue>) -> RectangleClass {
        RectangleClass(Arc::new(Rectangle::new(queue)))
    }
}

impl Rectangle {
    pub fn new(queue: Arc<Queue>) -> Rectangle {
        let (image, future) = ImmutableImage::from_iter([255u8; 4], ImageDimensions::Dim2d { width: 1, height: 1, array_layers: 1 }, MipmapsCount::One, Format::R8G8B8A8_UNORM, queue).unwrap();
        future.then_signal_fence().wait(None).unwrap();
        Rectangle(image)
    }
}

#[async_trait]
impl<T: ParameterValueType<'static, Image = ImageType>> ComponentClass<T> for RectangleClass {
    async fn generate_image(&self) -> bool {
        true
    }

    async fn generate_audio(&self) -> bool {
        false
    }

    async fn fixed_parameter_type(&self) -> &[(String, ParameterType)] {
        &[]
    }

    async fn default_variable_parameter_type(&self) -> &[(String, ParameterType)] {
        &[]
    }

    async fn instantiate(&self, this: &StaticPointer<RwLock<dyn ComponentClass<T>>>) -> ComponentInstance<T> {
        let left = StaticPointerOwned::new(RwLock::new(MarkerPin::new(TimelineTime::ZERO, MarkerTime::ZERO)));
        let right = StaticPointerOwned::new(RwLock::new(MarkerPin::new(TimelineTime::new(1.).unwrap(), MarkerTime::new(1.).unwrap())));
        let image_required_params = ImageRequiredParams::new_default(StaticPointerOwned::reference(&left), StaticPointerOwned::reference(&right));
        ComponentInstance::new_no_param(this.clone(), StaticPointerCow::Owned(left), StaticPointerCow::Owned(right), Some(image_required_params), None, Arc::clone(&self.0) as _)
    }
}

#[async_trait]
impl<T: ParameterValueType<'static, Image = ImageType>> ComponentProcessor<T> for Rectangle {
    async fn update_variable_parameter(&self, _: &mut [ParameterValueFixed], _: &mut Vec<(String, ParameterType)>) {}

    async fn natural_length(&self, _: &[ParameterValueFixed]) -> Duration {
        Duration::from_secs(1)
    }

    async fn get_processor(&self) -> ComponentProcessorBody<'_, T> {
        let rectangle = self.clone();
        ComponentProcessorBody::Native(Cow::Owned(vec![Arc::new(move |_: &_, _: &_| NativeProcessorExecutable {
            processor: Arc::new(rectangle.clone()),
            parameter: Arc::new([]),
        }) as _]))
    }
}

impl<T: ParameterValueType<'static, Image = ImageType>> NativeProcessor<T> for Rectangle {
    fn parameter_type(&self) -> &[ParameterTypeExceptComponentClass] {
        &[]
    }

    fn return_type(&self) -> &ParameterTypeExceptComponentClass {
        const TYPE: &ParameterTypeExceptComponentClass = &ParameterTypeExceptComponentClass::Image(());
        TYPE
    }

    fn process(&self, _: &[ParameterNativeProcessorInputFixed<ImageType, T::Audio>]) -> ParameterNativeProcessorInputFixed<ImageType, T::Audio> {
        ParameterNativeProcessorInputFixed::Image(ImageType(Arc::clone(&self.0)))
    }
}
