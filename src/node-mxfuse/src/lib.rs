use std::fs::File;
use std::io::{self, Cursor, SeekFrom};
use std::sync::{Arc, Mutex};

use mxfuse::{
    ByteSource, ClipSpec, DescriptorKind, EssenceType, Flavour, Identity, PixelComponent, Rational,
    ReadOptions, Timecode, TrackKind, TrackSpec, XmlMetadata,
};
use napi::bindgen_prelude::*;
use napi_derive::napi;

#[napi(object)]
pub struct NativeReadOptions {
    pub read_ahead: Option<u32>,
    pub cache_bytes: Option<u32>,
}

#[napi(object)]
pub struct NativePixelComponent {
    pub code: u32,
    pub depth: u32,
}

#[napi(object)]
pub struct NativeTimecode {
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
    pub frame: i32,
    pub drop_frame: bool,
}

#[napi(object)]
pub struct NativeTrack {
    pub index: u32,
    pub kind: String,
    pub essence_type: String,
    pub essence_container_ul: Buffer,
    pub coding_ul: Option<Buffer>,
    pub descriptor: i32,
    pub stored_width: Option<u32>,
    pub stored_height: Option<u32>,
    pub display_width: Option<u32>,
    pub display_height: Option<u32>,
    pub component_depth: Option<u32>,
    pub horiz_subsampling: Option<u32>,
    pub vert_subsampling: Option<u32>,
    pub frame_layout: Option<u32>,
    pub aspect_ratio_num: Option<i32>,
    pub aspect_ratio_den: Option<i32>,
    pub video_line_map: Option<Vec<i32>>,
    pub pixel_layout: Vec<NativePixelComponent>,
    pub color_primaries: Option<Buffer>,
    pub transfer_characteristic: Option<Buffer>,
    pub coding_equations: Option<Buffer>,
    pub sampling_rate: Option<u32>,
    pub channel_count: Option<u32>,
    pub quantization_bits: Option<u32>,
    pub edit_rate_num: i32,
    pub edit_rate_den: i32,
    pub duration: i64,
}

#[napi(object)]
pub struct NativeFrame {
    pub data: Buffer,
    pub element_key: Buffer,
    pub file_position: i64,
    pub kl_size: u8,
    pub position: i64,
    pub track_index: u32,
}

#[napi(object)]
pub struct NativeXmlMetadata {
    pub data: Buffer,
    pub scheme_id: Option<Buffer>,
    pub language: Option<String>,
    pub namespace: Option<String>,
    pub mime_type: Option<String>,
    pub is_xml: bool,
}

#[napi(object)]
pub struct NativePackage {
    pub frames: Vec<NativeFrame>,
}

#[napi(object)]
pub struct NativeClipInfo {
    pub edit_rate_num: i32,
    pub edit_rate_den: i32,
    pub duration: i64,
    pub tracks: Vec<NativeTrack>,
    pub start_timecode: Option<NativeTimecode>,
}

struct BufferSource {
    cursor: Cursor<Vec<u8>>,
}

impl ByteSource for BufferSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        std::io::Read::read(&mut self.cursor, buf)
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        std::io::Seek::seek(&mut self.cursor, pos)
    }

    fn size(&mut self) -> io::Result<u64> {
        Ok(self.cursor.get_ref().len() as u64)
    }
}

#[napi]
pub struct NativeClip {
    inner: Arc<Mutex<Option<mxfuse::Clip>>>,
}

impl NativeClip {
    async fn with_clip<T, F>(&self, func: F) -> napi::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut mxfuse::Clip) -> mxfuse::Result<T> + Send + 'static,
    {
        let inner = Arc::clone(&self.inner);
        napi::tokio::task::spawn_blocking(move || {
            let mut guard = inner
                .lock()
                .map_err(|_| Error::from_reason("clip lock poisoned"))?;
            let clip = guard
                .as_mut()
                .ok_or_else(|| Error::from_reason("clip is closed"))?;
            func(clip).map_err(|error| Error::from_reason(error.to_string()))
        })
        .await
        .map_err(|error| Error::from_reason(error.to_string()))?
    }
}

#[napi]
impl NativeClip {
    #[napi]
    pub async fn info(&self) -> napi::Result<NativeClipInfo> {
        self.with_clip(|clip| {
            Ok(NativeClipInfo {
                edit_rate_num: clip.edit_rate().num,
                edit_rate_den: clip.edit_rate().den,
                duration: clip.duration(),
                tracks: clip.tracks().iter().map(native_track_from).collect(),
                start_timecode: clip.start_timecode().map(|tc| NativeTimecode {
                    hour: i32::from(tc.hour),
                    minute: i32::from(tc.minute),
                    second: i32::from(tc.second),
                    frame: i32::from(tc.frame),
                    drop_frame: tc.drop_frame,
                }),
            })
        })
        .await
    }

    #[napi]
    pub async fn select(&self, indexes: Vec<u32>) -> napi::Result<()> {
        self.with_clip(move |clip| {
            let selected: Vec<mxfuse::Track> = clip
                .tracks()
                .iter()
                .filter(|track| indexes.contains(&(track.index as u32)))
                .cloned()
                .collect();
            clip.select(selected.iter())
        })
        .await
    }

    #[napi]
    pub async fn seek(&self, position: i64) -> napi::Result<()> {
        self.with_clip(move |clip| clip.seek(position)).await
    }

    #[napi]
    pub async fn read(&self, count: u32) -> napi::Result<Vec<NativePackage>> {
        self.with_clip(move |clip| {
            let packages = clip.read(count)?;
            Ok(packages
                .into_iter()
                .map(|package| NativePackage {
                    frames: package
                        .frames
                        .into_iter()
                        .map(|frame| NativeFrame {
                            data: Buffer::from(frame.data),
                            element_key: Buffer::from(frame.element_key.to_vec()),
                            file_position: frame.file_position,
                            kl_size: frame.kl_size,
                            position: frame.position,
                            track_index: frame.track_index as u32,
                        })
                        .collect(),
                })
                .collect())
        })
        .await
    }

    #[napi]
    pub async fn xml(&self) -> napi::Result<Vec<NativeXmlMetadata>> {
        self.with_clip(|clip| Ok(clip.xml().iter().map(xml_to_native).collect()))
            .await
    }

    #[napi]
    pub async fn close(&self) -> napi::Result<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| Error::from_reason("clip lock poisoned"))?;
        *guard = None;
        Ok(())
    }
}

#[napi]
pub async fn open_mxf_from_buffer(
    data: Buffer,
    options: Option<NativeReadOptions>,
) -> napi::Result<NativeClip> {
    let source = BufferSource {
        cursor: Cursor::new(data.to_vec()),
    };
    open_native(source, options)
}

#[napi]
pub async fn open_mxf_from_path(
    path: String,
    options: Option<NativeReadOptions>,
) -> napi::Result<NativeClip> {
    let file = File::open(&path).map_err(|error| Error::from_reason(error.to_string()))?;
    open_native(file, options)
}

fn open_native<S: ByteSource + 'static>(
    source: S,
    options: Option<NativeReadOptions>,
) -> napi::Result<NativeClip> {
    let options = options.unwrap_or(NativeReadOptions {
        read_ahead: None,
        cache_bytes: None,
    });
    let chosen = ReadOptions {
        read_ahead: options.read_ahead.unwrap_or(1 << 20),
        cache_bytes: options.cache_bytes.unwrap_or(64 << 20),
    };
    let clip =
        mxfuse::open_mxf(source, chosen).map_err(|error| Error::from_reason(error.to_string()))?;
    Ok(NativeClip {
        inner: Arc::new(Mutex::new(Some(clip))),
    })
}

fn native_track_from(track: &mxfuse::Track) -> NativeTrack {
    NativeTrack {
        index: track.index as u32,
        kind: kind_name(track.kind).to_string(),
        essence_type: track.essence_type.name().to_string(),
        essence_container_ul: Buffer::from(track.essence_container_ul.to_vec()),
        coding_ul: track.coding_ul.map(|ul| Buffer::from(ul.to_vec())),
        descriptor: track.descriptor.as_i32(),
        stored_width: track.stored_width,
        stored_height: track.stored_height,
        display_width: track.display_width,
        display_height: track.display_height,
        component_depth: track.component_depth,
        horiz_subsampling: track.subsampling.map(|pair| pair.0),
        vert_subsampling: track.subsampling.map(|pair| pair.1),
        frame_layout: track.frame_layout.map(u32::from),
        aspect_ratio_num: track.aspect_ratio.map(|ratio| ratio.num),
        aspect_ratio_den: track.aspect_ratio.map(|ratio| ratio.den),
        video_line_map: track
            .video_line_map
            .map(|(first, second)| vec![first, second]),
        pixel_layout: track
            .pixel_layout
            .iter()
            .map(|item| NativePixelComponent {
                code: u32::from(item.code),
                depth: u32::from(item.depth),
            })
            .collect(),
        color_primaries: track.color_primaries.map(|ul| Buffer::from(ul.to_vec())),
        transfer_characteristic: track
            .transfer_characteristic
            .map(|ul| Buffer::from(ul.to_vec())),
        coding_equations: track.coding_equations.map(|ul| Buffer::from(ul.to_vec())),
        sampling_rate: track.sampling_rate,
        channel_count: track.channel_count,
        quantization_bits: track.quantization_bits,
        edit_rate_num: track.edit_rate.num,
        edit_rate_den: track.edit_rate.den,
        duration: track.duration,
    }
}

fn kind_name(kind: TrackKind) -> &'static str {
    match kind {
        TrackKind::Picture => "picture",
        TrackKind::Sound => "sound",
        TrackKind::Data => "data",
        TrackKind::Other => "other",
    }
}

#[napi(object)]
pub struct NativeTrackSpec {
    pub essence_type: i32,
    pub sampling_rate: Option<u32>,
    pub channel_count: Option<u32>,
    pub quantization_bits: Option<u32>,
    pub stored_width: Option<u32>,
    pub stored_height: Option<u32>,
    pub essence_container_ul: Option<Buffer>,
    pub coding_ul: Option<Buffer>,
    pub element_type: Option<u32>,
    pub element_llen: Option<u32>,
    pub temporal_reordering: Option<bool>,
    pub descriptor: Option<i32>,
    pub component_depth: Option<u32>,
    pub horiz_subsampling: Option<u32>,
    pub vert_subsampling: Option<u32>,
    pub frame_layout: Option<u32>,
    pub aspect_ratio_num: Option<i32>,
    pub aspect_ratio_den: Option<i32>,
    pub video_line_map: Option<Vec<i32>>,
    pub pixel_layout: Option<Vec<NativePixelComponent>>,
    pub color_primaries: Option<Buffer>,
    pub transfer_characteristic: Option<Buffer>,
    pub coding_equations: Option<Buffer>,
}

#[napi(object)]
pub struct NativeClipSpec {
    pub edit_rate_num: i32,
    pub edit_rate_den: i32,
    pub flavour: Option<i32>,
    pub duration: Option<i64>,
    pub tracks: Vec<NativeTrackSpec>,
    pub xml: Option<Vec<NativeXmlMetadata>>,
    pub start_timecode: Option<NativeTimecode>,
    pub timecode_track: Option<bool>,
    pub system_item: Option<bool>,
    pub identity: Option<NativeIdentity>,
}

#[napi(object)]
pub struct NativeIdentity {
    pub company_name: Option<String>,
    pub product_name: Option<String>,
    pub version_string: Option<String>,
    pub product_version: Option<Vec<u32>>,
    pub product_uid: Option<Buffer>,
    pub creation_date: Option<Vec<i32>>,
    pub generation_uid: Option<Buffer>,
    pub material_package_uid: Option<Buffer>,
    pub file_source_package_uid: Option<Buffer>,
}

#[napi]
pub struct NativeWriter {
    inner: Arc<Mutex<Option<mxfuse::ClipWriter>>>,
}

impl NativeWriter {
    async fn with_writer<T, F>(&self, func: F) -> napi::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut mxfuse::ClipWriter) -> mxfuse::Result<T> + Send + 'static,
    {
        let inner = Arc::clone(&self.inner);
        napi::tokio::task::spawn_blocking(move || {
            let mut guard = inner
                .lock()
                .map_err(|_| Error::from_reason("writer lock poisoned"))?;
            let writer = guard
                .as_mut()
                .ok_or_else(|| Error::from_reason("writer is closed"))?;
            func(writer).map_err(|error| Error::from_reason(error.to_string()))
        })
        .await
        .map_err(|error| Error::from_reason(error.to_string()))?
    }
}

#[napi]
impl NativeWriter {
    #[napi]
    pub async fn write(&self, track_index: u32, data: Buffer) -> napi::Result<()> {
        let bytes = data.to_vec();
        self.with_writer(move |writer| writer.write(track_index as usize, &bytes))
            .await
    }

    #[napi]
    pub async fn finish(&self) -> napi::Result<()> {
        let inner = Arc::clone(&self.inner);
        napi::tokio::task::spawn_blocking(move || {
            let mut guard = inner
                .lock()
                .map_err(|_| Error::from_reason("writer lock poisoned"))?;
            let writer = guard
                .take()
                .ok_or_else(|| Error::from_reason("writer is closed"))?;
            writer
                .finish()
                .map_err(|error| Error::from_reason(error.to_string()))
        })
        .await
        .map_err(|error| Error::from_reason(error.to_string()))?
    }

    #[napi]
    pub async fn close(&self) -> napi::Result<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| Error::from_reason("writer lock poisoned"))?;
        *guard = None;
        Ok(())
    }
}

#[napi]
pub async fn write_mxf_to_path(path: String, spec: NativeClipSpec) -> napi::Result<NativeWriter> {
    let clip_spec = clip_spec_from_native(spec)?;
    napi::tokio::task::spawn_blocking(move || {
        let file = File::create(&path).map_err(|error| Error::from_reason(error.to_string()))?;
        let writer = mxfuse::write_mxf(file, clip_spec)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(NativeWriter {
            inner: Arc::new(Mutex::new(Some(writer))),
        })
    })
    .await
    .map_err(|error| Error::from_reason(error.to_string()))?
}

fn clip_spec_from_native(spec: NativeClipSpec) -> napi::Result<ClipSpec> {
    let mut tracks = Vec::with_capacity(spec.tracks.len());
    for track in spec.tracks {
        tracks.push(track_spec_from_native(track)?);
    }
    Ok(ClipSpec {
        edit_rate: Rational {
            num: spec.edit_rate_num,
            den: spec.edit_rate_den,
        },
        flavour: Flavour(spec.flavour.unwrap_or(0)),
        duration: spec.duration,
        tracks,
        xml: spec
            .xml
            .unwrap_or_default()
            .into_iter()
            .map(xml_from_native)
            .collect::<napi::Result<Vec<_>>>()?,
        start_timecode: spec.start_timecode.map(|tc| Timecode {
            hour: tc.hour as i16,
            minute: tc.minute as i16,
            second: tc.second as i16,
            frame: tc.frame as i16,
            drop_frame: tc.drop_frame,
        }),
        timecode_track: spec.timecode_track.unwrap_or(true),
        system_item: spec.system_item.unwrap_or(false),
        identity: spec.identity.map(identity_from_native).transpose()?,
    })
}

fn track_spec_from_native(track: NativeTrackSpec) -> napi::Result<TrackSpec> {
    let mut spec = TrackSpec::new(EssenceType::from_i32(track.essence_type));
    spec.sampling_rate = track.sampling_rate;
    spec.channel_count = track.channel_count;
    spec.quantization_bits = track.quantization_bits;
    spec.stored_width = track.stored_width;
    spec.stored_height = track.stored_height;
    spec.essence_container_ul = optional_ul(track.essence_container_ul, "essenceContainerUl")?;
    spec.coding_ul = optional_ul(track.coding_ul, "codingUl")?;
    spec.element_type = track.element_type.map(|value| value as u8);
    spec.element_llen = track.element_llen.map(|value| value as u8);
    spec.temporal_reordering = track.temporal_reordering.unwrap_or(false);
    spec.descriptor = track.descriptor.map(DescriptorKind::from_i32);
    spec.component_depth = track.component_depth;
    spec.subsampling = match (track.horiz_subsampling, track.vert_subsampling) {
        (Some(horiz), Some(vert)) => Some((horiz, vert)),
        _ => None,
    };
    spec.frame_layout = track.frame_layout.map(|value| value as u8);
    spec.aspect_ratio = match (track.aspect_ratio_num, track.aspect_ratio_den) {
        (Some(num), Some(den)) => Some(Rational { num, den }),
        _ => None,
    };
    spec.video_line_map = track.video_line_map.and_then(|values| {
        if values.len() >= 2 {
            Some((values[0], values[1]))
        } else {
            None
        }
    });
    spec.pixel_layout = track.pixel_layout.map(|items| {
        items
            .into_iter()
            .map(|item| PixelComponent {
                code: item.code as u8,
                depth: item.depth as u8,
            })
            .collect()
    });
    spec.color_primaries = optional_ul(track.color_primaries, "colorPrimaries")?;
    spec.transfer_characteristic =
        optional_ul(track.transfer_characteristic, "transferCharacteristic")?;
    spec.coding_equations = optional_ul(track.coding_equations, "codingEquations")?;
    Ok(spec)
}

fn identity_from_native(item: NativeIdentity) -> napi::Result<Identity> {
    Ok(Identity {
        company_name: item.company_name.filter(|value| !value.is_empty()),
        product_name: item.product_name.filter(|value| !value.is_empty()),
        version_string: item.version_string.filter(|value| !value.is_empty()),
        product_version: item.product_version.and_then(|values| {
            if values.len() >= 5 {
                Some((
                    values[0] as u16,
                    values[1] as u16,
                    values[2] as u16,
                    values[3] as u16,
                    values[4] as u16,
                ))
            } else {
                None
            }
        }),
        product_uid: optional_ul(item.product_uid, "productUid")?,
        creation_date: item.creation_date.and_then(|values| {
            if values.len() >= 7 {
                Some((
                    values[0] as i16,
                    values[1] as u8,
                    values[2] as u8,
                    values[3] as u8,
                    values[4] as u8,
                    values[5] as u8,
                    values[6] as u8,
                ))
            } else {
                None
            }
        }),
        generation_uid: optional_ul(item.generation_uid, "generationUid")?,
        material_package_uid: optional_umid(item.material_package_uid, "materialPackageUid")?,
        file_source_package_uid: optional_umid(
            item.file_source_package_uid,
            "fileSourcePackageUid",
        )?,
    })
}

fn optional_umid(value: Option<Buffer>, name: &str) -> napi::Result<Option<[u8; 32]>> {
    let Some(buffer) = value else {
        return Ok(None);
    };
    let bytes = buffer.to_vec();
    let array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Error::from_reason(format!("{name} must be 32 bytes")))?;
    Ok(Some(array))
}

fn xml_to_native(item: &XmlMetadata) -> NativeXmlMetadata {
    NativeXmlMetadata {
        data: Buffer::from(item.data.clone()),
        scheme_id: item.scheme_id.map(|ul| Buffer::from(ul.to_vec())),
        language: item.language.clone(),
        namespace: item.namespace.clone(),
        mime_type: item.mime_type.clone(),
        is_xml: item.is_xml,
    }
}

fn xml_from_native(item: NativeXmlMetadata) -> napi::Result<XmlMetadata> {
    Ok(XmlMetadata {
        data: item.data.to_vec(),
        scheme_id: optional_ul(item.scheme_id, "schemeId")?,
        language: item.language.filter(|value| !value.is_empty()),
        namespace: item.namespace.filter(|value| !value.is_empty()),
        mime_type: item.mime_type.filter(|value| !value.is_empty()),
        is_xml: item.is_xml,
    })
}

fn optional_ul(value: Option<Buffer>, name: &str) -> napi::Result<Option<[u8; 16]>> {
    let Some(buffer) = value else {
        return Ok(None);
    };
    let bytes = buffer.to_vec();
    let array: [u8; 16] = bytes
        .try_into()
        .map_err(|_| Error::from_reason(format!("{name} must be 16 bytes")))?;
    Ok(Some(array))
}
