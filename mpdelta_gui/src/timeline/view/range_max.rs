use rpds::RedBlackTreeMap;
use std::cmp::Ordering;
use std::fmt::Debug;
use std::ops::Range;

#[derive(Debug)]
pub struct RangeMax<K, V> {
    map: RedBlackTreeMap<K, Option<V>>,
}

impl<K, V> Clone for RangeMax<K, V>
where
    RedBlackTreeMap<K, Option<V>>: Clone,
{
    fn clone(&self) -> Self {
        RangeMax { map: self.map.clone() }
    }
}

impl<K, V> RangeMax<K, V>
where
    K: Ord + Clone,
    V: PartialOrd + Clone,
{
    pub fn new() -> RangeMax<K, V> {
        RangeMax { map: RedBlackTreeMap::new() }
    }

    #[must_use]
    pub fn insert(&self, range: Range<K>, value: V) -> RangeMax<K, V> {
        assert!(range.start < range.end);
        if self.map.is_empty() {
            let map = self.map.insert(range.start, Some(value)).insert(range.end, None);
            return RangeMax { map };
        }
        let first_key = self.map.first().unwrap().0.clone();
        let (last_key, None) = self.map.last().unwrap() else {
            unreachable!();
        };
        let last_key = last_key.clone();
        match range.end.cmp(&first_key) {
            Ordering::Less => {
                let map = self.map.insert(range.start, Some(value)).insert(range.end, None);
                return RangeMax { map };
            }
            Ordering::Equal => {
                let mut iter = self.map.iter();
                let (_, v) = iter.next().unwrap();
                let map;
                let map = if v.as_ref() == Some(&value) {
                    map = self.map.remove(&first_key);
                    &map
                } else {
                    &self.map
                };
                let map = map.insert(range.start, Some(value));
                return RangeMax { map };
            }
            Ordering::Greater => {}
        }
        match last_key.cmp(&range.start) {
            Ordering::Less => {
                let map = self.map.insert(range.start, Some(value)).insert(range.end, None);
                return RangeMax { map };
            }
            Ordering::Equal => {
                let mut rev_iter = self.map.iter().rev();
                rev_iter.next().unwrap();
                let (_, v) = rev_iter.next().unwrap();
                let map = if v.as_ref() == Some(&value) {
                    self.map.remove(&last_key).insert(range.end, None)
                } else {
                    self.map.insert(range.start, Some(value)).insert(range.end, None)
                };
                return RangeMax { map };
            }
            Ordering::Greater => {}
        }
        let map;
        let map = match self.map.range(..=&range.end).next_back().unwrap() {
            (k, _) if k == &range.end => &self.map,
            (_, None) => {
                map = self.map.insert(range.end.clone(), None);
                &map
            }
            (_, Some(v)) if v < &value => {
                map = self.map.insert(range.end.clone(), Some(v.clone()));
                &map
            }
            _ => &self.map,
        };
        let mut iter = map.range(..=&range.start).rev();
        let m;
        let (map, replace_start, concat_start) = match iter.next() {
            None => {
                m = map.insert(range.start.clone(), Some(value.clone()));
                (&m, range.start.clone(), range.start.clone())
            }
            Some((_, v)) if v.as_ref() < Some(&value) => {
                let start_key = iter.next().map_or_else(|| range.start.clone(), |(k, _)| k.clone());
                m = map.insert(range.start.clone(), Some(value.clone()));
                (&m, range.start.clone(), start_key)
            }
            Some((k, _)) => (map, k.clone(), k.clone()),
        };
        let map = map.range(&replace_start..&range.end).filter(|(_, v)| v.is_none() || v.as_ref().unwrap() < &value).fold(map.clone(), |map, (k, _)| map.insert(k.clone(), Some(value.clone())));
        let map = map
            .range(&concat_start..=&range.end)
            .scan(None, |prev: &mut Option<&Option<V>>, (k, value)| if prev.replace(value) == Some(value) { Some(Some(k.clone())) } else { Some(None) })
            .flatten()
            .fold(map.clone(), |map, k| map.remove(&k));
        RangeMax { map }
    }

    pub fn get(&self, range: Range<&K>) -> Option<&V> {
        assert!(range.start < range.end);
        let (start, _) = self.map.range(..=range.start).next_back().or_else(|| self.map.range(range.start..).next())?;
        self.map.range(start..range.end.max(start)).filter_map(|(_, v)| v.as_ref()).max_by(|a, b| PartialOrd::partial_cmp(a, b).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{array, iter};

    #[test]
    fn test_range_max() {
        fn all_range(range: Range<usize>) -> impl Iterator<Item = Range<usize>> {
            range.clone().flat_map(move |start| (start..range.end).map(move |end| start..end + 1))
        }
        fn all_choose<T: Clone, const N: usize>(list: &[T]) -> impl Iterator<Item = [T; N]> + '_ {
            (0..list.len().pow(N as u32)).map(|i| {
                array::from_fn(|j| {
                    let index = (i / list.len().pow(j as u32)) % list.len();
                    list[index].clone()
                })
            })
        }
        #[track_caller]
        fn assert_all_equal(range_max: &RangeMax<usize, usize>, array_impl: &[Option<usize>]) {
            for range in all_range(0..array_impl.len()) {
                assert_eq!(range_max.get(&range.start..&range.end).copied(), array_impl[range].iter().copied().flatten().max(), "{range_max:?} {array_impl:?}");
            }
        }
        #[track_caller]
        fn assert_internal_range_equal(range_max: &RangeMax<usize, usize>, array_impl: &[Option<usize>]) {
            let mut grouped = array_impl.iter().copied().chain(iter::once(None)).skip_while(Option::is_none).collect::<Vec<_>>();
            grouped.dedup();
            assert_eq!(range_max.map.size(), grouped.len(), "{range_max:?} {array_impl:?}");
        }
        const MAX: usize = 5;
        for range1 in all_range(0..MAX) {
            for range2 in all_range(0..MAX) {
                for range3 in all_range(0..MAX) {
                    for values in all_choose::<_, 3>(&[1, 2, 3]) {
                        let mut range_max = RangeMax::new();
                        let mut array_impl = vec![None; MAX];
                        assert_all_equal(&range_max, &array_impl);

                        range_max = range_max.insert(range1.clone(), values[0]);
                        array_impl[range1.clone()].iter_mut().filter(|o| o.is_none() || o.unwrap() <= values[0]).for_each(|o| *o = Some(values[0]));
                        assert_all_equal(&range_max, &array_impl);
                        assert_internal_range_equal(&range_max, &array_impl);

                        range_max = range_max.insert(range2.clone(), values[1]);
                        array_impl[range2.clone()].iter_mut().filter(|o| o.is_none() || o.unwrap() <= values[1]).for_each(|o| *o = Some(values[1]));
                        assert_all_equal(&range_max, &array_impl);
                        assert_internal_range_equal(&range_max, &array_impl);

                        range_max = range_max.insert(range3.clone(), values[2]);
                        array_impl[range3.clone()].iter_mut().filter(|o| o.is_none() || o.unwrap() <= values[2]).for_each(|o| *o = Some(values[2]));
                        assert_all_equal(&range_max, &array_impl);
                        assert_internal_range_equal(&range_max, &array_impl);
                    }
                }
            }
        }
    }
}
