use mpdelta_core::time::TimelineTime;
use multi_channel_audio::MultiChannelAudioSliceMut;

pub mod multi_channel_audio;

pub trait AudioProvider {
    fn sample_rate(&self) -> u32;
    fn channels(&self) -> usize;
    fn compute_audio(&mut self, begin: TimelineTime, dst: MultiChannelAudioSliceMut<f32>) -> usize;
}

pub trait AudioProviderCloneable: AudioProvider {
    fn clone_dyn(&self) -> Box<dyn AudioProviderCloneable + Send + Sync>;
}

impl<T> AudioProviderCloneable for T
where
    T: 'static + AudioProvider + Clone + Send + Sync,
{
    fn clone_dyn(&self) -> Box<dyn AudioProviderCloneable + Send + Sync> {
        Box::new(self.clone())
    }
}

pub struct AudioType(Box<dyn AudioProviderCloneable + Send + Sync>);

impl Clone for AudioType {
    fn clone(&self) -> Self {
        AudioType(self.0.clone_dyn())
    }
}

impl AudioProvider for AudioType {
    fn sample_rate(&self) -> u32 {
        self.0.sample_rate()
    }

    fn channels(&self) -> usize {
        self.0.channels()
    }

    fn compute_audio(&mut self, begin: TimelineTime, dst: MultiChannelAudioSliceMut<f32>) -> usize {
        self.0.compute_audio(begin, dst)
    }
}

impl AudioType {
    pub fn new<T>(audio: T) -> AudioType
    where
        T: AudioProviderCloneable + Send + Sync + 'static,
    {
        AudioType(Box::new(audio))
    }
}
