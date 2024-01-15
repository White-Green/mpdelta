use crate::options_value::{OptionValue, OptionValuesRefMut};
use indexmap::IndexMap;
use std::any::Any;
use std::borrow::Cow;
use std::ffi::CStr;
use std::fmt::Debug;
use std::path::Path;
use std::sync::Arc;

pub mod options_value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileFormat {
    // Video formats
    Mp4,
    Webm,
    // Audio formats
    Mp3,
    Wav,
    Flac,
    // Image formats
    Png,
    Jpeg,
    Webp,
}

impl FileFormat {
    pub fn is_video(self) -> bool {
        matches!(self, FileFormat::Mp4 | FileFormat::Webm)
    }

    pub fn is_audio(self) -> bool {
        matches!(self, FileFormat::Mp3 | FileFormat::Wav)
    }

    pub fn is_image(self) -> bool {
        matches!(self, FileFormat::Png | FileFormat::Jpeg | FileFormat::Webp)
    }

    pub fn extension(self) -> &'static str {
        match self {
            FileFormat::Mp4 => "mp4",
            FileFormat::Webm => "webm",
            FileFormat::Mp3 => "mp3",
            FileFormat::Wav => "wav",
            FileFormat::Flac => "flac",
            FileFormat::Png => "png",
            FileFormat::Jpeg => "jpeg",
            FileFormat::Webp => "webp",
        }
    }

    pub fn extension_c(self) -> &'static CStr {
        match self {
            FileFormat::Mp4 => CStr::from_bytes_with_nul(b"mp4\0").unwrap(),
            FileFormat::Webm => CStr::from_bytes_with_nul(b"webm\0").unwrap(),
            FileFormat::Mp3 => CStr::from_bytes_with_nul(b"mp3\0").unwrap(),
            FileFormat::Wav => CStr::from_bytes_with_nul(b"wav\0").unwrap(),
            FileFormat::Flac => CStr::from_bytes_with_nul(b"flac\0").unwrap(),
            FileFormat::Png => CStr::from_bytes_with_nul(b"png\0").unwrap(),
            FileFormat::Jpeg => CStr::from_bytes_with_nul(b"jpeg\0").unwrap(),
            FileFormat::Webp => CStr::from_bytes_with_nul(b"webp\0").unwrap(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VideoCodec {
    H264,
    H265,
    Av1,
    Png,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AudioCodec {
    Mp3,
    Aac,
    Flac,
    Opus,
}

#[derive(Debug, PartialEq)]
pub struct VideoOption {
    height: u32,
    width: u32,
    frame_rate: f64,
    bit_rate: usize,
    max_bit_rate: usize,
}

impl Default for VideoOption {
    fn default() -> Self {
        VideoOption {
            height: 1080,
            width: 1920,
            frame_rate: 60.,
            bit_rate: 4_000_000,
            max_bit_rate: 4_000_000,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct AudioOption {
    sample_rate: u32,
    bit_rate: usize,
    max_bit_rate: usize,
}

impl Default for AudioOption {
    fn default() -> Self {
        AudioOption {
            sample_rate: 44100,
            bit_rate: 192_000,
            max_bit_rate: 192_000,
        }
    }
}

pub trait HasOption {
    type Option: Debug + Default + PartialEq;
}

impl HasOption for VideoCodec {
    type Option = VideoOption;
}

impl HasOption for AudioCodec {
    type Option = AudioOption;
}

#[derive(Debug, PartialEq)]
pub struct CodecOptions<Codec: HasOption> {
    dependent_option: Codec::Option,
    options: IndexMap<Cow<'static, str>, OptionValue>,
}

impl<Codec: HasOption> CodecOptions<Codec> {
    pub fn new(options: IndexMap<Cow<'static, str>, OptionValue>) -> CodecOptions<Codec> {
        CodecOptions { dependent_option: Default::default(), options }
    }

    pub fn into_options(self) -> (Codec::Option, IndexMap<Cow<'static, str>, OptionValue>) {
        (self.dependent_option, self.options)
    }

    pub fn dependent_option(&self) -> &Codec::Option {
        &self.dependent_option
    }

    pub fn options(&self) -> &IndexMap<Cow<'static, str>, OptionValue> {
        &self.options
    }

    pub fn options_mut(&mut self) -> impl Iterator<Item = (&str, OptionValuesRefMut)> {
        self.options.iter_mut().map(|(k, v)| (k.as_ref(), v.as_ref()))
    }
}

impl CodecOptions<VideoCodec> {
    pub fn height(&self) -> u32 {
        self.dependent_option.height
    }

    pub fn set_height(&mut self, height: u32) {
        self.dependent_option.height = height;
    }

    pub fn width(&self) -> u32 {
        self.dependent_option.width
    }

    pub fn set_width(&mut self, width: u32) {
        self.dependent_option.width = width;
    }

    pub fn frame_rate(&self) -> f64 {
        self.dependent_option.frame_rate
    }

    pub fn set_frame_rate(&mut self, frame_rate: f64) {
        self.dependent_option.frame_rate = frame_rate;
    }

    pub fn bit_rate(&self) -> usize {
        self.dependent_option.bit_rate
    }

    pub fn set_bit_rate(&mut self, bit_rate: usize) {
        self.dependent_option.bit_rate = bit_rate;
    }

    pub fn max_bit_rate(&self) -> usize {
        self.dependent_option.max_bit_rate
    }

    pub fn set_max_bit_rate(&mut self, max_bit_rate: usize) {
        self.dependent_option.max_bit_rate = max_bit_rate;
    }
}

impl CodecOptions<AudioCodec> {
    pub fn sample_rate(&self) -> u32 {
        self.dependent_option.sample_rate
    }

    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        self.dependent_option.sample_rate = sample_rate;
    }

    pub fn bit_rate(&self) -> usize {
        self.dependent_option.bit_rate
    }

    pub fn set_bit_rate(&mut self, bit_rate: usize) {
        self.dependent_option.bit_rate = bit_rate;
    }

    pub fn max_bit_rate(&self) -> usize {
        self.dependent_option.max_bit_rate
    }

    pub fn set_max_bit_rate(&mut self, max_bit_rate: usize) {
        self.dependent_option.max_bit_rate = max_bit_rate;
    }
}

pub trait MediaCodecImplementHandle<Encoder>: Send + Sync + Any {
    fn eq(&self, rhs: &dyn MediaCodecImplementHandle<Encoder>) -> bool;
    fn supports(&self, file_format: FileFormat, video: Option<VideoCodec>, audio: Option<AudioCodec>) -> bool;
    fn create_encoder(&self, file_format: FileFormat, video: Option<(VideoCodec, CodecOptions<VideoCodec>)>, audio: Option<(AudioCodec, CodecOptions<AudioCodec>)>, output: &Path) -> Encoder;
}

pub struct CodecImplement<Codec, Encoder> {
    codec: Codec,
    default_codec_options: IndexMap<Cow<'static, str>, OptionValue>,
    handle: Arc<dyn MediaCodecImplementHandle<Encoder>>,
}

impl<Codec: Copy + HasOption, Encoder> CodecImplement<Codec, Encoder> {
    pub fn new(codec: Codec, default_codec_options: IndexMap<Cow<'static, str>, OptionValue>, handle: Arc<dyn MediaCodecImplementHandle<Encoder>>) -> CodecImplement<Codec, Encoder> {
        CodecImplement { codec, default_codec_options, handle }
    }

    pub fn codec(&self) -> Codec {
        self.codec
    }

    pub fn default_codec_options(&self) -> CodecOptions<Codec> {
        CodecOptions::new(self.default_codec_options.clone())
    }

    pub fn handler(&self) -> &Arc<dyn MediaCodecImplementHandle<Encoder>> {
        &self.handle
    }
}
