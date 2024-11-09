use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::time::TimelineTime;
use mpdelta_core_audio::multi_channel_audio::{MultiChannelAudio, MultiChannelAudioMutOp, MultiChannelAudioOp, MultiChannelAudioSliceMut};
use mpdelta_core_audio::{AudioProvider, AudioType};
use mpdelta_dsp::{Resample, WindowFunction};
use mpdelta_renderer::{AudioCombinerParam, AudioCombinerRequest, Combiner, CombinerBuilder, GlobalTime, LocalTime, TimeStretch};
use smallvec::{smallvec, SmallVec};
use std::cmp::Ordering;
use std::future::Future;
use std::ops::{Add, Mul};
use std::sync::Arc;
use std::{future, iter};

#[derive(Default)]
pub struct MPDeltaAudioMixerBuilder {}

impl MPDeltaAudioMixerBuilder {
    pub fn new() -> MPDeltaAudioMixerBuilder {
        Self::default()
    }
}

impl CombinerBuilder<AudioType> for MPDeltaAudioMixerBuilder {
    type Request = AudioCombinerRequest;
    type Param = AudioCombinerParam;
    type Combiner = MPDeltaAudioMixer;

    fn new_combiner(&self, request: Self::Request) -> Self::Combiner {
        MPDeltaAudioMixer::new(request.length)
    }
}

pub struct MPDeltaAudioMixer {
    length: TimelineTime,
    channels: usize,
    sample_rate: u32,
    buffer: Vec<(AudioType, Arc<TimeStretch<GlobalTime, LocalTime>>)>,
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
    type Param = AudioCombinerParam;

    fn add(&mut self, data: AudioType, param: Self::Param) {
        self.channels = self.channels.max(data.channels());
        self.sample_rate = self.sample_rate.max(data.sample_rate());
        self.buffer.push((data, param.time_map));
    }

    fn collect<'async_trait>(self) -> impl Future<Output = AudioType> + Send + 'async_trait
    where
        Self: 'async_trait,
        AudioType: 'async_trait,
    {
        future::ready(AudioType::new(MixedAudio {
            length: self.length,
            sample_rate: self.sample_rate,
            inner: Arc::new(MixedAudioInner {
                source: self.buffer,
                buffer: MultiChannelAudio::new(self.channels),
                single_audio_buffer: MultiChannelAudio::new(self.channels),
            }),
        }))
    }
}

#[derive(Clone)]
struct MixedAudio {
    length: TimelineTime,
    sample_rate: u32,
    inner: Arc<MixedAudioInner<f32, AudioType>>,
}

#[derive(Clone)]
struct MixedAudioInner<T, A> {
    source: Vec<(A, Arc<TimeStretch<GlobalTime, LocalTime>>)>,
    buffer: MultiChannelAudio<T>,
    single_audio_buffer: MultiChannelAudio<T>,
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

trait AnyAudioProvider<T> {
    fn sample_rate_any(&self) -> u32;
    fn compute_audio_any(&mut self, begin: TimelineTime, dst: MultiChannelAudioSliceMut<T>) -> usize;
}

impl AnyAudioProvider<f32> for AudioType {
    fn sample_rate_any(&self) -> u32 {
        <AudioType as AudioProvider>::sample_rate(self)
    }

    fn compute_audio_any(&mut self, begin: TimelineTime, dst: MultiChannelAudioSliceMut<f32>) -> usize {
        <AudioType as AudioProvider>::compute_audio(self, begin, dst)
    }
}

fn compute_audio_inner<T, A, F>(mixed_audio: &mut MixedAudioInner<T, A>, sample_rate: u32, begin: TimelineTime, end: TimelineTime, mut dst: MultiChannelAudioSliceMut<T>, combiner: F)
where
    T: WindowFunction + Clone + Default + Mul<Output = T> + Add<Output = T>,
    A: AnyAudioProvider<T>,
    F: Fn(&mut [T], &[T]),
{
    let begin = TimelineTime::new(begin.value().round_to_denominator(sample_rate));
    let end = TimelineTime::new(end.value().round_to_denominator(sample_rate));
    let round_by_sample_rate = |t: MixedFraction| {
        let (i, n) = t.deconstruct_with_round(sample_rate);
        MixedFraction::new(i, n, sample_rate)
    };
    let MixedAudioInner { source, buffer, single_audio_buffer } = mixed_audio;
    buffer.resize(dst.len(), T::default());
    dst.fill(T::default());
    for &mut (ref mut audio, ref param) in source.iter_mut() {
        if param.right().time() <= begin || end <= param.left().time() {
            continue;
        }
        let audio_sample_rate = audio.sample_rate_any();
        let begin_pos = begin.max(param.left().time());
        let end_pos = end.min(param.right().time());
        for time_map in param.map_range_iter(begin_pos.into()).take_while(|time_map| time_map.start().time() <= end_pos) {
            let time_range = time_map.start().max(begin_pos.into()).time().value()..time_map.end().min(end_pos.into()).time().value();
            let time_range = round_by_sample_rate(time_range.start)..round_by_sample_rate(time_range.end);
            let audio_range = time_map.map(TimelineTime::new(time_range.start).into()).time()..time_map.map(TimelineTime::new(time_range.end).into()).time();
            let audio_compute_start = audio_range.start.min(audio_range.end).max(TimelineTime::ZERO);
            let audio_compute_end = audio_range.start.max(audio_range.end);
            let (audio_sample_rate_scaled, _) = (MixedFraction::from_integer(audio_sample_rate as i32) * time_map.scale().abs()).deconstruct_with_round(1);
            let audio_sample_rate_scaled = audio_sample_rate_scaled as u32;
            let Ok(resample) = Resample::builder(audio_sample_rate_scaled, sample_rate).build::<T>() else {
                continue;
            };
            let mut resample: SmallVec<[_; 6]> = smallvec![resample; buffer.channels()];
            let default_buffer_len = resample[0].default_buffer_len();

            let compute_base_time = audio_compute_start.value().floor_to_denominator(audio_sample_rate);
            let request_begin = compute_base_time - MixedFraction::from_fraction(default_buffer_len as i64, audio_sample_rate);
            let leading_zeros = {
                let leading_zero_len = if request_begin.signum() < 0 { -request_begin } else { MixedFraction::ZERO };
                let (i, n) = leading_zero_len.deconstruct_with_round(audio_sample_rate);
                i as usize * audio_sample_rate as usize + n as usize
            };
            let request_begin = request_begin.max(MixedFraction::ZERO);
            let end = audio_compute_end.value().ceil_to_denominator(audio_sample_rate);
            let buffer_len = {
                let (i, n) = (end - request_begin).deconstruct_with_round(audio_sample_rate);
                i as usize * audio_sample_rate as usize + n as usize + default_buffer_len
            };
            single_audio_buffer.resize(buffer_len, T::default());
            single_audio_buffer.fill(T::default());
            let computed_len = audio.compute_audio_any(TimelineTime::new(request_begin), single_audio_buffer.slice_mut(..).unwrap());
            let result = single_audio_buffer.slice(..computed_len).unwrap();
            let Some(default_value) = result.get(0) else {
                continue;
            };
            let leading = result.slice(..default_buffer_len - leading_zeros).unwrap();
            let body = result.slice(default_buffer_len - leading_zeros..).unwrap();
            for (i, resample) in resample.iter_mut().enumerate() {
                resample.reset_buffer_with_default_buffer(iter::repeat(default_value).take(leading_zeros).chain(leading.iter()).map(|v| v[i].clone()));
                resample.extend(body.iter().map(|v| v[i].clone()));
                let last = body.get(body.len() - 1).unwrap()[i].clone();
                resample.extend(iter::repeat(last).take(default_buffer_len));
            }
            let skip = {
                let (i, n) = (time_map.map_inverse(audio_compute_start.into()).time() - time_map.map_inverse(TimelineTime::new(compute_base_time).into()).into()).value().deconstruct_with_round(sample_rate);
                i as usize * sample_rate as usize + n as usize
            };
            let mut resample = resample.iter_mut().map(|resample| resample.skip(skip)).collect::<SmallVec<[_; 6]>>();
            buffer.resize(0, T::default());
            let len = iter::from_fn(|| {
                let sample = resample.iter_mut().map(|resample| resample.next()).collect::<Option<SmallVec<[_; 6]>>>()?;
                buffer.push(&sample);
                Some(())
            })
            .count();

            let dst_offset = {
                let (i, n) = (time_range.start - begin.value()).deconstruct_with_round(sample_rate);
                usize::try_from(i).unwrap() * sample_rate as usize + n as usize
            };
            let dst_limit = {
                let (i, n) = (time_range.end - begin.value()).deconstruct_with_round(sample_rate);
                (usize::try_from(i).unwrap() * sample_rate as usize + n as usize).min(dst.len())
            };
            match audio_range.start.cmp(&audio_range.end) {
                Ordering::Less => {
                    for (a, b) in dst.slice_mut(dst_offset..dst_limit).unwrap().iter_mut().zip(buffer.slice(..len).unwrap().iter()) {
                        combiner(a, b)
                    }
                }
                Ordering::Greater => {
                    for (a, b) in dst.slice_mut(dst_offset..dst_limit).unwrap().iter_mut().zip(buffer.slice(..len).unwrap().iter().rev()) {
                        combiner(a, b)
                    }
                }
                Ordering::Equal => unreachable!(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mpdelta_core::component::marker_pin::{MarkerPin, MarkerTime};
    use mpdelta_core::core::IdGenerator;
    use mpdelta_core::mfrac;
    use mpdelta_core_test_util::TestIdGenerator;
    use mpdelta_dsp::test_util::FormalExpression;
    use mpdelta_renderer::InvalidateRange;
    use std::collections::HashMap;
    use uuid::Uuid;

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

    #[tokio::test]
    async fn test_audio_mix() {
        let id = TestIdGenerator::new();
        macro_rules! time_map {
            ($($markers:expr),+$(,)?) => {
                {
                    let mut time = HashMap::new();
                    macro_rules! marker {
                        ($t:expr) => {
                            {
                                let pin = MarkerPin::new_unlocked(id.generate_new(), );
                                time.insert(*pin.id(), TimelineTime::new($t));
                                pin
                            }
                        };
                        ($t:expr, $m:expr) => {
                            {
                                let pin = MarkerPin::new(id.generate_new(), MarkerTime::new($m).unwrap());
                                time.insert(*pin.id(), TimelineTime::new($t));
                                pin
                            }
                        };
                    }
                    let [left, markers @ .., right] = &[$($markers),+];
                    TimeStretch::new(left, markers, right, &time)
                }
            }
        }
        let mut mixer = MPDeltaAudioMixer::new(TimelineTime::new(MixedFraction::from_integer(10)));
        mixer.add(
            AudioType::new(ConstantAudio::new(1., 2, 24000, None)),
            AudioCombinerParam::new(Arc::new([]), Arc::new(time_map![marker!(mfrac!(1), mfrac!(0)), marker!(mfrac!(2))]), InvalidateRange::new()),
        );
        mixer.add(
            AudioType::new(ConstantAudio::new(2., 2, 44100, None)),
            AudioCombinerParam::new(Arc::new([]), Arc::new(time_map![marker!(mfrac!(2), mfrac!(0)), marker!(mfrac!(3), mfrac!(2))]), InvalidateRange::new()),
        );
        let mut audio = mixer.collect().await;
        assert_eq!(audio.sample_rate(), 44100);
        let mut buffer = MultiChannelAudio::new(2);
        buffer.resize(1024, 0.);
        let mut base = MixedFraction::from_fraction(1, 2);
        let mut signal = MultiChannelAudio::new(2);
        loop {
            let len = audio.compute_audio(TimelineTime::new(base), buffer.slice_mut(..).unwrap());
            buffer.slice(..len).unwrap().iter().for_each(|sig| signal.push(sig));
            if len < buffer.len() {
                break;
            }
            base = base + MixedFraction::from_fraction(len as i64, audio.sample_rate());
        }
        assert!(signal.slice(..44100 / 2 - 1).unwrap().iter().flatten().all(|s| s.abs() < 1. / 1024.));
        assert!(signal.slice(44100 / 2 + 1..44100 / 2 * 3 - 1).unwrap().iter().flatten().all(|s| (s - 1.).abs() < 1. / 1024.));
        assert!(signal.slice(44100 / 2 * 3 + 1..44100 / 2 * 5 - 1).unwrap().iter().flatten().all(|s| (s - 2.).abs() < 1. / 1024.));
    }

    #[tokio::test]
    async fn test_audio_mix_reverse() {
        let id = TestIdGenerator::new();
        macro_rules! time_map {
            ($($markers:expr),+$(,)?) => {
                {
                    let mut time = HashMap::new();
                    macro_rules! marker {
                        ($t:expr) => {
                            {
                                let pin = MarkerPin::new_unlocked(id.generate_new(), );
                                time.insert(*pin.id(), TimelineTime::new($t));
                                pin
                            }
                        };
                        ($t:expr, $m:expr) => {
                            {
                                let pin = MarkerPin::new(id.generate_new(), MarkerTime::new($m).unwrap());
                                time.insert(*pin.id(), TimelineTime::new($t));
                                pin
                            }
                        };
                    }
                    let [left, markers @ .., right] = &[$($markers),+];
                    TimeStretch::new(left, markers, right, &time)
                }
            }
        }
        let mut mixer = MPDeltaAudioMixer::new(TimelineTime::new(MixedFraction::from_integer(10)));
        mixer.add(
            AudioType::new(ConstantAudio::new(1., 2, 24000, None)),
            AudioCombinerParam::new(Arc::new([]), Arc::new(time_map![marker!(mfrac!(1), mfrac!(0)), marker!(mfrac!(2))]), InvalidateRange::new()),
        );
        mixer.add(
            AudioType::new(ConstantAudio::new(2., 2, 44100, None)),
            AudioCombinerParam::new(Arc::new([]), Arc::new(time_map![marker!(mfrac!(2), mfrac!(2)), marker!(mfrac!(3), mfrac!(0))]), InvalidateRange::new()),
        );
        let mut audio = mixer.collect().await;
        assert_eq!(audio.sample_rate(), 44100);
        let mut buffer = MultiChannelAudio::new(2);
        buffer.resize(1024, 0.);
        let mut base = MixedFraction::from_fraction(1, 2);
        let mut signal = MultiChannelAudio::new(2);
        loop {
            let len = audio.compute_audio(TimelineTime::new(base), buffer.slice_mut(..).unwrap());
            buffer.slice(..len).unwrap().iter().for_each(|sig| signal.push(sig));
            if len < buffer.len() {
                break;
            }
            base = base + MixedFraction::from_fraction(len as i64, audio.sample_rate());
        }
        assert!(signal.slice(..44100 / 2 - 1).unwrap().iter().flatten().all(|s| s.abs() < 1. / 1024.));
        assert!(signal.slice(44100 / 2 + 1..44100 / 2 * 3 - 1).unwrap().iter().flatten().all(|s| (s - 1.).abs() < 1. / 1024.));
        assert!(signal.slice(44100 / 2 * 3 + 1..44100 / 2 * 5 - 1).unwrap().iter().flatten().all(|s| (s - 2.).abs() < 1. / 1024.));
    }

    #[tokio::test]
    async fn test_audio_mix_stop() {
        let id = TestIdGenerator::new();
        macro_rules! time_map {
            ($($markers:expr),+$(,)?) => {
                {
                    let mut time = HashMap::new();
                    macro_rules! marker {
                        ($t:expr) => {
                            {
                                let pin = MarkerPin::new_unlocked(id.generate_new(), );
                                time.insert(*pin.id(), TimelineTime::new($t));
                                pin
                            }
                        };
                        ($t:expr, $m:expr) => {
                            {
                                let pin = MarkerPin::new(id.generate_new(), MarkerTime::new($m).unwrap());
                                time.insert(*pin.id(), TimelineTime::new($t));
                                pin
                            }
                        };
                    }
                    let [left, markers @ .., right] = &[$($markers),+];
                    TimeStretch::new(left, markers, right, &time)
                }
            }
        }
        let mut mixer = MPDeltaAudioMixer::new(TimelineTime::new(MixedFraction::from_integer(10)));
        mixer.add(
            AudioType::new(ConstantAudio::new(1., 2, 24000, None)),
            AudioCombinerParam::new(Arc::new([]), Arc::new(time_map![marker!(mfrac!(1), mfrac!(0)), marker!(mfrac!(2))]), InvalidateRange::new()),
        );
        mixer.add(
            AudioType::new(ConstantAudio::new(2., 2, 44100, None)),
            AudioCombinerParam::new(Arc::new([]), Arc::new(time_map![marker!(mfrac!(2), mfrac!(1)), marker!(mfrac!(3), mfrac!(1))]), InvalidateRange::new()),
        );
        let mut audio = mixer.collect().await;
        assert_eq!(audio.sample_rate(), 44100);
        let mut buffer = MultiChannelAudio::new(2);
        buffer.resize(1024, 0.);
        let mut base = MixedFraction::from_fraction(1, 2);
        let mut signal = MultiChannelAudio::new(2);
        loop {
            let len = audio.compute_audio(TimelineTime::new(base), buffer.slice_mut(..).unwrap());
            buffer.slice(..len).unwrap().iter().for_each(|sig| signal.push(sig));
            if len < buffer.len() {
                break;
            }
            base = base + MixedFraction::from_fraction(len as i64, audio.sample_rate());
        }
        assert!(signal.slice(..44100 / 2 - 1).unwrap().iter().flatten().all(|s| s.abs() < 1. / 1024.));
        assert!(signal.slice(44100 / 2 + 1..44100 / 2 * 3 - 1).unwrap().iter().flatten().all(|s| (s - 1.).abs() < 1. / 1024.));
    }

    struct FormalAudioProvider;

    impl AnyAudioProvider<FormalExpression> for FormalAudioProvider {
        fn sample_rate_any(&self) -> u32 {
            48_000
        }

        fn compute_audio_any(&mut self, begin: TimelineTime, mut dst: MultiChannelAudioSliceMut<FormalExpression>) -> usize {
            let (i, n) = begin.value().deconstruct_with_round(48_000);
            let begin = i as usize * 48000 + n as usize;
            dst.iter_mut().zip(begin..).for_each(|(dst, i)| dst.fill(FormalExpression::value(i)));
            dst.len()
        }
    }

    #[test]
    fn test_audio_mix_formal() {
        let left = MarkerPin::new(Uuid::from_u128(0), MarkerTime::ZERO);
        let right = MarkerPin::new_unlocked(Uuid::from_u128(1));
        let mut mixed_audio_inner = MixedAudioInner::<FormalExpression, FormalAudioProvider> {
            source: vec![(FormalAudioProvider, Arc::new(TimeStretch::new(&left, &[], &right, &HashMap::from([(*left.id(), TimelineTime::new(mfrac!(3, 100))), (*right.id(), TimelineTime::new(mfrac!(1)))]))))],
            buffer: MultiChannelAudio::new(1),
            single_audio_buffer: MultiChannelAudio::new(1),
        };
        let mut result = MultiChannelAudio::new(1);
        result.resize(512, FormalExpression::default());
        let expect = iter::repeat_n(FormalExpression::default(), 48_000 / 100 * 3).chain((0..).map(|i| FormalExpression::value(i) * FormalExpression::Window(0))).take(48_000).collect::<Vec<_>>();
        for i in 0.. {
            compute_audio_inner(&mut mixed_audio_inner, 48_000, TimelineTime::new(mfrac!(i * 512, 48_000)), TimelineTime::new(mfrac!((i + 1) * 512, 48_000)), result.slice_mut(..).unwrap(), |result, sig| {
                result.clone_from_slice(sig)
            });
            let expect = &expect[i as usize * 512..];
            let len = expect.len().min(512);
            assert_eq!(&result.as_linear()[..len], &expect[..len]);
            if expect.len() <= 512 {
                assert!(result.as_linear()[len..].iter().all(|v| v == &FormalExpression::default()));
                break;
            }
        }
    }
}
