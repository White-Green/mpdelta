use crate::component::marker_pin::MarkerTime;
use std::ops::Range;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ImagePlaceholder {
    id: Uuid,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct TimedImagePlaceholder {
    pub id: ImagePlaceholder,
    pub time: Vec<Option<Range<MarkerTime>>>,
}

#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct AudioPlaceholder {
    id: Uuid,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct TimedAudioPlaceholder {
    pub id: AudioPlaceholder,
    pub time: Vec<Option<Range<MarkerTime>>>,
}
