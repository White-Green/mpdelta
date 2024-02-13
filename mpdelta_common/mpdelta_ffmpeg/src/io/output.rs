use crate::io::FfmpegIoError;
use ffmpeg_next::format;
use ffmpeg_sys_next as ff;
use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::io::{Seek, Write};
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::ptr::NonNull;

pub struct Output<T> {
    output: ManuallyDrop<format::context::Output>,
    output_ptr: *mut T,
}

pub struct OutputBuilder<'a> {
    buffer_size: i32,
    file_type: Option<&'a str>,
    mime_type: Option<&'a str>,
    format: *const ff::AVOutputFormat,
}

impl<'a> OutputBuilder<'a> {
    pub fn buffer_size(mut self, buffer_size: i32) -> Self {
        self.buffer_size = buffer_size;
        self
    }

    pub fn file_type(mut self, file_type: &'a str) -> Self {
        self.file_type = Some(file_type);
        self
    }

    pub fn mime_type(mut self, mime_type: &'a str) -> Self {
        self.mime_type = Some(mime_type);
        self
    }

    pub fn format(mut self, format: *const ff::AVOutputFormat) -> Self {
        self.format = format;
        self
    }
}

impl<'a> Default for OutputBuilder<'a> {
    fn default() -> Self {
        OutputBuilder {
            buffer_size: 4096,
            file_type: None,
            mime_type: None,
            format: ptr::null(),
        }
    }
}

impl<'a> OutputBuilder<'a> {
    pub fn build<T>(self, output: T) -> Result<Output<T>, FfmpegIoError>
    where
        T: Write + Seek,
    {
        let OutputBuilder { buffer_size, file_type, mime_type, format } = self;
        let format = guess_format(file_type, mime_type, format)?;
        let buffer = unsafe { ff::av_malloc(buffer_size as usize) };
        let buffer = NonNull::new(buffer).ok_or(FfmpegIoError::AllocationError)?;
        let output = Box::new(output);
        let output_ptr = ptr::from_mut(Box::leak(output));
        let io_context = unsafe { ff::avio_alloc_context(buffer.as_ptr().cast(), buffer_size, 1, output_ptr.cast(), None, Some(super::write::<T>), Some(super::seek::<T>)) };
        let Some(io_context) = NonNull::new(io_context) else {
            unsafe { ff::av_free(buffer.as_ptr()) };
            return Err(FfmpegIoError::IOContextCreationError);
        };

        let mut format_context = ptr::null_mut();
        let ret = unsafe { ff::avformat_alloc_output_context2(&mut format_context, format, ptr::null(), ptr::null()) };
        if ret < 0 {
            unsafe {
                ff::av_freep((*io_context.as_ptr()).buffer.cast());
                ff::avio_context_free(&mut io_context.as_ptr());
            }
            return Err(ffmpeg_next::Error::from(ret).into());
        }
        assert!(!format_context.is_null());
        let output = unsafe {
            (*format_context).pb = io_context.as_ptr();
            format::context::Output::wrap(format_context)
        };
        Ok(Output { output: ManuallyDrop::new(output), output_ptr })
    }
}

impl Output<()> {
    pub fn builder<'a>() -> OutputBuilder<'a> {
        OutputBuilder::default()
    }
}

impl<T> Output<T> {
    pub fn new(output: T) -> Result<Output<T>, FfmpegIoError>
    where
        T: Write + Seek,
    {
        OutputBuilder::default().build(output)
    }
}

unsafe impl<T> Send for Output<T> where T: Send {}

unsafe impl<T> Sync for Output<T> where T: Sync {}

impl<T> Drop for Output<T> {
    fn drop(&mut self) {
        unsafe {
            // avio_closeの中で.opaqueをURLContext*として参照しているっぽい(https://ffmpeg.org/doxygen/6.0/aviobuf_8c_source.html#l01247)ので、変なdrop処理が走らないようnullに書き替えておく
            (*(*self.output.as_mut_ptr()).pb).opaque = ptr::null_mut();
            ManuallyDrop::drop(&mut self.output);
            drop(Box::from_raw(self.output_ptr));
        }
    }
}

impl<T> Deref for Output<T> {
    type Target = format::context::Output;

    fn deref(&self) -> &Self::Target {
        &self.output
    }
}

impl<T> DerefMut for Output<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.output
    }
}

fn guess_format(file_type: Option<&str>, mime_type: Option<&str>, format: *const ff::AVOutputFormat) -> Result<*const ff::AVOutputFormat, FfmpegIoError> {
    if !format.is_null() {
        return Ok(format);
    }
    let file_type = file_type.and_then(|file_type| match CStr::from_bytes_with_nul(file_type.as_bytes()) {
        Ok(file_type) => Some(Cow::Borrowed(file_type)),
        Err(_) => CString::new(file_type).ok().map(Cow::Owned),
    });
    let file_type = file_type.as_ref().map_or(ptr::null(), |file_type| CStr::as_ptr(file_type));
    let mime_type = mime_type.and_then(|mime_type| match CStr::from_bytes_with_nul(mime_type.as_bytes()) {
        Ok(mime_type) => Some(Cow::Borrowed(mime_type)),
        Err(_) => CString::new(mime_type).ok().map(Cow::Owned),
    });
    let mime_type = mime_type.as_ref().map_or(ptr::null(), |mime_type| CStr::as_ptr(mime_type));
    let format = unsafe { ff::av_guess_format(file_type, ptr::null(), mime_type) };
    if format.is_null() {
        return Err(FfmpegIoError::GuessFormatError);
    }
    Ok(format)
}
