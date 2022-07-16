use crate::component::marker_pin::MarkerPin;
use crate::ptr::StaticPointer;
use std::mem;
use std::time::Duration;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct MarkerLink {
    pub from: StaticPointer<MarkerPin>,
    pub to: StaticPointer<MarkerPin>,
    pub len: Duration,
}

impl MarkerLink {
    pub fn flip(&mut self) {
        mem::swap(&mut self.from, &mut self.to);
    }
}
