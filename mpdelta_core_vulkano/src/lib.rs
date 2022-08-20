use std::sync::Arc;
use vulkano::image::ImageAccess;

#[derive(Debug, Clone)]
pub struct ImageType(pub Arc<dyn ImageAccess + 'static>);

impl From<Arc<dyn ImageAccess + 'static>> for ImageType {
    fn from(value: Arc<dyn ImageAccess + 'static>) -> Self {
        ImageType(value as Arc<dyn ImageAccess + 'static>)
    }
}

impl From<ImageType> for Arc<dyn ImageAccess + 'static> {
    fn from(ImageType(value): ImageType) -> Self {
        value
    }
}
