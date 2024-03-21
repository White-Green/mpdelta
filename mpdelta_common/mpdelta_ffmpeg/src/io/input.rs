use crate::io::FfmpegIoError;
use bitflags::bitflags;
use ffmpeg_next::format;
use ffmpeg_next::util::range::Range;
use ffmpeg_sys_next as ff;
use std::io::{Read, Seek};
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::ptr::NonNull;

pub struct Input<T> {
    input: ManuallyDrop<format::context::Input>,
    input_ptr: *mut T,
}

pub struct InputBuilder {
    buffer_size: i32,
}

impl InputBuilder {
    pub fn buffer_size(mut self, buffer_size: i32) -> Self {
        self.buffer_size = buffer_size;
        self
    }
}

impl Default for InputBuilder {
    fn default() -> Self {
        InputBuilder { buffer_size: 4096 }
    }
}

impl InputBuilder {
    pub fn build<T>(self, input: T) -> Result<Input<T>, FfmpegIoError>
    where
        T: Read + Seek,
    {
        let InputBuilder { buffer_size } = self;
        let buffer = unsafe { ff::av_malloc(buffer_size as usize) };
        let buffer = NonNull::new(buffer).ok_or(FfmpegIoError::AllocationError)?;
        let input = Box::new(input);
        let input_ptr = ptr::from_mut(Box::leak(input));
        let io_context = unsafe { ff::avio_alloc_context(buffer.as_ptr().cast(), buffer_size, 0, input_ptr.cast(), Some(super::read::<T>), None, Some(super::seek::<T>)) };
        let Some(io_context) = NonNull::new(io_context) else {
            unsafe {
                ff::av_free(buffer.as_ptr());
                drop(Box::from_raw(input_ptr));
            };
            return Err(FfmpegIoError::IOContextCreationError);
        };

        let mut format_context = unsafe { ff::avformat_alloc_context() };
        if format_context.is_null() {
            unsafe {
                ff::av_freep((*io_context.as_ptr()).buffer.cast());
                ff::avio_context_free(&mut io_context.as_ptr());
                drop(Box::from_raw(input_ptr));
            }
            return Err(FfmpegIoError::AllocationError);
        }
        assert!(!format_context.is_null());
        unsafe {
            (*format_context).pb = io_context.as_ptr();
        }
        let ret = unsafe { ff::avformat_open_input(&mut format_context, ptr::null(), ptr::null_mut(), ptr::null_mut()) };
        if ret < 0 {
            unsafe {
                ff::av_freep((*io_context.as_ptr()).buffer.cast());
                ff::avio_context_free(&mut io_context.as_ptr());
                drop(Box::from_raw(input_ptr));
            }
            return Err(ffmpeg_next::Error::from(ret).into());
        }
        let ret = unsafe { ff::avformat_find_stream_info(format_context, ptr::null_mut()) };
        if ret < 0 {
            unsafe {
                (*format_context).pb = ptr::null_mut();
                ff::avformat_free_context(format_context);
                ff::av_freep((*io_context.as_ptr()).buffer.cast());
                ff::avio_context_free(&mut io_context.as_ptr());
                drop(Box::from_raw(input_ptr));
            }
            return Err(ffmpeg_next::Error::from(ret).into());
        }
        let input = unsafe { format::context::Input::wrap(format_context) };
        Ok(Input { input: ManuallyDrop::new(input), input_ptr })
    }
}

impl Input<()> {
    pub fn builder() -> InputBuilder {
        InputBuilder::default()
    }
}

bitflags! {
    pub struct SeekFlag: i32 {
        const BACKWARD = ffmpeg_sys_next::AVSEEK_FLAG_BACKWARD;
        const BYTE = ffmpeg_sys_next::AVSEEK_FLAG_BYTE;
        const ANY = ffmpeg_sys_next::AVSEEK_FLAG_ANY;
        const FRAME = ffmpeg_sys_next::AVSEEK_FLAG_FRAME;
    }
}

impl<T: Read + Seek> Input<T> {
    pub fn new(input: T) -> Result<Input<T>, FfmpegIoError> {
        InputBuilder::default().build(input)
    }

    pub fn seek_with_flag(&mut self, stream_index: Option<i32>, timestamp: i64, range: impl Range<i64>, flags: SeekFlag) -> Result<(), FfmpegIoError> {
        let ret = unsafe { ff::avformat_seek_file(self.input.as_mut_ptr(), stream_index.unwrap_or(-1), range.start().cloned().unwrap_or(i64::MIN), timestamp, range.end().cloned().unwrap_or(i64::MAX), flags.bits()) };
        if ret < 0 {
            return Err(ffmpeg_next::Error::from(ret).into());
        }
        Ok(())
    }
}

unsafe impl<T> Send for Input<T> where T: Send {}

unsafe impl<T> Sync for Input<T> where T: Sync {}

impl<T> Drop for Input<T> {
    fn drop(&mut self) {
        unsafe {
            // avio_closeの中で.opaqueをURLContext*として参照しているっぽい(https://ffmpeg.org/doxygen/6.0/aviobuf_8c_source.html#l01247)ので、変なdrop処理が走らないようnullに書き替えておく
            (*(*self.input.as_mut_ptr()).pb).opaque = ptr::null_mut();
            ManuallyDrop::drop(&mut self.input);
            drop(Box::from_raw(self.input_ptr));
        }
    }
}

impl<T> Deref for Input<T> {
    type Target = format::context::Input;

    fn deref(&self) -> &Self::Target {
        &self.input
    }
}

impl<T> DerefMut for Input<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.input
    }
}
