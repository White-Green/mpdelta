use futures::prelude::stream::{self, StreamExt};
use futures::TryStreamExt;
use rpds::{vector_sync, Vector, VectorSync};
use serde::de::{Error, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::marker::PhantomData;
use std::{future, mem};
use thiserror::Error;

#[derive(Eq, PartialEq)]
pub struct TimeSplitValuePersistent<Time, Value> {
    data: VectorSync<(Time, Value)>,
    end: Time,
}

impl<Time, Value> Clone for TimeSplitValuePersistent<Time, Value>
where
    Time: Clone,
{
    fn clone(&self) -> Self {
        TimeSplitValuePersistent { data: self.data.clone(), end: self.end.clone() }
    }
}

#[cfg(any(feature = "proptest", test))]
const _: () = {
    use proptest::collection::{vec, SizeRange};
    use proptest::prelude::*;
    use std::sync::Arc;
    impl<Time: Arbitrary, Value: Arbitrary> Arbitrary for TimeSplitValuePersistent<Time, Value> {
        type Parameters = (Time::Parameters, <Vec<(Time, Value)> as Arbitrary>::Parameters);

        fn arbitrary_with(args: Self::Parameters) -> Self::Strategy {
            let (time, mut vec) = args;
            if vec.0.start() == 0 {
                vec.0 = (1..vec.0.end_excl()).into();
            }
            (Vec::<(Time, Value)>::arbitrary_with(vec), Time::arbitrary_with(time)).prop_map(|(data, end)| TimeSplitValuePersistent { data: VectorSync::from_iter(data), end })
        }

        type Strategy = proptest::strategy::Map<(<Vec<(Time, Value)> as Arbitrary>::Strategy, Time::Strategy), fn((Vec<(Time, Value)>, Time)) -> TimeSplitValuePersistent<Time, Value>>;
    }

    impl<Time, Value> TimeSplitValuePersistent<Time, Value>
    where
        Time: Debug,
        Value: Debug,
    {
        pub fn strategy_from(time: impl Strategy<Value = Time>, value: impl Strategy<Value = Value>, value_len: impl Into<SizeRange>) -> impl Strategy<Value = TimeSplitValuePersistent<Time, Value>> {
            let value_len = value_len.into();
            assert_ne!(value_len.start(), 0);
            let time = Arc::new(time);
            (vec((Arc::clone(&time), value), value_len), time).prop_map(|(data, end)| TimeSplitValuePersistent { data: VectorSync::from_iter(data), end })
        }
    }
};

#[macro_export]
macro_rules! time_split_value_persistent {
    ($($value:expr),+$(,)?) => {
        time_split_value_persistent![@ $($value),+;]
    };
    (@ $time:expr, $value:expr, $($tail:expr),*; $(,)?$($set:expr),*) => {
        time_split_value_persistent![@ $($tail),*; $($set),*, ($time, $value)]
    };
    (@ $time:expr; $(,)?$($set:expr),+) => {
        $crate::common::time_split_value_persistent::TimeSplitValuePersistent::by_data_end(FromIterator::from_iter([$($set),*]), $time)
    };
}
use crate::common::time_split_value::TimeSplitValue;
pub use time_split_value_persistent;

pub struct TimeSplitValueViewPersistent<'a, Time, Value, TimeMutability, ValueMutability> {
    value: &'a mut TimeSplitValuePersistent<Time, Value>,
    phantom: PhantomData<(TimeMutability, ValueMutability)>,
}

pub struct Immutable;

pub struct Mutable;

struct DebugTime<'a, Time, Value>(&'a TimeSplitValuePersistent<Time, Value>);

impl<Time: Debug, Value: Debug> Debug for TimeSplitValuePersistent<Time, Value> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.data.iter().flat_map(|(time, value)| [time as &dyn Debug, value as &dyn Debug])).entry(&self.end).finish()
    }
}

impl<'a, Time: Debug, Value> Debug for DebugTime<'a, Time, Value> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.0.data.iter().map(|(time, _)| time as &dyn Debug)).entry(&self.0.end).finish()
    }
}

#[derive(Debug, Error)]
#[error("time split value is empty")]
pub struct EmptyValue;

#[derive(Debug, Error)]
#[error("merge failed")]
pub struct MergeFailed;

impl<Time, Value> TimeSplitValuePersistent<Time, Value> {
    pub fn new(begin: Time, default_value: Value, end: Time) -> TimeSplitValuePersistent<Time, Value> {
        TimeSplitValuePersistent { data: vector_sync![(begin, default_value)], end }
    }

    pub fn by_data_end(data: VectorSync<(Time, Value)>, end: Time) -> TimeSplitValuePersistent<Time, Value> {
        assert!(!data.is_empty());
        TimeSplitValuePersistent { data, end }
    }

    pub fn len_value(&self) -> usize {
        self.data.len()
    }

    pub fn len_time(&self) -> usize {
        self.data.len() + 1
    }

    pub fn map_time_value<'a, Time2, Value2>(&'a self, mut map_time: impl FnMut(&'a Time) -> Time2, mut map_value: impl FnMut(&'a Value) -> Value2) -> TimeSplitValuePersistent<Time2, Value2> {
        let TimeSplitValuePersistent { data, end } = self;
        TimeSplitValuePersistent {
            data: data.into_iter().map(|(time, value)| (map_time(time), map_value(value))).collect(),
            end: map_time(end),
        }
    }

    pub fn map_time_value_to_normal<'a, Time2, Value2>(&'a self, mut map_time: impl FnMut(&'a Time) -> Time2, mut map_value: impl FnMut(&'a Value) -> Value2) -> TimeSplitValue<Time2, Value2> {
        let TimeSplitValuePersistent { data, end } = self;
        TimeSplitValue::by_data_end(data.into_iter().map(|(time, value)| (map_time(time), map_value(value))).collect(), map_time(end))
    }

    pub fn try_map_time_value<'a, Err, Time2, Value2>(&'a self, mut map_time: impl FnMut(&'a Time) -> Result<Time2, Err>, mut map_value: impl FnMut(&'a Value) -> Result<Value2, Err>) -> Result<TimeSplitValuePersistent<Time2, Value2>, Err> {
        let TimeSplitValuePersistent { data, end } = self;
        Ok(TimeSplitValuePersistent {
            data: data.into_iter().map(|(time, value)| Ok((map_time(time)?, map_value(value)?))).collect::<Result<_, _>>()?,
            end: map_time(end)?,
        })
    }

    pub async fn map_time_value_async<'a, Time2, F1: 'a + Future<Output = Time2>, Value2, F2: 'a + Future<Output = Value2>>(&'a self, map_time: impl Fn(&'a Time) -> F1, map_value: impl Fn(&'a Value) -> F2) -> TimeSplitValuePersistent<Time2, Value2> {
        let TimeSplitValuePersistent { data, end } = self;
        let map_time = &map_time;
        let map_value = &map_value;
        TimeSplitValuePersistent {
            data: stream::iter(data).then(|(time, value)| async move { (map_time(time).await, map_value(value).await) }).collect().await,
            end: map_time(end).await,
        }
    }

    pub async fn try_map_time_value_async<'a, Err, Time2, F1: 'a + Future<Output = Result<Time2, Err>>, Value2, F2: 'a + Future<Output = Result<Value2, Err>>>(&'a self, map_time: impl Fn(&'a Time) -> F1, map_value: impl Fn(&'a Value) -> F2) -> Result<TimeSplitValuePersistent<Time2, Value2>, Err> {
        let TimeSplitValuePersistent { data, end } = self;
        let map_time = &map_time;
        let map_value = &map_value;
        Ok(TimeSplitValuePersistent {
            data: stream::iter(data).then(|(time, value)| async move { Ok((map_time(time).await?, map_value(value).await?)) }).try_collect().await?,
            end: map_time(end).await?,
        })
    }

    pub fn map_time<'a, Time2>(&'a self, map: impl FnMut(&'a Time) -> Time2) -> TimeSplitValuePersistent<Time2, Value>
    where
        Value: Clone,
    {
        self.map_time_value(map, Value::clone)
    }

    pub fn map_value<'a, Value2>(&'a self, map: impl FnMut(&'a Value) -> Value2) -> TimeSplitValuePersistent<Time, Value2>
    where
        Time: Clone,
    {
        self.map_time_value(Time::clone, map)
    }

    pub async fn map_time_async<'a, Time2, F: 'a + Future<Output = Time2>>(&'a self, map: impl Fn(&'a Time) -> F) -> TimeSplitValuePersistent<Time2, Value>
    where
        Value: Clone,
    {
        self.map_time_value_async(map, |v| future::ready(v.clone())).await
    }

    pub async fn map_value_async<'a, Value2, F: 'a + Future<Output = Value2>>(&'a self, map: impl Fn(&'a Value) -> F) -> TimeSplitValuePersistent<Time, Value2>
    where
        Time: Clone,
    {
        self.map_time_value_async(|t| future::ready(t.clone()), map).await
    }

    pub fn push_last(&mut self, value: Value, time: Time) {
        self.data.push_back_mut((mem::replace(&mut self.end, time), value));
    }

    pub fn last(&self) -> Option<(&Value, &Time)> {
        self.data.last().map(|(_, value)| (value, &self.end))
    }

    pub fn pop_last(&mut self) -> Result<(), EmptyValue>
    where
        Time: Clone,
        Value: Clone,
    {
        if self.data.len() == 1 {
            return Err(EmptyValue);
        }
        let (time, _) = self.data.last().unwrap();
        self.end = time.clone();
        self.data.drop_last_mut();
        Ok(())
    }

    pub fn push_first(&mut self, time: Time, value: Value) {
        self.data.insert_mut(0, (time, value));
    }

    pub fn first(&self) -> Option<(&Time, &Value)> {
        self.data.first().map(|(time, value)| (time, value))
    }

    pub fn pop_first(&mut self) -> Result<(), EmptyValue> {
        if self.data.len() == 1 {
            return Err(EmptyValue);
        }
        self.data.remove_mut(0);
        Ok(())
    }

    pub fn split_value(&mut self, value_at: usize, time: Time, left_value: Value, right_value: Value) -> Option<Value>
    where
        Time: Clone,
        Value: Clone,
    {
        let (_, value) = self.data.get_mut(value_at)?;
        let result_value = mem::replace(value, left_value);
        self.data.insert_mut(value_at + 1, (time, right_value));
        Some(result_value)
    }

    pub fn split_value_by_clone(&mut self, value_at: usize, time: Time) -> Option<()>
    where
        Value: Clone,
    {
        let (_, value) = self.data.get(value_at)?;
        let new_value = value.clone();
        self.data.insert_mut(value_at + 1, (time, new_value));
        Some(())
    }

    pub fn merge_two_values(&mut self, time_at: usize, replace_value: Value) -> Option<(Value, Time, Value)>
    where
        Time: Clone,
        Value: Clone,
    {
        let replace_index = time_at.checked_sub(1)?;
        if time_at >= self.data.len() {
            return None;
        }
        let (time, right_value) = self.data.get(time_at).unwrap().clone();
        self.data.remove_mut(time_at);
        let left_value = mem::replace(&mut self.data[replace_index].1, replace_value);
        Some((left_value, time, right_value))
    }

    pub fn merge_two_values_by_left(&mut self, time_at: usize) -> Result<(), MergeFailed> {
        if time_at == 0 || time_at >= self.data.len() {
            return Err(MergeFailed);
        }
        self.data.remove_mut(time_at);
        Ok(())
    }

    pub fn merge_two_values_by_right(&mut self, time_at: usize) -> Result<(), MergeFailed>
    where
        Time: Clone,
        Value: Clone,
    {
        if time_at == 0 || time_at >= self.data.len() {
            return Err(MergeFailed);
        }
        let (_, right_value) = self.data.get(time_at).unwrap();
        let right_value = right_value.clone();
        self.data.remove_mut(time_at);
        self.data[time_at - 1].1 = right_value;
        Ok(())
    }

    pub fn get_value(&self, index: usize) -> Option<(&Time, &Value, &Time)> {
        match (self.data.get(index), self.data.get(index + 1)) {
            (None, _) => None,
            (Some((left, value)), None) => Some((left, value, &self.end)),
            (Some((left, value)), Some((right, _))) => Some((left, value, right)),
        }
    }

    pub fn get_value_mut(&mut self, index: usize) -> Option<&mut Value>
    where
        Time: Clone,
        Value: Clone,
    {
        self.data.get_mut(index).map(|(_, value)| value)
    }

    pub fn get_time(&self, index: usize) -> Option<(Option<&Value>, &Time, Option<&Value>)> {
        match index.cmp(&self.data.len()) {
            Ordering::Less => {
                let (time, value) = self.data.get(index).unwrap();
                Some((self.data.get(index.wrapping_sub(1)).map(|(_, value)| value), time, Some(value)))
            }
            Ordering::Equal => Some((self.data.last().map(|(_, value)| value), &self.end, None)),
            Ordering::Greater => None,
        }
    }

    pub fn get_time_mut(&mut self, index: usize) -> Option<&mut Time>
    where
        Time: Clone,
        Value: Clone,
    {
        match index.cmp(&self.data.len()) {
            Ordering::Less => Some(&mut self.data.get_mut(index).unwrap().0),
            Ordering::Equal => Some(&mut self.end),
            Ordering::Greater => None,
        }
    }

    pub fn binary_search_by(&self, compare: impl Fn(&Time) -> Ordering) -> Result<usize, usize> {
        let mut base = 0;
        let mut size = self.data.len();
        while size > 1 {
            let mid = base + size / 2;
            match compare(&self.data[mid].0) {
                Ordering::Less => base = mid,
                Ordering::Equal => return Ok(mid),
                Ordering::Greater => {}
            }
            size -= size / 2;
        }
        match compare(&self.data[base].0) {
            Ordering::Less if base == self.data.len() - 1 => match compare(&self.end) {
                Ordering::Less => Err(self.data.len() + 1),
                Ordering::Equal => Ok(self.data.len()),
                Ordering::Greater => Err(self.data.len()),
            },
            Ordering::Less => Err(base + 1),
            Ordering::Equal => Ok(base),
            Ordering::Greater => Err(base),
        }
    }
}

impl<Time: Debug, Value> TimeSplitValuePersistent<Time, Value> {
    pub fn debug_time(&self) -> impl Debug + '_ {
        DebugTime(self)
    }
}

impl<'a, Time: Debug, Value: Debug, TimeMutability, ValueMutability> Debug for TimeSplitValueViewPersistent<'a, Time, Value, TimeMutability, ValueMutability> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.value, f)
    }
}

impl<'a, Time, Value, TimeMutability, ValueMutability> From<&'a mut TimeSplitValuePersistent<Time, Value>> for TimeSplitValueViewPersistent<'a, Time, Value, TimeMutability, ValueMutability> {
    fn from(value: &'a mut TimeSplitValuePersistent<Time, Value>) -> Self {
        Self::new(value)
    }
}

impl<'a, Time, Value, TimeMutability, ValueMutability> TimeSplitValueViewPersistent<'a, Time, Value, TimeMutability, ValueMutability> {
    pub fn new(value: &mut TimeSplitValuePersistent<Time, Value>) -> TimeSplitValueViewPersistent<Time, Value, TimeMutability, ValueMutability> {
        TimeSplitValueViewPersistent { value, phantom: Default::default() }
    }

    pub fn len_value(&self) -> usize {
        self.value.len_value()
    }

    pub fn len_time(&self) -> usize {
        self.value.len_time()
    }

    pub fn get_value(&self, index: usize) -> Option<(&Time, &Value, &Time)> {
        self.value.get_value(index)
    }

    pub fn get_time(&self, index: usize) -> Option<(Option<&Value>, &Time, Option<&Value>)> {
        self.value.get_time(index)
    }
}

impl<'a, Time, Value, TimeMutability> TimeSplitValueViewPersistent<'a, Time, Value, TimeMutability, Mutable>
where
    Time: Clone,
    Value: Clone,
{
    pub fn get_value_mut(&mut self, index: usize) -> Option<&mut Value> {
        self.value.get_value_mut(index)
    }
}

impl<'a, Time, Value, ValueMutability> TimeSplitValueViewPersistent<'a, Time, Value, Mutable, ValueMutability>
where
    Time: Clone,
    Value: Clone,
{
    pub fn get_time_mut(&mut self, index: usize) -> Option<&mut Time> {
        self.value.get_time_mut(index)
    }
}

impl<Time, Value> Serialize for TimeSplitValuePersistent<Time, Value>
where
    Time: Serialize,
    Value: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut serializer = serializer.serialize_seq(Some(self.len_time() + self.len_value()))?;
        for (time, value) in self.data.iter() {
            serializer.serialize_element(time)?;
            serializer.serialize_element(value)?;
        }
        serializer.serialize_element(&self.end)?;
        serializer.end()
    }
}

impl<'de, Time, Value> Deserialize<'de> for TimeSplitValuePersistent<Time, Value>
where
    Time: Deserialize<'de>,
    Value: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TimeSplitValueVisitor<Time, Value>(PhantomData<(Time, Value)>);
        impl<'de, Time, Value> Visitor<'de> for TimeSplitValueVisitor<Time, Value>
        where
            Time: Deserialize<'de>,
            Value: Deserialize<'de>,
        {
            type Value = TimeSplitValuePersistent<Time, Value>;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("a sequence of time-value pairs")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut data = Vector::new_sync();
                loop {
                    let Some(time) = seq.next_element()? else {
                        return Err(<A::Error>::custom("time value required"));
                    };
                    if let Some(value) = seq.next_element()? {
                        data.push_back_mut((time, value));
                    } else {
                        return Ok(TimeSplitValuePersistent { data, end: time });
                    }
                }
            }
        }
        deserializer.deserialize_seq(TimeSplitValueVisitor(PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::any;
    use proptest::proptest;

    #[test]
    fn test_time_split_value() {
        let mut value = TimeSplitValuePersistent::new('a', 0, 'x');
        assert_eq!(value, TimeSplitValuePersistent { data: vector_sync![('a', 0)], end: 'x' });
        value.push_last(24, 'y');
        value.push_last(25, 'z');
        assert_eq!(
            value,
            TimeSplitValuePersistent {
                data: vector_sync![('a', 0), ('x', 24), ('y', 25)],
                end: 'z'
            }
        );
        assert_eq!(value.get_value(0), Some((&'a', &0, &'x')));
        assert_eq!(value.get_value(1), Some((&'x', &24, &'y')));
        assert_eq!(value.get_value(2), Some((&'y', &25, &'z')));
        assert_eq!(value.get_value(3), None);
        assert_eq!(value.get_value_mut(0), Some(&mut 0));
        assert_eq!(value.get_value_mut(1), Some(&mut 24));
        assert_eq!(value.get_value_mut(2), Some(&mut 25));
        assert_eq!(value.get_value_mut(3), None);
        assert_eq!(value.get_time(0), Some((None, &'a', Some(&0))));
        assert_eq!(value.get_time(1), Some((Some(&0), &'x', Some(&24))));
        assert_eq!(value.get_time(2), Some((Some(&24), &'y', Some(&25))));
        assert_eq!(value.get_time(3), Some((Some(&25), &'z', None)));
        assert_eq!(value.get_time(4), None);
        assert_eq!(value.get_time_mut(0), Some(&mut 'a'));
        assert_eq!(value.get_time_mut(1), Some(&mut 'x'));
        assert_eq!(value.get_time_mut(2), Some(&mut 'y'));
        assert_eq!(value.get_time_mut(3), Some(&mut 'z'));
        assert_eq!(value.get_time_mut(4), None);
        assert_eq!(value.split_value(0, 'b', 1, 2), Some(0));
        assert_eq!(value, time_split_value_persistent!['a', 1, 'b', 2, 'x', 24, 'y', 25, 'z']);
        assert_eq!(value.split_value(3, 'α', 100, 200), Some(25));
        assert_eq!(value, time_split_value_persistent!['a', 1, 'b', 2, 'x', 24, 'y', 100, 'α', 200, 'z']);
        assert_eq!(value.split_value(5, 'α', 100, 200), None);
        assert_eq!(value, time_split_value_persistent!['a', 1, 'b', 2, 'x', 24, 'y', 100, 'α', 200, 'z']);
        assert_eq!(value.merge_two_values(4, 128), Some((100, 'α', 200)));
        assert_eq!(value, time_split_value_persistent!['a', 1, 'b', 2, 'x', 24, 'y', 128, 'z']);
        assert_eq!(value.merge_two_values(1, 5), Some((1, 'b', 2)));
        assert_eq!(value, time_split_value_persistent!['a', 5, 'x', 24, 'y', 128, 'z']);
        assert_eq!(value.merge_two_values(0, 10), None);
        assert_eq!(value, time_split_value_persistent!['a', 5, 'x', 24, 'y', 128, 'z']);
        assert_eq!(value.merge_two_values(3, 10), None);
        assert_eq!(value, time_split_value_persistent!['a', 5, 'x', 24, 'y', 128, 'z']);

        assert!(value.merge_two_values_by_left(0).is_err());
        assert_eq!(value, time_split_value_persistent!['a', 5, 'x', 24, 'y', 128, 'z']);
        assert!(value.merge_two_values_by_left(3).is_err());
        assert_eq!(value, time_split_value_persistent!['a', 5, 'x', 24, 'y', 128, 'z']);
        assert!(value.merge_two_values_by_right(0).is_err());
        assert_eq!(value, time_split_value_persistent!['a', 5, 'x', 24, 'y', 128, 'z']);
        assert!(value.merge_two_values_by_right(3).is_err());
        assert_eq!(value, time_split_value_persistent!['a', 5, 'x', 24, 'y', 128, 'z']);
        assert!(value.merge_two_values_by_left(1).is_ok());
        assert_eq!(value, time_split_value_persistent!['a', 5, 'y', 128, 'z']);
        assert!(value.merge_two_values_by_right(1).is_ok());
        assert_eq!(value, time_split_value_persistent!['a', 128, 'z']);

        assert_eq!(value.split_value_by_clone(0, 'b'), Some(()));
        assert_eq!(value, time_split_value_persistent!['a', 128, 'b', 128, 'z']);

        assert!(value.pop_last().is_ok());
        assert_eq!(value, time_split_value_persistent!['a', 128, 'b']);
        value.push_last(128, 'z');
        assert!(value.pop_first().is_ok());
        assert_eq!(value, time_split_value_persistent!['b', 128, 'z']);
        assert!(value.pop_last().is_err());
        assert!(value.pop_first().is_err());
    }

    #[test]
    fn test_time_split_value_binary_search() {
        let value = time_split_value_persistent![0, (), 1, (), 1, (), 2, (), 3, (), 5, (), 8];
        assert_eq!(value.binary_search_by(|time| time.cmp(&0)), Ok(0));
        assert_eq!(value.binary_search_by(|time| time.cmp(&2)), Ok(3));
        assert_eq!(value.binary_search_by(|time| time.cmp(&4)), Err(5));
        assert_eq!(value.binary_search_by(|time| time.cmp(&5)), Ok(5));
        assert_eq!(value.binary_search_by(|time| time.cmp(&6)), Err(6));
        assert_eq!(value.binary_search_by(|time| time.cmp(&7)), Err(6));
        assert_eq!(value.binary_search_by(|time| time.cmp(&8)), Ok(6));
        assert_eq!(value.binary_search_by(|time| time.cmp(&9)), Err(7));
        assert_eq!(value.binary_search_by(|time| time.cmp(&10)), Err(7));

        let value = time_split_value_persistent![1, (), 3];
        assert_eq!(value.binary_search_by(|time| time.cmp(&0)), Err(0));
        assert_eq!(value.binary_search_by(|time| time.cmp(&1)), Ok(0));
        assert_eq!(value.binary_search_by(|time| time.cmp(&2)), Err(1));
        assert_eq!(value.binary_search_by(|time| time.cmp(&3)), Ok(1));
        assert_eq!(value.binary_search_by(|time| time.cmp(&4)), Err(2));

        let value = time_split_value_persistent![1, (), 3, (), 5];
        assert_eq!(value.binary_search_by(|time| time.cmp(&0)), Err(0));
        assert_eq!(value.binary_search_by(|time| time.cmp(&1)), Ok(0));
        assert_eq!(value.binary_search_by(|time| time.cmp(&2)), Err(1));
        assert_eq!(value.binary_search_by(|time| time.cmp(&3)), Ok(1));
        assert_eq!(value.binary_search_by(|time| time.cmp(&4)), Err(2));
        assert_eq!(value.binary_search_by(|time| time.cmp(&5)), Ok(2));
        assert_eq!(value.binary_search_by(|time| time.cmp(&6)), Err(3));
    }

    #[test]
    fn test_time_split_value_serde() {
        let value = time_split_value_persistent!['a', 0, 'x', 24, 'y', 25, 'z'];
        let serialized = serde_json::to_string(&value).unwrap();
        assert_eq!(serialized, r#"["a",0,"x",24,"y",25,"z"]"#);
        let deserialized: TimeSplitValuePersistent<char, i32> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, value);
    }

    proptest! {
        #[test]
        fn test_time_split_value_serde_prop(value in any::<TimeSplitValuePersistent<String, usize>>()) {
            let serialized = serde_json::to_string(&value).unwrap();
            let deserialized:TimeSplitValuePersistent<String, usize> = serde_json::from_str(&serialized).unwrap();
            assert_eq!(deserialized, value);
        }
    }
}
