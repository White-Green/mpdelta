use crate::common::mixed_fraction::atomic::AtomicMixedFraction;
use crate::common::mixed_fraction::MixedFraction;
use crate::component::marker_pin::MarkerTime;
use std::fmt::{Debug, Formatter};
use std::hash::Hash;
use std::ops::{Add, Div, Neg, Sub};
use std::sync::atomic;

/// タイムライン上での時間(秒)
/// (-∞, ∞)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimelineTime(MixedFraction);

impl TimelineTime {
    pub const ZERO: TimelineTime = TimelineTime(MixedFraction::ZERO);
    pub const MAX: TimelineTime = TimelineTime(MixedFraction::MAX);
    pub const MIN: TimelineTime = TimelineTime(MixedFraction::MIN);

    pub fn new(time: MixedFraction) -> TimelineTime {
        TimelineTime(time)
    }

    pub fn value(&self) -> MixedFraction {
        self.0
    }
}

impl From<MarkerTime> for TimelineTime {
    fn from(value: MarkerTime) -> Self {
        TimelineTime(value.value())
    }
}

impl Neg for TimelineTime {
    type Output = TimelineTime;

    fn neg(self) -> Self::Output {
        TimelineTime(self.0.saturating_neg())
    }
}

impl Add for TimelineTime {
    type Output = TimelineTime;

    fn add(self, rhs: Self) -> Self::Output {
        TimelineTime(self.0.saturating_add(rhs.0))
    }
}

impl Sub for TimelineTime {
    type Output = TimelineTime;

    fn sub(self, rhs: Self) -> Self::Output {
        TimelineTime(self.0.saturating_sub(rhs.0))
    }
}

impl Div for TimelineTime {
    type Output = MixedFraction;

    fn div(self, rhs: Self) -> Self::Output {
        self.0 / rhs.0
    }
}

pub struct AtomicTimelineTime(AtomicMixedFraction);

impl AtomicTimelineTime {
    pub fn new(value: TimelineTime) -> AtomicTimelineTime {
        AtomicTimelineTime(AtomicMixedFraction::new(value.0))
    }

    pub fn load(&self, ordering: atomic::Ordering) -> TimelineTime {
        TimelineTime(self.0.load(ordering))
    }

    pub fn store(&self, value: TimelineTime, ordering: atomic::Ordering) {
        self.0.store(value.0, ordering)
    }
}

impl From<TimelineTime> for AtomicTimelineTime {
    fn from(value: TimelineTime) -> Self {
        AtomicTimelineTime::new(value)
    }
}

impl Debug for AtomicTimelineTime {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.load(atomic::Ordering::Acquire).fmt(f)
    }
}

impl Clone for AtomicTimelineTime {
    fn clone(&self) -> Self {
        AtomicTimelineTime::new(self.load(atomic::Ordering::Acquire))
    }
}
