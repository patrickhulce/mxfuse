//! Raw FFI declarations for the hand-written bmx C++ shim.

#![allow(non_camel_case_types, non_snake_case, clippy::missing_safety_doc)]

use std::os::raw::{c_char, c_int, c_void};

pub const MXFUSE_OK: c_int = 0;
pub const MXFUSE_ERR: c_int = -1;
pub const MXFUSE_ERR_NO_FRAME: c_int = 1;
pub const MXFUSE_ERROR_LEN: usize = 512;
pub const MXFUSE_UL_LEN: usize = 16;
pub const MXFUSE_KEY_LEN: usize = 16;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MxfuseError {
    pub message: [c_char; MXFUSE_ERROR_LEN],
}

impl Default for MxfuseError {
    fn default() -> Self {
        Self {
            message: [0; MXFUSE_ERROR_LEN],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MxfuseByteSourceVtable {
    pub read: Option<unsafe extern "C" fn(*mut c_void, *mut u8, u32) -> i32>,
    pub write: Option<unsafe extern "C" fn(*mut c_void, *const u8, u32) -> i32>,
    pub seek: Option<unsafe extern "C" fn(*mut c_void, i64, c_int) -> c_int>,
    pub tell: Option<unsafe extern "C" fn(*mut c_void) -> i64>,
    pub size: Option<unsafe extern "C" fn(*mut c_void) -> i64>,
    pub is_seekable: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    pub close: Option<unsafe extern "C" fn(*mut c_void)>,
}

#[repr(C)]
pub struct MxfuseWriter {
    _opaque: [u8; 0],
}

pub const MXFUSE_TRACK_HAS_SAMPLING_RATE: u32 = 1 << 0;
pub const MXFUSE_TRACK_HAS_CHANNEL_COUNT: u32 = 1 << 1;
pub const MXFUSE_TRACK_HAS_QUANT_BITS: u32 = 1 << 2;
pub const MXFUSE_TRACK_HAS_STORED_WIDTH: u32 = 1 << 3;
pub const MXFUSE_TRACK_HAS_STORED_HEIGHT: u32 = 1 << 4;
pub const MXFUSE_TRACK_HAS_CONTAINER_UL: u32 = 1 << 5;
pub const MXFUSE_TRACK_HAS_CODING_UL: u32 = 1 << 6;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MxfuseTrackSpec {
    pub essence_type: i32,
    pub flags: u32,
    pub sampling_rate: MxfuseRational,
    pub channel_count: u32,
    pub quantization_bits: u32,
    pub stored_width: u32,
    pub stored_height: u32,
    pub essence_container_ul: [u8; MXFUSE_UL_LEN],
    pub picture_coding_ul: [u8; MXFUSE_UL_LEN],
}

impl Default for MxfuseTrackSpec {
    fn default() -> Self {
        Self {
            essence_type: 0,
            flags: 0,
            sampling_rate: MxfuseRational::default(),
            channel_count: 0,
            quantization_bits: 0,
            stored_width: 0,
            stored_height: 0,
            essence_container_ul: [0; MXFUSE_UL_LEN],
            picture_coding_ul: [0; MXFUSE_UL_LEN],
        }
    }
}

#[repr(C)]
pub struct MxfuseReader {
    _opaque: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct MxfuseRational {
    pub num: i32,
    pub den: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct MxfuseClipInfo {
    pub edit_rate: MxfuseRational,
    pub duration: i64,
    pub num_tracks: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MxfuseTrackInfo {
    pub index: usize,
    pub data_def: i32,
    pub essence_type: i32,
    pub essence_container_ul: [u8; MXFUSE_UL_LEN],
    pub edit_rate: MxfuseRational,
    pub duration: i64,
    pub enabled: c_int,
}

impl Default for MxfuseTrackInfo {
    fn default() -> Self {
        Self {
            index: 0,
            data_def: 0,
            essence_type: 0,
            essence_container_ul: [0; MXFUSE_UL_LEN],
            edit_rate: MxfuseRational::default(),
            duration: 0,
            enabled: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MxfuseFrameView {
    pub data: *mut u8,
    pub size: u32,
    pub element_key: [u8; MXFUSE_KEY_LEN],
    pub file_position: i64,
    pub kl_size: u8,
    pub position: i64,
}

impl Default for MxfuseFrameView {
    fn default() -> Self {
        Self {
            data: std::ptr::null_mut(),
            size: 0,
            element_key: [0; MXFUSE_KEY_LEN],
            file_position: 0,
            kl_size: 0,
            position: 0,
        }
    }
}

extern "C" {
    pub fn mxfuse_reader_open(
        vt: *const MxfuseByteSourceVtable,
        ctx: *mut c_void,
        cache_bytes: u32,
        out: *mut *mut MxfuseReader,
        err: *mut MxfuseError,
    ) -> c_int;

    pub fn mxfuse_reader_free(reader: *mut MxfuseReader);

    pub fn mxfuse_reader_clip_info(
        reader: *mut MxfuseReader,
        out: *mut MxfuseClipInfo,
        err: *mut MxfuseError,
    ) -> c_int;

    pub fn mxfuse_reader_track_info(
        reader: *mut MxfuseReader,
        index: usize,
        out: *mut MxfuseTrackInfo,
        err: *mut MxfuseError,
    ) -> c_int;

    pub fn mxfuse_reader_set_enable(
        reader: *mut MxfuseReader,
        index: usize,
        enable: c_int,
        err: *mut MxfuseError,
    ) -> c_int;

    pub fn mxfuse_reader_seek(
        reader: *mut MxfuseReader,
        position: i64,
        err: *mut MxfuseError,
    ) -> c_int;

    pub fn mxfuse_reader_read(
        reader: *mut MxfuseReader,
        num_samples: u32,
        out_read: *mut u32,
        err: *mut MxfuseError,
    ) -> c_int;

    pub fn mxfuse_reader_pop_frame(
        reader: *mut MxfuseReader,
        track: usize,
        out: *mut MxfuseFrameView,
        err: *mut MxfuseError,
    ) -> c_int;

    pub fn mxfuse_frame_free(view: *mut MxfuseFrameView);

    pub fn mxfuse_essence_type_name(essence_type: i32) -> *const c_char;

    pub fn mxfuse_writer_open(
        vt: *const MxfuseByteSourceVtable,
        ctx: *mut c_void,
        flavour: c_int,
        edit_rate: MxfuseRational,
        duration: i64,
        out: *mut *mut MxfuseWriter,
        err: *mut MxfuseError,
    ) -> c_int;

    pub fn mxfuse_writer_create_track(
        writer: *mut MxfuseWriter,
        spec: *const MxfuseTrackSpec,
        out_index: *mut u32,
        err: *mut MxfuseError,
    ) -> c_int;

    pub fn mxfuse_writer_prepare(writer: *mut MxfuseWriter, err: *mut MxfuseError) -> c_int;

    pub fn mxfuse_writer_write_samples(
        writer: *mut MxfuseWriter,
        track_index: u32,
        data: *const u8,
        size: u32,
        num_samples: u32,
        err: *mut MxfuseError,
    ) -> c_int;

    pub fn mxfuse_writer_complete(writer: *mut MxfuseWriter, err: *mut MxfuseError) -> c_int;

    pub fn mxfuse_writer_free(writer: *mut MxfuseWriter);
}
