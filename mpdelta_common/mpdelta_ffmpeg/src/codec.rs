use ffmpeg_next::format::{Pixel, Sample};
use ffmpeg_next::{codec, Codec, Rational};
use ffmpeg_sys_next::{avcodec_alloc_context3, AVPixelFormat, AVRational, AVSampleFormat};
use std::marker::PhantomData;

pub fn codec_context_time_base(context: codec::Context) -> Rational {
    unsafe { (*context.as_ptr()).time_base.into() }
}

pub fn new_codec_context_from_codec(codec: Codec) -> codec::Context {
    unsafe { codec::Context::wrap(avcodec_alloc_context3(codec.as_ptr()), None) }
}

/// array of supported framerates, or None if any
pub fn codec_supported_frame_rate(codec: &Codec) -> Option<SupportedFramerateIterator<'_>> {
    SupportedFramerateIterator::new(codec)
}

/// array of supported pixel formats, or None if unknown
pub fn codec_supported_pixel_format(codec: &Codec) -> Option<PixelFormatIterator<'_>> {
    PixelFormatIterator::new(codec)
}

/// array of supported audio samplerates, or None if unknown
pub fn codec_supported_sample_rate(codec: &Codec) -> Option<SupportedSampleRateIterator<'_>> {
    SupportedSampleRateIterator::new(codec)
}

/// array of supported sample formats, or None if unknown
pub fn codec_supported_sample_format(codec: &Codec) -> Option<SampleFormatIterator<'_>> {
    SampleFormatIterator::new(codec)
}

pub struct SupportedFramerateIterator<'a> {
    ptr: *const AVRational,
    _phantom: PhantomData<&'a ()>,
}

impl<'a> SupportedFramerateIterator<'a> {
    pub fn new(codec: &'a Codec) -> Option<Self> {
        let ptr = unsafe { codec.as_ptr() };
        if ptr.is_null() {
            return None;
        }
        let ptr = unsafe { (*ptr).supported_framerates };
        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr, _phantom: PhantomData })
        }
    }
}

impl Iterator for SupportedFramerateIterator<'_> {
    type Item = Rational;

    fn next(&mut self) -> Option<Self::Item> {
        assert!(!self.ptr.is_null());
        let rational = unsafe { self.ptr.read() };
        if rational.num == 0 && rational.den == 0 {
            None
        } else {
            self.ptr = unsafe { self.ptr.add(1) };
            Some(Rational::from(rational))
        }
    }
}

pub struct PixelFormatIterator<'a> {
    ptr: *const AVPixelFormat,
    _phantom: PhantomData<&'a ()>,
}

impl<'a> PixelFormatIterator<'a> {
    pub fn new(codec: &'a Codec) -> Option<Self> {
        let ptr = unsafe { codec.as_ptr() };
        if ptr.is_null() {
            return None;
        }
        let ptr = unsafe { (*ptr).pix_fmts };
        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr, _phantom: PhantomData })
        }
    }
}

impl Iterator for PixelFormatIterator<'_> {
    type Item = Pixel;

    fn next(&mut self) -> Option<Self::Item> {
        assert!(!self.ptr.is_null());
        let pixel_format = unsafe { self.ptr.read() };
        if pixel_format == AVPixelFormat::AV_PIX_FMT_NONE {
            None
        } else {
            self.ptr = unsafe { self.ptr.add(1) };
            Some(Pixel::from(pixel_format))
        }
    }
}

pub struct SupportedSampleRateIterator<'a> {
    ptr: *const i32,
    _phantom: PhantomData<&'a ()>,
}

impl<'a> SupportedSampleRateIterator<'a> {
    pub fn new(codec: &'a Codec) -> Option<Self> {
        let ptr = unsafe { codec.as_ptr() };
        if ptr.is_null() {
            return None;
        }
        let ptr = unsafe { (*ptr).supported_samplerates };
        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr, _phantom: PhantomData })
        }
    }
}

impl Iterator for SupportedSampleRateIterator<'_> {
    type Item = i32;

    fn next(&mut self) -> Option<Self::Item> {
        assert!(!self.ptr.is_null());
        let sample_rate = unsafe { self.ptr.read() };
        if sample_rate == 0 {
            None
        } else {
            self.ptr = unsafe { self.ptr.add(1) };
            Some(sample_rate)
        }
    }
}

pub struct SampleFormatIterator<'a> {
    ptr: *const AVSampleFormat,
    _phantom: PhantomData<&'a ()>,
}

impl<'a> SampleFormatIterator<'a> {
    pub fn new(codec: &'a Codec) -> Option<Self> {
        let ptr = unsafe { codec.as_ptr() };
        if ptr.is_null() {
            return None;
        }
        let ptr = unsafe { (*ptr).sample_fmts };
        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr, _phantom: PhantomData })
        }
    }
}

impl Iterator for SampleFormatIterator<'_> {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        assert!(!self.ptr.is_null());
        let sample_format = unsafe { self.ptr.read() };
        if sample_format == AVSampleFormat::AV_SAMPLE_FMT_NONE {
            None
        } else {
            self.ptr = unsafe { self.ptr.add(1) };
            Some(Sample::from(sample_format))
        }
    }
}
