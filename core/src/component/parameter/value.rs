use std::collections::BTreeSet;
use std::time::Duration;

pub trait TimedValue<T> {
    fn value_at(&self, at: Duration) -> &T;
    fn change_at(&self) -> &BTreeSet<Duration>;
    fn has_different_value(&self, a: Duration, b: Duration) -> bool;
}
