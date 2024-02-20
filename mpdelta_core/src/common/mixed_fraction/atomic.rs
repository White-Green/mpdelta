use crate::common::mixed_fraction::MixedFraction;
use std::sync::atomic;
use std::sync::atomic::AtomicI64;

pub struct AtomicMixedFraction(AtomicI64);

impl AtomicMixedFraction {
    pub fn new(value: MixedFraction) -> AtomicMixedFraction {
        AtomicMixedFraction(AtomicI64::new(value.0))
    }

    pub fn load(&self, ordering: atomic::Ordering) -> MixedFraction {
        MixedFraction(self.0.load(ordering))
    }

    pub fn store(&self, value: MixedFraction, ordering: atomic::Ordering) {
        self.0.store(value.0, ordering);
    }
}
