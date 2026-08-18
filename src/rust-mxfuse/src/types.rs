use std::fmt;

/// A rational edit rate (`numerator / denominator`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rational {
    pub num: i32,
    pub den: i32,
}

impl fmt::Display for Rational {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.num, self.den)
    }
}

/// High-level track classification from libMXF's `MXFDataDefEnum`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackKind {
    Picture,
    Sound,
    Data,
    Other,
}

impl TrackKind {
    pub fn from_data_def(data_def: i32) -> Self {
        match data_def {
            1 => Self::Picture,
            2 => Self::Sound,
            4 => Self::Data,
            _ => Self::Other,
        }
    }
}

/// bmx `EssenceType` value. Use [`EssenceType::name`] for the C++ enumerator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EssenceType(pub i32);

impl EssenceType {
    pub const UNKNOWN: Self = Self(0);
    pub const UNC_HD_1080P: Self = Self(35);
    pub const WAVE_PCM: Self = Self(90);
    pub const OPAQUE_PICTURE: Self = Self(97);
    pub const OPAQUE_SOUND: Self = Self(98);
    pub const OPAQUE_DATA: Self = Self(99);

    pub fn from_i32(value: i32) -> Self {
        Self(value)
    }

    pub fn as_i32(self) -> i32 {
        self.0
    }

    pub fn name(self) -> &'static str {
        let ptr = unsafe { mxfuse_sys::mxfuse_essence_type_name(self.0) };
        if ptr.is_null() {
            return "UNKNOWN_ESSENCE_TYPE";
        }
        unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_str()
            .unwrap_or("UNKNOWN_ESSENCE_TYPE")
    }
}

impl fmt::Display for EssenceType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// File descriptor class used for an opaque track.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DescriptorKind {
    Default = 0,
    Cdci = 1,
    Rgba = 2,
    WaveAudio = 3,
    GenericData = 4,
}

impl DescriptorKind {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Cdci,
            2 => Self::Rgba,
            3 => Self::WaveAudio,
            4 => Self::GenericData,
            _ => Self::Default,
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

/// One component of an RGBA `PixelLayout`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PixelComponent {
    pub code: u8,
    pub depth: u8,
}

/// Material Package start timecode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timecode {
    pub hour: i16,
    pub minute: i16,
    pub second: i16,
    pub frame: i16,
    pub drop_frame: bool,
}

/// Writer Identification and package UIDs. Unset fields keep bmx's generated values.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Identity {
    pub company_name: Option<String>,
    pub product_name: Option<String>,
    pub version_string: Option<String>,
    pub product_version: Option<(u16, u16, u16, u16, u16)>,
    pub product_uid: Option<[u8; 16]>,
    pub creation_date: Option<(i16, u8, u8, u8, u8, u8, u8)>,
    pub generation_uid: Option<[u8; 16]>,
    pub material_package_uid: Option<[u8; 32]>,
    pub file_source_package_uid: Option<[u8; 32]>,
}

/// A track description captured at open time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Track {
    pub index: usize,
    pub kind: TrackKind,
    pub essence_type: EssenceType,
    pub essence_container_ul: [u8; 16],
    pub coding_ul: Option<[u8; 16]>,
    pub descriptor: DescriptorKind,
    pub stored_width: Option<u32>,
    pub stored_height: Option<u32>,
    pub display_width: Option<u32>,
    pub display_height: Option<u32>,
    pub component_depth: Option<u32>,
    pub subsampling: Option<(u32, u32)>,
    pub frame_layout: Option<u8>,
    pub aspect_ratio: Option<Rational>,
    pub video_line_map: Option<(i32, i32)>,
    pub pixel_layout: Vec<PixelComponent>,
    pub color_primaries: Option<[u8; 16]>,
    pub transfer_characteristic: Option<[u8; 16]>,
    pub coding_equations: Option<[u8; 16]>,
    pub sampling_rate: Option<u32>,
    pub channel_count: Option<u32>,
    pub quantization_bits: Option<u32>,
    pub edit_rate: Rational,
    pub duration: i64,
}

/// One essence payload with the KLV key and length stripped.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub data: Vec<u8>,
    pub element_key: [u8; 16],
    pub file_position: i64,
    pub kl_size: u8,
    pub position: i64,
    pub track_index: usize,
}

/// The frames belonging to one edit unit, across the selected tracks.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Package {
    pub frames: Vec<Frame>,
}

/// Tuning knobs for index-driven access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadOptions {
    /// Bytes pulled on a short read so tiny KLV-header fetches are amortized.
    pub read_ahead: u32,
    /// Paged LRU via `mxf_cache_file_open`. Zero disables the cache wrapper.
    pub cache_bytes: u32,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            read_ahead: 1 << 20,
            cache_bytes: 64 << 20,
        }
    }
}

/// OP1a flavour flags. `SINGLE_PASS` writes a closed-complete header and never
/// seeks backward.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Flavour(pub i32);

impl Flavour {
    pub const DEFAULT: Self = Self(0);
    pub const SINGLE_PASS: Self = Self(0x0008);
}

impl Default for Flavour {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One output track. Opaque types require container / coding ULs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackSpec {
    pub essence_type: EssenceType,
    pub sampling_rate: Option<u32>,
    pub channel_count: Option<u32>,
    pub quantization_bits: Option<u32>,
    pub stored_width: Option<u32>,
    pub stored_height: Option<u32>,
    pub essence_container_ul: Option<[u8; 16]>,
    pub coding_ul: Option<[u8; 16]>,
    pub element_type: Option<u8>,
    pub element_llen: Option<u8>,
    pub temporal_reordering: bool,
    pub descriptor: Option<DescriptorKind>,
    pub component_depth: Option<u32>,
    pub subsampling: Option<(u32, u32)>,
    pub frame_layout: Option<u8>,
    pub aspect_ratio: Option<Rational>,
    pub video_line_map: Option<(i32, i32)>,
    pub pixel_layout: Option<Vec<PixelComponent>>,
    pub color_primaries: Option<[u8; 16]>,
    pub transfer_characteristic: Option<[u8; 16]>,
    pub coding_equations: Option<[u8; 16]>,
}

impl TrackSpec {
    pub fn new(essence_type: EssenceType) -> Self {
        Self {
            essence_type,
            sampling_rate: None,
            channel_count: None,
            quantization_bits: None,
            stored_width: None,
            stored_height: None,
            essence_container_ul: None,
            coding_ul: None,
            element_type: None,
            element_llen: None,
            temporal_reordering: false,
            descriptor: None,
            component_depth: None,
            subsampling: None,
            frame_layout: None,
            aspect_ratio: None,
            video_line_map: None,
            pixel_layout: None,
            color_primaries: None,
            transfer_characteristic: None,
            coding_equations: None,
        }
    }
}

/// Clip-level XML (ST 434 / generic stream), not an essence track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlMetadata {
    pub data: Vec<u8>,
    pub scheme_id: Option<[u8; 16]>,
    pub language: Option<String>,
    pub namespace: Option<String>,
    pub mime_type: Option<String>,
    pub is_xml: bool,
}

impl XmlMetadata {
    pub fn new(data: impl Into<Vec<u8>>) -> Self {
        Self {
            data: data.into(),
            scheme_id: None,
            language: None,
            namespace: None,
            mime_type: None,
            is_xml: true,
        }
    }
}

/// Clip-level write specification. Duration is required for single-pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipSpec {
    pub edit_rate: Rational,
    pub flavour: Flavour,
    pub duration: Option<i64>,
    pub tracks: Vec<TrackSpec>,
    pub xml: Vec<XmlMetadata>,
    pub start_timecode: Option<Timecode>,
    pub timecode_track: bool,
    pub system_item: bool,
    pub identity: Option<Identity>,
}

impl Default for ClipSpec {
    fn default() -> Self {
        Self {
            edit_rate: Rational { num: 25, den: 1 },
            flavour: Flavour::DEFAULT,
            duration: None,
            tracks: Vec::new(),
            xml: Vec::new(),
            start_timecode: None,
            timecode_track: true,
            system_item: false,
            identity: None,
        }
    }
}
