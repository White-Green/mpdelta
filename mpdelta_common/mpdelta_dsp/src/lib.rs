use num::Integer;
use std::collections::VecDeque;
use std::f64::consts::PI;
use std::iter;
use std::ops::{Add, Mul};
use thiserror::Error;

pub trait WindowFunction: Sized {
    fn window(length: usize, target_sum: f64) -> Box<[Self]>;
}

pub fn window_at(x: f64) -> f64 {
    kaiser(x) * sinc(x)
}

fn kaiser(x: f64) -> f64 {
    // beta=5.0のkaiser windowをマクローリン展開したもの
    let x = x * x;
    [
        1.20366808005201e-05,
        -0.000203052303120899,
        0.00280488605194762,
        -0.0310611957038986,
        0.268651553343972,
        -1.75616418239140,
        8.31418418561665,
        -26.9040368988311,
        54.7050467707007,
        -60.8391053561263,
        27.2398718236045,
    ]
    .into_iter()
    .fold(-5.94625589656062e-07, |acc, a| acc * x + a)
}

fn sinc(x: f64) -> f64 {
    // IEEE754 float32の精度で誤差無しになる閾値
    if -1.5e-5 < x && x < 1.5e-5 {
        1.
    } else {
        let x = x * PI * 10.;
        x.sin() / x
    }
}

impl WindowFunction for f32 {
    fn window(length: usize, target_sum: f64) -> Box<[Self]> {
        assert_eq!(length & 1, 1);
        if length == 1 {
            return Box::new([target_sum as f32]);
        }
        let length_half = length / 2;
        let filter_half = (0..=length_half).map(|f| f as f64 / length_half as f64).map(window_at).collect::<Vec<_>>();
        let filter = filter_half.iter().copied().rev().chain(filter_half.iter().copied().skip(1)).collect::<Box<[_]>>();
        let sum = filter.iter().copied().sum::<f64>();
        let scaling = target_sum / sum;
        filter.iter().copied().map(|f| (f * scaling) as f32).collect()
    }
}

pub struct ResampleBuilder {
    original_freq: u32,
    target_freq: u32,
}

impl ResampleBuilder {
    pub fn new(original_freq: u32, target_freq: u32) -> ResampleBuilder {
        ResampleBuilder { original_freq, target_freq }
    }

    pub fn build<T>(self) -> Result<Resample<T>, ResampleConstructError>
    where
        T: WindowFunction + Clone + Default + Mul<Output = T> + Add<Output = T>,
    {
        Resample::from_builder(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResampleConstructError {
    #[error("invalid frequency")]
    InvalidFrequency,
}

#[derive(Debug, Clone)]
pub struct Resample<T = f32> {
    filter: Box<[(Box<[T]>, usize)]>,
    buffer: VecDeque<T>,
    default_buffer_length: usize,
    filter_index: usize,
    default_filter_index: usize,
}

impl Resample {
    pub fn builder(original_freq: u32, target_freq: u32) -> ResampleBuilder {
        ResampleBuilder { original_freq, target_freq }
    }
}

impl<T> Resample<T>
where
    T: WindowFunction + Clone + Default + Mul<Output = T> + Add<Output = T>,
{
    fn from_builder(builder: ResampleBuilder) -> Result<Resample<T>, ResampleConstructError> {
        let ResampleBuilder { original_freq, target_freq } = builder;
        if original_freq == 0 || target_freq == 0 {
            return Err(ResampleConstructError::InvalidFrequency);
        }
        let gcd = original_freq.gcd(&target_freq);
        let original = (original_freq / gcd) as usize;
        let target = (target_freq / gcd) as usize;
        let pqmax = original.max(target);
        if pqmax == 1 {
            let filter = T::window(1, 1.);
            assert_eq!(filter.len(), 1);
            return Ok(Resample {
                filter: Box::new([(filter, 1)]),
                buffer: VecDeque::new(),
                default_buffer_length: 0,
                filter_index: 0,
                default_filter_index: 0,
            });
        }
        const N: usize = 10;
        let filter = T::window(pqmax * N * 2 + 1, target as f64);
        assert_eq!(filter.len(), pqmax * N * 2 + 1);

        let default_buffer_length = filter.len() / 2 / target;
        let buffer = vec![T::default(); default_buffer_length].into();

        let filter = (0..target)
            .map(|n| {
                let filter_offset = (target - n) * original % target;
                let step = ((n + 1) * original + target - 1) / target - (n * original + target - 1) / target;
                let filter = iter::successors(Some(filter_offset), |n| Some(*n + target)).map_while(|n| filter.get(n)).cloned().collect::<Box<[_]>>();
                (filter, step)
            })
            .collect::<Box<[_]>>();
        // println!("{:?}", filter.iter().map(|(_, step)| *step).collect::<Vec<_>>());
        let start_offset = filter.len() / 2 % target;
        let (filter_index, _) = (0..target).map(|n| (target - n) * original % target).enumerate().find(|(_, offset)| *offset == start_offset).unwrap();

        Ok(Resample {
            filter,
            buffer,
            default_buffer_length,
            filter_index,
            default_filter_index: filter_index,
        })
    }

    pub fn default_buffer_len(&self) -> usize {
        self.default_buffer_length
    }

    pub fn reset_buffer_with_default_buffer(&mut self, default_buffer: impl IntoIterator<Item = T>) {
        self.buffer.clear();
        self.buffer.extend(default_buffer.into_iter().take(self.default_buffer_length));
        iter::repeat(T::default()).take(self.default_buffer_length - self.buffer.len()).for_each(|v| self.buffer.push_front(v));
        self.filter_index = self.default_filter_index;
    }

    pub fn reset_buffer(&mut self) {
        self.buffer.clear();
        self.buffer.resize(self.default_buffer_length, T::default());
        self.filter_index = self.default_filter_index;
    }

    pub fn fill_tail_by_zero(&mut self) {
        self.buffer.extend(iter::repeat(T::default()).take(self.default_buffer_length));
    }

    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    pub fn extend(&mut self, items: impl IntoIterator<Item = T>) {
        self.buffer.extend(items)
    }

    pub fn take_result(&mut self) -> Box<[T]> {
        let mut result = Vec::new();
        let iter = self.filter[self.filter_index..].iter().chain(self.filter.iter().cycle()).scan(0, |sum, (filter, offset)| {
            let current_sum = *sum;
            *sum += *offset;
            Some((filter, current_sum))
        });
        for (i, (filter, offset)) in iter.enumerate() {
            if offset + filter.len() < self.buffer.len() {
                let value = self.buffer.range(offset..).cloned().zip(filter.iter().cloned()).map(|(v, f)| v * f).reduce(Add::add).unwrap_or_else(T::default);
                result.push(value);
            } else {
                self.buffer.drain(..offset);
                self.filter_index = (self.filter_index + i) % self.filter.len();
                break;
            }
        }
        result.into_boxed_slice()
    }
}

impl<T> Iterator for Resample<T>
where
    T: Clone + Default + Mul<Output = T> + Add<Output = T>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let (filter, offset) = &self.filter[self.filter_index];
        if self.buffer.len() < filter.len() {
            return None;
        }
        let value = self.buffer.iter().cloned().zip(filter.iter().cloned()).map(|(v, f)| v * f).reduce(Add::add).unwrap_or_else(T::default);
        self.buffer.drain(..offset);
        self.filter_index = (self.filter_index + 1) % self.filter.len();
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_f32() {
        assert_eq!(*f32::window(1, 1.), [1.]);
    }
}
