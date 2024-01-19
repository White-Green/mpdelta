use dashmap::DashMap;
use ffmpeg_next::codec::{Capabilities, Id};
use ffmpeg_next::encoder::{audio, video};
use ffmpeg_next::format;
use ffmpeg_next::format::sample::Type;
use ffmpeg_next::format::{Pixel, Sample};
use ffmpeg_next::software::{resampling, scaling};
use ffmpeg_next::{codec, encoder, frame, ChannelLayout, Codec, Dictionary, Packet, Rational};
use indexmap::IndexMap;
use mpdelta_core::time::TimelineTime;
use mpdelta_core_audio::multi_channel_audio::{MultiChannelAudio, MultiChannelAudioMutOp, MultiChannelAudioOp};
use mpdelta_core_audio::{AudioProvider, AudioType};
use mpdelta_core_vulkano::ImageType;
use mpdelta_dsp::Resample;
use mpdelta_ffmpeg::codec::{codec_supported_pixel_format, codec_supported_sample_format, codec_supported_sample_rate, new_codec_context_from_codec};
use mpdelta_ffmpeg::io::FfmpegIoError;
use mpdelta_multimedia::options_value::{OptionValue, ValueTypeString, ValueWithDefault};
use mpdelta_multimedia::{AudioCodec, CodecImplement, CodecOptions, FileFormat, MediaCodecImplementHandle, VideoCodec};
use mpdelta_renderer::{VideoEncoder, VideoEncoderBuilder, VideoEncoderBuilderDyn};
use once_cell::sync::Lazy;
use std::borrow::Cow;
use std::fmt::Debug;
use std::fs::{File, OpenOptions};
use std::io::{Seek, Write};
use std::ops::ControlFlow;
use std::path::Path;
use std::ptr;
use std::sync::mpsc::Receiver;
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use thiserror::Error;
use vulkano::buffer::{BufferCreateInfo, BufferUsage};
use vulkano::command_buffer::allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo};
use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBufferUsage, CopyImageToBufferInfo, PrimaryCommandBufferAbstract};
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryAllocator, MemoryTypeFilter};
use vulkano::sync::GpuFuture;
use vulkano_util::context::VulkanoContext;

pub struct FfmpegEncoderBuilder {
    vulkano_context: Arc<VulkanoContext>,
}

impl FfmpegEncoderBuilder {
    pub fn new(vulkano_context: Arc<VulkanoContext>) -> FfmpegEncoderBuilder {
        FfmpegEncoderBuilder { vulkano_context }
    }

    pub fn available_video_codec<Encoder: From<FfmpegEncodeSettings<File>>>(self: &Arc<Self>) -> impl IntoIterator<Item = CodecImplement<VideoCodec, Encoder>> {
        [CodecImplement::new(
            VideoCodec::H264,
            IndexMap::from([(
                Cow::Borrowed("profile"),
                OptionValue::String {
                    value: ValueWithDefault::Value(String::from("high")),
                    ty: ValueTypeString::Candidates(Arc::new([Cow::Borrowed("baseline"), Cow::Borrowed("main"), Cow::Borrowed("high"), Cow::Borrowed("high10"), Cow::Borrowed("high422"), Cow::Borrowed("high444")])),
                },
            )]),
            Arc::clone(self) as Arc<dyn MediaCodecImplementHandle<Encoder>>,
        )]
    }

    pub fn available_audio_codec<Encoder: From<FfmpegEncodeSettings<File>>>(self: &Arc<Self>) -> impl IntoIterator<Item = CodecImplement<AudioCodec, Encoder>> {
        [CodecImplement::new(AudioCodec::Aac, IndexMap::new(), Arc::clone(self) as Arc<dyn MediaCodecImplementHandle<Encoder>>)]
    }
}

fn find_video_encoder(video: VideoCodec) -> Option<Codec> {
    match video {
        VideoCodec::H264 => encoder::find(Id::H264),
        VideoCodec::H265 => encoder::find(Id::H265),
        VideoCodec::Av1 => encoder::find(Id::AV1),
        VideoCodec::Png => encoder::find(Id::PNG),
    }
}

fn find_audio_encoder(audio: AudioCodec) -> Option<Codec> {
    match audio {
        AudioCodec::Mp3 => encoder::find(Id::MP3),
        AudioCodec::Aac => encoder::find(Id::AAC),
        AudioCodec::Flac => encoder::find(Id::FLAC),
        AudioCodec::Opus => encoder::find(Id::OPUS),
    }
}

fn as_dictionary(value: &IndexMap<Cow<'static, str>, OptionValue>) -> Dictionary<'static> {
    let mut dictionary = Dictionary::new();
    for (key, value) in value {
        match value {
            OptionValue::Bool { value: ValueWithDefault::Default } | OptionValue::Int { value: ValueWithDefault::Default, .. } | OptionValue::Float { value: ValueWithDefault::Default, .. } | OptionValue::String { value: ValueWithDefault::Default, .. } => {}
            OptionValue::Bool { value: ValueWithDefault::Value(value) } => dictionary.set(key, &value.to_string()),
            OptionValue::Int { value: ValueWithDefault::Value(value), .. } => dictionary.set(key, &value.to_string()),
            OptionValue::Float { value: ValueWithDefault::Value(value), .. } => dictionary.set(key, &value.to_string()),
            OptionValue::String { value: ValueWithDefault::Value(value), .. } => dictionary.set(key, value),
        }
    }
    dictionary
}

impl<Encoder: From<FfmpegEncodeSettings<File>>> MediaCodecImplementHandle<Encoder> for FfmpegEncoderBuilder {
    fn eq(&self, rhs: &dyn MediaCodecImplementHandle<Encoder>) -> bool {
        // TODO:ptr::addr_eqがstabilizeしたら書き替える
        ptr::eq(self as &dyn MediaCodecImplementHandle<Encoder> as *const dyn MediaCodecImplementHandle<Encoder> as *const (), rhs as *const dyn MediaCodecImplementHandle<Encoder> as *const ())
    }

    fn supports(&self, file_format: FileFormat, video: Option<VideoCodec>, audio: Option<AudioCodec>) -> bool {
        if video.is_none() && audio.is_none() {
            return false;
        }

        static CACHE_VIDEO: Lazy<DashMap<(FileFormat, VideoCodec), bool>> = Lazy::new(Default::default);
        static CACHE_AUDIO: Lazy<DashMap<(FileFormat, AudioCodec), bool>> = Lazy::new(Default::default);
        if let Some(video) = video {
            if let Some(cached) = CACHE_VIDEO.get(&(file_format, video)) {
                return *cached;
            }
            let encoder = find_video_encoder(video);
            let result = encoder.is_some_and(|encoder| mpdelta_ffmpeg::supports(file_format, encoder.id()));
            CACHE_VIDEO.entry((file_format, video)).or_insert(result);
            if !result {
                return false;
            }
        }
        if let Some(audio) = audio {
            if let Some(cached) = CACHE_AUDIO.get(&(file_format, audio)) {
                return *cached;
            }
            let encoder = find_audio_encoder(audio);
            let result = encoder.is_some_and(|encoder| mpdelta_ffmpeg::supports(file_format, encoder.id()));
            CACHE_AUDIO.entry((file_format, audio)).or_insert(result);
            if !result {
                return false;
            }
        }
        true
    }

    fn create_encoder(&self, file_format: FileFormat, video: Option<(VideoCodec, CodecOptions<VideoCodec>)>, audio: Option<(AudioCodec, CodecOptions<AudioCodec>)>, output: &Path) -> Encoder {
        assert!(MediaCodecImplementHandle::<Encoder>::supports(self, file_format, video.as_ref().map(|&(codec, _)| codec), audio.as_ref().map(|&(codec, _)| codec)));
        let output = OpenOptions::new().write(true).create(true).open(output).unwrap();
        output.set_len(0).unwrap();
        Encoder::from(FfmpegEncodeSettings {
            vulkano_context: Arc::clone(&self.vulkano_context),
            file_format,
            video,
            audio,
            output: Some(output),
        })
    }
}

pub struct FfmpegEncodeSettings<Output> {
    vulkano_context: Arc<VulkanoContext>,
    file_format: FileFormat,
    video: Option<(VideoCodec, CodecOptions<VideoCodec>)>,
    audio: Option<(AudioCodec, CodecOptions<AudioCodec>)>,
    output: Option<Output>,
}

#[derive(Debug, Error)]
pub enum FfmpegError {
    #[error("{0}")]
    Ffmpeg(#[from] ffmpeg_next::Error),
    #[error("{0}")]
    FfmpegIo(#[from] FfmpegIoError),
}

impl<Output> VideoEncoderBuilder<ImageType, AudioType> for FfmpegEncodeSettings<Output>
where
    Output: Write + Seek + Send + Sync + 'static,
{
    type Err = FfmpegError;
    type Encoder = FfmpegEncoder;

    fn build(&mut self) -> Result<Self::Encoder, Self::Err> {
        let FfmpegEncodeSettings { vulkano_context, file_format, video, audio, output } = self;
        let mut output = mpdelta_ffmpeg::io::Output::builder().file_type(file_format.extension()).build(output.take().unwrap())?;
        let global_header = output.format().flags().contains(format::Flags::GLOBAL_HEADER);
        let mut video_stream = None;
        if let Some((codec, options)) = video.take() {
            let codec = find_video_encoder(codec).unwrap();
            let format = codec_supported_pixel_format(&codec).and_then(|mut iter| iter.next()).unwrap_or(Pixel::RGBA);
            let mut ost = output.add_stream(codec)?;
            let mut encoder = new_codec_context_from_codec(codec).encoder().video().unwrap();
            encoder.set_parameters(ost.parameters()).unwrap();
            encoder.set_bit_rate(options.bit_rate());
            encoder.set_max_bit_rate(options.max_bit_rate());
            encoder.set_height(options.height());
            encoder.set_width(options.width());
            encoder.set_format(format);
            let frame_rate: Rational = options.frame_rate().into();
            encoder.set_frame_rate(Some(frame_rate));
            encoder.set_time_base(frame_rate.invert());
            encoder.set_gop((options.frame_rate() * 10.).floor() as u32);
            if global_header {
                encoder.set_flags(codec::Flags::GLOBAL_HEADER);
            }
            let encoder = encoder.open_as_with(codec, as_dictionary(options.options())).expect("error opening encoder with supplied settings");
            ost.set_parameters(&encoder);
            video_stream = Some((ost.index(), encoder, options));
        }
        let mut audio_stream = None;
        if let Some((codec, options)) = audio.take() {
            let codec = find_audio_encoder(codec).unwrap();
            let sample_format = codec_supported_sample_format(&codec).and_then(|iter| iter.max_by_key(|format| (format.bytes() << 1) | format.is_planar() as usize)).unwrap_or(Sample::F32(Type::Planar));
            let sample_rate = codec_supported_sample_rate(&codec).and_then(|iter| iter.min_by_key(|&rate| options.sample_rate().abs_diff(rate as u32))).unwrap_or(options.sample_rate() as i32);
            let mut ost = output.add_stream(codec)?;
            let mut encoder = new_codec_context_from_codec(codec).encoder().audio()?;
            encoder.set_parameters(ost.parameters()).unwrap();
            encoder.set_format(sample_format);
            encoder.set_rate(sample_rate);
            encoder.set_channel_layout(ChannelLayout::STEREO);
            encoder.set_channels(2);
            encoder.set_bit_rate(options.bit_rate());
            encoder.set_max_bit_rate(options.max_bit_rate());
            if global_header {
                encoder.set_flags(codec::Flags::GLOBAL_HEADER);
            }
            let encoder = encoder.open_as_with(codec, as_dictionary(options.options())).expect("error opening encoder with supplied settings");
            ost.set_parameters(&encoder);
            audio_stream = Some((ost.index(), encoder, options, codec.capabilities().contains(Capabilities::VARIABLE_FRAME_SIZE)));
        }
        let requires_image = video_stream.is_some();
        let requires_audio = audio_stream.is_some();
        let (image_sender, image_receiver) = mpsc::channel();
        let (audio_sender, audio_receiver) = mpsc::channel();
        let handle = std::thread::spawn(encode_thread(Arc::clone(vulkano_context), output, video_stream, audio_stream, image_receiver, audio_receiver));
        Ok(FfmpegEncoder {
            requires_image,
            requires_audio,
            image_sender,
            audio_sender,
            handle: Some(handle),
        })
    }
}

impl<Output> From<FfmpegEncodeSettings<Output>> for Box<dyn VideoEncoderBuilderDyn<ImageType, AudioType>>
where
    Output: Write + Seek + Send + Sync + 'static,
{
    fn from(value: FfmpegEncodeSettings<Output>) -> Self {
        Box::new(value)
    }
}

fn encode_thread<T: Write + Seek + Send + Sync + 'static>(
    vulkano_context: Arc<VulkanoContext>,
    mut output: mpdelta_ffmpeg::io::Output<T>,
    video_stream: Option<(usize, video::Encoder, CodecOptions<VideoCodec>)>,
    audio_stream: Option<(usize, audio::Encoder, CodecOptions<AudioCodec>, bool)>,
    image_receiver: Receiver<EncoderMessage<ImageType>>,
    audio_receiver: Receiver<EncoderMessage<AudioType>>,
) -> impl FnOnce() + Send + 'static {
    move || {
        output.write_header().unwrap();
        let mut video_stream = video_stream.map(|(id, mut encoder, options)| {
            let mut rgba_frame = frame::Video::new(Pixel::RGBA, options.width(), options.height());
            let mut encoder_native_format_frame = frame::Video::new(encoder.format(), options.width(), options.height());
            let mut format_conversion_context = scaling::Context::get(Pixel::RGBA, options.width(), options.height(), encoder.format(), options.width(), options.height(), scaling::Flags::FAST_BILINEAR).unwrap();
            let buffer = vulkano::buffer::Buffer::new_slice::<[u8; 4]>(
                Arc::clone(vulkano_context.memory_allocator()) as Arc<dyn MemoryAllocator>,
                BufferCreateInfo {
                    usage: BufferUsage::TRANSFER_DST,
                    ..BufferCreateInfo::default()
                },
                AllocationCreateInfo {
                    memory_type_filter: MemoryTypeFilter::HOST_RANDOM_ACCESS,
                    ..AllocationCreateInfo::default()
                },
                options.width() as u64 * options.height() as u64,
            )
            .unwrap();
            let command_buffer_allocator = StandardCommandBufferAllocator::new(Arc::clone(vulkano_context.device()), StandardCommandBufferAllocatorCreateInfo::default());
            let mut timestamp = 0;
            let mut image_receiver = Some(image_receiver);
            let stream_time_base = output.stream(id).unwrap().time_base();
            move |video_packet: &mut Packet| -> ControlFlow<()> {
                loop {
                    if encoder.receive_packet(video_packet).is_ok() {
                        video_packet.rescale_ts(Rational::from(options.frame_rate()).invert(), stream_time_base);
                        video_packet.set_stream(id);
                        return ControlFlow::Continue(());
                    }
                    let Some(image_receiver_ref) = image_receiver.as_ref() else {
                        return ControlFlow::Break(());
                    };
                    match image_receiver_ref.recv().unwrap() {
                        EncoderMessage::Push(ImageType(image)) => {
                            let [width, height, _] = image.extent();
                            let mut builder = AutoCommandBufferBuilder::primary(&command_buffer_allocator, 0, CommandBufferUsage::OneTimeSubmit).unwrap();
                            builder.copy_image_to_buffer(CopyImageToBufferInfo::image_buffer(image, buffer.clone())).unwrap();
                            builder.build().unwrap().execute(Arc::clone(vulkano_context.graphics_queue())).unwrap().then_signal_fence_and_flush().unwrap().wait(None).unwrap();
                            rgba_frame.set_width(width);
                            rgba_frame.set_height(height);
                            rgba_frame.plane_mut(0).copy_from_slice(&buffer.read().unwrap());
                            format_conversion_context.cached(Pixel::RGBA, width, height, encoder.format(), options.width(), options.height(), scaling::Flags::FAST_BILINEAR);
                            format_conversion_context.run(&rgba_frame, &mut encoder_native_format_frame).unwrap();
                            encoder_native_format_frame.set_pts(Some(timestamp));
                            timestamp += 1;
                            encoder.send_frame(&encoder_native_format_frame).unwrap();
                        }
                        EncoderMessage::Finish => {
                            encoder.send_eof().unwrap();
                            image_receiver = None;
                        }
                    }
                }
            }
        });
        let mut audio_stream = audio_stream.map(|(id, mut encoder, options, variable_frame_size)| {
            let frame_size = if variable_frame_size { encoder.rate() / 20 } else { encoder.frame_size() };
            let mut f32_frame = frame::Audio::new(Sample::F32(Type::Planar), frame_size as usize, ChannelLayout::STEREO);
            let mut encoder_native_format_frame = frame::Audio::new(encoder.format(), frame_size as usize, ChannelLayout::STEREO);
            f32_frame.set_rate(encoder.rate());
            encoder_native_format_frame.set_rate(encoder.rate());
            let mut format_conversion_context = resampling::Context::get(Sample::F32(Type::Planar), ChannelLayout::STEREO, encoder.rate(), encoder.format(), ChannelLayout::STEREO, encoder.rate()).unwrap();
            let mut audio_buffer = MultiChannelAudio::new(2);
            audio_buffer.resize(frame_size as usize, 0.);
            let mut audio = None;
            let mut src_timestamp = 0;
            let mut encoder_timestamp = 0;
            let stream_time_base = output.stream(id).unwrap().time_base();
            let mut next_break = false;
            let mut nb = false;
            move |audio_packet: &mut Packet| -> ControlFlow<()> {
                loop {
                    if encoder.receive_packet(audio_packet).is_ok() {
                        audio_packet.rescale_ts(Rational::new(1, options.sample_rate() as i32), stream_time_base);
                        audio_packet.set_stream(id);
                        return ControlFlow::Continue(());
                    }
                    if next_break {
                        return ControlFlow::Break(());
                    }
                    if audio.is_none() {
                        let EncoderMessage::Push(new_audio) = audio_receiver.recv().unwrap() else {
                            panic!();
                        };
                        let audio_sample_rate = new_audio.sample_rate();
                        let resample = Resample::builder(audio_sample_rate, encoder.rate()).build().unwrap();
                        audio = Some((new_audio, [resample.clone(), resample]));
                    }
                    let (audio, resample) = audio.as_mut().unwrap();
                    audio_buffer.resize(frame_size as usize, 0.);
                    let mut offset = 0;
                    loop {
                        f32_frame.plane_mut::<f32>(0)[offset..].iter_mut().zip(resample[0].by_ref()).for_each(|(frame, sample)| *frame = sample);
                        offset += f32_frame.plane_mut::<f32>(1)[offset..].iter_mut().zip(resample[1].by_ref()).map(|(frame, sample)| *frame = sample).count();
                        if offset >= f32_frame.samples() {
                            break;
                        }
                        if nb {
                            next_break = true;
                            break;
                        }
                        let len = audio.compute_audio(TimelineTime::new((src_timestamp * frame_size) as f64 / audio.sample_rate() as f64).unwrap(), audio_buffer.slice_mut(..).unwrap());
                        src_timestamp += 1;
                        if len == 0 {
                            resample.iter_mut().for_each(Resample::fill_tail_by_zero);
                            nb = true;
                        } else {
                            let audio = audio_buffer.slice(..len).unwrap();
                            resample[0].extend(audio.iter().map(|audio| audio[0]));
                            resample[1].extend(audio.iter().map(|audio| audio[1]));
                        }
                    }
                    format_conversion_context.run(&f32_frame, &mut encoder_native_format_frame).unwrap();
                    encoder_native_format_frame.set_pts(Some((encoder_timestamp * frame_size) as i64));
                    encoder.send_frame(&encoder_native_format_frame).unwrap();
                    encoder_timestamp += 1;
                }
            }
        });
        let mut video_packet = video_stream.as_ref().map(|_| Packet::empty());
        let mut audio_packet = audio_stream.as_ref().map(|_| Packet::empty());
        if let Some(video_packet_ref) = &mut video_packet {
            if let ControlFlow::Break(()) = video_stream.as_mut().unwrap()(video_packet_ref) {
                video_packet = None;
            }
        }
        if let Some(audio_packet_ref) = &mut audio_packet {
            if let ControlFlow::Break(()) = audio_stream.as_mut().unwrap()(audio_packet_ref) {
                audio_packet = None;
            }
        }
        loop {
            match (&mut video_packet, &mut audio_packet) {
                (Some(video_packet_ref), Some(audio_packet_ref)) => {
                    if video_packet_ref.dts().unwrap_or(i64::MAX) <= audio_packet_ref.dts().unwrap_or(i64::MAX) {
                        video_packet_ref.write_interleaved(&mut output).unwrap();
                        if let ControlFlow::Break(()) = video_stream.as_mut().unwrap()(video_packet_ref) {
                            video_packet = None;
                        }
                    } else {
                        audio_packet_ref.write_interleaved(&mut output).unwrap();
                        if let ControlFlow::Break(()) = audio_stream.as_mut().unwrap()(audio_packet_ref) {
                            audio_packet = None;
                        }
                    }
                }
                (Some(video_packet_ref), None) => {
                    video_packet_ref.write_interleaved(&mut output).unwrap();
                    if let ControlFlow::Break(()) = video_stream.as_mut().unwrap()(video_packet_ref) {
                        video_packet = None;
                    }
                }
                (None, Some(audio_packet_ref)) => {
                    audio_packet_ref.write_interleaved(&mut output).unwrap();
                    if let ControlFlow::Break(()) = audio_stream.as_mut().unwrap()(audio_packet_ref) {
                        audio_packet = None;
                    }
                }
                (None, None) => break,
            }
        }
        output.write_trailer().unwrap();
    }
}

enum EncoderMessage<T> {
    Push(T),
    Finish,
}

pub struct FfmpegEncoder {
    requires_image: bool,
    requires_audio: bool,
    image_sender: mpsc::Sender<EncoderMessage<ImageType>>,
    audio_sender: mpsc::Sender<EncoderMessage<AudioType>>,
    handle: Option<JoinHandle<()>>,
}

impl VideoEncoder<ImageType, AudioType> for FfmpegEncoder {
    fn requires_image(&self) -> bool {
        self.requires_image
    }

    fn push_frame(&mut self, frame: ImageType) {
        self.image_sender.send(EncoderMessage::Push(frame)).unwrap();
    }

    fn requires_audio(&self) -> bool {
        self.requires_audio
    }

    fn set_audio(&mut self, audio: AudioType) {
        self.audio_sender.send(EncoderMessage::Push(audio)).unwrap();
    }

    fn finish(&mut self) {
        self.image_sender.send(EncoderMessage::Finish).unwrap();
        self.audio_sender.send(EncoderMessage::Finish).unwrap();
        self.handle.take().unwrap().join().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mpdelta_core_audio::multi_channel_audio::MultiChannelAudioSliceMut;
    use std::fmt::Arguments;
    use std::io;
    use std::io::{Cursor, IoSlice, SeekFrom};
    use std::sync::Mutex;
    use vulkano::command_buffer::ClearColorImageInfo;
    use vulkano::format::{ClearColorValue, Format};
    use vulkano::image::{Image, ImageCreateInfo, ImageUsage};
    use vulkano::instance::InstanceCreateInfo;
    use vulkano::Version;
    use vulkano_util::context::VulkanoConfig;

    const TEST_VIDEO_LENGTH: f64 = 5.;
    const TEST_VIDEO_FRAME_RATE: f64 = 60.;

    struct WriteWrapper(Arc<Mutex<Cursor<Vec<u8>>>>);

    impl Write for WriteWrapper {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            <Cursor<Vec<u8>> as Write>::write(&mut self.0.lock().unwrap(), buf)
        }

        fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
            <Cursor<Vec<u8>> as Write>::write_vectored(&mut self.0.lock().unwrap(), bufs)
        }

        fn flush(&mut self) -> io::Result<()> {
            <Cursor<Vec<u8>> as Write>::flush(&mut self.0.lock().unwrap())
        }

        fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
            <Cursor<Vec<u8>> as Write>::write_all(&mut self.0.lock().unwrap(), buf)
        }

        fn write_fmt(&mut self, fmt: Arguments<'_>) -> io::Result<()> {
            <Cursor<Vec<u8>> as Write>::write_fmt(&mut self.0.lock().unwrap(), fmt)
        }
    }

    impl Seek for WriteWrapper {
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            <Cursor<Vec<u8>> as Seek>::seek(&mut self.0.lock().unwrap(), pos)
        }

        fn rewind(&mut self) -> io::Result<()> {
            <Cursor<Vec<u8>> as Seek>::rewind(&mut self.0.lock().unwrap())
        }

        fn stream_position(&mut self) -> io::Result<u64> {
            <Cursor<Vec<u8>> as Seek>::stream_position(&mut self.0.lock().unwrap())
        }
    }

    #[derive(Debug, Clone)]
    struct TestAudio;

    impl AudioProvider for TestAudio {
        fn sample_rate(&self) -> u32 {
            48_000
        }

        fn channels(&self) -> usize {
            2
        }

        fn compute_audio(&mut self, begin: TimelineTime, mut dst: MultiChannelAudioSliceMut<f32>) -> usize {
            let all_audio_len = (TEST_VIDEO_LENGTH * self.sample_rate() as f64).round() as usize;
            let len = all_audio_len.saturating_sub(begin.value() as usize * self.sample_rate() as usize).min(dst.len());
            let mut dst = dst.slice_mut(..len).unwrap();
            dst.fill(0.);
            for (t, data) in dst.iter_mut().enumerate() {
                let t = t as f64 / self.sample_rate() as f64 + begin.value();
                let f = (t * 440. * 2. * std::f64::consts::PI).sin() * 0.3;
                if let Some(data) = data.get_mut(t as usize % 2) {
                    *data = f as f32;
                }
            }
            len
        }
    }

    #[test]
    fn test_encode_mp4_h264_aac() {
        ffmpeg_next::init().unwrap();
        let vulkano_context = Arc::new(VulkanoContext::new(VulkanoConfig {
            instance_create_info: InstanceCreateInfo {
                max_api_version: Some(Version::V1_2),
                ..InstanceCreateInfo::default()
            },
            ..VulkanoConfig::default()
        }));
        let output = Arc::new(Mutex::new(Cursor::new(Vec::new())));
        let mut video_options = CodecOptions::new(IndexMap::new());
        video_options.set_height(1080);
        video_options.set_width(1920);
        video_options.set_frame_rate(TEST_VIDEO_FRAME_RATE);
        let mut audio_options = CodecOptions::new(Default::default());
        audio_options.set_sample_rate(48_000);
        audio_options.set_bit_rate(192_000);
        audio_options.set_max_bit_rate(192_000);
        let mut encoder = FfmpegEncodeSettings {
            vulkano_context: Arc::clone(&vulkano_context),
            file_format: FileFormat::Mp4,
            video: Some((VideoCodec::H264, video_options)),
            audio: Some((AudioCodec::Aac, audio_options)),
            output: Some(WriteWrapper(Arc::clone(&output))),
        };
        let mut encoder = encoder.build().unwrap();
        assert!(encoder.requires_audio());
        assert!(encoder.requires_image());
        encoder.set_audio(AudioType::new(TestAudio));
        let command_buffer_allocator = StandardCommandBufferAllocator::new(Arc::clone(vulkano_context.device()), StandardCommandBufferAllocatorCreateInfo::default());
        let images = (0..8)
            .map(|i| {
                let image = Image::new(
                    Arc::clone(vulkano_context.memory_allocator()) as Arc<dyn MemoryAllocator>,
                    ImageCreateInfo {
                        format: Format::R8G8B8A8_UNORM,
                        extent: [1920, 1080, 1],
                        usage: ImageUsage::SAMPLED | ImageUsage::TRANSFER_SRC | ImageUsage::TRANSFER_DST,
                        ..ImageCreateInfo::default()
                    },
                    AllocationCreateInfo::default(),
                )
                .unwrap();
                let mut builder = AutoCommandBufferBuilder::primary(&command_buffer_allocator, 0, CommandBufferUsage::OneTimeSubmit).unwrap();
                builder
                    .clear_color_image(ClearColorImageInfo {
                        clear_value: ClearColorValue::Float([((i >> 2) & 1) as f32, ((i >> 1) & 1) as f32, (i & 1) as f32, 1.0]),
                        ..ClearColorImageInfo::image(Arc::clone(&image))
                    })
                    .unwrap();
                builder.build().unwrap().execute(Arc::clone(vulkano_context.graphics_queue())).unwrap().then_signal_fence_and_flush().unwrap().wait(None).unwrap();
                image
            })
            .collect::<Vec<_>>();

        for i in 0..(TEST_VIDEO_FRAME_RATE * TEST_VIDEO_LENGTH).round() as usize {
            let f = ((i * 2) as f64 / TEST_VIDEO_FRAME_RATE).floor() as usize & 0b111;
            let f = f ^ (f >> 1);
            encoder.push_frame(ImageType(Arc::clone(&images[f])));
        }
        encoder.finish();
        std::fs::write("test_encode_mp4_h264_aac.mp4", output.lock().unwrap().get_ref()).unwrap();
    }

    #[test]
    fn test_encode_flac() {
        ffmpeg_next::init().unwrap();
        let vulkano_context = Arc::new(VulkanoContext::new(VulkanoConfig {
            instance_create_info: InstanceCreateInfo {
                max_api_version: Some(Version::V1_2),
                ..InstanceCreateInfo::default()
            },
            ..VulkanoConfig::default()
        }));
        let output = Arc::new(Mutex::new(Cursor::new(Vec::new())));
        let mut audio_options = CodecOptions::new(Default::default());
        audio_options.set_sample_rate(48_000);
        audio_options.set_bit_rate(192_000);
        audio_options.set_max_bit_rate(192_000);
        let mut encoder = FfmpegEncodeSettings {
            vulkano_context: Arc::clone(&vulkano_context),
            file_format: FileFormat::Flac,
            video: None,
            audio: Some((AudioCodec::Flac, audio_options)),
            output: Some(WriteWrapper(Arc::clone(&output))),
        };
        let mut encoder = encoder.build().unwrap();
        assert!(encoder.requires_audio());
        assert!(!encoder.requires_image());
        encoder.set_audio(AudioType::new(TestAudio));
        encoder.finish();
        std::fs::write("test_encode_flac.flac", output.lock().unwrap().get_ref()).unwrap();
    }
}
