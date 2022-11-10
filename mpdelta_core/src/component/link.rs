use crate::component::marker_pin::MarkerPin;
use crate::ptr::StaticPointer;
use crate::time::TimelineTime;
use qcell::TCell;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::mem;

#[derive(Debug)]
pub struct MarkerLink<K> {
    pub from: StaticPointer<TCell<K, MarkerPin>>,
    pub to: StaticPointer<TCell<K, MarkerPin>>,
    pub len: TimelineTime,
}

impl<K> MarkerLink<K> {
    pub fn flip(&mut self) {
        mem::swap(&mut self.from, &mut self.to);
    }
}

impl<K> Clone for MarkerLink<K> {
    fn clone(&self) -> Self {
        let MarkerLink { ref from, ref to, len } = *self;
        MarkerLink { from: from.clone(), to: to.clone(), len }
    }
}

impl<K> PartialEq for MarkerLink<K> {
    fn eq(&self, other: &Self) -> bool {
        self.from == other.from && self.to == other.to && self.len == other.len
    }
}

impl<K> Eq for MarkerLink<K> {}

impl<K> Hash for MarkerLink<K> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.from.hash(state);
        self.to.hash(state);
        self.len.hash(state);
    }
}
