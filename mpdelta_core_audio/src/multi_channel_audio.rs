use std::ops::{Bound, RangeBounds};
use std::slice::{ChunksExact, ChunksExactMut};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MultiChannelAudio<T> {
    channels: usize,
    data: Vec<T>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MultiChannelAudioSlice<'a, T> {
    channels: usize,
    data: &'a [T],
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MultiChannelAudioSliceMut<'a, T> {
    channels: usize,
    data: &'a mut [T],
}

pub trait MultiChannelAudioOp<T> {
    fn channels(&self) -> usize;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn get(&self, i: usize) -> Option<&[T]>;
    fn slice(&self, range: impl RangeBounds<usize>) -> Option<MultiChannelAudioSlice<T>>;
    fn as_linear(&self) -> &[T];
    fn iter(&self) -> ChunksExact<T>;
}

pub trait MultiChannelAudioMutOp<T>: MultiChannelAudioOp<T> {
    fn get_mut(&mut self, i: usize) -> Option<&mut [T]>;
    fn slice_mut(&mut self, range: impl RangeBounds<usize>) -> Option<MultiChannelAudioSliceMut<T>>;
    fn iter_mut(&mut self) -> ChunksExactMut<T>;
    fn fill(&mut self, value: T)
    where
        T: Clone;
}

fn multiply_range(times: usize, range: impl RangeBounds<usize>) -> (Bound<usize>, Bound<usize>) {
    let start = match range.start_bound() {
        Bound::Excluded(&value) => Bound::Excluded(value * times),
        Bound::Included(&value) => Bound::Included(value * times),
        Bound::Unbounded => Bound::Unbounded,
    };
    let end = match range.end_bound() {
        Bound::Excluded(&value) => Bound::Excluded(value * times),
        Bound::Included(&value) => Bound::Included(value * times),
        Bound::Unbounded => Bound::Unbounded,
    };
    (start, end)
}

impl<T: Clone> MultiChannelAudio<T> {
    pub fn new(channels: usize) -> MultiChannelAudio<T> {
        assert!(channels >= 1);
        MultiChannelAudio { channels, data: Vec::new() }
    }

    pub fn push(&mut self, value: &[T]) {
        assert_eq!(value.len(), self.channels);
        self.data.extend_from_slice(value);
    }

    pub fn resize(&mut self, new_len: usize, value: T) {
        self.data.resize(new_len * self.channels, value);
    }
}

impl<T> MultiChannelAudioOp<T> for MultiChannelAudio<T> {
    fn channels(&self) -> usize {
        self.channels
    }

    fn len(&self) -> usize {
        self.data.len() / self.channels
    }

    fn get(&self, i: usize) -> Option<&[T]> {
        self.data.get(i * self.channels..(i + 1) * self.channels)
    }

    fn slice(&self, range: impl RangeBounds<usize>) -> Option<MultiChannelAudioSlice<T>> {
        let data = self.data.get(multiply_range(self.channels, range))?;
        Some(MultiChannelAudioSlice { channels: self.channels, data })
    }

    fn as_linear(&self) -> &[T] {
        &self.data
    }

    fn iter(&self) -> ChunksExact<T> {
        self.data.chunks_exact(self.channels)
    }
}

impl<T> MultiChannelAudioMutOp<T> for MultiChannelAudio<T> {
    fn get_mut(&mut self, i: usize) -> Option<&mut [T]> {
        self.data.get_mut(i * self.channels..(i + 1) * self.channels)
    }

    fn slice_mut(&mut self, range: impl RangeBounds<usize>) -> Option<MultiChannelAudioSliceMut<T>> {
        let data = self.data.get_mut(multiply_range(self.channels, range))?;
        Some(MultiChannelAudioSliceMut { channels: self.channels, data })
    }

    fn iter_mut(&mut self) -> ChunksExactMut<T> {
        self.data.chunks_exact_mut(self.channels)
    }

    fn fill(&mut self, value: T)
    where
        T: Clone,
    {
        self.data.fill(value)
    }
}

impl<'a, T> MultiChannelAudioOp<T> for MultiChannelAudioSlice<'a, T> {
    fn channels(&self) -> usize {
        self.channels
    }

    fn len(&self) -> usize {
        self.data.len() / self.channels
    }

    fn get(&self, i: usize) -> Option<&[T]> {
        self.data.get(i * self.channels..(i + 1) * self.channels)
    }

    fn slice(&self, range: impl RangeBounds<usize>) -> Option<MultiChannelAudioSlice<T>> {
        let data = self.data.get(multiply_range(self.channels, range))?;
        Some(MultiChannelAudioSlice { channels: self.channels, data })
    }

    fn as_linear(&self) -> &[T] {
        self.data
    }

    fn iter(&self) -> ChunksExact<T> {
        self.data.chunks_exact(self.channels)
    }
}

impl<'a, T> MultiChannelAudioOp<T> for MultiChannelAudioSliceMut<'a, T> {
    fn channels(&self) -> usize {
        self.channels
    }

    fn len(&self) -> usize {
        self.data.len() / self.channels
    }

    fn get(&self, i: usize) -> Option<&[T]> {
        self.data.get(i * self.channels..(i + 1) * self.channels)
    }

    fn slice(&self, range: impl RangeBounds<usize>) -> Option<MultiChannelAudioSlice<T>> {
        let data = self.data.get(multiply_range(self.channels, range))?;
        Some(MultiChannelAudioSlice { channels: self.channels, data })
    }

    fn as_linear(&self) -> &[T] {
        self.data
    }

    fn iter(&self) -> ChunksExact<T> {
        self.data.chunks_exact(self.channels)
    }
}

impl<'a, T> MultiChannelAudioMutOp<T> for MultiChannelAudioSliceMut<'a, T> {
    fn get_mut(&mut self, i: usize) -> Option<&mut [T]> {
        self.data.get_mut(i * self.channels..(i + 1) * self.channels)
    }

    fn slice_mut(&mut self, range: impl RangeBounds<usize>) -> Option<MultiChannelAudioSliceMut<T>> {
        let data = self.data.get_mut(multiply_range(self.channels, range))?;
        Some(MultiChannelAudioSliceMut { channels: self.channels, data })
    }

    fn iter_mut(&mut self) -> ChunksExactMut<T> {
        self.data.chunks_exact_mut(self.channels)
    }

    fn fill(&mut self, value: T)
    where
        T: Clone,
    {
        self.data.fill(value)
    }
}
