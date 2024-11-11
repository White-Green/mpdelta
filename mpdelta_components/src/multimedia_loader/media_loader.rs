use arrayvec::ArrayVec;
use ffmpeg_next::format::{sample, Sample};
use ffmpeg_next::frame::audio;
use ffmpeg_next::media::Type;
use ffmpeg_next::{codec, decoder, format, frame, Rational};
use image::RgbaImage;
use mpdelta_core::common::mixed_fraction::MixedFraction;
use mpdelta_core::time::TimelineTime;
use mpdelta_core_audio::multi_channel_audio::{MultiChannelAudioMutOp, MultiChannelAudioOp, MultiChannelAudioSliceMut};
use mpdelta_core_audio::AudioProvider;
use mpdelta_ffmpeg::io::input::SeekFlag;
use num::Integer;
use smallvec::SmallVec;
use std::io::{Read, Seek};
use std::ops::{ControlFlow, Range};
use std::sync::{Arc, Mutex as StdMutex};

fn into_mixed_fraction(value: Rational) -> MixedFraction {
    let numerator = value.numerator();
    let denominator = value.denominator();
    let (integer, numerator) = numerator.div_rem(&denominator);
    let (integer, numerator) = if numerator < 0 { (integer + 1, numerator + denominator) } else { (integer, numerator) };
    MixedFraction::new(integer, u32::try_from(numerator).unwrap(), u32::try_from(denominator).unwrap())
}

struct ImageCache {
    pts_range: Range<i64>,
    images: Vec<(i64, Arc<RgbaImage>)>,
}

pub(super) struct VideoReader<T> {
    ictx: mpdelta_ffmpeg::io::Input<T>,
    parameters: codec::Parameters,
    stream_index: usize,
    time_base: MixedFraction,
    duration: i64,
    image_cache: ArrayVec<ImageCache, 2>,
    last_accessed: usize,
}

impl<T> VideoReader<T>
where
    T: Read + Seek,
{
    pub(super) fn new(file: T) -> Option<VideoReader<T>> {
        let ictx = mpdelta_ffmpeg::io::Input::new(file).unwrap();
        let input = ictx.streams().best(Type::Video)?;
        let stream_id = input.index();
        let duration = input.duration();
        let time_base = into_mixed_fraction(input.time_base());

        let parameters = input.parameters();
        let context_decoder = codec::context::Context::from_parameters(parameters.clone()).ok()?;
        let _ = context_decoder.decoder().video().ok()?;

        Some(VideoReader {
            ictx,
            parameters,
            stream_index: stream_id,
            time_base,
            duration,
            image_cache: ArrayVec::new(),
            last_accessed: 0,
        })
    }

    pub(super) fn duration(&self) -> Option<MixedFraction> {
        if self.duration < 0 {
            return None;
        }
        Some(MixedFraction::from_integer(i32::try_from(self.duration).unwrap()) * self.time_base)
    }

    pub(super) fn read_image_at(&mut self, time: TimelineTime) -> Arc<RgbaImage> {
        let time = time.value();
        let pts = time.div_floor(self.time_base).unwrap();
        if let Some((cache_index, cache)) = self.image_cache.iter_mut().enumerate().find_map(|(i, ImageCache { pts_range, images })| pts_range.contains(&pts).then_some((i, images))) {
            self.last_accessed = cache_index;
            let i = cache.binary_search_by_key(&pts, |&(pts, _)| pts).unwrap_or_else(|i| i.saturating_sub(1));
            return Arc::clone(&cache[i].1);
        }
        if self.image_cache.is_full() {
            self.image_cache.remove((self.last_accessed + 1) % self.image_cache.len());
        }

        self.ictx.seek_with_flag(Some(self.stream_index as i32), pts, ..pts, SeekFlag::FRAME).unwrap();

        let context_decoder = codec::context::Context::from_parameters(self.parameters.clone()).unwrap();
        let mut decoder = context_decoder.decoder().video().unwrap();
        let mut scaler = ffmpeg_next::software::scaling::Context::get(decoder.format(), decoder.width(), decoder.height(), ffmpeg_next::format::Pixel::RGBA, decoder.width(), decoder.height(), ffmpeg_next::software::scaling::Flags::FAST_BILINEAR).unwrap();

        let mut start_pts = None;
        let mut end_pts = None;
        let mut images = Vec::new();
        let mut decoded = ffmpeg_next::frame::Video::empty();
        let mut rgb_frame = ffmpeg_next::frame::Video::empty();
        'outer: {
            let mut process_frame = |decoder: &mut decoder::Video| {
                while decoder.receive_frame(&mut decoded).is_ok() {
                    if decoded.is_key() {
                        if start_pts.is_some() {
                            end_pts = Some(decoded.pts().unwrap());
                            return ControlFlow::Break(());
                        }
                        start_pts = Some(decoded.pts().unwrap());
                    }
                    scaler.run(&decoded, &mut rgb_frame).unwrap();
                    let image = if rgb_frame.stride(0) == rgb_frame.width() as usize * 4 {
                        RgbaImage::from_vec(rgb_frame.width(), rgb_frame.height(), rgb_frame.data(0)[..rgb_frame.width() as usize * rgb_frame.height() as usize * 4].to_vec()).unwrap()
                    } else {
                        let mut image = RgbaImage::new(rgb_frame.width(), rgb_frame.height());
                        for (dst, src) in image.chunks_mut(rgb_frame.width() as usize * 4).zip(rgb_frame.data(0).chunks(rgb_frame.stride(0))) {
                            dst.copy_from_slice(&src[..dst.len()]);
                        }
                        image
                    };
                    images.push((decoded.pts().unwrap(), Arc::new(image)));
                }
                ControlFlow::Continue(())
            };
            for (_, packet) in self.ictx.packets().filter(|(stream, _)| stream.index() == self.stream_index) {
                decoder.send_packet(&packet).unwrap();
                if let ControlFlow::Break(()) = process_frame(&mut decoder) {
                    break 'outer;
                }
            }
            decoder.send_eof().unwrap();
            process_frame(&mut decoder);
        }
        let start_pts = start_pts.unwrap();
        let range = match end_pts {
            Some(end_pts) => start_pts..end_pts,
            None if self.duration >= 0 => start_pts..self.duration,
            None => start_pts..i64::MAX,
        };
        self.image_cache.push(ImageCache { pts_range: range, images });
        self.last_accessed = self.image_cache.len() - 1;
        let ImageCache { images, .. } = self.image_cache.last().unwrap();
        let i = images.binary_search_by_key(&pts, |&(pts, _)| pts).unwrap_or_else(|i| i.saturating_sub(1));
        Arc::clone(&images[i].1)
    }
}

pub(super) struct AudioReader<T> {
    inner: Arc<StdMutex<AudioReaderInner<T>>>,
    sample_rate: u32,
    channels: u16,
    duration: Option<MixedFraction>,
}

struct AudioReaderInner<T> {
    ictx: mpdelta_ffmpeg::io::Input<T>,
    parameters: codec::Parameters,
    stream_index: usize,
    time_base: MixedFraction,
    sample_rate: u32,
    channels: u16,
}

impl<T> AudioReader<T>
where
    T: Read + Seek,
{
    pub(super) fn new(file: T) -> Option<AudioReader<T>> {
        let ictx = mpdelta_ffmpeg::io::Input::new(file).unwrap();
        let input = ictx.streams().best(Type::Audio)?;
        let stream_index = input.index();
        let duration = input.duration();
        let time_base = into_mixed_fraction(input.time_base());

        let parameters = input.parameters();
        let context_decoder = codec::context::Context::from_parameters(parameters.clone()).ok()?;
        let decoder = context_decoder.decoder().audio().ok()?;
        let sample_rate = decoder.rate();
        let channels = decoder.channels();
        let inner = AudioReaderInner {
            ictx,
            parameters,
            stream_index,
            time_base,
            sample_rate,
            channels,
        };
        Some(AudioReader {
            inner: Arc::new(StdMutex::new(inner)),
            sample_rate,
            channels,
            duration: (duration >= 0).then(|| MixedFraction::from_integer(i32::try_from(duration).unwrap()) * time_base),
        })
    }

    pub(super) fn duration(&self) -> Option<MixedFraction> {
        self.duration
    }
}

impl<T> Clone for AudioReader<T> {
    fn clone(&self) -> Self {
        let &AudioReader { ref inner, sample_rate, channels, duration } = self;
        AudioReader {
            inner: Arc::clone(inner),
            sample_rate,
            channels,
            duration,
        }
    }
}

impl<T> AudioProvider for AudioReader<T>
where
    T: Read + Seek,
{
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> usize {
        usize::from(self.channels)
    }

    fn compute_audio(&mut self, begin: TimelineTime, dst: MultiChannelAudioSliceMut<f32>) -> usize {
        self.inner.lock().unwrap_or_else(|error| error.into_inner()).compute_audio(begin, dst)
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
struct SampleI64(i64);

unsafe impl audio::Sample for SampleI64 {
    fn is_valid(format: Sample, _channels: u16) -> bool {
        matches!(format, Sample::I64(_))
    }
}

impl<T> AudioProvider for AudioReaderInner<T>
where
    T: Read + Seek,
{
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> usize {
        usize::from(self.channels)
    }

    fn compute_audio(&mut self, begin: TimelineTime, mut dst: MultiChannelAudioSliceMut<f32>) -> usize {
        let begin = begin.value();
        let pts = begin.div_floor(self.time_base).unwrap();
        // 音声のstream_indexに合わせてシークすると音声が壊れる謎
        self.ictx.seek_with_flag(None, pts, ..pts, SeekFlag::FRAME).unwrap();
        dst.fill(0.);

        let context_decoder = codec::context::Context::from_parameters(self.parameters.clone()).unwrap();
        let decoder = context_decoder.decoder().audio().unwrap();
        let format = decoder.format();
        let frame_provider = FfmpegFrameProvider::new(&mut self.ictx, decoder, self.stream_index);
        let avg_f32 = |v1, v2| (v1 + v2) / 2.;
        match format {
            Sample::None => 0,
            Sample::U8(sample::Type::Packed) => compute_audio_packed(frame_provider, dst, self.sample_rate, pts, self.time_base, avg_f32, |v: u8| (v as f32 - 127.) / 127.),
            Sample::U8(sample::Type::Planar) => compute_audio_planar(frame_provider, dst, self.sample_rate, pts, self.time_base, avg_f32, |v: u8| (v as f32 - 127.) / 127.),
            Sample::I16(sample::Type::Packed) => compute_audio_packed(frame_provider, dst, self.sample_rate, pts, self.time_base, avg_f32, |v: i16| v as f32 / i16::MAX as f32),
            Sample::I16(sample::Type::Planar) => compute_audio_planar(frame_provider, dst, self.sample_rate, pts, self.time_base, avg_f32, |v: i16| v as f32 / i16::MAX as f32),
            Sample::I32(sample::Type::Packed) => compute_audio_packed(frame_provider, dst, self.sample_rate, pts, self.time_base, avg_f32, |v: i32| v as f32 / i32::MAX as f32),
            Sample::I32(sample::Type::Planar) => compute_audio_planar(frame_provider, dst, self.sample_rate, pts, self.time_base, avg_f32, |v: i32| v as f32 / i32::MAX as f32),
            Sample::I64(sample::Type::Packed) => compute_audio_packed(frame_provider, dst, self.sample_rate, pts, self.time_base, avg_f32, |v: SampleI64| v.0 as f32 / i64::MAX as f32),
            Sample::I64(sample::Type::Planar) => compute_audio_planar(frame_provider, dst, self.sample_rate, pts, self.time_base, avg_f32, |v: SampleI64| v.0 as f32 / i64::MAX as f32),
            Sample::F32(sample::Type::Packed) => compute_audio_packed(frame_provider, dst, self.sample_rate, pts, self.time_base, avg_f32, |v: f32| v),
            Sample::F32(sample::Type::Planar) => compute_audio_planar(frame_provider, dst, self.sample_rate, pts, self.time_base, avg_f32, |v: f32| v),
            Sample::F64(sample::Type::Packed) => compute_audio_packed(frame_provider, dst, self.sample_rate, pts, self.time_base, avg_f32, |v: f64| v as f32),
            Sample::F64(sample::Type::Planar) => compute_audio_planar(frame_provider, dst, self.sample_rate, pts, self.time_base, avg_f32, |v: f64| v as f32),
        }
    }
}

fn compute_audio_packed<F, Raw, Sample>(mut frame_provider: F, mut dst: MultiChannelAudioSliceMut<Sample>, sample_rate: u32, start_pts: i64, time_base: MixedFraction, avg: impl Fn(Sample, Sample) -> Sample, convert: impl Fn(Raw) -> Sample) -> usize
where
    F: PackedAudioFrameProvider<Raw>,
    Raw: Clone,
    Sample: Clone,
{
    let (time_base_integer, time_base_numerator, time_base_denominator) = time_base.deconstruct();
    assert!(time_base_integer >= 0);
    assert_ne!(time_base_denominator, 0);

    let mut offset = 0;
    while let Some(decoded) = frame_provider.next() {
        let plane_offset = (start_pts - decoded.pts()).max(0) as usize;
        let plane_offset = if plane_offset > 0 {
            let i = plane_offset * sample_rate as usize;
            i.checked_mul(time_base_integer as usize).unwrap() + i.checked_mul(time_base_numerator as usize).unwrap() / time_base_denominator as usize
        } else {
            0
        };
        let samples = decoded.samples();
        if plane_offset >= samples.len() / decoded.channels() {
            continue;
        }
        let Some(mut dst) = dst.slice_mut(offset..) else {
            break;
        };

        for (dst, values) in dst.iter_mut().zip(samples.chunks(decoded.channels()).skip(plane_offset)) {
            match (dst.len(), values.len()) {
                (1, 2..) => dst[0] = avg(convert(values[0].clone()), convert(values[1].clone())),
                (2.., 1) => {
                    let v = convert(values[0].clone());
                    dst[0] = v.clone();
                    dst[1] = v;
                }
                _ => {
                    for (dst, value) in dst.iter_mut().zip(values) {
                        *dst = convert(value.clone());
                    }
                }
            }
        }
        offset += samples.len() / decoded.channels() - plane_offset;
    }
    offset.min(dst.len())
}

fn compute_audio_planar<F, Raw, Sample>(mut frame_provider: F, mut dst: MultiChannelAudioSliceMut<Sample>, sample_rate: u32, start_pts: i64, time_base: MixedFraction, avg: impl Fn(Sample, Sample) -> Sample, convert: impl Fn(Raw) -> Sample) -> usize
where
    F: for<'a> PlanarAudioFrameProvider<Raw>,
    Raw: Clone,
    Sample: Clone,
{
    let (time_base_integer, time_base_numerator, time_base_denominator) = time_base.deconstruct();
    assert!(time_base_integer >= 0);
    assert_ne!(time_base_denominator, 0);

    let mut offset = 0;
    while let Some(decoded) = frame_provider.next() {
        let plane_offset = (start_pts - decoded.pts()).max(0) as usize;
        let plane_offset = if plane_offset > 0 {
            let i = plane_offset * sample_rate as usize;
            i.checked_mul(time_base_integer as usize).unwrap() + i.checked_mul(time_base_numerator as usize).unwrap() / time_base_denominator as usize
        } else {
            0
        };
        let samples = decoded.samples();
        if plane_offset >= samples[0].len() {
            continue;
        }
        let Some(mut dst) = dst.slice_mut(offset..) else {
            break;
        };

        for (i, dst) in dst.iter_mut().take(samples[0].len() - plane_offset).enumerate() {
            let i = i + plane_offset;
            match (dst.len(), samples.len()) {
                (1, 2..) => dst[0] = avg(convert(samples[0][i].clone()), convert(samples[1][i].clone())),
                (2.., 1) => {
                    let v = convert(samples[0][i].clone());
                    dst[0] = v.clone();
                    dst[1] = v;
                }
                _ => {
                    for (dst, value) in dst.iter_mut().zip(samples.iter()) {
                        *dst = convert(value[i].clone());
                    }
                }
            }
        }
        offset += samples[0].len() - plane_offset;
    }
    offset.min(dst.len())
}

trait PackedAudioFrameProvider<T> {
    type Frame<'a>: PackedAudioFrame<T>
    where
        Self: 'a;
    fn next(&mut self) -> Option<Self::Frame<'_>>;
}

trait PlanarAudioFrameProvider<T> {
    type Frame<'a>: PlanarAudioFrame<T>
    where
        Self: 'a;
    fn next(&mut self) -> Option<Self::Frame<'_>>;
}

struct FfmpegFrameProvider<'a> {
    packet_iter: format::context::input::PacketIter<'a>,
    decoder: codec::decoder::audio::Audio,
    stream_index: usize,
    frame: frame::Audio,
    eof: bool,
}

impl<'a> FfmpegFrameProvider<'a> {
    fn new(ictx: &'a mut mpdelta_ffmpeg::io::Input<impl Read + Seek>, decoder: codec::decoder::audio::Audio, stream_index: usize) -> FfmpegFrameProvider<'a> {
        FfmpegFrameProvider {
            packet_iter: ictx.packets(),
            decoder,
            stream_index,
            frame: frame::Audio::empty(),
            eof: false,
        }
    }

    // Generatorが来たら再実装したい
    fn next_frame(&mut self) -> Option<&frame::Audio> {
        loop {
            if self.decoder.receive_frame(&mut self.frame).is_ok() {
                return Some(&self.frame);
            }
            match self.packet_iter.by_ref().find(|(stream, _)| stream.index() == self.stream_index) {
                Some((_, packet)) => {
                    self.decoder.send_packet(&packet).unwrap();
                }
                None => {
                    if self.eof {
                        return None;
                    } else {
                        self.decoder.send_eof().unwrap();
                        self.eof = true;
                    }
                }
            }
        }
    }
}

impl<'a, T> PackedAudioFrameProvider<T> for FfmpegFrameProvider<'a>
where
    T: audio::Sample,
{
    type Frame<'b>
        = &'b frame::Audio
    where
        Self: 'b;

    fn next(&mut self) -> Option<Self::Frame<'_>> {
        self.next_frame()
    }
}

impl<'a, T> PlanarAudioFrameProvider<T> for FfmpegFrameProvider<'a>
where
    T: audio::Sample,
{
    type Frame<'b>
        = &'b frame::Audio
    where
        Self: 'b;

    fn next(&mut self) -> Option<Self::Frame<'_>> {
        self.next_frame()
    }
}

trait PackedAudioFrame<T> {
    fn pts(&self) -> i64;
    fn channels(&self) -> usize;
    fn samples(&self) -> &[T];
}

trait PlanarAudioFrame<T> {
    fn pts(&self) -> i64;
    fn samples(&self) -> SmallVec<[&[T]; 6]>;
}

impl<'a, T: audio::Sample> PackedAudioFrame<T> for &'a frame::Audio {
    fn pts(&self) -> i64 {
        frame::Frame::pts(self).unwrap()
    }

    fn channels(&self) -> usize {
        frame::Audio::channels(self) as usize
    }

    fn samples(&self) -> &[T] {
        // Audio::plane()の計算にバグがあり、Packedの場合には全データを取得できない
        // そのため、その問題の発生しないdata(0)によりFrame内のデータを取得する
        // SAFETY: 数値型のみを対象とするtransmuteなので安全
        let ([], plane, []) = (unsafe { self.data(0).align_to::<T>() }) else { panic!("Packet::data(0) is not aligned by {}", std::any::type_name::<T>()) };
        plane
    }
}

impl<'a, T: audio::Sample> PlanarAudioFrame<T> for &'a frame::Audio {
    fn pts(&self) -> i64 {
        frame::Frame::pts(self).unwrap()
    }

    fn samples(&self) -> SmallVec<[&[T]; 6]> {
        (0..frame::Audio::channels(self)).map(|p| self.plane::<T>(p as usize)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavSpec};
    use mpdelta_core_audio::multi_channel_audio::MultiChannelAudio;
    use mpdelta_dsp::test_util::FormalExpression;
    use std::fs::OpenOptions;
    use std::path::Path;
    use std::{fs, io, iter, mem};

    fn read_image_and_audio(name: &str, input: impl Read + Seek + Clone, contains_video: bool, contains_audio: bool) {
        ffmpeg_next::init().unwrap();
        const TEST_OUTPUT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../test_output/", env!("CARGO_PKG_NAME"));
        let output_dir = Path::new(TEST_OUTPUT_DIR).join(name);
        let _ = fs::remove_dir_all(&output_dir);
        fs::create_dir_all(&output_dir).unwrap();
        let video_reader = VideoReader::new(input.clone());
        let audio_reader = AudioReader::new(input);
        assert_eq!(video_reader.is_some(), contains_video);
        assert_eq!(audio_reader.is_some(), contains_audio);
        if let Some(mut video_reader) = video_reader {
            let duration = video_reader.duration();
            if let Some(duration) = duration {
                let (integer, numerator) = duration.deconstruct_with_round(60);
                for n in 0..numerator {
                    let time = TimelineTime::new(MixedFraction::new(integer, n, 60));
                    video_reader.read_image_at(time).save(output_dir.join(format!("frame{integer:04}_{n:02}.png"))).unwrap();
                }
                for i in (0..integer).rev() {
                    for n in (0..60).rev() {
                        let time = TimelineTime::new(MixedFraction::new(i, n, 60));
                        video_reader.read_image_at(time).save(output_dir.join(format!("frame{i:04}_{n:02}.png"))).unwrap();
                    }
                }
            } else {
                let time = TimelineTime::ZERO;
                video_reader.read_image_at(time).save(output_dir.join("frame.png")).unwrap();
            }
        }
        if let Some(mut audio_reader) = audio_reader {
            let duration = audio_reader.duration();
            let sample_rate = audio_reader.sample_rate();
            let channels = audio_reader.channels();
            let length = duration.unwrap_or(MixedFraction::from_integer(10)) * MixedFraction::from_integer(sample_rate as i32);
            let length_integer = length.deconstruct_with_round(1).0 as usize;

            let mut audio_single_channel = MultiChannelAudio::new(1);
            audio_single_channel.resize(length_integer + 100, 0f32);
            let result_len = audio_reader.compute_audio(TimelineTime::ZERO, audio_single_channel.slice_mut(..).unwrap());
            if duration.is_some() {
                assert_eq!(result_len, length_integer);
            }
            let mut out = OpenOptions::new().create_new(true).write(true).open(output_dir.join("audio_single_channel.wav")).unwrap();
            let mut writer = hound::WavWriter::new(
                &mut out,
                WavSpec {
                    channels: 1,
                    sample_rate,
                    bits_per_sample: 32,
                    sample_format: SampleFormat::Float,
                },
            )
            .unwrap();
            audio_single_channel.slice(..result_len).unwrap().as_linear().iter().for_each(|&s| writer.write_sample(s).unwrap());
            writer.flush().unwrap();

            let mut audio_multi_channel = MultiChannelAudio::new(channels);
            audio_multi_channel.resize(length_integer + 100, 0f32);
            let result_len = audio_reader.compute_audio(TimelineTime::ZERO, audio_multi_channel.slice_mut(..).unwrap());
            if duration.is_some() {
                assert_eq!(result_len, length_integer);
            }
            let mut out = OpenOptions::new().create_new(true).write(true).open(output_dir.join("audio_multi_channel.wav")).unwrap();
            let mut writer = hound::WavWriter::new(
                &mut out,
                WavSpec {
                    channels: channels as u16,
                    sample_rate,
                    bits_per_sample: 32,
                    sample_format: SampleFormat::Float,
                },
            )
            .unwrap();
            audio_multi_channel.slice(..result_len).unwrap().as_linear().iter().for_each(|&s| writer.write_sample(s).unwrap());
            writer.flush().unwrap();

            let mut audio_multi_channel = MultiChannelAudio::new(channels);
            audio_multi_channel.resize(length_integer + 100, 0f32);
            let result_len = audio_reader.compute_audio(TimelineTime::new(MixedFraction::from_integer(1)), audio_multi_channel.slice_mut(..).unwrap());
            if duration.is_some() {
                assert_eq!(result_len, length_integer - sample_rate as usize);
            }
            let mut out = OpenOptions::new().create_new(true).write(true).open(output_dir.join("audio_offset_1sec.wav")).unwrap();
            let mut writer = hound::WavWriter::new(
                &mut out,
                WavSpec {
                    channels: channels as u16,
                    sample_rate,
                    bits_per_sample: 32,
                    sample_format: SampleFormat::Float,
                },
            )
            .unwrap();
            audio_multi_channel.slice(..result_len).unwrap().as_linear().iter().for_each(|&s| writer.write_sample(s).unwrap());
            writer.flush().unwrap();
        }
    }

    #[test]
    fn test_load_mp4() {
        const MEDIA: &[u8] = include_bytes!("./decode_test_video.mp4");
        read_image_and_audio("mp4", io::Cursor::new(MEDIA), true, true);
    }

    #[test]
    fn test_load_png() {
        const MEDIA: &[u8] = include_bytes!("./decode_test_image.png");
        read_image_and_audio("png", io::Cursor::new(MEDIA), true, false);
    }

    #[test]
    fn test_load_flac() {
        const MEDIA: &[u8] = include_bytes!("./decode_test_audio.flac");
        read_image_and_audio("flac", io::Cursor::new(MEDIA), false, true);
    }

    #[test]
    fn test_load_gif() {
        const MEDIA: &[u8] = include_bytes!("./decode_test_gif.gif");
        read_image_and_audio("gif", io::Cursor::new(MEDIA), true, false);
    }

    #[test]
    fn test_compute_audio_packed() {
        struct TestFrameProvider {
            pts: i64,
            samples: Vec<FormalExpression>,
        }
        struct Frame<'a> {
            pts: i64,
            samples: &'a [FormalExpression],
        }
        impl PackedAudioFrameProvider<FormalExpression> for TestFrameProvider {
            type Frame<'a> = Frame<'a>;
            fn next(&mut self) -> Option<Self::Frame<'_>> {
                let next_pts = self.pts + 32;
                let samples = self.samples.get((self.pts * 2) as usize..)?;
                Some(Frame {
                    pts: mem::replace(&mut self.pts, next_pts),
                    samples: &samples[..samples.len().min(64)],
                })
            }
        }
        impl<'a> PackedAudioFrame<FormalExpression> for Frame<'a> {
            fn pts(&self) -> i64 {
                self.pts
            }
            fn channels(&self) -> usize {
                2
            }
            fn samples(&self) -> &[FormalExpression] {
                self.samples
            }
        }

        let f = TestFrameProvider {
            pts: 0,
            samples: (0..2048 + 24).map(FormalExpression::value).collect(),
        };
        let mut dst = MultiChannelAudio::new(2);
        dst.resize(1024, FormalExpression::Zero);

        let len = compute_audio_packed(f, dst.slice_mut(..).unwrap(), 48000, 24, MixedFraction::from_fraction(1, 48000), |_, _| unreachable!(), |v| v);
        assert_eq!(len, 1012);
        assert_eq!(dst.as_linear(), &(48..2048 + 24).map(FormalExpression::value).chain(iter::repeat(FormalExpression::Zero)).take(2048).collect::<Vec<_>>());
    }

    #[test]
    fn test_compute_audio_planar() {
        struct TestFrameProvider {
            pts: i64,
            samples1: Vec<FormalExpression>,
            samples2: Vec<FormalExpression>,
        }
        struct Frame<'a> {
            pts: i64,
            samples1: &'a [FormalExpression],
            samples2: &'a [FormalExpression],
        }
        impl PlanarAudioFrameProvider<FormalExpression> for TestFrameProvider {
            type Frame<'a> = Frame<'a>;
            fn next(&mut self) -> Option<Self::Frame<'_>> {
                let next_pts = self.pts + 32;
                let samples1 = self.samples1.get(self.pts as usize..)?;
                let samples2 = self.samples2.get(self.pts as usize..)?;
                Some(Frame {
                    pts: mem::replace(&mut self.pts, next_pts),
                    samples1: &samples1[..samples1.len().min(32)],
                    samples2: &samples2[..samples2.len().min(32)],
                })
            }
        }
        impl<'a> PlanarAudioFrame<FormalExpression> for Frame<'a> {
            fn pts(&self) -> i64 {
                self.pts
            }
            fn samples(&self) -> SmallVec<[&[FormalExpression]; 6]> {
                [self.samples1, self.samples2].into_iter().collect()
            }
        }

        let f = TestFrameProvider {
            pts: 0,
            samples1: (0..1024 + 12).map(|i| i * 2).map(FormalExpression::value).collect(),
            samples2: (0..1024 + 12).map(|i| i * 2 + 1).map(FormalExpression::value).collect(),
        };
        let mut dst = MultiChannelAudio::new(2);
        dst.resize(1024, FormalExpression::Zero);

        let len = compute_audio_planar(f, dst.slice_mut(..).unwrap(), 48000, 24, MixedFraction::from_fraction(1, 48000), |_, _| unreachable!(), |v| v);
        assert_eq!(len, 1012);
        assert_eq!(dst.as_linear(), &(48..2048 + 24).map(FormalExpression::value).chain(iter::repeat(FormalExpression::Zero)).take(2048).collect::<Vec<_>>());
    }
}
