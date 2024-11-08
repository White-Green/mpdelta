use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::component::marker_pin::{MarkerPin, MarkerPinId};
use mpdelta_core::time::TimelineTime;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::Range;
use std::{convert, iter};

macro_rules! define_time_type {
    ($type_name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $type_name(TimelineTime);

        impl $type_name {
            pub const ZERO: $type_name = $type_name(TimelineTime::ZERO);

            pub const fn new(time: TimelineTime) -> $type_name {
                $type_name(time)
            }

            pub const fn time(self) -> TimelineTime {
                self.0
            }
        }

        impl From<TimelineTime> for $type_name {
            fn from(time: TimelineTime) -> $type_name {
                $type_name(time)
            }
        }

        impl From<$type_name> for TimelineTime {
            fn from(time: $type_name) -> TimelineTime {
                time.0
            }
        }
    };
}

define_time_type!(GlobalTime);
define_time_type!(LocalTime);

#[derive(Debug, Clone)]
pub struct TimeStretch<From, To> {
    left: TimelineTime,
    right: TimelineTime,
    markers: Vec<(TimelineTime, TimelineTime)>,
    _phantom: PhantomData<(From, To)>,
}

impl TimeStretch<GlobalTime, LocalTime> {
    pub fn new(left: &MarkerPin, markers: &[MarkerPin], right: &MarkerPin, timeline_time: &HashMap<MarkerPinId, TimelineTime>) -> TimeStretch<GlobalTime, LocalTime> {
        let markers = iter::once(left).chain(markers.iter()).chain(iter::once(right)).filter_map(|marker| Some((timeline_time[marker.id()], marker.locked_component_time()?.into()))).collect::<Vec<_>>();
        let left = timeline_time[left.id()];
        let right = timeline_time[right.id()];
        TimeStretch { left, right, markers, _phantom: PhantomData }
    }
}

impl<F, T> TimeStretch<F, T>
where
    F: From<TimelineTime>,
    T: From<TimelineTime>,
    TimelineTime: From<F> + From<T>,
{
    pub fn new_default(length: TimelineTime) -> TimeStretch<F, T> {
        assert!(length >= TimelineTime::ZERO);
        TimeStretch {
            left: TimelineTime::ZERO,
            right: length,
            markers: Vec::new(),
            _phantom: PhantomData,
        }
    }

    pub fn left(&self) -> F {
        self.left.into()
    }

    pub fn right(&self) -> F {
        self.right.into()
    }

    pub fn map(&self, at: F) -> Option<T> {
        let at = TimelineTime::from(at);
        let TimeStretch { left, right, ref markers, _phantom: _ } = *self;
        if at < left || right < at {
            return None;
        }
        match *markers.as_slice() {
            [] => Some(T::from(at - left)),
            [(timeline_time, component_time)] => Some(T::from(at - timeline_time + component_time)),
            [(timeline_time1, component_time1), (timeline_time2, component_time2)] => {
                let p = (at - timeline_time1) / (timeline_time2 - timeline_time1);
                let time = component_time1.value() + (component_time2.value() - component_time1.value()) * p;
                Some(T::from(TimelineTime::new(time)))
            }
            ref markers => {
                let i = markers.binary_search_by_key(&at, |&(time, _)| time).unwrap_or_else(|x| x);
                let [(timeline_time1, component_time1), (timeline_time2, component_time2)] = markers[i.saturating_sub(1).min(markers.len() - 2)..][..2] else {
                    unreachable!()
                };
                let p = (at - timeline_time1) / (timeline_time2 - timeline_time1);
                let time = component_time1.value() + (component_time2.value() - component_time1.value()) * p;
                Some(T::from(TimelineTime::new(time)))
            }
        }
    }

    pub fn map_range_iter(&self, start: F) -> impl Iterator<Item = TimeStretchSegment<F, T>> + '_ {
        let start = TimelineTime::from(start);
        let i = self.markers.binary_search_by_key(&start, |&(time, _)| time).map_or_else(convert::identity, |x| x + 1);
        let mut markers = &self.markers[i.saturating_sub(1).min(self.markers.len().saturating_sub(2))..];
        let (left, right) = match markers {
            [] => (TimeStretchSegment::new(self.left..self.right, self.left..self.right, TimelineTime::ZERO..self.right - self.left), None),
            &[(source, target)] => (TimeStretchSegment::new(self.left..self.right, self.left..self.right, target - (source - self.left)..target + (self.right - source)), None),
            &[(s1, t1), (s2, t2)] => {
                markers = &[];
                (TimeStretchSegment::new(self.left..self.right, s1..s2, t1..t2), None)
            }
            &[(s1, t1), (s2, t2), ..] => {
                let &[(s3, t3), (s4, t4)] = markers.last_chunk().unwrap();
                markers = &markers[1..markers.len() - 1];
                (TimeStretchSegment::new(self.left..s2, s1..s2, t1..t2), Some(TimeStretchSegment::new(s3..self.right, s3..s4, t3..t4)))
            }
        };
        iter::once(left)
            .chain(
                markers
                    .windows(2)
                    .map(|markers| {
                        let &[(source1, target1), (source2, target2)] = markers else {
                            unreachable!();
                        };
                        TimeStretchSegment::new(source1..source2, source1..source2, target1..target2)
                    })
                    .skip_while(move |segment| segment.time_range.end <= start),
            )
            .chain(right)
    }

    pub fn invert(&self) -> Option<TimeStretch<T, F>> {
        match self.markers.as_slice() {
            [] => Some(TimeStretch {
                left: self.left,
                right: self.right,
                markers: Vec::new(),
                _phantom: PhantomData,
            }),
            &[(from, to)] => Some(TimeStretch {
                left: self.left - from + to,
                right: self.right - from + to,
                markers: vec![(to, from)],
                _phantom: PhantomData,
            }),
            array => {
                if array.windows(2).all(|w| w[0].1 < w[1].1) {
                    Some(TimeStretch {
                        left: array[0].1,
                        right: array.last().unwrap().1,
                        markers: array.iter().map(|&(from, to)| (to, from)).collect::<Vec<_>>(),
                        _phantom: PhantomData,
                    })
                } else if array.windows(2).all(|w| w[0].1 > w[1].1) {
                    Some(TimeStretch {
                        left: array.last().unwrap().1,
                        right: array[0].1,
                        markers: array.iter().rev().map(|&(from, to)| (to, from)).collect::<Vec<_>>(),
                        _phantom: PhantomData,
                    })
                } else {
                    None
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeStretchSegment<From, To> {
    time_range: Range<TimelineTime>,
    slope: MixedFraction,
    intercept: MixedFraction,
    _phantom: PhantomData<(From, To)>,
}

impl<F, T> TimeStretchSegment<F, T>
where
    F: From<TimelineTime>,
    T: From<TimelineTime>,
    TimelineTime: From<F> + From<T>,
{
    fn new(time_range: Range<TimelineTime>, source_range: Range<TimelineTime>, target_range: Range<TimelineTime>) -> TimeStretchSegment<F, T> {
        let slope = (target_range.end.value() - target_range.start.value()) / (source_range.end.value() - source_range.start.value());
        let intercept = target_range.start.value() - slope * source_range.start.value();
        TimeStretchSegment { time_range, slope, intercept, _phantom: PhantomData }
    }

    pub fn start(&self) -> F {
        self.time_range.start.into()
    }

    pub fn end(&self) -> F {
        self.time_range.end.into()
    }

    pub fn map(&self, at: F) -> T {
        let time = self.slope * TimelineTime::from(at).value() + self.intercept;
        T::from(TimelineTime::new(time))
    }

    pub fn map_inverse(&self, at: T) -> F {
        let time = (TimelineTime::from(at).value() - self.intercept) / self.slope;
        F::from(TimelineTime::new(time))
    }

    pub fn scale(&self) -> MixedFraction {
        self.slope
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use mpdelta_core::component::marker_pin::{MarkerPin, MarkerTime};
    use mpdelta_core::mfrac;
    use mpdelta_core_test_util::TestIdGenerator;

    #[test]
    fn test_time_map() {
        let id = TestIdGenerator::new();
        fn time_map_for_test((markers, time_map): &(Vec<MarkerPin>, HashMap<MarkerPinId, TimelineTime>), at: GlobalTime) -> Option<LocalTime> {
            assert!(markers.len() >= 2);
            let [left, markers @ .., right] = markers.as_slice() else { unreachable!() };
            TimeStretch::new(left, markers, right, time_map).map(at)
        }
        macro_rules! markers {
            ($($markers:expr),*$(,)?) => {
                {
                    let mut markers = Vec::new();
                    let mut time_map = HashMap::new();
                    macro_rules! marker {
                        ($t:expr) => {
                            let marker_pin_id = ::mpdelta_core::core::IdGenerator::generate_new(&id);
                            let marker_pin = MarkerPin::new_unlocked(marker_pin_id);
                            time_map.insert(*marker_pin.id(), TimelineTime::new($t));
                            markers.push(marker_pin);
                        };
                        ($t:expr, $m:expr) => {
                            let marker_pin_id = ::mpdelta_core::core::IdGenerator::generate_new(&id);
                            let marker_pin = MarkerPin::new(marker_pin_id, MarkerTime::new($m).unwrap());
                            time_map.insert(*marker_pin.id(), TimelineTime::new($t));
                            markers.push(marker_pin);
                        };
                    }
                    $($markers;)*
                    (markers, time_map)
                }
            }
        }

        let markers = markers![marker!(mfrac!(3)), marker!(mfrac!(4)), marker!(mfrac!(5)), marker!(mfrac!(6))];
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(3, 0, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(0, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(3, 5, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(0, 5, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(5, 0, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(2, 0, 10)) == MixedFraction::ZERO);
        let markers = markers![marker!(mfrac!(3)), marker!(mfrac!(4)), marker!(mfrac!(5), mfrac!(10)), marker!(mfrac!(6))];
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(3, 0, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(8, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(3, 5, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(8, 5, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(5, 0, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(10, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(5, 5, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(10, 5, 10)) == MixedFraction::ZERO);
        let markers = markers![marker!(mfrac!(3)), marker!(mfrac!(4), mfrac!(8)), marker!(mfrac!(5), mfrac!(10)), marker!(mfrac!(6))];
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(3, 0, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(6, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(3, 5, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(7, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(4, 5, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(9, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(5, 0, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(10, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(5, 5, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(11, 0, 10)) == MixedFraction::ZERO);
        let markers = markers![
            marker!(mfrac!(3)),
            marker!(mfrac!(4), mfrac!(8)),
            marker!(mfrac!(5), mfrac!(10)),
            marker!(mfrac!(6)),
            marker!(mfrac!(7), mfrac!(13)),
            marker!(mfrac!(8)),
            marker!(mfrac!(10), mfrac!(10)),
        ];
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(3, 0, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(6, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(3, 5, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(7, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(4, 5, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(9, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(5, 0, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(10, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(6, 0, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(11, 5, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(7, 0, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(13, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(8, 0, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(12, 0, 10)) == MixedFraction::ZERO);
        assert_matches!(time_map_for_test(&markers, GlobalTime::new(TimelineTime::new(mfrac!(10, 0, 10)))), Some(v) if (TimelineTime::from(v).value() - mfrac!(10, 0, 10)) == MixedFraction::ZERO);
    }

    #[test]
    fn test_time_map_range_iter() {
        let id = TestIdGenerator::new();
        fn time_map_for_test((markers, time_map): &(Vec<MarkerPin>, HashMap<MarkerPinId, TimelineTime>), at: GlobalTime) -> Vec<TimeStretchSegment<GlobalTime, LocalTime>> {
            assert!(markers.len() >= 2);
            let [left, markers @ .., right] = markers.as_slice() else { unreachable!() };
            TimeStretch::new(left, markers, right, time_map).map_range_iter(at).collect()
        }
        macro_rules! markers {
            ($($markers:expr),*$(,)?) => {
                {
                    let mut markers = Vec::new();
                    let mut time_map = HashMap::new();
                    macro_rules! marker {
                        ($t:expr) => {
                            let marker_pin_id = ::mpdelta_core::core::IdGenerator::generate_new(&id);
                            let marker_pin = MarkerPin::new_unlocked(marker_pin_id);
                            time_map.insert(*marker_pin.id(), TimelineTime::new($t));
                            markers.push(marker_pin);
                        };
                        ($t:expr, $m:expr) => {
                            let marker_pin_id = ::mpdelta_core::core::IdGenerator::generate_new(&id);
                            let marker_pin = MarkerPin::new(marker_pin_id, MarkerTime::new($m).unwrap());
                            time_map.insert(*marker_pin.id(), TimelineTime::new($t));
                            markers.push(marker_pin);
                        };
                    }
                    $($markers;)*
                    (markers, time_map)
                }
            }
        }
        macro_rules! t {
            ($($t:tt)*) => {
                TimelineTime::new(mfrac!($($t)*))
            }
        }
        let markers = markers![marker!(mfrac!(3)), marker!(mfrac!(4)), marker!(mfrac!(5)), marker!(mfrac!(6))];
        assert_eq!(time_map_for_test(&markers, GlobalTime::new(t!(3, 0, 10))), vec![TimeStretchSegment::new(t!(3)..t!(6), t!(3)..t!(6), t!(0)..t!(3))]);
        assert_eq!(time_map_for_test(&markers, GlobalTime::new(t!(3, 5, 10))), vec![TimeStretchSegment::new(t!(3)..t!(6), t!(3)..t!(6), t!(0)..t!(3))]);
        assert_eq!(time_map_for_test(&markers, GlobalTime::new(t!(5, 0, 10))), vec![TimeStretchSegment::new(t!(3)..t!(6), t!(3)..t!(6), t!(0)..t!(3))]);
        let markers = markers![marker!(mfrac!(3)), marker!(mfrac!(4)), marker!(mfrac!(5), mfrac!(10)), marker!(mfrac!(6))];
        assert_eq!(time_map_for_test(&markers, GlobalTime::new(t!(3, 0, 10))), vec![TimeStretchSegment::new(t!(3)..t!(6), t!(3)..t!(6), t!(8)..t!(11))]);
        assert_eq!(time_map_for_test(&markers, GlobalTime::new(t!(3, 5, 10))), vec![TimeStretchSegment::new(t!(3)..t!(6), t!(3)..t!(6), t!(8)..t!(11))]);
        assert_eq!(time_map_for_test(&markers, GlobalTime::new(t!(5, 0, 10))), vec![TimeStretchSegment::new(t!(3)..t!(6), t!(3)..t!(6), t!(8)..t!(11))]);
        assert_eq!(time_map_for_test(&markers, GlobalTime::new(t!(5, 5, 10))), vec![TimeStretchSegment::new(t!(3)..t!(6), t!(3)..t!(6), t!(8)..t!(11))]);
        let markers = markers![marker!(mfrac!(3)), marker!(mfrac!(4), mfrac!(8)), marker!(mfrac!(5), mfrac!(10)), marker!(mfrac!(6))];
        assert_eq!(time_map_for_test(&markers, GlobalTime::new(t!(3, 0, 10))), vec![TimeStretchSegment::new(t!(3)..t!(6), t!(4)..t!(5), t!(8)..t!(10))]);
        assert_eq!(time_map_for_test(&markers, GlobalTime::new(t!(3, 5, 10))), vec![TimeStretchSegment::new(t!(3)..t!(6), t!(4)..t!(5), t!(8)..t!(10))]);
        assert_eq!(time_map_for_test(&markers, GlobalTime::new(t!(4, 5, 10))), vec![TimeStretchSegment::new(t!(3)..t!(6), t!(4)..t!(5), t!(8)..t!(10))]);
        assert_eq!(time_map_for_test(&markers, GlobalTime::new(t!(5, 0, 10))), vec![TimeStretchSegment::new(t!(3)..t!(6), t!(4)..t!(5), t!(8)..t!(10))]);
        assert_eq!(time_map_for_test(&markers, GlobalTime::new(t!(5, 5, 10))), vec![TimeStretchSegment::new(t!(3)..t!(6), t!(4)..t!(5), t!(8)..t!(10))]);
        let markers = markers![
            marker!(mfrac!(3)),
            marker!(mfrac!(4), mfrac!(8)),
            marker!(mfrac!(5), mfrac!(10)),
            marker!(mfrac!(6)),
            marker!(mfrac!(7), mfrac!(13)),
            marker!(mfrac!(8)),
            marker!(mfrac!(10), mfrac!(10)),
        ];
        assert_eq!(
            time_map_for_test(&markers, GlobalTime::new(t!(3, 0, 10))),
            vec![
                TimeStretchSegment::new(t!(3)..t!(5), t!(4)..t!(5), t!(8)..t!(10)),
                TimeStretchSegment::new(t!(5)..t!(7), t!(5)..t!(7), t!(10)..t!(13)),
                TimeStretchSegment::new(t!(7)..t!(10), t!(7)..t!(10), t!(13)..t!(10)),
            ]
        );
        assert_eq!(
            time_map_for_test(&markers, GlobalTime::new(t!(3, 5, 10))),
            vec![
                TimeStretchSegment::new(t!(3)..t!(5), t!(4)..t!(5), t!(8)..t!(10)),
                TimeStretchSegment::new(t!(5)..t!(7), t!(5)..t!(7), t!(10)..t!(13)),
                TimeStretchSegment::new(t!(7)..t!(10), t!(7)..t!(10), t!(13)..t!(10)),
            ]
        );
        assert_eq!(
            time_map_for_test(&markers, GlobalTime::new(t!(4, 5, 10))),
            vec![
                TimeStretchSegment::new(t!(3)..t!(5), t!(4)..t!(5), t!(8)..t!(10)),
                TimeStretchSegment::new(t!(5)..t!(7), t!(5)..t!(7), t!(10)..t!(13)),
                TimeStretchSegment::new(t!(7)..t!(10), t!(7)..t!(10), t!(13)..t!(10)),
            ]
        );
        assert_eq!(
            time_map_for_test(&markers, GlobalTime::new(t!(5, 0, 10))),
            vec![TimeStretchSegment::new(t!(3)..t!(7), t!(5)..t!(7), t!(10)..t!(13)), TimeStretchSegment::new(t!(7)..t!(10), t!(7)..t!(10), t!(13)..t!(10))]
        );
        assert_eq!(
            time_map_for_test(&markers, GlobalTime::new(t!(6, 0, 10))),
            vec![TimeStretchSegment::new(t!(3)..t!(7), t!(5)..t!(7), t!(10)..t!(13)), TimeStretchSegment::new(t!(7)..t!(10), t!(7)..t!(10), t!(13)..t!(10))]
        );
        assert_eq!(time_map_for_test(&markers, GlobalTime::new(t!(7, 0, 10))), vec![TimeStretchSegment::new(t!(3)..t!(10), t!(7)..t!(10), t!(13)..t!(10))]);
        assert_eq!(time_map_for_test(&markers, GlobalTime::new(t!(8, 0, 10))), vec![TimeStretchSegment::new(t!(3)..t!(10), t!(7)..t!(10), t!(13)..t!(10))]);
        assert_eq!(time_map_for_test(&markers, GlobalTime::new(t!(10, 0, 10))), vec![TimeStretchSegment::new(t!(3)..t!(10), t!(7)..t!(10), t!(13)..t!(10))]);
    }

    #[test]
    fn test_time_map_segment() {
        macro_rules! t {
            ($($t:tt)*) => {
                TimelineTime::new(mfrac!($($t)*))
            }
        }
        let segment = TimeStretchSegment::<GlobalTime, LocalTime>::new(t!(3)..t!(6), t!(3)..t!(6), t!(0)..t!(3));
        assert_eq!(segment.scale(), mfrac!(1));
        assert_eq!(segment.map(GlobalTime::new(t!(3, 0, 10))), LocalTime::new(t!(0, 0, 10)));
        assert_eq!(segment.map(GlobalTime::new(t!(4, 5, 10))), LocalTime::new(t!(1, 5, 10)));
        assert_eq!(segment.map(GlobalTime::new(t!(6, 0, 10))), LocalTime::new(t!(3, 0, 10)));
        let segment = TimeStretchSegment::<GlobalTime, LocalTime>::new(t!(3)..t!(6), t!(3)..t!(6), t!(8)..t!(11));
        assert_eq!(segment.scale(), mfrac!(1));
        assert_eq!(segment.map(GlobalTime::new(t!(3, 0, 10))), LocalTime::new(t!(8, 0, 10)));
        assert_eq!(segment.map(GlobalTime::new(t!(4, 5, 10))), LocalTime::new(t!(9, 5, 10)));
        assert_eq!(segment.map(GlobalTime::new(t!(6, 0, 10))), LocalTime::new(t!(11, 0, 10)));
        let segment = TimeStretchSegment::<GlobalTime, LocalTime>::new(t!(3)..t!(6), t!(4)..t!(5), t!(8)..t!(10));
        assert_eq!(segment.scale(), mfrac!(2));
        assert_eq!(segment.map(GlobalTime::new(t!(4, 0, 10))), LocalTime::new(t!(8, 0, 10)));
        assert_eq!(segment.map(GlobalTime::new(t!(4, 5, 10))), LocalTime::new(t!(9, 0, 10)));
        assert_eq!(segment.map(GlobalTime::new(t!(5, 0, 10))), LocalTime::new(t!(10, 0, 10)));
        let segment = TimeStretchSegment::<GlobalTime, LocalTime>::new(t!(3)..t!(5), t!(4)..t!(5), t!(8)..t!(10));
        assert_eq!(segment.scale(), mfrac!(2));
        assert_eq!(segment.map(GlobalTime::new(t!(4, 0, 10))), LocalTime::new(t!(8, 0, 10)));
        assert_eq!(segment.map(GlobalTime::new(t!(4, 5, 10))), LocalTime::new(t!(9, 0, 10)));
        assert_eq!(segment.map(GlobalTime::new(t!(5, 0, 10))), LocalTime::new(t!(10, 0, 10)));
        let segment = TimeStretchSegment::<GlobalTime, LocalTime>::new(t!(5)..t!(7), t!(5)..t!(7), t!(10)..t!(13));
        assert_eq!(segment.scale(), mfrac!(3, 2));
        assert_eq!(segment.map(GlobalTime::new(t!(5, 0, 10))), LocalTime::new(t!(10, 0, 10)));
        assert_eq!(segment.map(GlobalTime::new(t!(6, 0, 10))), LocalTime::new(t!(11, 5, 10)));
        assert_eq!(segment.map(GlobalTime::new(t!(7, 0, 10))), LocalTime::new(t!(13, 0, 10)));
        let segment = TimeStretchSegment::<GlobalTime, LocalTime>::new(t!(7)..t!(10), t!(7)..t!(10), t!(13)..t!(10));
        assert_eq!(segment.scale(), mfrac!(-1));
        assert_eq!(segment.map(GlobalTime::new(t!(7, 0, 10))), LocalTime::new(t!(13, 0, 10)));
        assert_eq!(segment.map(GlobalTime::new(t!(8, 5, 10))), LocalTime::new(t!(11, 5, 10)));
        assert_eq!(segment.map(GlobalTime::new(t!(10, 0, 10))), LocalTime::new(t!(10, 0, 10)));
    }
}
