use crate::component::marker_pin::MarkerPinHandle;
use crate::ptr::{StaticPointer, StaticPointerCow, StaticPointerOwned};
use crate::time::TimelineTime;
use qcell::TCell;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::mem;

#[derive(Debug)]
pub struct MarkerLink<K> {
    from: MarkerPinHandle<K>,
    to: MarkerPinHandle<K>,
    len: TimelineTime,
}

pub type MarkerLinkHandle<K> = StaticPointer<TCell<K, MarkerLink<K>>>;
pub type MarkerLinkHandleOwned<K> = StaticPointerOwned<TCell<K, MarkerLink<K>>>;
pub type MarkerLinkHandleCow<K> = StaticPointerCow<TCell<K, MarkerLink<K>>>;

impl<K> MarkerLink<K> {
    #[track_caller]
    pub fn new(from: MarkerPinHandle<K>, to: MarkerPinHandle<K>, len: TimelineTime) -> MarkerLink<K> {
        assert_ne!(from, to);
        MarkerLink { from, to, len }
    }

    pub fn from(&self) -> &MarkerPinHandle<K> {
        &self.from
    }

    pub fn to(&self) -> &MarkerPinHandle<K> {
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
