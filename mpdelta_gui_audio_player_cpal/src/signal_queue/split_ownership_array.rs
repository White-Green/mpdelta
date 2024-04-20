use crossbeam_utils::CachePadded;
#[cfg(loom)]
use loom::sync::atomic::{self, AtomicBool, AtomicUsize};
use std::cell::UnsafeCell;
use std::ptr::NonNull;
#[cfg(not(loom))]
use std::sync::atomic::{self, AtomicBool, AtomicUsize};
use std::{array, cmp};

struct Inner<T, const N: usize> {
    data: UnsafeCell<CachePadded<[T; N]>>,
    indices: [CachePadded<AtomicUsize>; 2],
    one_dropped: AtomicBool,
}

impl<T, const N: usize> Inner<T, N> {
    fn new(initializer: impl Fn() -> T) -> Inner<T, N> {
        Inner {
            data: UnsafeCell::new(CachePadded::new(array::from_fn(|_| initializer()))),
            indices: [CachePadded::new(AtomicUsize::new(0)), CachePadded::new(AtomicUsize::new(1))],
            one_dropped: AtomicBool::new(false),
        }
    }
}

pub struct SplitOwnershipArray<T, const N: usize> {
    inner: NonNull<Inner<T, N>>,
    secondary_index_copy: usize,
    primary_index: bool,
}

// SAFETY: 並行性による問題は抑制しているのでTがSend/Syncでさえあれば良い
unsafe impl<T: Send + Sync, const N: usize> Send for SplitOwnershipArray<T, N> {}

unsafe impl<T: Send + Sync, const N: usize> Sync for SplitOwnershipArray<T, N> {}

impl<T, const N: usize> SplitOwnershipArray<T, N> {
    const INDEX_MASK: usize = {
        assert!(N.is_power_of_two());
        N - 1
    };

    pub fn new(initializer: impl Fn() -> T) -> (SplitOwnershipArray<T, N>, SplitOwnershipArray<T, N>) {
        let inner = NonNull::new(Box::leak(Box::new(Inner::new(initializer)))).unwrap();
        let left = SplitOwnershipArray { inner, secondary_index_copy: 1, primary_index: false };
        let right = SplitOwnershipArray { inner, secondary_index_copy: 0, primary_index: true };
        (left, right)
    }

    pub fn get_slice_mut(&mut self, request_len: usize) -> [&mut [T]; 2] {
        // SAFETY: newとdropの実装によりinnerは有効なポインタであるということが保証されるためsafe
        let inner = unsafe { self.inner.as_ref() };
        let primary_index = inner.indices[self.primary_index as usize].load(atomic::Ordering::Acquire) & Self::INDEX_MASK;
        if (self.secondary_index_copy.wrapping_sub(primary_index) & Self::INDEX_MASK) >= request_len {
            // SAFETY: data配列内の、自身が所有権を持っている部分にのみアクセスするためsafe
            let all_data = unsafe { &mut *inner.data.get() };
            return if primary_index < self.secondary_index_copy {
                [&mut all_data[primary_index..self.secondary_index_copy], &mut []]
            } else {
                let (left, right) = all_data.split_at_mut(primary_index);
                [right, &mut left[..self.secondary_index_copy]]
            };
        }
        self.secondary_index_copy = inner.indices[(!self.primary_index) as usize].load(atomic::Ordering::Acquire) & Self::INDEX_MASK;
        // SAFETY: data配列内の、自身が所有権を持っている部分にのみアクセスするためsafe
        let all_data = unsafe { &mut *inner.data.get() };
        if primary_index < self.secondary_index_copy {
            [&mut all_data[primary_index..self.secondary_index_copy], &mut []]
        } else {
            let (left, right) = all_data.split_at_mut(primary_index);
            [right, &mut left[..self.secondary_index_copy]]
        }
    }

    pub fn release_array(&mut self, len: usize) -> Result<(), ()> {
        if len == 0 {
            return Ok(());
        }
        if len > N - 2 {
            return Err(());
        }
        // SAFETY: newとdropの実装によりinnerは有効なポインタであるということが保証されるためsafe
        let inner = unsafe { self.inner.as_ref() };
        let current_primary_index = inner.indices[self.primary_index as usize].load(atomic::Ordering::Relaxed) & Self::INDEX_MASK;
        let next_primary_index = (current_primary_index + len) & Self::INDEX_MASK;
        match (current_primary_index.cmp(&self.secondary_index_copy), next_primary_index.cmp(&self.secondary_index_copy), current_primary_index.cmp(&next_primary_index)) {
            (cmp::Ordering::Equal, _, _) => unreachable!(),
            (cmp::Ordering::Less, cmp::Ordering::Less, cmp::Ordering::Less) | (cmp::Ordering::Greater, cmp::Ordering::Greater, cmp::Ordering::Less) | (cmp::Ordering::Greater, cmp::Ordering::Less, cmp::Ordering::Greater) => {
                inner.indices[self.primary_index as usize].store(next_primary_index, atomic::Ordering::Release);
                return Ok(());
            }
            _ => {}
        }
        self.secondary_index_copy = inner.indices[(!self.primary_index) as usize].load(atomic::Ordering::Relaxed) & Self::INDEX_MASK;
        match (current_primary_index.cmp(&self.secondary_index_copy), next_primary_index.cmp(&self.secondary_index_copy), current_primary_index.cmp(&next_primary_index)) {
            (cmp::Ordering::Equal, _, _) => unreachable!(),
            (cmp::Ordering::Less, cmp::Ordering::Less, cmp::Ordering::Less) | (cmp::Ordering::Greater, cmp::Ordering::Greater, cmp::Ordering::Less) | (cmp::Ordering::Greater, cmp::Ordering::Less, cmp::Ordering::Greater) => {
                inner.indices[self.primary_index as usize].store(next_primary_index, atomic::Ordering::Release);
                Ok(())
            }
            _ => Err(()),
        }
    }
}

impl<T, const N: usize> Drop for SplitOwnershipArray<T, N> {
    fn drop(&mut self) {
        let inner = unsafe { self.inner.as_mut() };
        if inner.one_dropped.compare_exchange(false, true, atomic::Ordering::AcqRel, atomic::Ordering::Relaxed).is_ok() {
            return;
        }
        // SAFETY: この部分はちょうど1回のみ実行されるためsafe
        unsafe {
            drop(Box::from_raw(self.inner.as_ptr()));
        }
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::*;

    #[test]
    fn test_split_ownership_array() {
        let (mut left, mut right) = SplitOwnershipArray::<i32, 8>::new(|| 0);
        assert_eq!(left.get_slice_mut(1), [&mut [0][..], &mut []]);
        assert_eq!(right.get_slice_mut(7), [&mut [0; 7][..], &mut []]);
        let x = right.get_slice_mut(3);
        x[0][..3].copy_from_slice(&[1, 2, 3]);
        assert_eq!(right.release_array(7), Err(()));
        assert_eq!(right.release_array(3), Ok(()));
        assert_eq!(left.get_slice_mut(4), [&mut [0, 1, 2, 3][..], &mut []]);
        assert_eq!(right.get_slice_mut(4), [&mut [0; 4][..], &mut []]);
        assert_eq!(left.release_array(4), Err(()));
        assert_eq!(left.get_slice_mut(4), [&mut [0, 1, 2, 3][..], &mut []]);
        assert_eq!(right.get_slice_mut(4), [&mut [0; 4][..], &mut []]);
    }
}

#[cfg(all(loom, test))]
mod tests_loom {
    use super::*;

    #[test]
    fn test_split_ownership_array_concurrency() {
        loom::model(|| {
            let (mut left, mut right) = SplitOwnershipArray::<AtomicUsize, 8>::new(AtomicUsize::default);
            assert_eq!(right.release_array(3), Ok(()));
            loom::thread::spawn(move || {
                for i in 1..4 {
                    right.get_slice_mut(8).into_iter().flatten().for_each(|v| {
                        assert_eq!(v.load(atomic::Ordering::Relaxed), 0);
                        v.store(1, atomic::Ordering::Relaxed);
                    });
                    right.get_slice_mut(8).into_iter().flatten().for_each(|v| {
                        let value = v.load(atomic::Ordering::Relaxed);
                        assert!(value == 1 || value == 0);
                        v.store(0, atomic::Ordering::Relaxed);
                    });
                    while right.release_array(i).is_err() {
                        loom::thread::yield_now();
                    }
                }
            });
            for i in 1..4 {
                left.get_slice_mut(8).into_iter().flatten().for_each(|v| {
                    assert_eq!(v.load(atomic::Ordering::Relaxed), 0);
                    v.store(2, atomic::Ordering::Relaxed);
                });
                left.get_slice_mut(8).into_iter().flatten().for_each(|v| {
                    let value = v.load(atomic::Ordering::Relaxed);
                    assert!(value == 2 || value == 0);
                    v.store(0, atomic::Ordering::Relaxed);
                });
                while left.release_array(i).is_err() {
                    loom::thread::yield_now();
                }
            }
        });
    }
}
