use async_trait::async_trait;
use mpdelta_core::component::parameter::{ParameterType, ParameterTypeExceptComponentClass, ParameterValueFixed, ParameterValueType, ValueFixed};
use mpdelta_core::component::processor::{ComponentProcessor, ComponentProcessorBody, NativeProcessorExecutable};
use mpdelta_core::native::processor::{NativeProcessor, ParameterNativeProcessorInputFixed};
use mpdelta_core_vulkano::ImageType;
use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;
use vulkano::device::Queue;
use vulkano::format::Format;
use vulkano::image::{ImageAccess, ImageDimensions, ImmutableImage, MipmapsCount};
use vulkano::sync::GpuFuture;

#[derive(Clone)]
pub struct Rectangle(Arc<dyn ImageAccess + 'static>);

impl Rectangle {
    pub fn new(queue: Arc<Queue>) -> Rectangle {
        let (image, future) = ImmutableImage::from_iter([255u8; 4], ImageDimensions::Dim2d { width: 1, height: 1, array_layers: 1 }, MipmapsCount::One, Format::R8G8B8A8_UNORM, queue).unwrap();
        future.then_signal_fence().wait(None).unwrap();
        Rectangle(image)
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
