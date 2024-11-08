use rpds::{RedBlackTreeMap, RedBlackTreeMapSync};
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::ops::Range;

pub struct InvalidateRange<T> {
    invalid_ranges: RedBlackTreeMapSync<T, T>,
}

impl<T> Clone for InvalidateRange<T>
where
    T: Ord,
{
    fn clone(&self) -> Self {
        InvalidateRange { invalid_ranges: self.invalid_ranges.clone() }
    }
}

impl<T> Debug for InvalidateRange<T>
where
    T: Debug + Ord,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.invalid_ranges.iter().map(|(k, v)| k..v)).finish()
    }
}

impl<T> Default for InvalidateRange<T>
where
    T: Clone + Ord,
{
    fn default() -> Self {
        InvalidateRange::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidityRange {
    Valid,
    Invalid,
    PartiallyInvalid,
}

impl<T> InvalidateRange<T>
where
    T: Clone + Ord,
{
    pub fn new() -> InvalidateRange<T> {
        InvalidateRange { invalid_ranges: RedBlackTreeMap::new_sync() }
    }

    pub fn validity(&self, at: &T) -> bool {
        !self.invalid_ranges.range(..=at).next_back().is_some_and(|(_, end)| at < end)
    }

    pub fn validity_range(&self, range: Range<&T>) -> ValidityRange {
        let Range { start, end } = range;
        if let Some((_, start_range_end)) = self.invalid_ranges.range(..=start).next_back() {
            if end <= start_range_end {
                return ValidityRange::Invalid;
            }
            if start < start_range_end {
                return ValidityRange::PartiallyInvalid;
            }
        }
        if self.invalid_ranges.range(start..end).next().is_some() {
            ValidityRange::PartiallyInvalid
        } else {
            ValidityRange::Valid
        }
    }

    pub fn invalidate(&mut self, range: Range<T>) {
        let Range { start, end } = range;
        let start = if let Some((left_range_start, left_range_end)) = self.invalid_ranges.range(..=&start).next_back() {
            if &end <= left_range_end {
                return;
            }
            if &start <= left_range_end {
                left_range_start.clone()
            } else {
                start
            }
        } else {
            start
        };
        let end = if let Some((_, right_range_end)) = self.invalid_ranges.range(..=&end).next_back() { right_range_end.clone().max(end) } else { end };
        for (k, _) in self.invalid_ranges.clone().range(&start..=&end) {
            self.invalid_ranges.remove_mut(k);
        }
        self.invalid_ranges.insert_mut(start, end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalidate_range() {
        fn all_range(range: Range<usize>) -> impl Iterator<Item = Range<usize>> {
            range.clone().flat_map(move |start| (start..range.end).map(move |end| start..end + 1))
        }
        const MAX: usize = 6;
        for range1 in all_range(1..MAX) {
            for range2 in all_range(1..MAX) {
                for range3 in all_range(1..MAX) {
                    let mut invalidate_range = InvalidateRange::new();
                    let mut vector_validity = [true; MAX + 1];
                    for range in [&range1, &range2, &range3] {
                        vector_validity[range.clone()].fill(false);
                        invalidate_range.invalidate(range.clone());
                        assert!(vector_validity.iter().enumerate().all(|(i, &v)| invalidate_range.validity(&i) == v), "{invalidate_range:?}.validity() != {vector_validity:?}.validity()");
                        assert_eq!(vector_validity.windows(2).filter(|w| w == &[true, false]).count(), invalidate_range.invalid_ranges.size(), "{invalidate_range:?} != {vector_validity:?}");
                        for range in all_range(0..MAX + 1) {
                            let validity_range = invalidate_range.validity_range(&range.start..&range.end);
                            let expected_validity_range = {
                                let range = &vector_validity[range.clone()];
                                if range.iter().all(|&v| v) {
                                    ValidityRange::Valid
                                } else if range.iter().all(|&v| !v) {
                                    ValidityRange::Invalid
                                } else {
                                    ValidityRange::PartiallyInvalid
                                }
                            };
                            assert_eq!(validity_range, expected_validity_range, "{invalidate_range:?}.validity_range({range:?}) != {vector_validity:?}.validity_range({range:?})");
                        }
                    }
                }
            }
        }
    }
}
