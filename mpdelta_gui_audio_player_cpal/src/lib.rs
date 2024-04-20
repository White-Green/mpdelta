use crate::signal_queue::signal_queue;
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, FromSample, SampleFormat, SizedSample, SupportedStreamConfig, SupportedStreamConfigsError};
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::time::TimelineTime;
use mpdelta_core_audio::multi_channel_audio::{MultiChannelAudio, MultiChannelAudioMutOp, MultiChannelAudioOp};
use mpdelta_core_audio::{AudioProvider, AudioType};
use mpdelta_dsp::{Resample, ResampleBuilder};
use mpdelta_gui::AudioTypePlayer;
use std::fmt::Debug;
use std::sync::{Arc, PoisonError, RwLock, Weak};
use std::thread;
use std::time::Duration;
use thiserror::Error;
use tokio::runtime::Handle;
use tokio::sync::mpsc::UnboundedSender;

mod signal_queue;

pub struct CpalAudioPlayer {
    inner: Arc<RwLock<CpalAudioPlayerInner>>,
}

#[derive(Debug, Error)]
pub enum CpalAudioPlayerCreationError {
    #[error("{0}")]
    SupportedStreamConfigsError(#[from] SupportedStreamConfigsError),
    #[error("unknown sample format: {0}")]
    UnknownSampleFormat(SampleFormat),
}

impl CpalAudioPlayer {
    pub fn new<D>(device: D, runtime: &Handle) -> Result<CpalAudioPlayer, CpalAudioPlayerCreationError>
    where
        D: DeviceCreator + Clone + Send + 'static,
    {
        let inner = CpalAudioPlayerInner::new(device, runtime.clone())?;
        Ok(CpalAudioPlayer { inner })
    }
}

pub trait DeviceCreator {
    fn create_device(&self) -> Device;
}

impl<F> DeviceCreator for F
where
    F: Fn() -> Device,
{
    fn create_device(&self) -> Device {
        self()
    }
}

#[derive(Debug, Clone)]
enum StreamMessage {
    Play,
    Pause,
}

struct CpalAudioPlayerInner {
    play_message_sender: UnboundedSender<StreamMessage>,
    seek_message_sender: UnboundedSender<TimelineTime>,
    audio_sender: UnboundedSender<AudioType>,
}

fn create_config(device: &Device) -> Result<SupportedStreamConfig, CpalAudioPlayerCreationError> {
    let supported_output = device.supported_output_configs()?;
    let range = supported_output.max_by_key(|c| 0xf0 + c.sample_format().sample_size() - (c.channels() as usize * 16)).unwrap();
    Ok(range.with_max_sample_rate())
}

impl CpalAudioPlayerInner {
    fn new<D>(device_creator: D, runtime: Handle) -> Result<Arc<RwLock<CpalAudioPlayerInner>>, CpalAudioPlayerCreationError>
    where
        D: DeviceCreator + Clone + Send + 'static,
    {
        let device = device_creator.create_device();
        let config = create_config(&device)?;
        let new = |(play_message_sender, seek_message_sender, audio_sender)| CpalAudioPlayerInner { play_message_sender, seek_message_sender, audio_sender };
        let this_creator = {
            let runtime = runtime.clone();
            |this: Weak<RwLock<CpalAudioPlayerInner>>| move || CpalAudioPlayerInner::new_without_arc(device_creator.clone(), runtime.clone(), this.clone())
        };
        match config.sample_format() {
            SampleFormat::I8 => Ok(Arc::new_cyclic(|this| RwLock::new(new(create_stream_thread::<i8, _>(device, config, &runtime, this.clone(), this_creator(this.clone())))))),
            SampleFormat::I16 => Ok(Arc::new_cyclic(|this| RwLock::new(new(create_stream_thread::<i16, _>(device, config, &runtime, this.clone(), this_creator(this.clone())))))),
            SampleFormat::I32 => Ok(Arc::new_cyclic(|this| RwLock::new(new(create_stream_thread::<i32, _>(device, config, &runtime, this.clone(), this_creator(this.clone())))))),
            SampleFormat::I64 => Ok(Arc::new_cyclic(|this| RwLock::new(new(create_stream_thread::<i64, _>(device, config, &runtime, this.clone(), this_creator(this.clone())))))),
            SampleFormat::U8 => Ok(Arc::new_cyclic(|this| RwLock::new(new(create_stream_thread::<u8, _>(device, config, &runtime, this.clone(), this_creator(this.clone())))))),
            SampleFormat::U16 => Ok(Arc::new_cyclic(|this| RwLock::new(new(create_stream_thread::<u16, _>(device, config, &runtime, this.clone(), this_creator(this.clone())))))),
            SampleFormat::U32 => Ok(Arc::new_cyclic(|this| RwLock::new(new(create_stream_thread::<u32, _>(device, config, &runtime, this.clone(), this_creator(this.clone())))))),
            SampleFormat::U64 => Ok(Arc::new_cyclic(|this| RwLock::new(new(create_stream_thread::<u64, _>(device, config, &runtime, this.clone(), this_creator(this.clone())))))),
            SampleFormat::F32 => Ok(Arc::new_cyclic(|this| RwLock::new(new(create_stream_thread::<f32, _>(device, config, &runtime, this.clone(), this_creator(this.clone())))))),
            SampleFormat::F64 => Ok(Arc::new_cyclic(|this| RwLock::new(new(create_stream_thread::<f64, _>(device, config, &runtime, this.clone(), this_creator(this.clone())))))),
            format => Err(CpalAudioPlayerCreationError::UnknownSampleFormat(format)),
        }
    }

    fn new_without_arc<D>(device_creator: D, runtime: Handle, this: Weak<RwLock<CpalAudioPlayerInner>>) -> Result<CpalAudioPlayerInner, CpalAudioPlayerCreationError>
    where
        D: DeviceCreator + Clone + Send + 'static,
    {
        let device = device_creator.create_device();
        let config = create_config(&device)?;
        let this_creator = {
            let runtime = runtime.clone();
            |this: Weak<RwLock<CpalAudioPlayerInner>>| move || CpalAudioPlayerInner::new_without_arc(device_creator.clone(), runtime.clone(), this.clone())
        };
        let (play_message_sender, seek_message_sender, audio_sender) = match config.sample_format() {
            SampleFormat::I8 => create_stream_thread::<i8, _>(device, config, &runtime, this.clone(), this_creator(this)),
            SampleFormat::I16 => create_stream_thread::<i16, _>(device, config, &runtime, this.clone(), this_creator(this)),
            SampleFormat::I32 => create_stream_thread::<i32, _>(device, config, &runtime, this.clone(), this_creator(this)),
            SampleFormat::I64 => create_stream_thread::<i64, _>(device, config, &runtime, this.clone(), this_creator(this)),
            SampleFormat::U8 => create_stream_thread::<u8, _>(device, config, &runtime, this.clone(), this_creator(this)),
            SampleFormat::U16 => create_stream_thread::<u16, _>(device, config, &runtime, this.clone(), this_creator(this)),
            SampleFormat::U32 => create_stream_thread::<u32, _>(device, config, &runtime, this.clone(), this_creator(this)),
            SampleFormat::U64 => create_stream_thread::<u64, _>(device, config, &runtime, this.clone(), this_creator(this)),
            SampleFormat::F32 => create_stream_thread::<f32, _>(device, config, &runtime, this.clone(), this_creator(this)),
            SampleFormat::F64 => create_stream_thread::<f64, _>(device, config, &runtime, this.clone(), this_creator(this)),
            format => return Err(CpalAudioPlayerCreationError::UnknownSampleFormat(format)),
        };
        Ok(CpalAudioPlayerInner { play_message_sender, seek_message_sender, audio_sender })
    }
}

fn create_stream_thread<S, F>(device: Device, config: SupportedStreamConfig, runtime: &Handle, this: Weak<RwLock<CpalAudioPlayerInner>>, this_creator: F) -> (UnboundedSender<StreamMessage>, UnboundedSender<TimelineTime>, UnboundedSender<AudioType>)
where
    S: SizedSample + FromSample<f32> + Debug + Send + Sync + 'static,
    F: Fn() -> Result<CpalAudioPlayerInner, CpalAudioPlayerCreationError> + Clone + Send + 'static,
{
    assert_eq!(S::FORMAT, config.sample_format());
    let (play_message_sender, mut play_message_receiver) = tokio::sync::mpsc::unbounded_channel::<StreamMessage>();
    let (seek_message_sender, mut seek_message_receiver) = tokio::sync::mpsc::unbounded_channel::<TimelineTime>();
    let (audio_sender, mut audio_receiver) = tokio::sync::mpsc::unbounded_channel::<AudioType>();
    let stream_config = config.config();
    let stream_channels = stream_config.channels;
    let stream_sample_rate = stream_config.sample_rate.0;
    let (mut signal_sender, mut signal_receiver) = signal_queue::<S>();
    {
        let this2 = this.clone();
        let this_creator2 = this_creator.clone();
        thread::spawn(move || {
            let stream = device
                .build_output_stream(
                    &stream_config,
                    move |data: &mut [S], _info| {
                        signal_receiver.receive_signal(data);
                    },
                    move |err| {
                        eprintln!("{err}");
                        *this.upgrade().unwrap().write().unwrap_or_else(PoisonError::into_inner) = this_creator().unwrap();
                    },
                    None,
                )
                .unwrap();
            stream.pause().unwrap();
            loop {
                let Some(message) = play_message_receiver.blocking_recv() else {
                    return;
                };
                match message {
                    StreamMessage::Play => {
                        if let Err(_err) = stream.play() {
                            *this2.upgrade().unwrap().write().unwrap_or_else(PoisonError::into_inner) = this_creator2().unwrap();
                            return;
                        }
                    }
                    StreamMessage::Pause => {
                        if let Err(_err) = stream.pause() {
                            *this2.upgrade().unwrap().write().unwrap_or_else(PoisonError::into_inner) = this_creator2().unwrap();
                            return;
                        }
                    }
                }
            }
        });
    }
    runtime.spawn(async move {
        let mut play_time = TimelineTime::ZERO;
        let mut audio = loop {
            tokio::select! {
                message = seek_message_receiver.recv() => {
                    let Some(at) = message else { return; };
                    play_time = at;
                }
                new_audio = audio_receiver.recv() => {
                    let Some(new_audio) = new_audio else { return; };
                    break new_audio;
                }
            }
        };
        let mut resample = vec![ResampleBuilder::new(audio.sample_rate(), stream_sample_rate).build().unwrap(); stream_channels as usize];
        let mut audio_buffer = MultiChannelAudio::new(stream_channels as usize);
        let mut signal_buffer = Vec::new();
        let mut timer = tokio::time::interval(Duration::from_millis(1));
        loop {
            tokio::select! {
                _ = timer.tick() => {
                    loop {
                        let mut offset = 0;
                        while signal_buffer.len() > offset && signal_sender.send_signal(signal_buffer[offset..].iter().copied().take(128)).is_ok() {
                            offset += 128;
                        }
                        signal_buffer.drain(..offset.min(signal_buffer.len()));
                        if !signal_buffer.is_empty() {
                            break;
                        }
                        audio_buffer.resize((1024 * audio.sample_rate() as usize / stream_sample_rate as usize / stream_channels as usize).clamp(10, 1024), 0.);
                        let result_len = audio.compute_audio(play_time, audio_buffer.slice_mut(..).unwrap());
                        audio_buffer.iter().take(result_len).for_each(|audio_data| resample.iter_mut().zip(audio_data).for_each(|(resample, &a)| resample.extend([a])));
                        'buffer_create: loop {
                            for r in resample.iter_mut() {
                                let Some(s) = r.next() else { break 'buffer_create; };
                                signal_buffer.push(S::from_sample(s));
                            }
                        }
                        if result_len == audio_buffer.len() {
                            play_time = play_time + TimelineTime::new(MixedFraction::from_fraction(result_len as i64, audio.sample_rate()));
                        } else {
                            play_time = TimelineTime::ZERO;
                        }
                    }
                }
                message = seek_message_receiver.recv() => {
                    let Some(at) = message else { break; };
                    resample.iter_mut().for_each(Resample::reset_buffer);
                    signal_sender.flush();
                    signal_buffer.clear();
                    play_time = at;
                }
                new_audio = audio_receiver.recv() => {
                    let Some(new_audio) = new_audio else { break; };
                    if audio.sample_rate() != new_audio.sample_rate() {
                        resample = vec![ResampleBuilder::new(new_audio.sample_rate(), stream_sample_rate).build().unwrap(); stream_channels as usize];
                    } else {
                        resample.iter_mut().for_each(Resample::reset_buffer);
                    }
                    signal_sender.flush();
                    signal_buffer.clear();
                    audio = new_audio;
                }
            }
        }
    });
    (play_message_sender, seek_message_sender, audio_sender)
}

impl AudioTypePlayer<AudioType> for CpalAudioPlayer {
    fn set_audio(&self, audio: AudioType) {
        if let Err(err) = self.inner.read().unwrap().audio_sender.send(audio) {
            eprintln!("[{}:{}] {err}", file!(), line!());
        }
    }

    fn seek(&self, time: TimelineTime) {
        if let Err(err) = self.inner.read().unwrap().seek_message_sender.send(time) {
            eprintln!("[{}:{}] {err}", file!(), line!());
        }
    }

    fn play(&self) {
        if let Err(err) = self.inner.read().unwrap().play_message_sender.send(StreamMessage::Play) {
            eprintln!("[{}:{}] {err}", file!(), line!());
        }
    }

    fn pause(&self) {
        if let Err(err) = self.inner.read().unwrap().play_message_sender.send(StreamMessage::Pause) {
            eprintln!("[{}:{}] {err}", file!(), line!());
        }
    }
}
