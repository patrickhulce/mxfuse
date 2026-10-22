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

/// Writer Identification and package UIDs.
///
/// Creation date, generation UID, and package UMIDs that are `None` keep
/// bmx's generated values. Company/product/version/product UID are applied
/// as one `SetProductInfo` call: if any of those is set, the rest of that
/// group is written as empty / null rather than left as bmx defaults.
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

/// RP2057 / bmx text encoding. Wrapping (inline vs Generic Stream) follows the
/// encoding detected from the payload, not this field.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(i32)]
pub enum TextEncoding {
    Unknown = 0,
    Utf16 = 1,
    #[default]
    Utf8 = 2,
}

impl TextEncoding {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Utf16,
            2 => Self::Utf8,
            _ => Self::Unknown,
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

/// RP2057 scheme identity: a 16-byte UL, or a name/URN hashed to a UUID v3 UL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XmlScheme {
    Ul([u8; 16]),
    Name(String),
}

impl XmlScheme {
    pub fn from_ul(ul: [u8; 16]) -> Self {
        Self::Ul(ul)
    }

    pub fn from_name(name: impl Into<String>) -> Self {
        Self::Name(name.into())
    }

    /// Parse a 32-char hex UL, otherwise treat the string as a scheme name.
    pub fn parse(value: &str) -> Self {
        match parse_hex_ul(value) {
            Some(ul) => Self::Ul(ul),
            None => Self::Name(value.to_string()),
        }
    }

    /// The UL stored in MXF. Names are hashed with bmx's UUID v3 + half-swap.
    pub fn to_ul(&self) -> crate::error::Result<[u8; 16]> {
        match self {
            Self::Ul(ul) => Ok(*ul),
            Self::Name(name) => scheme_ul_from_name(name),
        }
    }
}

impl From<[u8; 16]> for XmlScheme {
    fn from(ul: [u8; 16]) -> Self {
        Self::Ul(ul)
    }
}

impl From<&str> for XmlScheme {
    fn from(value: &str) -> Self {
        Self::parse(value)
    }
}

impl From<String> for XmlScheme {
    fn from(value: String) -> Self {
        Self::parse(&value)
    }
}

fn parse_hex_ul(value: &str) -> Option<[u8; 16]> {
    if value.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn scheme_ul_from_name(name: &str) -> crate::error::Result<[u8; 16]> {
    use std::ffi::CString;

    use mxfuse_sys::{mxfuse_xml_scheme_from_name, MxfuseError, MXFUSE_OK};

    let c_name =
        CString::new(name).map_err(|_| crate::error::Error::new("xml scheme contains NUL"))?;
    let mut out = [0u8; 16];
    let mut err = MxfuseError::default();
    let status =
        unsafe { mxfuse_xml_scheme_from_name(c_name.as_ptr(), out.as_mut_ptr(), &mut err) };
    if status != MXFUSE_OK {
        return Err(crate::error::Error::from_shim(&err));
    }
    Ok(out)
}

/// Clip-level XML (ST 434 / generic stream), not an essence track.
///
/// `scheme_id` is a URN/name or a 16-byte UL on write. Read always returns
/// [`XmlScheme::Ul`]. `mime_type` and `is_xml` are populated on read. On write,
/// the payload is stored as XML; `mime_type` is ignored. Payloads must not start
/// with a BOM; inline vs Generic Stream follows detected encoding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlMetadata {
    pub data: Vec<u8>,
    pub scheme_id: Option<XmlScheme>,
    pub language: Option<String>,
    pub namespace: Option<String>,
    pub mime_type: Option<String>,
    pub is_xml: bool,
    pub text_encoding: TextEncoding,
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
            text_encoding: TextEncoding::Utf8,
        }
    }

    /// True when `scheme` hashes to (or equals) the stored 16-byte UL.
    pub fn is_scheme_match(scheme: impl Into<XmlScheme>, ul: &[u8; 16]) -> bool {
        scheme
            .into()
            .to_ul()
            .ok()
            .is_some_and(|expected| expected == *ul)
    }

    /// True when this object's scheme identity matches `scheme`.
    pub fn matches_scheme(&self, scheme: impl Into<XmlScheme>) -> bool {
        let Some(stored) = self.scheme_id.as_ref().and_then(|item| item.to_ul().ok()) else {
            return false;
        };
        Self::is_scheme_match(scheme, &stored)
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
    /// MaterialPackage name (the clip name). Empty or `None` leaves it unset.
    pub clip_name: Option<String>,
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
            clip_name: None,
        }
    }
}
