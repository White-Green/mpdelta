use crate::component::marker_pin::MarkerPinId;
use crate::time::TimelineTime;
use std::fmt::Debug;
use std::hash::Hash;
use std::mem;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MarkerLink {
    from: MarkerPinId,
    to: MarkerPinId,
    len: TimelineTime,
}

impl MarkerLink {
    #[track_caller]
    pub fn new(from: MarkerPinId, to: MarkerPinId, len: TimelineTime) -> MarkerLink {
        assert_ne!(from, to);
        MarkerLink { from, to, len }
    }

    pub fn from(&self) -> &MarkerPinId {
        &self.from
    }

    pub fn to(&self) -> &MarkerPinId {
        &self.to
    }

    pub fn len(&self) -> TimelineTime {
        self.len
    }

    pub fn set_len(&mut self, len: TimelineTime) {
        self.len = len;
    }

    pub fn flip(&mut self) {
        mem::swap(&mut self.from, &mut self.to);
    }
}
