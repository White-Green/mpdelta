use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::time::TimelineTime;
use mpdelta_core_audio::multi_channel_audio::{MultiChannelAudio, MultiChannelAudioMutOp, MultiChannelAudioOp, MultiChannelAudioSliceMut};
use mpdelta_core_audio::{AudioProvider, AudioType};
use mpdelta_dsp::Resample;
use mpdelta_renderer::{Combiner, CombinerBuilder, TimeMap, TimeMapSegment};
use smallvec::{smallvec, SmallVec};
use std::cmp::Ordering;
use std::iter;
use std::num::Wrapping;
use std::ops::Range;
use std::sync::Arc;

#[derive(Default)]
pub struct MPDeltaAudioMixerBuilder {}

impl MPDeltaAudioMixerBuilder {
    pub fn new() -> MPDeltaAudioMixerBuilder {
        Self::default()
    }
}

impl CombinerBuilder<AudioType> for MPDeltaAudioMixerBuilder {
    type Request = TimelineTime;
    type Param = TimeMap;
    type Combiner = MPDeltaAudioMixer;

    fn new_combiner(&self, request: Self::Request) -> Self::Combiner {
        MPDeltaAudioMixer::new(request)
    }
}

pub struct MPDeltaAudioMixer {
    length: TimelineTime,
    channels: usize,
    sample_rate: u32,
    buffer: Vec<(AudioType, TimeMap)>,
}

impl MPDeltaAudioMixer {
    fn new(length: TimelineTime) -> MPDeltaAudioMixer {
        MPDeltaAudioMixer {
            length,
            channels: 1,
            sample_rate: 1,
            buffer: Vec::new(),
        }
    }
}

impl Combiner<AudioType> for MPDeltaAudioMixer {
    type Param = TimeMap;

    fn add(&mut self, data: AudioType, param: Self::Param) {
        self.channels = self.channels.max(data.channels());
        self.sample_rate = self.sample_rate.max(data.sample_rate());
        self.buffer.push((data, param));
    }

    fn collect(self) -> AudioType {
        AudioType::new(MixedAudio {
            length: self.length,
            sample_rate: self.sample_rate,
            inner: Arc::new(MixedAudioInner {
                source: self.buffer,
                buffer: MultiChannelAudio::new(self.channels),
                single_audio_buffer: MultiChannelAudio::new(self.channels),
            }),
        })
    }
}

#[derive(Clone)]
struct MixedAudio {
    length: TimelineTime,
    sample_rate: u32,
    inner: Arc<MixedAudioInner>,
}

#[derive(Clone)]
struct MixedAudioInner {
    source: Vec<(AudioType, TimeMap)>,
    buffer: MultiChannelAudio<f32>,
    single_audio_buffer: MultiChannelAudio<f32>,
}

impl AudioProvider for MixedAudio {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> usize {
        self.inner.buffer.channels()
    }

    fn compute_audio(&mut self, begin: TimelineTime, mut dst: MultiChannelAudioSliceMut<f32>) -> usize {
        let end = begin + TimelineTime::new(MixedFraction::from_fraction(dst.len() as i64, self.sample_rate));
        // copy on write
        let mixed_audio = Arc::make_mut(&mut self.inner);
        match (dst.channels(), mixed_audio.buffer.channels()) {
            (0, _) | (_, 0) => unreachable!(),
            (1, 1) => compute_audio_inner(mixed_audio, self.sample_rate, begin, end, dst.slice_mut(..).unwrap(), |a, b| a[0] += b[0]),
            (1, _) => compute_audio_inner(mixed_audio, self.sample_rate, begin, end, dst.slice_mut(..).unwrap(), |a, b| a[0] += (b[0] + b[1]) / 2.),
            (_, 1) => compute_audio_inner(mixed_audio, self.sample_rate, begin, end, dst.slice_mut(..).unwrap(), |a, b| a[..2].iter_mut().for_each(|a| *a += b[0])),
            (_, _) => compute_audio_inner(mixed_audio, self.sample_rate, begin, end, dst.slice_mut(..).unwrap(), |a, b| a.iter_mut().zip(b).for_each(|(a, b)| *a += *b)),
        }
        (((self.length - begin).value() * MixedFraction::from_integer(self.sample_rate as i32)).deconstruct().0.max(0) as usize).min(dst.len())
    }
}

fn compute_audio_inner<F>(mixed_audio: &mut MixedAudioInner, sample_rate: u32, begin: TimelineTime, end: TimelineTime, mut dst: MultiChannelAudioSliceMut<f32>, combiner: F)
where
    F: Fn(&mut [f32], &[f32]),
{
    let MixedAudioInner { source, buffer, single_audio_buffer } = mixed_audio;
    buffer.resize(dst.len(), 0.);
    dst.fill(0.);
    for &mut (ref mut audio, ref param) in source.iter_mut() {
        if param.right() <= begin || end <= param.left() {
            continue;
        }
        let begin_pos = begin.max(param.left());
        let end_pos = end.min(param.right());
        let calc_buffer_len = |range: Range<MixedFraction>, sample_rate: u32| {
            let start = range.start.deconstruct_with_round(sample_rate);
            let end = range.end.deconstruct_with_round(sample_rate);
            let start = (Wrapping(start.0 as usize), Wrapping(start.1 as usize));
            let end = (Wrapping(end.0 as usize), Wrapping(end.1 as usize));
            let sample_rate = Wrapping(sample_rate as usize);
            let Wrapping(result) = (end.0 * sample_rate + end.1) - (start.0 * sample_rate + start.1);
            result
        };
        let mut buffer_offset = 0;
        for TimeMapSegment { time_range, source_range, target_range } in param.map_range_iter(begin_pos).take_while(|TimeMapSegment { time_range, .. }| time_range.start <= end_pos) {
            let map = |at: MixedFraction| (at - source_range.start.value()) * (target_range.end.value() - target_range.start.value()) / (source_range.end.value() - source_range.start.value()) + target_range.start.value();
            let time_range = time_range.start.max(begin_pos).value()..time_range.end.min(end_pos).value();
            let audio_range = map(time_range.start)..map(time_range.end);
            let buffer_len = calc_buffer_len(time_range.clone(), sample_rate);
            match audio_range.start.cmp(&audio_range.end) {
                Ordering::Less => {
                    let single_buffer_len = calc_buffer_len(audio_range.clone(), audio.sample_rate());
                    let mut resample: SmallVec<[_; 6]> = smallvec![Resample::builder(single_buffer_len as u32, buffer_len as u32).build().unwrap(); buffer.channels()];
                    let default_buffer_len = resample[0].default_buffer_len();
                    single_audio_buffer.resize(single_buffer_len + default_buffer_len * 2 + 10, 0.);
                    single_audio_buffer.fill(0.);
                    let len = audio.compute_audio(TimelineTime::new(audio_range.start - MixedFraction::from_fraction(default_buffer_len as i64, audio.sample_rate())), single_audio_buffer.slice_mut(..).unwrap());
                    let audio_slice = single_audio_buffer.slice(..len).unwrap();
                    for (i, resample) in resample.iter_mut().enumerate() {
                        resample.reset_buffer_with_default_buffer(audio_slice.iter().map(|sample| sample[i]));
                    }
                    for sample in audio_slice.iter().skip(default_buffer_len) {
                        resample.iter_mut().zip(sample).for_each(|(resample, &sample)| resample.extend(iter::once(sample)));
                    }
                    resample.iter_mut().for_each(Resample::fill_tail_by_zero);
                    for sample in buffer.slice_mut(buffer_offset..).unwrap().iter_mut().take(buffer_len) {
                        sample.iter_mut().zip(resample.iter_mut()).try_for_each(|(sample, resample)| {
                            *sample = resample.next()?;
                            Some(())
                        });
                    }
                }
                Ordering::Greater => {
                    let single_buffer_len = calc_buffer_len(audio_range.end..audio_range.start, audio.sample_rate());
                    let mut resample: SmallVec<[_; 6]> = smallvec![Resample::builder(single_buffer_len as u32, buffer_len as u32).build().unwrap(); buffer.channels()];
                    let default_buffer_len = resample[0].buffer_len();
                    single_audio_buffer.resize(single_buffer_len + default_buffer_len * 2 + 10, 0.);
                    single_audio_buffer.fill(0.);
                    let len = audio.compute_audio(TimelineTime::new(audio_range.end - MixedFraction::from_fraction(default_buffer_len as i64, audio.sample_rate())), single_audio_buffer.slice_mut(..).unwrap());
                    let audio_slice = single_audio_buffer.slice(..len).unwrap();
                    for (i, resample) in resample.iter_mut().enumerate() {
                        resample.reset_buffer_with_default_buffer(audio_slice.iter().map(|sample| sample[i]));
                    }
                    for sample in audio_slice.iter().skip(default_buffer_len) {
                        resample.iter_mut().zip(sample).for_each(|(resample, &sample)| resample.extend(iter::once(sample)));
                    }
                    resample.iter_mut().for_each(Resample::fill_tail_by_zero);
                    let result_len = buffer
                        .slice_mut(buffer_offset..buffer_offset + buffer_len)
                        .unwrap()
                        .iter_mut()
                        .rev()
                        .filter_map(|sample| {
                            sample.iter_mut().zip(resample.iter_mut()).try_for_each(|(sample, resample)| {
                                *sample = resample.next()?;
                                Some(())
                            })
                        })
                        .count();
                    if result_len < buffer_len {
                        let channels = buffer.channels();
                        buffer.as_linear_mut().copy_within((buffer_len - result_len) * channels.., 0);
                        buffer.as_linear_mut()[result_len * channels..].fill(0.);
                    }
                }
                Ordering::Equal => {}
            }
            buffer_offset += buffer_len;
        }
        for (a, b) in dst
            .slice_mut(((param.left() - begin).value() * MixedFraction::from_integer(sample_rate as i32)).deconstruct().0.max(0) as usize..)
            .unwrap()
            .iter_mut()
            .zip(buffer.slice(..).unwrap().iter())
        {
            combiner(a, b)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mpdelta_core::component::marker_pin::{MarkerPin, MarkerTime};
    use mpdelta_core::component::parameter::ParameterValueType;
    use mpdelta_core::mfrac;
    use mpdelta_core::ptr::StaticPointerOwned;
    use qcell::{TCell, TCellOwner};

    #[derive(Clone)]
    struct ConstantAudio {
        value: f32,
        channels: usize,
        sample_rate: u32,
        length: Option<TimelineTime>,
    }

    impl ConstantAudio {
        fn new(value: f32, channels: usize, sample_rate: u32, length: Option<TimelineTime>) -> ConstantAudio {
            ConstantAudio { value, channels, sample_rate, length }
        }
    }

    impl AudioProvider for ConstantAudio {
        fn sample_rate(&self) -> u32 {
            self.sample_rate
        }

        fn channels(&self) -> usize {
            self.channels
        }

        fn compute_audio(&mut self, begin: TimelineTime, mut dst: MultiChannelAudioSliceMut<f32>) -> usize {
            let len = if let Some(length) = self.length {
                let (sec, smp) = (length - begin).value().deconstruct_with_round(self.sample_rate);
                sec as usize * self.sample_rate as usize + smp as usize
            } else {
                dst.len()
            };
            dst.slice_mut(..len).unwrap().iter_mut().for_each(|samples| samples[..self.channels].fill(self.value));
            len
        }
    }

    struct TestParameterValueType;

    impl ParameterValueType for TestParameterValueType {
        type Image = ();
        type Audio = ();
        type Binary = ();
        type String = ();
        type Integer = ();
        type RealNumber = ();
        type Boolean = ();
        type Dictionary = ();
        type Array = ();
        type ComponentClass = ();
    }

    #[test]
    fn test_audio_mix() {
        struct K;
        let key = TCellOwner::new();
        macro_rules! time_map {
            ($($m:expr),+$(,)?) => {
                {
                    let [left, markers @ .., right] = &[$($m),+];
                    TimeMap::new::<K, TestParameterValueType>(left, markers, right, &key).unwrap()
                }
            }
        }
        macro_rules! marker {
            ($t:expr$(,)?) => {
                StaticPointerOwned::new(TCell::new(MarkerPin::new_unlocked(TimelineTime::new($t))))
            };
            ($t:expr, $m:expr$(,)?) => {
                StaticPointerOwned::new(TCell::new(MarkerPin::new(TimelineTime::new($t), MarkerTime::new($m).unwrap())))
            };
        }
        let mut mixer = MPDeltaAudioMixer::new(TimelineTime::new(MixedFraction::from_integer(10)));
        mixer.add(AudioType::new(ConstantAudio::new(1., 2, 24000, None)), time_map![marker!(mfrac!(1), mfrac!(0)), marker!(mfrac!(2))]);
        mixer.add(AudioType::new(ConstantAudio::new(2., 2, 44100, None)), time_map![marker!(mfrac!(2), mfrac!(0)), marker!(mfrac!(3), mfrac!(2))]);
        let mut audio = mixer.collect();
        let mut buffer = MultiChannelAudio::new(2);
        buffer.resize(1024, 0.);
        let mut base = MixedFraction::from_fraction(1, 2);
        loop {
            let len = audio.compute_audio(TimelineTime::new(base), buffer.slice_mut(..).unwrap());
            if len < buffer.len() {
                break;
            }
            base = base + MixedFraction::from_fraction(buffer.len() as i64, audio.sample_rate());
        }
    }
}
