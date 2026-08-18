use std::ffi::CString;
use std::io::SeekFrom;
use std::os::raw::{c_int, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use mxfuse_sys::{
    mxfuse_writer_add_xml, mxfuse_writer_complete, mxfuse_writer_configure,
    mxfuse_writer_create_track, mxfuse_writer_free, mxfuse_writer_open, mxfuse_writer_prepare,
    mxfuse_writer_write_samples, MxfuseByteSourceVtable, MxfuseClipOptions, MxfuseError,
    MxfuseRational, MxfuseTrackSpec, MxfuseWriter, MXFUSE_CLIP_HAS_COMPANY,
    MXFUSE_CLIP_HAS_CREATION_DATE, MXFUSE_CLIP_HAS_FILE_SOURCE_UID, MXFUSE_CLIP_HAS_GENERATION_UID,
    MXFUSE_CLIP_HAS_MATERIAL_UID, MXFUSE_CLIP_HAS_PRODUCT, MXFUSE_CLIP_HAS_PRODUCT_UID,
    MXFUSE_CLIP_HAS_PRODUCT_VERSION, MXFUSE_CLIP_HAS_START_TIMECODE, MXFUSE_CLIP_HAS_SYSTEM_ITEM,
    MXFUSE_CLIP_HAS_TIMECODE_TRACK, MXFUSE_CLIP_HAS_VERSION_STRING, MXFUSE_NAME_LEN, MXFUSE_OK,
    MXFUSE_TRACK_HAS_ASPECT_RATIO, MXFUSE_TRACK_HAS_CHANNEL_COUNT, MXFUSE_TRACK_HAS_CODING_EQ,
    MXFUSE_TRACK_HAS_CODING_UL, MXFUSE_TRACK_HAS_COLOR_PRIMARIES, MXFUSE_TRACK_HAS_COMPONENT_DEPTH,
    MXFUSE_TRACK_HAS_CONTAINER_UL, MXFUSE_TRACK_HAS_DESCRIPTOR, MXFUSE_TRACK_HAS_ELEMENT_LLEN,
    MXFUSE_TRACK_HAS_ELEMENT_TYPE, MXFUSE_TRACK_HAS_FRAME_LAYOUT, MXFUSE_TRACK_HAS_PIXEL_LAYOUT,
    MXFUSE_TRACK_HAS_QUANT_BITS, MXFUSE_TRACK_HAS_SAMPLING_RATE, MXFUSE_TRACK_HAS_STORED_HEIGHT,
    MXFUSE_TRACK_HAS_STORED_WIDTH, MXFUSE_TRACK_HAS_SUBSAMPLING, MXFUSE_TRACK_HAS_TEMPORAL_REORDER,
    MXFUSE_TRACK_HAS_TRANSFER, MXFUSE_TRACK_HAS_VIDEO_LINE_MAP, MXFUSE_VERSION_LEN,
};

use crate::error::{Error, Result};
use crate::source::ByteSink;
use crate::types::{ClipSpec, EssenceType, Identity, TrackSpec, XmlMetadata};

struct SinkBox {
    sink: Box<dyn ByteSink>,
}

unsafe extern "C" fn sink_read(_ctx: *mut c_void, _data: *mut u8, _count: u32) -> i32 {
    0
}

unsafe extern "C" fn sink_write(ctx: *mut c_void, data: *const u8, count: u32) -> i32 {
    if ctx.is_null() || data.is_null() {
        return -1;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let sink = &mut *(ctx as *mut SinkBox);
        let buf = std::slice::from_raw_parts(data, count as usize);
        let mut written = 0;
        while written < buf.len() {
            let n = sink.sink.write(&buf[written..])?;
            if n == 0 {
                break;
            }
            written += n;
        }
        Ok::<usize, std::io::Error>(written)
    }));
    match result {
        Ok(Ok(n)) => i32::try_from(n).unwrap_or(-1),
        _ => -1,
    }
}

unsafe extern "C" fn sink_seek(ctx: *mut c_void, offset: i64, whence: c_int) -> c_int {
    if ctx.is_null() {
        return 0;
    }
    let pos = match whence {
        0 => SeekFrom::Start(offset as u64),
        1 => SeekFrom::Current(offset),
        2 => SeekFrom::End(offset),
        _ => return 0,
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        let sink = &mut *(ctx as *mut SinkBox);
        sink.sink.seek(pos)
    }));
    match result {
        Ok(Ok(_)) => 1,
        _ => 0,
    }
}

unsafe extern "C" fn sink_tell(ctx: *mut c_void) -> i64 {
    if ctx.is_null() {
        return -1;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let sink = &mut *(ctx as *mut SinkBox);
        sink.sink.tell()
    }));
    match result {
        Ok(Ok(pos)) => pos as i64,
        _ => -1,
    }
}

unsafe extern "C" fn sink_size(_ctx: *mut c_void) -> i64 {
    -1
}

unsafe extern "C" fn sink_is_seekable(ctx: *mut c_void) -> c_int {
    if ctx.is_null() {
        return 0;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let sink = &*(ctx as *mut SinkBox);
        sink.sink.is_seekable()
    }));
    match result {
        Ok(true) => 1,
        _ => 0,
    }
}

unsafe extern "C" fn sink_close(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    drop(Box::from_raw(ctx as *mut SinkBox));
}

const VTABLE: MxfuseByteSourceVtable = MxfuseByteSourceVtable {
    read: Some(sink_read),
    write: Some(sink_write),
    seek: Some(sink_seek),
    tell: Some(sink_tell),
    size: Some(sink_size),
    is_seekable: Some(sink_is_seekable),
    close: Some(sink_close),
};

/// An opened MXF writer. One writer per thread; drop without [`ClipWriter::finish`]
/// frees the handle without calling CompleteWrite.
pub struct ClipWriter {
    raw: *mut MxfuseWriter,
    tracks: Vec<TrackWriteInfo>,
}

struct TrackWriteInfo {
    essence_type: EssenceType,
    bytes_per_sample: u32,
}

unsafe impl Send for ClipWriter {}

impl ClipWriter {
    /// Write one edit unit of essence for `track_index`.
    ///
    /// Picture and opaque tracks treat the buffer as one sample. `WAVE_PCM`
    /// treats it as `len / bytes_per_sample` samples.
    pub fn write(&mut self, track_index: usize, data: &[u8]) -> Result<()> {
        if self.raw.is_null() {
            return Err(Error::new("writer is closed"));
        }
        let info = self
            .tracks
            .get(track_index)
            .ok_or_else(|| Error::new("track index out of range"))?;
        let num_samples = if info.essence_type == EssenceType::WAVE_PCM {
            if info.bytes_per_sample == 0
                || !(data.len() as u32).is_multiple_of(info.bytes_per_sample)
            {
                return Err(Error::new(
                    "WAVE_PCM payload is not a multiple of sample size",
                ));
            }
            data.len() as u32 / info.bytes_per_sample
        } else {
            1
        };
        let size = u32::try_from(data.len()).map_err(|_| Error::new("payload exceeds u32"))?;
        let mut err = MxfuseError::default();
        let status = unsafe {
            mxfuse_writer_write_samples(
                self.raw,
                track_index as u32,
                data.as_ptr(),
                size,
                num_samples,
                &mut err,
            )
        };
        if status != MXFUSE_OK {
            return Err(Error::from_shim(&err));
        }
        Ok(())
    }

    /// Finalize the file. Consumes the writer.
    pub fn finish(mut self) -> Result<()> {
        self.complete_write()
    }

    fn complete_write(&mut self) -> Result<()> {
        if self.raw.is_null() {
            return Err(Error::new("writer is closed"));
        }
        let mut err = MxfuseError::default();
        if unsafe { mxfuse_writer_complete(self.raw, &mut err) } != MXFUSE_OK {
            return Err(Error::from_shim(&err));
        }
        Ok(())
    }
}

impl Drop for ClipWriter {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe { mxfuse_writer_free(self.raw) };
            self.raw = std::ptr::null_mut();
        }
    }
}

/// Open an MXF writer. The sink is owned by the returned writer.
pub fn write_mxf<S: ByteSink + 'static>(sink: S, spec: ClipSpec) -> Result<ClipWriter> {
    if spec.tracks.is_empty() {
        return Err(Error::new("ClipSpec.tracks must not be empty"));
    }
    let ctx = Box::into_raw(Box::new(SinkBox {
        sink: Box::new(sink),
    }));
    let duration = spec.duration.unwrap_or(-1);
    let mut raw = std::ptr::null_mut();
    let mut err = MxfuseError::default();
    let status = unsafe {
        mxfuse_writer_open(
            &VTABLE,
            ctx as *mut c_void,
            spec.flavour.0,
            MxfuseRational {
                num: spec.edit_rate.num,
                den: spec.edit_rate.den,
            },
            duration,
            &mut raw,
            &mut err,
        )
    };
    if status != MXFUSE_OK {
        return Err(Error::from_shim(&err));
    }

    if let Err(error) = configure_writer(raw, &spec) {
        unsafe { mxfuse_writer_free(raw) };
        return Err(error);
    }

    let mut tracks = Vec::with_capacity(spec.tracks.len());
    for track in &spec.tracks {
        let mut out_index = 0u32;
        let mut err = MxfuseError::default();
        let c_spec = track_spec_to_c(track);
        if unsafe { mxfuse_writer_create_track(raw, &c_spec, &mut out_index, &mut err) }
            != MXFUSE_OK
        {
            unsafe { mxfuse_writer_free(raw) };
            return Err(Error::from_shim(&err));
        }
        tracks.push(TrackWriteInfo {
            essence_type: track.essence_type,
            bytes_per_sample: pcm_bytes_per_sample(track),
        });
    }

    for item in &spec.xml {
        if let Err(error) = add_xml(raw, item) {
            unsafe { mxfuse_writer_free(raw) };
            return Err(error);
        }
    }

    let mut err = MxfuseError::default();
    if unsafe { mxfuse_writer_prepare(raw, &mut err) } != MXFUSE_OK {
        unsafe { mxfuse_writer_free(raw) };
        return Err(Error::from_shim(&err));
    }

    Ok(ClipWriter { raw, tracks })
}

fn add_xml(raw: *mut MxfuseWriter, item: &XmlMetadata) -> Result<()> {
    if item.data.is_empty() {
        return Err(Error::new("xml payload must not be empty"));
    }
    let size = u32::try_from(item.data.len()).map_err(|_| Error::new("xml payload exceeds u32"))?;
    let language = match item.language.as_deref() {
        Some(value) => {
            Some(CString::new(value).map_err(|_| Error::new("xml language contains NUL"))?)
        }
        None => None,
    };
    let namespace = match item.namespace.as_deref() {
        Some(value) => {
            Some(CString::new(value).map_err(|_| Error::new("xml namespace contains NUL"))?)
        }
        None => None,
    };
    let mut err = MxfuseError::default();
    let status = unsafe {
        mxfuse_writer_add_xml(
            raw,
            item.data.as_ptr(),
            size,
            item.scheme_id
                .as_ref()
                .map_or(std::ptr::null(), |ul| ul.as_ptr()),
            language
                .as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr()),
            namespace
                .as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr()),
            &mut err,
        )
    };
    if status != MXFUSE_OK {
        return Err(Error::from_shim(&err));
    }
    Ok(())
}

fn configure_writer(raw: *mut MxfuseWriter, spec: &ClipSpec) -> Result<()> {
    let mut options = MxfuseClipOptions::default();
    if let Some(timecode) = spec.start_timecode {
        options.flags |= MXFUSE_CLIP_HAS_START_TIMECODE;
        options.start_timecode.hour = timecode.hour;
        options.start_timecode.minute = timecode.minute;
        options.start_timecode.second = timecode.second;
        options.start_timecode.frame = timecode.frame;
        options.start_timecode.drop_frame = i32::from(timecode.drop_frame);
    }
    options.flags |= MXFUSE_CLIP_HAS_TIMECODE_TRACK;
    options.timecode_track = i32::from(spec.timecode_track);
    if spec.system_item {
        options.flags |= MXFUSE_CLIP_HAS_SYSTEM_ITEM;
        options.system_item = 1;
    }
    if let Some(identity) = &spec.identity {
        apply_identity(&mut options, identity)?;
    }
    if options.flags == 0 {
        return Ok(());
    }
    let mut err = MxfuseError::default();
    if unsafe { mxfuse_writer_configure(raw, &options, &mut err) } != MXFUSE_OK {
        return Err(Error::from_shim(&err));
    }
    Ok(())
}

fn apply_identity(options: &mut MxfuseClipOptions, identity: &Identity) -> Result<()> {
    if let Some(name) = &identity.company_name {
        options.flags |= MXFUSE_CLIP_HAS_COMPANY;
        copy_c_array(&mut options.company_name, name, "company_name")?;
    }
    if let Some(name) = &identity.product_name {
        options.flags |= MXFUSE_CLIP_HAS_PRODUCT;
        copy_c_array(&mut options.product_name, name, "product_name")?;
    }
    if let Some(version) = &identity.version_string {
        options.flags |= MXFUSE_CLIP_HAS_VERSION_STRING;
        copy_c_array(&mut options.version_string, version, "version_string")?;
    }
    if let Some(version) = identity.product_version {
        options.flags |= MXFUSE_CLIP_HAS_PRODUCT_VERSION;
        options.product_version = [version.0, version.1, version.2, version.3, version.4];
    }
    if let Some(uid) = identity.product_uid {
        options.flags |= MXFUSE_CLIP_HAS_PRODUCT_UID;
        options.product_uid = uid;
    }
    if let Some(date) = identity.creation_date {
        options.flags |= MXFUSE_CLIP_HAS_CREATION_DATE;
        options.creation_year = date.0;
        options.creation_month = date.1;
        options.creation_day = date.2;
        options.creation_hour = date.3;
        options.creation_min = date.4;
        options.creation_sec = date.5;
        options.creation_qmsec = date.6;
    }
    if let Some(uid) = identity.generation_uid {
        options.flags |= MXFUSE_CLIP_HAS_GENERATION_UID;
        options.generation_uid = uid;
    }
    if let Some(uid) = identity.material_package_uid {
        options.flags |= MXFUSE_CLIP_HAS_MATERIAL_UID;
        options.material_package_uid = uid;
    }
    if let Some(uid) = identity.file_source_package_uid {
        options.flags |= MXFUSE_CLIP_HAS_FILE_SOURCE_UID;
        options.file_source_package_uid = uid;
    }
    let _ = (MXFUSE_NAME_LEN, MXFUSE_VERSION_LEN);
    Ok(())
}

fn copy_c_array(dest: &mut [std::os::raw::c_char], value: &str, name: &str) -> Result<()> {
    let bytes = value.as_bytes();
    if bytes.len() >= dest.len() {
        return Err(Error::new(format!(
            "{name} exceeds {} bytes",
            dest.len() - 1
        )));
    }
    for (index, byte) in bytes.iter().enumerate() {
        dest[index] = *byte as std::os::raw::c_char;
    }
    dest[bytes.len()] = 0;
    Ok(())
}

fn pcm_bytes_per_sample(track: &TrackSpec) -> u32 {
    if track.essence_type != EssenceType::WAVE_PCM {
        return 0;
    }
    let channels = track.channel_count.unwrap_or(1);
    let bits = track.quantization_bits.unwrap_or(16);
    channels * bits.div_ceil(8)
}

fn track_spec_to_c(track: &TrackSpec) -> MxfuseTrackSpec {
    let mut spec = MxfuseTrackSpec {
        essence_type: track.essence_type.as_i32(),
        ..MxfuseTrackSpec::default()
    };
    if let Some(rate) = track.sampling_rate {
        spec.flags |= MXFUSE_TRACK_HAS_SAMPLING_RATE;
        spec.sampling_rate = MxfuseRational {
            num: rate as i32,
            den: 1,
        };
    }
    if let Some(count) = track.channel_count {
        spec.flags |= MXFUSE_TRACK_HAS_CHANNEL_COUNT;
        spec.channel_count = count;
    }
    if let Some(bits) = track.quantization_bits {
        spec.flags |= MXFUSE_TRACK_HAS_QUANT_BITS;
        spec.quantization_bits = bits;
    }
    if let Some(width) = track.stored_width {
        spec.flags |= MXFUSE_TRACK_HAS_STORED_WIDTH;
        spec.stored_width = width;
    }
    if let Some(height) = track.stored_height {
        spec.flags |= MXFUSE_TRACK_HAS_STORED_HEIGHT;
        spec.stored_height = height;
    }
    if let Some(ul) = track.essence_container_ul {
        spec.flags |= MXFUSE_TRACK_HAS_CONTAINER_UL;
        spec.essence_container_ul = ul;
    }
    if let Some(ul) = track.coding_ul {
        spec.flags |= MXFUSE_TRACK_HAS_CODING_UL;
        spec.picture_coding_ul = ul;
    }
    if let Some(element_type) = track.element_type {
        spec.flags |= MXFUSE_TRACK_HAS_ELEMENT_TYPE;
        spec.element_type = element_type;
    }
    if let Some(llen) = track.element_llen {
        spec.flags |= MXFUSE_TRACK_HAS_ELEMENT_LLEN;
        spec.element_llen = llen;
    }
    if track.temporal_reordering {
        spec.flags |= MXFUSE_TRACK_HAS_TEMPORAL_REORDER;
        spec.temporal_reordering = 1;
    }
    if let Some(kind) = track.descriptor {
        spec.flags |= MXFUSE_TRACK_HAS_DESCRIPTOR;
        spec.descriptor_kind = kind.as_i32();
    }
    if let Some(depth) = track.component_depth {
        spec.flags |= MXFUSE_TRACK_HAS_COMPONENT_DEPTH;
        spec.component_depth = depth;
    }
    if let Some((horiz, vert)) = track.subsampling {
        spec.flags |= MXFUSE_TRACK_HAS_SUBSAMPLING;
        spec.horiz_subsampling = horiz;
        spec.vert_subsampling = vert;
    }
    if let Some(layout) = track.frame_layout {
        spec.flags |= MXFUSE_TRACK_HAS_FRAME_LAYOUT;
        spec.frame_layout = layout;
    }
    if let Some(ratio) = track.aspect_ratio {
        spec.flags |= MXFUSE_TRACK_HAS_ASPECT_RATIO;
        spec.aspect_ratio = MxfuseRational {
            num: ratio.num,
            den: ratio.den,
        };
    }
    if let Some((first, second)) = track.video_line_map {
        spec.flags |= MXFUSE_TRACK_HAS_VIDEO_LINE_MAP;
        spec.video_line_map = [first, second];
    }
    if let Some(layout) = &track.pixel_layout {
        spec.flags |= MXFUSE_TRACK_HAS_PIXEL_LAYOUT;
        spec.pixel_layout_count = u8::try_from(layout.len().min(8)).unwrap_or(8);
        for (index, component) in layout.iter().take(8).enumerate() {
            spec.pixel_layout[index * 2] = component.code;
            spec.pixel_layout[index * 2 + 1] = component.depth;
        }
    }
    if let Some(ul) = track.color_primaries {
        spec.flags |= MXFUSE_TRACK_HAS_COLOR_PRIMARIES;
        spec.color_primaries = ul;
    }
    if let Some(ul) = track.transfer_characteristic {
        spec.flags |= MXFUSE_TRACK_HAS_TRANSFER;
        spec.transfer_characteristic = ul;
    }
    if let Some(ul) = track.coding_equations {
        spec.flags |= MXFUSE_TRACK_HAS_CODING_EQ;
        spec.coding_equations = ul;
    }
    spec
}
