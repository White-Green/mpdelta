use crate::time::TimelineTime;
use futures::prelude::stream::{self, StreamExt};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::sync::Arc;

pub trait Easing<T>: Send + Sync {
    fn id(&self) -> &str;
    fn easing(&self, from: &T, to: &T, changing: f64) -> T;
}

pub struct DefaultEasing;

impl<T: Clone> Easing<T> for DefaultEasing {
    fn id(&self) -> &str {
        "default"
    }

    fn easing(&self, left: &T, _: &T, _: f64) -> T {
        left.clone()
    }
}

pub struct LinearEasing;

impl<T: Copy + Into<f64> + TryFrom<f64> + Default> Easing<T> for LinearEasing {
    fn id(&self) -> &str {
        "default"
    }

    fn easing(&self, &left: &T, &right: &T, p: f64) -> T {
        let left = left.into();
        let right = right.into();
        T::try_from(left * (1. - p) + right * p).unwrap_or_default()
    }
}

#[derive(Clone)]
pub struct EasingValue<Value> {
    pub from: Value,
    pub to: Value,
    pub easing: Arc<dyn Easing<Value>>,
}

impl<Value: Debug> Debug for EasingValue<Value> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EasingValue").field("from", &self.from).field("to", &self.to).finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrameVariableValue<Value> {
    values: BTreeMap<TimelineTime, Value>,
}

impl<Value> FrameVariableValue<Value> {
    pub fn new() -> FrameVariableValue<Value> {
        FrameVariableValue { values: BTreeMap::new() }
    }

    pub fn get(&self, time: TimelineTime) -> Option<&Value> {
        match (self.values.range(..time).next_back(), self.values.range(time..).next()) {
            (Some((time1, value1)), Some((time2, value2))) => Some(if time.value() - time1.value() < time2.value() - time.value() { value1 } else { value2 }),
            (Some((_, value1)), _) => Some(value1),
            (_, Some((_, value2))) => Some(value2),
            (_, _) => None,
        }
    }

    pub fn insert(&mut self, key: TimelineTime, value: Value) {
        self.values.insert(key, value);
    }

    pub fn map<T>(self, mut map: impl FnMut(Value) -> T) -> FrameVariableValue<T> {
        FrameVariableValue {
            values: self.values.into_iter().map(|(k, v)| (k, map(v))).collect(),
        }
    }

    pub async fn map_async<T, F: Future<Output = T>>(self, map: impl Fn(Value) -> F) -> FrameVariableValue<T> {
        let map = &map;
        FrameVariableValue {
            values: stream::iter(self.values).then(|(k, v)| async move { (k, map(v).await) }).collect().await,
        }
    }

    pub fn map_ref<T>(&self, mut map: impl FnMut(&Value) -> T) -> FrameVariableValue<T> {
        FrameVariableValue { values: self.values.iter().map(|(&k, v)| (k, map(v))).collect() }
    }

    pub fn map_time(self, mut map: impl FnMut(TimelineTime) -> TimelineTime) -> FrameVariableValue<Value> {
        FrameVariableValue {
            values: self.values.into_iter().map(|(k, v)| (map(k), v)).collect(),
        }
    }

    pub fn first_time(&self) -> Option<TimelineTime> {
        self.values.iter().next().map(|v| *v.0)
    }

    pub fn last_time(&self) -> Option<TimelineTime> {
        self.values.iter().next_back().map(|v| *v.0)
    }
}

impl<T> From<BTreeMap<TimelineTime, T>> for FrameVariableValue<T> {
    fn from(values: BTreeMap<TimelineTime, T>) -> Self {
        FrameVariableValue { values }
    }
}
