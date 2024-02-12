use ffmpeg_sys_next as ff;
use std::ffi::{c_int, c_void};
use std::io::{Seek, SeekFrom, Write};
use std::slice;
use thiserror::Error;

pub mod output;

pub use output::Output;

#[derive(Debug, Error)]
pub enum FfmpegIoError {
    #[error("failed to allocate memory")]
    AllocationError,
    #[error("failed to create AVIOContext")]
    IOContextCreationError,
    #[error("failed to guess format")]
    GuessFormatError,
    #[error("{0}")]
    FfmpegError(#[from] ffmpeg_next::Error),
}

unsafe extern "C" fn write<T: Write>(opaque: *mut c_void, buffer: *mut u8, len: c_int) -> c_int {
    let output = &mut *(opaque as *mut T);
    output.write_all(slice::from_raw_parts(buffer, len as usize)).map_or(-1, |_| len)
}

unsafe extern "C" fn seek<T: Seek>(opaque: *mut c_void, seek: i64, whence: c_int) -> i64 {
    let output = &mut *(opaque as *mut T);
    match whence {
        ff::SEEK_SET => output.seek(SeekFrom::Start(seek as u64)).map_or(-1, |pos| pos as i64),
        ff::SEEK_CUR => output.seek(SeekFrom::Current(seek)).map_or(-1, |pos| pos as i64),
        ff::SEEK_END => output.seek(SeekFrom::End(seek)).map_or(-1, |pos| pos as i64),
        ff::AVSEEK_SIZE => {
            let Ok(first_pos) = output.stream_position() else {
                return -1;
            };
            let Ok(last_pos) = output.seek(SeekFrom::End(0)) else {
                return -1;
            };
            let Ok(_) = output.seek(SeekFrom::Start(first_pos)) else {
                return -1;
            };
            last_pos as i64
        }
        _ => -1,
    }
}
