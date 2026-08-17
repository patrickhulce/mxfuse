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
    pub seek: Option<unsafe extern "C" fn(*mut c_void, i64, c_int) -> c_int>,
    pub tell: Option<unsafe extern "C" fn(*mut c_void) -> i64>,
    pub size: Option<unsafe extern "C" fn(*mut c_void) -> i64>,
    pub close: Option<unsafe extern "C" fn(*mut c_void)>,
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
}
