//! Raw FFI declarations for the hand-written bmx C++ shim.

#![allow(non_camel_case_types, non_snake_case, clippy::missing_safety_doc)]

use std::os::raw::{c_char, c_int, c_void};

pub const MXFUSE_OK: c_int = 0;
pub const MXFUSE_ERR: c_int = -1;
pub const MXFUSE_ERR_NO_FRAME: c_int = 1;
pub const MXFUSE_ERROR_LEN: usize = 512;
pub const MXFUSE_UL_LEN: usize = 16;
pub const MXFUSE_KEY_LEN: usize = 16;
pub const MXFUSE_UMID_LEN: usize = 32;
pub const MXFUSE_XML_LANG_LEN: usize = 32;
pub const MXFUSE_XML_MIME_LEN: usize = 64;
pub const MXFUSE_XML_NS_LEN: usize = 128;
pub const MXFUSE_NAME_LEN: usize = 64;
pub const MXFUSE_VERSION_LEN: usize = 64;
pub const MXFUSE_PIXEL_LAYOUT_LEN: usize = 16;

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
pub const MXFUSE_TRACK_HAS_ELEMENT_TYPE: u32 = 1 << 7;
pub const MXFUSE_TRACK_HAS_ELEMENT_LLEN: u32 = 1 << 8;
pub const MXFUSE_TRACK_HAS_TEMPORAL_REORDER: u32 = 1 << 9;
pub const MXFUSE_TRACK_HAS_DESCRIPTOR: u32 = 1 << 10;
pub const MXFUSE_TRACK_HAS_COMPONENT_DEPTH: u32 = 1 << 11;
pub const MXFUSE_TRACK_HAS_SUBSAMPLING: u32 = 1 << 12;
pub const MXFUSE_TRACK_HAS_FRAME_LAYOUT: u32 = 1 << 13;
pub const MXFUSE_TRACK_HAS_ASPECT_RATIO: u32 = 1 << 14;
pub const MXFUSE_TRACK_HAS_VIDEO_LINE_MAP: u32 = 1 << 15;
pub const MXFUSE_TRACK_HAS_PIXEL_LAYOUT: u32 = 1 << 16;
pub const MXFUSE_TRACK_HAS_COLOR_PRIMARIES: u32 = 1 << 17;
pub const MXFUSE_TRACK_HAS_TRANSFER: u32 = 1 << 18;
pub const MXFUSE_TRACK_HAS_CODING_EQ: u32 = 1 << 19;

pub const MXFUSE_CLIP_HAS_START_TIMECODE: u32 = 1 << 0;
pub const MXFUSE_CLIP_HAS_TIMECODE_TRACK: u32 = 1 << 1;
pub const MXFUSE_CLIP_HAS_SYSTEM_ITEM: u32 = 1 << 2;
pub const MXFUSE_CLIP_HAS_COMPANY: u32 = 1 << 3;
pub const MXFUSE_CLIP_HAS_PRODUCT: u32 = 1 << 4;
pub const MXFUSE_CLIP_HAS_VERSION_STRING: u32 = 1 << 5;
pub const MXFUSE_CLIP_HAS_PRODUCT_VERSION: u32 = 1 << 6;
pub const MXFUSE_CLIP_HAS_PRODUCT_UID: u32 = 1 << 7;
pub const MXFUSE_CLIP_HAS_CREATION_DATE: u32 = 1 << 8;
pub const MXFUSE_CLIP_HAS_GENERATION_UID: u32 = 1 << 9;
pub const MXFUSE_CLIP_HAS_MATERIAL_UID: u32 = 1 << 10;
pub const MXFUSE_CLIP_HAS_FILE_SOURCE_UID: u32 = 1 << 11;

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
    pub element_type: u8,
    pub element_llen: u8,
    pub temporal_reordering: c_int,
    pub descriptor_kind: i32,
    pub component_depth: u32,
    pub horiz_subsampling: u32,
    pub vert_subsampling: u32,
    pub frame_layout: u8,
    pub aspect_ratio: MxfuseRational,
    pub video_line_map: [i32; 2],
    pub pixel_layout: [u8; MXFUSE_PIXEL_LAYOUT_LEN],
    pub pixel_layout_count: u8,
    pub color_primaries: [u8; MXFUSE_UL_LEN],
    pub transfer_characteristic: [u8; MXFUSE_UL_LEN],
    pub coding_equations: [u8; MXFUSE_UL_LEN],
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
            element_type: 0,
            element_llen: 0,
            temporal_reordering: 0,
            descriptor_kind: 0,
            component_depth: 0,
            horiz_subsampling: 0,
            vert_subsampling: 0,
            frame_layout: 0,
            aspect_ratio: MxfuseRational::default(),
            video_line_map: [0; 2],
            pixel_layout: [0; MXFUSE_PIXEL_LAYOUT_LEN],
            pixel_layout_count: 0,
            color_primaries: [0; MXFUSE_UL_LEN],
            transfer_characteristic: [0; MXFUSE_UL_LEN],
            coding_equations: [0; MXFUSE_UL_LEN],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct MxfuseTimecode {
    pub hour: i16,
    pub minute: i16,
    pub second: i16,
    pub frame: i16,
    pub drop_frame: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MxfuseClipOptions {
    pub flags: u32,
    pub start_timecode: MxfuseTimecode,
    pub timecode_track: c_int,
    pub system_item: c_int,
    pub company_name: [c_char; MXFUSE_NAME_LEN],
    pub product_name: [c_char; MXFUSE_NAME_LEN],
    pub version_string: [c_char; MXFUSE_VERSION_LEN],
    pub product_version: [u16; 5],
    pub product_uid: [u8; MXFUSE_UL_LEN],
    pub creation_year: i16,
    pub creation_month: u8,
    pub creation_day: u8,
    pub creation_hour: u8,
    pub creation_min: u8,
    pub creation_sec: u8,
    pub creation_qmsec: u8,
    pub generation_uid: [u8; MXFUSE_UL_LEN],
    pub material_package_uid: [u8; MXFUSE_UMID_LEN],
    pub file_source_package_uid: [u8; MXFUSE_UMID_LEN],
}

impl Default for MxfuseClipOptions {
    fn default() -> Self {
        Self {
            flags: 0,
            start_timecode: MxfuseTimecode::default(),
            timecode_track: 1,
            system_item: 0,
            company_name: [0; MXFUSE_NAME_LEN],
            product_name: [0; MXFUSE_NAME_LEN],
            version_string: [0; MXFUSE_VERSION_LEN],
            product_version: [0; 5],
            product_uid: [0; MXFUSE_UL_LEN],
            creation_year: 0,
            creation_month: 0,
            creation_day: 0,
            creation_hour: 0,
            creation_min: 0,
            creation_sec: 0,
            creation_qmsec: 0,
            generation_uid: [0; MXFUSE_UL_LEN],
            material_package_uid: [0; MXFUSE_UMID_LEN],
            file_source_package_uid: [0; MXFUSE_UMID_LEN],
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
    pub has_start_timecode: c_int,
    pub start_timecode: MxfuseTimecode,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MxfuseTrackInfo {
    pub index: usize,
    pub data_def: i32,
    pub essence_type: i32,
    pub essence_container_ul: [u8; MXFUSE_UL_LEN],
    pub coding_ul: [u8; MXFUSE_UL_LEN],
    pub descriptor_kind: i32,
    pub stored_width: u32,
    pub stored_height: u32,
    pub display_width: u32,
    pub display_height: u32,
    pub component_depth: u32,
    pub horiz_subsampling: u32,
    pub vert_subsampling: u32,
    pub frame_layout: u8,
    pub aspect_ratio: MxfuseRational,
    pub video_line_map: [i32; 2],
    pub pixel_layout: [u8; MXFUSE_PIXEL_LAYOUT_LEN],
    pub pixel_layout_count: u8,
    pub color_primaries: [u8; MXFUSE_UL_LEN],
    pub transfer_characteristic: [u8; MXFUSE_UL_LEN],
    pub coding_equations: [u8; MXFUSE_UL_LEN],
    pub sampling_rate: u32,
    pub channel_count: u32,
    pub quantization_bits: u32,
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
            coding_ul: [0; MXFUSE_UL_LEN],
            descriptor_kind: 0,
            stored_width: 0,
            stored_height: 0,
            display_width: 0,
            display_height: 0,
            component_depth: 0,
            horiz_subsampling: 0,
            vert_subsampling: 0,
            frame_layout: 0,
            aspect_ratio: MxfuseRational::default(),
            video_line_map: [0; 2],
            pixel_layout: [0; MXFUSE_PIXEL_LAYOUT_LEN],
            pixel_layout_count: 0,
            color_primaries: [0; MXFUSE_UL_LEN],
            transfer_characteristic: [0; MXFUSE_UL_LEN],
            coding_equations: [0; MXFUSE_UL_LEN],
            sampling_rate: 0,
            channel_count: 0,
            quantization_bits: 0,
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

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MxfuseXmlView {
    pub data: *mut u8,
    pub size: u32,
    pub scheme_id: [u8; MXFUSE_UL_LEN],
    pub language: [c_char; MXFUSE_XML_LANG_LEN],
    pub mime_type: [c_char; MXFUSE_XML_MIME_LEN],
    pub ns: [c_char; MXFUSE_XML_NS_LEN],
    pub is_xml: c_int,
}

impl Default for MxfuseXmlView {
    fn default() -> Self {
        Self {
            data: std::ptr::null_mut(),
            size: 0,
            scheme_id: [0; MXFUSE_UL_LEN],
            language: [0; MXFUSE_XML_LANG_LEN],
            mime_type: [0; MXFUSE_XML_MIME_LEN],
            ns: [0; MXFUSE_XML_NS_LEN],
            is_xml: 0,
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

    pub fn mxfuse_reader_num_xml(
        reader: *mut MxfuseReader,
        out: *mut usize,
        err: *mut MxfuseError,
    ) -> c_int;

    pub fn mxfuse_reader_xml(
        reader: *mut MxfuseReader,
        index: usize,
        out: *mut MxfuseXmlView,
        err: *mut MxfuseError,
    ) -> c_int;

    pub fn mxfuse_xml_free(view: *mut MxfuseXmlView);

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

    pub fn mxfuse_writer_configure(
        writer: *mut MxfuseWriter,
        options: *const MxfuseClipOptions,
        err: *mut MxfuseError,
    ) -> c_int;

    pub fn mxfuse_writer_create_track(
        writer: *mut MxfuseWriter,
        spec: *const MxfuseTrackSpec,
        out_index: *mut u32,
        err: *mut MxfuseError,
    ) -> c_int;

    pub fn mxfuse_writer_add_xml(
        writer: *mut MxfuseWriter,
        data: *const u8,
        size: u32,
        scheme_id: *const u8,
        language: *const c_char,
        ns: *const c_char,
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
