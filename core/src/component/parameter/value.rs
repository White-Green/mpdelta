pub trait Easing<T> {
    fn id(&self) -> &str;
    fn easing(&self, from: &T, to: &T, changing: f64) -> T;
}

pub struct EasingValue<Value> {
    pub from: Value,
    pub to: Value,
    pub easing: Box<dyn Easing<Value>>,
}
