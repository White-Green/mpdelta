use std::sync::Arc;
use vulkano::image::Image;

#[derive(Debug, Clone)]
pub struct ImageType(pub Arc<Image>);

impl From<Arc<Image>> for ImageType {
    fn from(value: Arc<Image>) -> Self {
        ImageType(value)
    }
}

impl From<ImageType> for Arc<Image> {
    fn from(ImageType(value): ImageType) -> Self {
        value
    }
}
