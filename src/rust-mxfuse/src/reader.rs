use std::io::SeekFrom;
use std::os::raw::{c_int, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use mxfuse_sys::{
    mxfuse_frame_free, mxfuse_reader_clip_info, mxfuse_reader_free, mxfuse_reader_open,
    mxfuse_reader_pop_frame, mxfuse_reader_read, mxfuse_reader_seek, mxfuse_reader_set_enable,
    mxfuse_reader_track_info, MxfuseByteSourceVtable, MxfuseClipInfo, MxfuseError, MxfuseFrameView,
    MxfuseReader, MxfuseTrackInfo, MXFUSE_ERR_NO_FRAME, MXFUSE_OK,
};

use crate::error::{Error, Result};
use crate::source::{ByteSource, ReadAhead};
use crate::types::{EssenceType, Frame, Package, Rational, ReadOptions, Track, TrackKind};

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

const VTABLE: MxfuseByteSourceVtable = MxfuseByteSourceVtable {
    read: Some(src_read),
    seek: Some(src_seek),
    tell: Some(src_tell),
    size: Some(src_size),
    close: Some(src_close),
};

/// An opened MXF clip. One reader per thread; do not share across tasks.
pub struct Clip {
    raw: *mut MxfuseReader,
    tracks: Vec<Track>,
    edit_rate: Rational,
    duration: i64,
}

unsafe impl Send for Clip {}

impl Clip {
    pub fn edit_rate(&self) -> Rational {
        self.edit_rate
    }

    pub fn duration(&self) -> i64 {
        self.duration
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
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
    pub fn read(&mut self, count: u32) -> Result<Vec<Package>> {
        let mut err = MxfuseError::default();
        let mut n = 0u32;
        let status = unsafe { mxfuse_reader_read(self.raw, count, &mut n, &mut err) };
        if status != MXFUSE_OK {
            return Err(Error::from_shim(&err));
        }
        let mut packages = vec![Package::default(); n as usize];
        for track in &self.tracks {
            for package in packages.iter_mut() {
                match pop_frame(self.raw, track.index)? {
                    Some(frame) => package.frames.push(frame),
                    None => break,
                }
            }
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
        tracks.push(Track {
            index: track_info.index,
            kind: TrackKind::from_data_def(track_info.data_def),
            essence_type: EssenceType::from_i32(track_info.essence_type),
            essence_container_ul: track_info.essence_container_ul,
            edit_rate: Rational {
                num: track_info.edit_rate.num,
                den: track_info.edit_rate.den,
            },
            duration: track_info.duration,
        });
    }

    Ok(Clip {
        raw,
        tracks,
        edit_rate: Rational {
            num: info.edit_rate.num,
            den: info.edit_rate.den,
        },
        duration: info.duration,
    })
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
    };
    unsafe { mxfuse_frame_free(&mut view) };
    Ok(Some(frame))
}
