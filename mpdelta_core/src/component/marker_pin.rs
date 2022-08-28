use crate::time::TimelineTime;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

/// 固定マーカの位置のコンポーネントの長さに対する割合
/// \[0.0, ∞)
#[derive(Debug, Clone, Copy)]
pub struct MarkerTime(f64);

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MarkerPin {
    cached_timeline_time: TimelineTime,
    locked_component_time: Option<MarkerTime>,
}

impl MarkerTime {
    pub const ZERO: MarkerTime = MarkerTime(0.0);

    pub fn new(value: f64) -> Option<MarkerTime> {
        if value.is_finite() && 0. <= value {
            Some(MarkerTime(if value == -0.0 { 0.0 } else { value }))
        } else {
            None
        }
    }

    pub fn value(&self) -> f64 {
        self.0
    }
}

impl PartialEq for MarkerTime {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for MarkerTime {}

impl PartialOrd for MarkerTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl Ord for MarkerTime {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.partial_cmp(&other.0).unwrap()
    }
}

impl Hash for MarkerTime {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.0.to_ne_bytes())
    }
}

impl MarkerPin {
    pub fn new(timeline_time: TimelineTime, component_time: MarkerTime) -> MarkerPin {
        MarkerPin {
            cached_timeline_time: timeline_time,
            locked_component_time: Some(component_time),
        }
    }

    pub fn new_unlocked(timeline_time: TimelineTime) -> MarkerPin {
        MarkerPin {
            cached_timeline_time: timeline_time,
            locked_component_time: None,
        }
    }

    pub fn cached_timeline_time(&self) -> TimelineTime {
        self.cached_timeline_time
    }

    pub fn cache_timeline_time(&mut self, time: TimelineTime) {
        self.cached_timeline_time = time;
    }

    pub fn locked_component_time(&self) -> Option<MarkerTime> {
        self.locked_component_time
    }
}

#[cfg(test)]
mod tests {
    use crate::component::marker_pin::MarkerTime;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    #[test]
    fn marker_time() {
        assert_eq!(MarkerTime::new(-f64::EPSILON), None);
        assert_eq!(MarkerTime::new(-0.), Some(MarkerTime(0.)));
        assert_eq!(MarkerTime::new(0.), Some(MarkerTime(0.)));
        assert_eq!(MarkerTime::new(0.5), Some(MarkerTime(0.5)));
        assert_eq!(MarkerTime::new(1.), Some(MarkerTime(1.)));
        assert_eq!(MarkerTime::new(1. + f64::EPSILON), Some(MarkerTime(1. + f64::EPSILON)));
        assert_eq!(MarkerTime::new(f64::INFINITY), None);
        assert_eq!(MarkerTime::new(f64::NAN), None);

        let hash_0 = {
            let mut hasher = DefaultHasher::new();
            MarkerTime::new(0.).unwrap().hash(&mut hasher);
            hasher.finish()
        };
        let hash_negative_0 = {
            let mut hasher = DefaultHasher::new();
            MarkerTime::new(-0.).unwrap().hash(&mut hasher);
            hasher.finish()
        };
        assert_eq!(hash_0, hash_negative_0);
    }
}
