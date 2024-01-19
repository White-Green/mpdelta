use ffmpeg_sys_next::{avformat_alloc_output_context2, avformat_free_context, avformat_query_codec, AVFormatContext, FF_COMPLIANCE_STRICT};
use mpdelta_multimedia::FileFormat;
use std::ptr;

pub mod codec;
pub mod io;

pub fn supports(file_format: FileFormat, codec: ffmpeg_next::codec::Id) -> bool {
    let file_extension = file_format.extension_c();
    let mut output_context: *mut AVFormatContext = ptr::null_mut();
    let result = unsafe { avformat_alloc_output_context2(&mut output_context, ptr::null(), file_extension.as_ptr(), ptr::null()) };
    if result < 0 {
        return false;
    }
    assert!(!output_context.is_null());
    let result = unsafe { avformat_query_codec((*output_context).oformat, codec.into(), FF_COMPLIANCE_STRICT) } == 1;
    unsafe { avformat_free_context(output_context) };
    result
}
