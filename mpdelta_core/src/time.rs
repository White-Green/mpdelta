use crate::common::mixed_fraction::MixedFraction;
use crate::component::marker_pin::MarkerTime;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::{Add, Div, Neg, Sub};

/// タイムライン上での時間(秒)
/// (-∞, ∞)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(any(feature = "proptest", test), derive(proptest_derive::Arbitrary))]
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
