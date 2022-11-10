use crate::component::marker_pin::MarkerTime;
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Neg;
use std::sync::atomic;
use std::sync::atomic::AtomicU64;

/// タイムライン上での時間(秒)
/// (-∞, ∞)
#[derive(Debug, Clone, Copy)]
pub struct TimelineTime(f64);

impl TimelineTime {
    pub const ZERO: TimelineTime = TimelineTime(0.0);
    pub const MAX: TimelineTime = TimelineTime(f64::MAX);
    pub const MIN: TimelineTime = TimelineTime(-f64::MAX);

    pub fn new(time: f64) -> Option<TimelineTime> {
        if time.is_finite() {
            Some(TimelineTime(if time == -0.0 { 0.0 } else { time }))
        } else {
            None
        }
    }

    pub fn value(&self) -> f64 {
        self.0
    }
}

impl From<MarkerTime> for TimelineTime {
    fn from(value: MarkerTime) -> Self {
        TimelineTime(value.value())
    }
}

impl PartialEq for TimelineTime {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for TimelineTime {}

impl PartialOrd for TimelineTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Ord for TimelineTime {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.partial_cmp(&other.0).unwrap()
    }
}

impl Hash for TimelineTime {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.0.to_ne_bytes())
    }
}

impl Neg for TimelineTime {
    type Output = TimelineTime;

    fn neg(self) -> Self::Output {
        // 表現可能な数の範囲が正負で同一なのでsafe
        TimelineTime(-self.0)
    }
}

pub struct AtomicTimelineTime(AtomicU64);

impl AtomicTimelineTime {
    pub fn new(value: TimelineTime) -> AtomicTimelineTime {
        AtomicTimelineTime(AtomicU64::new(value.0.to_bits()))
    }

    pub fn load(&self, ordering: atomic::Ordering) -> TimelineTime {
        TimelineTime(f64::from_bits(self.0.load(ordering)))
    }

    pub fn store(&self, value: TimelineTime, ordering: atomic::Ordering) {
        self.0.store(value.0.to_bits(), ordering)
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

#[cfg(test)]
mod tests {
    use crate::time::TimelineTime;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    #[test]
    fn timeline_time() {
        assert_eq!(TimelineTime::new(f64::NEG_INFINITY), None);
        assert_eq!(TimelineTime::new(f64::MIN), Some(TimelineTime(f64::MIN)));
        assert_eq!(TimelineTime::new(-0.), Some(TimelineTime(0.)));
        assert_eq!(TimelineTime::new(0.), Some(TimelineTime(0.)));
        assert_eq!(TimelineTime::new(0.5), Some(TimelineTime(0.5)));
        assert_eq!(TimelineTime::new(1.), Some(TimelineTime(1.)));
        assert_eq!(TimelineTime::new(f64::MAX), Some(TimelineTime(f64::MAX)));
        assert_eq!(TimelineTime::new(f64::INFINITY), None);
        assert_eq!(TimelineTime::new(f64::NAN), None);

        let hash_0 = {
            let mut hasher = DefaultHasher::new();
            TimelineTime::new(0.).unwrap().hash(&mut hasher);
            hasher.finish()
        };
        let hash_negative_0 = {
            let mut hasher = DefaultHasher::new();
            TimelineTime::new(-0.).unwrap().hash(&mut hasher);
            hasher.finish()
        };
        assert_eq!(hash_0, hash_negative_0);
    }
}
