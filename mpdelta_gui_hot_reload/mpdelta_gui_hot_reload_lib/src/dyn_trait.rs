use mpdelta_core::time::TimelineTime;
use mpdelta_gui::AudioTypePlayer;

pub trait AudioTypePlayerDyn<Audio> {
    fn set_audio_dyn(&self, audio: Audio);
    fn seek_dyn(&self, time: TimelineTime);
    fn play_dyn(&self);
    fn pause_dyn(&self);
}

impl<Audio, O> AudioTypePlayerDyn<Audio> for O
where
    O: AudioTypePlayer<Audio>,
{
    fn set_audio_dyn(&self, audio: Audio) {
        self.set_audio(audio)
    }

    fn seek_dyn(&self, time: TimelineTime) {
        self.seek(time)
    }

    fn play_dyn(&self) {
        self.play()
    }

    fn pause_dyn(&self) {
        self.pause()
    }
}

impl<Audio> AudioTypePlayer<Audio> for dyn AudioTypePlayerDyn<Audio> + Send + Sync {
    fn set_audio(&self, audio: Audio) {
        self.set_audio_dyn(audio)
    }

    fn seek(&self, time: TimelineTime) {
        self.seek_dyn(time)
    }

    fn play(&self) {
        self.play_dyn()
    }

    fn pause(&self) {
        self.pause_dyn()
    }
}
