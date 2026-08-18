use std::io::SeekFrom;
use std::os::raw::{c_char, c_int, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use mxfuse_sys::{
    mxfuse_frame_free, mxfuse_reader_clip_info, mxfuse_reader_free, mxfuse_reader_num_xml,
    mxfuse_reader_open, mxfuse_reader_pop_frame, mxfuse_reader_read, mxfuse_reader_seek,
    mxfuse_reader_set_enable, mxfuse_reader_track_info, mxfuse_reader_xml, mxfuse_xml_free,
    MxfuseByteSourceVtable, MxfuseClipInfo, MxfuseError, MxfuseFrameView, MxfuseReader,
    MxfuseTrackInfo, MxfuseXmlView, MXFUSE_ERR_NO_FRAME, MXFUSE_OK,
};

use crate::error::{Error, Result};
use crate::source::{ByteSource, ReadAhead};
use crate::types::{
    DescriptorKind, EssenceType, Frame, Package, PixelComponent, Rational, ReadOptions, Timecode,
    Track, TrackKind, XmlMetadata,
};

struct SourceBox {
    source: Box<dyn ByteSource>,
}

unsafe extern "C" fn src_read(ctx: *mut c_void, data: *mut u8, count: u32) -> i32 {
    if ctx.is_null() || data.is_null() {
        return -1;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let source = &mut *(ctx as *mut SourceBox);
        let buf = std::slice::from_raw_parts_mut(data, count as usize);
        source.source.read(buf)
    }));
    match result {
        Ok(Ok(n)) => i32::try_from(n).unwrap_or(-1),
        _ => -1,
    }
}

unsafe extern "C" fn src_seek(ctx: *mut c_void, offset: i64, whence: c_int) -> c_int {
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
        let source = &mut *(ctx as *mut SourceBox);
        source.source.seek(pos)
    }));
    match result {
        Ok(Ok(_)) => 1,
        _ => 0,
    }
}

unsafe extern "C" fn src_tell(ctx: *mut c_void) -> i64 {
    if ctx.is_null() {
        return -1;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let source = &mut *(ctx as *mut SourceBox);
        source.source.seek(SeekFrom::Current(0))
    }));
    match result {
        Ok(Ok(pos)) => pos as i64,
        _ => -1,
    }
}

unsafe extern "C" fn src_size(ctx: *mut c_void) -> i64 {
    if ctx.is_null() {
        return -1;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let source = &mut *(ctx as *mut SourceBox);
        source.source.size()
    }));
    match result {
        Ok(Ok(size)) => size as i64,
        _ => -1,
    }
}

unsafe extern "C" fn src_close(ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    drop(Box::from_raw(ctx as *mut SourceBox));
}

unsafe extern "C" fn src_write(_ctx: *mut c_void, _data: *const u8, _count: u32) -> i32 {
    -1
}

unsafe extern "C" fn src_is_seekable(_ctx: *mut c_void) -> c_int {
    1
}

const VTABLE: MxfuseByteSourceVtable = MxfuseByteSourceVtable {
    read: Some(src_read),
    write: Some(src_write),
    seek: Some(src_seek),
    tell: Some(src_tell),
    size: Some(src_size),
    is_seekable: Some(src_is_seekable),
    close: Some(src_close),
};

/// An opened MXF clip. One reader per thread; do not share across tasks.
pub struct Clip {
    raw: *mut MxfuseReader,
    tracks: Vec<Track>,
    edit_rate: Rational,
    duration: i64,
    start_timecode: Option<Timecode>,
    xml: Vec<XmlMetadata>,
}

unsafe impl Send for Clip {}

impl Clip {
    pub fn edit_rate(&self) -> Rational {
        self.edit_rate
    }

    pub fn duration(&self) -> i64 {
        self.duration
    }

    pub fn start_timecode(&self) -> Option<Timecode> {
        self.start_timecode
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn xml(&self) -> &[XmlMetadata] {
        &self.xml
    }

    /// Enable only the given tracks. Unselected tracks are never fetched.
    pub fn select<'a, I>(&mut self, tracks: I) -> Result<()>
    where
        I: IntoIterator<Item = &'a Track>,
    {
        let enabled: Vec<usize> = tracks.into_iter().map(|track| track.index).collect();
        for track in &self.tracks {
            let on = enabled.contains(&track.index);
            let mut err = MxfuseError::default();
            if unsafe { mxfuse_reader_set_enable(self.raw, track.index, i32::from(on), &mut err) }
                != MXFUSE_OK
            {
                return Err(Error::from_shim(&err));
            }
        }
        Ok(())
    }

    pub fn seek(&mut self, position: i64) -> Result<()> {
        let mut err = MxfuseError::default();
        if unsafe { mxfuse_reader_seek(self.raw, position, &mut err) } != MXFUSE_OK {
            return Err(Error::from_shim(&err));
        }
        Ok(())
    }

    /// Read `count` edit units and return one [`Package`] per unit.
    ///
    /// Each call asks bmx for a single sample. Passing `count > 1` to bmx
    /// concatenates those samples into one payload, so this loops instead.
    pub fn read(&mut self, count: u32) -> Result<Vec<Package>> {
        let mut packages = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let mut err = MxfuseError::default();
            let mut n = 0u32;
            let status = unsafe { mxfuse_reader_read(self.raw, 1, &mut n, &mut err) };
            if status != MXFUSE_OK {
                return Err(Error::from_shim(&err));
            }
            if n == 0 {
                break;
            }
            let mut package = Package::default();
            for track in &self.tracks {
                if let Some(frame) = pop_frame(self.raw, track.index)? {
                    package.frames.push(frame);
                }
            }
            packages.push(package);
        }
        Ok(packages)
    }
}

impl Drop for Clip {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe { mxfuse_reader_free(self.raw) };
            self.raw = std::ptr::null_mut();
        }
    }
}

/// Open an MXF source. The source is owned by the returned clip.
pub fn open_mxf<S: ByteSource + 'static>(source: S, options: ReadOptions) -> Result<Clip> {
    let wrapped: Box<dyn ByteSource> = if options.read_ahead == 0 {
        Box::new(source)
    } else {
        Box::new(ReadAhead::new(source, options.read_ahead as usize)?)
    };
    let ctx = Box::into_raw(Box::new(SourceBox { source: wrapped }));
    let mut raw = std::ptr::null_mut();
    let mut err = MxfuseError::default();
    let status = unsafe {
        mxfuse_reader_open(
            &VTABLE,
            ctx as *mut c_void,
            options.cache_bytes,
            &mut raw,
            &mut err,
        )
    };
    if status != MXFUSE_OK {
        return Err(Error::from_shim(&err));
    }

    let mut info = MxfuseClipInfo::default();
    let mut err = MxfuseError::default();
    if unsafe { mxfuse_reader_clip_info(raw, &mut info, &mut err) } != MXFUSE_OK {
        unsafe { mxfuse_reader_free(raw) };
        return Err(Error::from_shim(&err));
    }

    let mut tracks = Vec::with_capacity(info.num_tracks);
    for index in 0..info.num_tracks {
        let mut track_info = MxfuseTrackInfo::default();
        let mut err = MxfuseError::default();
        if unsafe { mxfuse_reader_track_info(raw, index, &mut track_info, &mut err) } != MXFUSE_OK {
            unsafe { mxfuse_reader_free(raw) };
            return Err(Error::from_shim(&err));
        }
        tracks.push(track_from_info(&track_info));
    }

    let mut xml = Vec::new();
    let mut xml_count = 0usize;
    let mut err = MxfuseError::default();
    if unsafe { mxfuse_reader_num_xml(raw, &mut xml_count, &mut err) } != MXFUSE_OK {
        unsafe { mxfuse_reader_free(raw) };
        return Err(Error::from_shim(&err));
    }
    for index in 0..xml_count {
        match pop_xml(raw, index) {
            Ok(item) => xml.push(item),
            Err(error) => {
                unsafe { mxfuse_reader_free(raw) };
                return Err(error);
            }
        }
    }

    Ok(Clip {
        raw,
        tracks,
        edit_rate: Rational {
            num: info.edit_rate.num,
            den: info.edit_rate.den,
        },
        duration: info.duration,
        start_timecode: if info.has_start_timecode != 0 {
            Some(Timecode {
                hour: info.start_timecode.hour,
                minute: info.start_timecode.minute,
                second: info.start_timecode.second,
                frame: info.start_timecode.frame,
                drop_frame: info.start_timecode.drop_frame != 0,
            })
        } else {
            None
        },
        xml,
    })
}

fn optional_ul(bytes: &[u8; 16]) -> Option<[u8; 16]> {
    if bytes.iter().all(|byte| *byte == 0) {
        None
    } else {
        Some(*bytes)
    }
}

fn optional_dim(value: u32) -> Option<u32> {
    if value == 0 {
        None
    } else {
        Some(value)
    }
}

fn track_from_info(info: &MxfuseTrackInfo) -> Track {
    let mut pixel_layout = Vec::new();
    for index in 0..info.pixel_layout_count as usize {
        pixel_layout.push(PixelComponent {
            code: info.pixel_layout[index * 2],
            depth: info.pixel_layout[index * 2 + 1],
        });
    }
    Track {
        index: info.index,
        kind: TrackKind::from_data_def(info.data_def),
        essence_type: EssenceType::from_i32(info.essence_type),
        essence_container_ul: info.essence_container_ul,
        coding_ul: optional_ul(&info.coding_ul),
        descriptor: DescriptorKind::from_i32(info.descriptor_kind),
        stored_width: optional_dim(info.stored_width),
        stored_height: optional_dim(info.stored_height),
        display_width: optional_dim(info.display_width),
        display_height: optional_dim(info.display_height),
        component_depth: optional_dim(info.component_depth),
        subsampling: if info.horiz_subsampling == 0 && info.vert_subsampling == 0 {
            None
        } else {
            Some((info.horiz_subsampling, info.vert_subsampling))
        },
        frame_layout: if info.stored_width == 0 && info.stored_height == 0 {
            None
        } else {
            Some(info.frame_layout)
        },
        aspect_ratio: if info.aspect_ratio.num == 0 {
            None
        } else {
            Some(Rational {
                num: info.aspect_ratio.num,
                den: info.aspect_ratio.den,
            })
        },
        video_line_map: if info.video_line_map[0] == 0 && info.video_line_map[1] == 0 {
            None
        } else {
            Some((info.video_line_map[0], info.video_line_map[1]))
        },
        pixel_layout,
        color_primaries: optional_ul(&info.color_primaries),
        transfer_characteristic: optional_ul(&info.transfer_characteristic),
        coding_equations: optional_ul(&info.coding_equations),
        sampling_rate: optional_dim(info.sampling_rate),
        channel_count: optional_dim(info.channel_count),
        quantization_bits: optional_dim(info.quantization_bits),
        edit_rate: Rational {
            num: info.edit_rate.num,
            den: info.edit_rate.den,
        },
        duration: info.duration,
    }
}

fn pop_xml(raw: *mut MxfuseReader, index: usize) -> Result<XmlMetadata> {
    let mut view = MxfuseXmlView::default();
    let mut err = MxfuseError::default();
    let status = unsafe { mxfuse_reader_xml(raw, index, &mut view, &mut err) };
    if status != MXFUSE_OK {
        return Err(Error::from_shim(&err));
    }
    let data = if view.data.is_null() || view.size == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(view.data, view.size as usize) }.to_vec()
    };
    let language = c_array_to_string(&view.language);
    let mime_type = c_array_to_string(&view.mime_type);
    let namespace = c_array_to_string(&view.ns);
    let scheme_id = if view.scheme_id.iter().all(|byte| *byte == 0) {
        None
    } else {
        Some(view.scheme_id)
    };
    let item = XmlMetadata {
        data,
        scheme_id,
        language,
        namespace,
        mime_type,
        is_xml: view.is_xml != 0,
    };
    unsafe { mxfuse_xml_free(&mut view) };
    Ok(item)
}

fn c_array_to_string(bytes: &[c_char]) -> Option<String> {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    if end == 0 {
        return None;
    }
    let raw: Vec<u8> = bytes[..end].iter().map(|b| *b as u8).collect();
    let text = String::from_utf8_lossy(&raw).into_owned();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn pop_frame(raw: *mut MxfuseReader, track: usize) -> Result<Option<Frame>> {
    let mut view = MxfuseFrameView::default();
    let mut err = MxfuseError::default();
    let status = unsafe { mxfuse_reader_pop_frame(raw, track, &mut view, &mut err) };
    if status == MXFUSE_ERR_NO_FRAME {
        return Ok(None);
    }
    if status != MXFUSE_OK {
        return Err(Error::from_shim(&err));
    }
    let data = if view.data.is_null() || view.size == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(view.data, view.size as usize) }.to_vec()
    };
    let frame = Frame {
        data,
        element_key: view.element_key,
        file_position: view.file_position,
        kl_size: view.kl_size,
        position: view.position,
        track_index: track,
    };
    unsafe { mxfuse_frame_free(&mut view) };
    Ok(Some(frame))
}
