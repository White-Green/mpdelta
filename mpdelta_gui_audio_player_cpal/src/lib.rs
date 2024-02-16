use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{BufferSize, Device, FromSample, SampleFormat, SizedSample, StreamConfig, SupportedBufferSize, SupportedStreamConfig, SupportedStreamConfigsError};
use crossbeam_queue::ArrayQueue;
use mpdelta_core::time::TimelineTime;
use mpdelta_core_audio::multi_channel_audio::{MultiChannelAudio, MultiChannelAudioMutOp, MultiChannelAudioOp};
use mpdelta_core_audio::AudioProvider;
use mpdelta_core_audio::AudioType;
use mpdelta_dsp::{Resample, ResampleBuilder};
use mpdelta_gui::AudioTypePlayer;
use std::cmp::Ordering;
use std::fmt::Debug;
use std::sync::{Arc, PoisonError, RwLock, Weak};
use std::thread;
use thiserror::Error;
use tokio::runtime::Handle;
use tokio::sync::mpsc::UnboundedSender;

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
    S: SizedSample + FromSample<f32> + Debug + Send + 'static,
    F: Fn() -> Result<CpalAudioPlayerInner, CpalAudioPlayerCreationError> + Clone + Send + 'static,
{
    assert_eq!(S::FORMAT, config.sample_format());
    const QUEUE_CAPACITY: usize = 16;
    let provide_queue = Arc::new(ArrayQueue::<Box<[S]>>::new(QUEUE_CAPACITY));
    let (return_queue_sender, mut return_queue_receiver) = tokio::sync::mpsc::channel(QUEUE_CAPACITY);
    let (play_message_sender, mut play_message_receiver) = tokio::sync::mpsc::unbounded_channel::<StreamMessage>();
    let (seek_message_sender, mut seek_message_receiver) = tokio::sync::mpsc::unbounded_channel::<TimelineTime>();
    let (audio_sender, mut audio_receiver) = tokio::sync::mpsc::unbounded_channel::<AudioType>();
    let stream_config = config.config();
    let (stream_config, mut initialized) = match *config.buffer_size() {
        SupportedBufferSize::Range { min, max } => {
            let buffer_size = ((stream_config.sample_rate.0 as f64 * 0.05).round() as u32).next_power_of_two().clamp(min, max);
            let stream_config = StreamConfig {
                buffer_size: BufferSize::Fixed(buffer_size),
                ..stream_config
            };
            for _ in 0..QUEUE_CAPACITY {
                return_queue_sender.blocking_send(vec![S::EQUILIBRIUM; buffer_size as usize].into_boxed_slice()).unwrap();
            }
            (stream_config, true)
        }
        SupportedBufferSize::Unknown => (stream_config, false),
    };
    let stream_channels = stream_config.channels;
    let stream_sample_rate = stream_config.sample_rate.0;
    {
        let return_queue_sender = return_queue_sender.clone();
        let provide_queue = Arc::clone(&provide_queue);
        let this2 = this.clone();
        let this_creator2 = this_creator.clone();
        thread::spawn(move || {
            let mut current_buffer = None::<Box<[S]>>;
            let mut current_index = 0;
            let stream = device
                .build_output_stream(
                    &stream_config,
                    move |mut data: &mut [S], _info| {
                        if !initialized {
                            for _ in 0..QUEUE_CAPACITY {
                                return_queue_sender.blocking_send(vec![S::EQUILIBRIUM; data.len()].into_boxed_slice()).unwrap();
                            }
                            initialized = true;
                        }
                        loop {
                            if let Some(buffer) = &current_buffer {
                                let buffer = &buffer[current_index..];
                                match data.len().cmp(&buffer.len()) {
                                    Ordering::Less => {
                                        let data_len = data.len();
                                        data.copy_from_slice(&buffer[..data_len]);
                                        current_index += data_len;
                                        break;
                                    }
                                    Ordering::Equal => {
                                        data.copy_from_slice(buffer);
                                        let _ = return_queue_sender.blocking_send(current_buffer.take().unwrap());
                                        break;
                                    }
                                    Ordering::Greater => {
                                        let buffer_len = buffer.len();
                                        data[..buffer_len].copy_from_slice(buffer);
                                        let _ = return_queue_sender.blocking_send(current_buffer.take().unwrap());
                                        data = &mut data[buffer_len..];
                                    }
                                }
                            } else if let Some(buffer) = provide_queue.pop() {
                                current_buffer = Some(buffer);
                                current_index = 0;
                            } else {
                                println!("no data");
                                data.fill(S::EQUILIBRIUM);
                                break;
                            }
                        }
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
        loop {
            tokio::select! {
                data = return_queue_receiver.recv() => {
                    let Some(mut data) = data else { break; };
                    let sample_len = (data.len() * audio.sample_rate() as usize / stream_channels as usize / stream_sample_rate as usize).max(10);
                    assert_eq!(data.len() % resample.len(), 0);
                    let mut data_ref = &mut data[..];
                    loop {
                        let proceed_count = data_ref.chunks_exact_mut(resample.len()).filter_map(|data| data.iter_mut().zip(resample.iter_mut()).try_for_each(|(data, resample)| {
                            *data = resample.next().map(|a| S::from_sample(a * 0.5))?;
                            Some(())
                        })).count();
                        if data_ref.len() == proceed_count * resample.len() {
                            break;
                        }
                        data_ref = &mut data_ref[proceed_count * resample.len()..];
                        audio_buffer.resize(sample_len, 0.);
                        let result_len = audio.compute_audio(play_time, audio_buffer.slice_mut(..).unwrap());
                        audio_buffer.iter().take(result_len).for_each(|audio_data| resample.iter_mut().zip(audio_data).for_each(|(resample, &a)| resample.extend([a])));
                        if result_len == sample_len {
                            play_time = play_time + TimelineTime::new(result_len as f64 / audio.sample_rate() as f64).unwrap();
                        } else {
                            play_time = TimelineTime::ZERO;
                        }
                    }
                    provide_queue.push(data).unwrap();
                }
                message = seek_message_receiver.recv() => {
                    let Some(at) = message else { break; };
                    resample.iter_mut().for_each(Resample::reset_buffer);
                    while let Some(data) = provide_queue.pop() {
                        return_queue_sender.try_send(data).unwrap();
                    }
                    play_time = at;
                }
                new_audio = audio_receiver.recv() => {
                    let Some(new_audio) = new_audio else { break; };
                    if audio.sample_rate() != new_audio.sample_rate() {
                        resample = vec![ResampleBuilder::new(new_audio.sample_rate(), stream_sample_rate).build().unwrap(); stream_channels as usize];
                    } else {
                        resample.iter_mut().for_each(Resample::reset_buffer);
                    }
                    while let Some(data) = provide_queue.pop() {
                        return_queue_sender.try_send(data).unwrap();
                    }
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
