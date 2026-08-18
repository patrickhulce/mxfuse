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

/// A track description captured at open time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Track {
    pub index: usize,
    pub kind: TrackKind,
    pub essence_type: EssenceType,
    pub essence_container_ul: [u8; 16],
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
    pub picture_coding_ul: Option<[u8; 16]>,
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
            picture_coding_ul: None,
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
}
