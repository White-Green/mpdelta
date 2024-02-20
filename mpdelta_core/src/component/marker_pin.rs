use crate::common::mixed_fraction::MixedFraction;
use crate::ptr::{StaticPointer, StaticPointerCow, StaticPointerOwned};
use crate::time::{AtomicTimelineTime, TimelineTime};
use qcell::TCell;
use std::hash::Hash;
use std::sync::atomic;

/// 固定マーカの位置のコンポーネントの長さに対する割合
/// \[0.0, ∞)
#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct MarkerTime(MixedFraction);

#[derive(Debug, Clone)]
pub struct MarkerPin {
    cached_timeline_time: AtomicTimelineTime,
    locked_component_time: Option<MarkerTime>,
}

pub type MarkerPinHandle<K> = StaticPointer<TCell<K, MarkerPin>>;
pub type MarkerPinHandleOwned<K> = StaticPointerOwned<TCell<K, MarkerPin>>;
pub type MarkerPinHandleCow<K> = StaticPointerCow<TCell<K, MarkerPin>>;

impl MarkerTime {
    pub const ZERO: MarkerTime = MarkerTime(MixedFraction::ZERO);

    pub fn new(value: MixedFraction) -> Option<MarkerTime> {
        (MixedFraction::ZERO <= value).then_some(MarkerTime(value))
    }

    pub fn value(&self) -> MixedFraction {
        self.0
    }
}

impl MarkerPin {
    pub fn new(timeline_time: TimelineTime, component_time: MarkerTime) -> MarkerPin {
        MarkerPin {
            cached_timeline_time: timeline_time.into(),
            locked_component_time: Some(component_time),
        }
    }

    pub fn new_unlocked(timeline_time: TimelineTime) -> MarkerPin {
        MarkerPin {
            cached_timeline_time: timeline_time.into(),
            locked_component_time: None,
        }
    }

    pub fn cached_timeline_time(&self) -> TimelineTime {
        self.cached_timeline_time.load(atomic::Ordering::Acquire)
    }

    pub fn cache_timeline_time(&self, time: TimelineTime) {
        self.cached_timeline_time.store(time, atomic::Ordering::Release);
    }

    pub fn locked_component_time(&self) -> Option<MarkerTime> {
        self.locked_component_time
    }

    pub fn set_locked_component_time(&mut self, time: Option<MarkerTime>) {
        self.locked_component_time = time;
    }
}
