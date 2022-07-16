use std::fmt::{Debug, Formatter};

pub trait Easing<T>: Send + Sync {
    fn id(&self) -> &str;
    fn easing(&self, from: &T, to: &T, changing: f64) -> T;
}

pub struct EasingValue<Value> {
    pub from: Value,
    pub to: Value,
    pub easing: Box<dyn Easing<Value>>,
}

impl<Value: Debug> Debug for EasingValue<Value> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EasingValue").field("from", &self.from).field("to", &self.to).finish_non_exhaustive()
    }
}
