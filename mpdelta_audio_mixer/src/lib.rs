use mpdelta_core::time::TimelineTime;
use mpdelta_core_audio::multi_channel_audio::{MultiChannelAudio, MultiChannelAudioMutOp, MultiChannelAudioOp, MultiChannelAudioSliceMut};
use mpdelta_core_audio::{AudioProvider, AudioType};
use mpdelta_renderer::{Combiner, CombinerBuilder, TimeMap};
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
}

impl AudioProvider for MixedAudio {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> usize {
        self.inner.buffer.channels()
    }

    fn compute_audio(&mut self, begin: TimelineTime, mut dst: MultiChannelAudioSliceMut<f32>) -> usize {
        let end = begin + TimelineTime::new(dst.len() as f64 / 96000.).unwrap();
        // copy on write
        let mixed_audio = Arc::make_mut(&mut self.inner);
        match (dst.channels(), mixed_audio.buffer.channels()) {
            (0, _) | (_, 0) => unreachable!(),
            (1, 1) => compute_audio_inner(mixed_audio, self.sample_rate, begin, end, dst.slice_mut(..).unwrap(), |a, b| a[0] += b[0]),
            (1, _) => compute_audio_inner(mixed_audio, self.sample_rate, begin, end, dst.slice_mut(..).unwrap(), |a, b| a[0] += (b[0] + b[1]) / 2.),
            (_, 1) => compute_audio_inner(mixed_audio, self.sample_rate, begin, end, dst.slice_mut(..).unwrap(), |a, b| a[..2].iter_mut().for_each(|a| *a += b[0])),
            (_, _) => compute_audio_inner(mixed_audio, self.sample_rate, begin, end, dst.slice_mut(..).unwrap(), |a, b| a.iter_mut().zip(b).for_each(|(a, b)| *a += *b)),
        }
        (((self.length - begin).value() * 96000.) as usize).min(dst.len())
    }
}

fn compute_audio_inner<F>(mixed_audio: &mut MixedAudioInner, sample_rate: u32, begin: TimelineTime, end: TimelineTime, mut dst: MultiChannelAudioSliceMut<f32>, combiner: F)
where
    F: Fn(&mut [f32], &[f32]),
{
    let MixedAudioInner { source, buffer } = mixed_audio;
    buffer.resize(dst.len(), 0.);
    dst.fill(0.);
    for (audio, param) in source.iter_mut() {
        if param.right() <= begin || end <= param.left() {
            continue;
        }
        let offset = (begin - param.left()).max(TimelineTime::ZERO);
        // TODO: 再生速度変化の対応が未だ
        // TODO: サンプリングレートが違う場合の対応も未だ
        let computed_len = audio.compute_audio(param.map(offset).unwrap(), buffer.slice_mut(..).unwrap());
        for (a, b) in dst.slice_mut(((param.left() - begin).value() * sample_rate as f64) as usize..).unwrap().iter_mut().zip(buffer.slice(..computed_len).unwrap().iter()) {
            combiner(a, b)
        }
    }
}

#[cfg(test)]
mod tests {}
