use crate::component::marker_pin::MarkerPin;
use crate::ptr::StaticPointer;
use crate::time::TimelineTime;
use std::mem;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct MarkerLink {
    pub from: StaticPointer<RwLock<MarkerPin>>,
    pub to: StaticPointer<RwLock<MarkerPin>>,
    pub len: TimelineTime,
}

impl MarkerLink {
    pub fn flip(&mut self) {
        mem::swap(&mut self.from, &mut self.to);
    }
}
