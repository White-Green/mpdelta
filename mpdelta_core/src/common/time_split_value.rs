use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::mem;

#[derive(Clone, Eq, PartialEq)]
pub struct TimeSplitValue<Time, Value> {
    data: Vec<(Time, Value)>,
    end: Time,
}

pub struct TimeSplitValueView<'a, Time, Value, TimeMutability, ValueMutability> {
    value: &'a mut TimeSplitValue<Time, Value>,
    phantom: PhantomData<(TimeMutability, ValueMutability)>,
}

pub struct Immutable;

pub struct Mutable;

impl<Time: Debug, Value: Debug> Debug for TimeSplitValue<Time, Value> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.data.iter().flat_map(|(time, value)| [time as &dyn Debug, value as &dyn Debug])).entry(&self.end).finish()
    }
}

impl<Time, Value> TimeSplitValue<Time, Value> {
    pub fn new(begin: Time, default_value: Value, end: Time) -> TimeSplitValue<Time, Value> {
        TimeSplitValue { data: vec![(begin, default_value)], end }
    }

    pub fn push(&mut self, value: Value, time: Time) {
        self.data.push((mem::replace(&mut self.end, time), value));
    }

    pub fn split_value(&mut self, value_at: usize, time: Time, left_value: Value, right_value: Value) -> Option<Value> {
        let (_, value) = self.data.get_mut(value_at)?;
        let result_value = mem::replace(value, left_value);
        self.data.insert(value_at + 1, (time, right_value));
        Some(result_value)
    }

    pub fn merge_two_values(&mut self, time_at: usize, replace_value: Value) -> Option<(Value, Time, Value)> {
        let replace_index = time_at.checked_sub(1)?;
        if time_at >= self.data.len() {
            return None;
        }
        let (time, right_value) = self.data.remove(time_at);
        let left_value = mem::replace(&mut self.data[replace_index].1, replace_value);
        Some((left_value, time, right_value))
    }

    pub fn get_value(&self, index: usize) -> Option<(&Time, &Value, &Time)> {
        match self.data.get(index..) {
            None | Some([]) => None,
            Some([(left, value)]) => Some((left, value, &self.end)),
            Some([(left, value), (right, _), ..]) => Some((left, value, right)),
        }
    }

    pub fn get_value_mut(&mut self, index: usize) -> Option<(&Time, &mut Value, &Time)> {
        match self.data.get_mut(index..) {
            None | Some([]) => None,
            Some([(left, value)]) => Some((left, value, &self.end)),
            Some([(left, value), (right, _), ..]) => Some((left, value, right)),
        }
    }

    pub fn get_time(&self, index: usize) -> Option<(Option<&Value>, &Time, Option<&Value>)> {
        match index.cmp(&self.data.len()) {
            Ordering::Less => {
                let (left, right) = self.data.split_at(index);
                let (time, value) = right.first().unwrap();
                Some((left.last().map(|(_, value)| value), time, Some(value)))
            }
            Ordering::Equal => Some((self.data.last().map(|(_, value)| value), &self.end, None)),
            Ordering::Greater => None,
        }
    }

    pub fn get_time_mut(&mut self, index: usize) -> Option<(Option<&Value>, &mut Time, Option<&Value>)> {
        match index.cmp(&self.data.len()) {
            Ordering::Less => {
                let (left, right) = self.data.split_at_mut(index);
                let (time, value) = right.first_mut().unwrap();
                Some((left.last().map(|(_, value)| value), time, Some(value)))
            }
            Ordering::Equal => Some((self.data.last().map(|(_, value)| value), &mut self.end, None)),
            Ordering::Greater => None,
        }
    }
}

impl<'a, Time: Debug, Value: Debug, TimeMutability, ValueMutability> Debug for TimeSplitValueView<'a, Time, Value, TimeMutability, ValueMutability> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.value, f)
    }
}

impl<'a, Time, Value, TimeMutability, ValueMutability> From<&'a mut TimeSplitValue<Time, Value>> for TimeSplitValueView<'a, Time, Value, TimeMutability, ValueMutability> {
    fn from(value: &'a mut TimeSplitValue<Time, Value>) -> Self {
        Self::new(value)
    }
}

impl<'a, Time, Value, TimeMutability, ValueMutability> TimeSplitValueView<'a, Time, Value, TimeMutability, ValueMutability> {
    pub fn new(value: &mut TimeSplitValue<Time, Value>) -> TimeSplitValueView<Time, Value, TimeMutability, ValueMutability> {
        TimeSplitValueView { value, phantom: Default::default() }
    }

    pub fn get_value(&self, index: usize) -> Option<(&Time, &Value, &Time)> {
        self.value.get_value(index)
    }

    pub fn get_time(&self, index: usize) -> Option<(Option<&Value>, &Time, Option<&Value>)> {
        self.value.get_time(index)
    }
}

impl<'a, Time, Value, TimeMutability> TimeSplitValueView<'a, Time, Value, TimeMutability, Mutable> {
    pub fn get_value_mut(&mut self, index: usize) -> Option<(&Time, &mut Value, &Time)> {
        self.value.get_value_mut(index)
    }
}

impl<'a, Time, Value, ValueMutability> TimeSplitValueView<'a, Time, Value, Mutable, ValueMutability> {
    pub fn get_time_mut(&mut self, index: usize) -> Option<(Option<&Value>, &mut Time, Option<&Value>)> {
        self.value.get_time_mut(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_split_value() {
        let mut value = TimeSplitValue::new('a', 0, 'x');
        assert_eq!(value, TimeSplitValue { data: vec![('a', 0)], end: 'x' });
        value.push(24, 'y');
        value.push(25, 'z');
        assert_eq!(value, TimeSplitValue { data: vec![('a', 0), ('x', 24), ('y', 25)], end: 'z' });
        assert_eq!(value.get_value(0), Some((&'a', &0, &'x')));
        assert_eq!(value.get_value(1), Some((&'x', &24, &'y')));
        assert_eq!(value.get_value(2), Some((&'y', &25, &'z')));
        assert_eq!(value.get_value(3), None);
        assert_eq!(value.get_value_mut(0), Some((&'a', &mut 0, &'x')));
        assert_eq!(value.get_value_mut(1), Some((&'x', &mut 24, &'y')));
        assert_eq!(value.get_value_mut(2), Some((&'y', &mut 25, &'z')));
        assert_eq!(value.get_value_mut(3), None);
        assert_eq!(value.get_time(0), Some((None, &'a', Some(&0))));
        assert_eq!(value.get_time(1), Some((Some(&0), &'x', Some(&24))));
        assert_eq!(value.get_time(2), Some((Some(&24), &'y', Some(&25))));
        assert_eq!(value.get_time(3), Some((Some(&25), &'z', None)));
        assert_eq!(value.get_time(4), None);
        assert_eq!(value.get_time_mut(0), Some((None, &mut 'a', Some(&0))));
        assert_eq!(value.get_time_mut(1), Some((Some(&0), &mut 'x', Some(&24))));
        assert_eq!(value.get_time_mut(2), Some((Some(&24), &mut 'y', Some(&25))));
        assert_eq!(value.get_time_mut(3), Some((Some(&25), &mut 'z', None)));
        assert_eq!(value.get_time_mut(4), None);
        assert_eq!(value.split_value(0, 'b', 1, 2), Some(0));
        assert_eq!(
            value,
            TimeSplitValue {
                data: vec![('a', 1), ('b', 2), ('x', 24), ('y', 25)],
                end: 'z',
            }
        );
        assert_eq!(value.split_value(3, 'α', 100, 200), Some(25));
        assert_eq!(
            value,
            TimeSplitValue {
                data: vec![('a', 1), ('b', 2), ('x', 24), ('y', 100), ('α', 200)],
                end: 'z',
            }
        );
        assert_eq!(value.split_value(5, 'α', 100, 200), None);
        assert_eq!(
            value,
            TimeSplitValue {
                data: vec![('a', 1), ('b', 2), ('x', 24), ('y', 100), ('α', 200)],
                end: 'z',
            }
        );
        assert_eq!(value.merge_two_values(4, 128), Some((100, 'α', 200)));
        assert_eq!(
            value,
            TimeSplitValue {
                data: vec![('a', 1), ('b', 2), ('x', 24), ('y', 128)],
                end: 'z',
            }
        );
        assert_eq!(value.merge_two_values(1, 5), Some((1, 'b', 2)));
        assert_eq!(value, TimeSplitValue { data: vec![('a', 5), ('x', 24), ('y', 128)], end: 'z' });
        assert_eq!(value.merge_two_values(0, 10), None);
        assert_eq!(value, TimeSplitValue { data: vec![('a', 5), ('x', 24), ('y', 128)], end: 'z' });
        assert_eq!(value.merge_two_values(3, 10), None);
        assert_eq!(value, TimeSplitValue { data: vec![('a', 5), ('x', 24), ('y', 128)], end: 'z' });
    }
}
