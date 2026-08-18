use std::io::SeekFrom;
use std::os::raw::{c_int, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use mxfuse_sys::{
    mxfuse_writer_complete, mxfuse_writer_create_track, mxfuse_writer_free, mxfuse_writer_open,
    mxfuse_writer_prepare, mxfuse_writer_write_samples, MxfuseByteSourceVtable, MxfuseError,
    MxfuseRational, MxfuseTrackSpec, MxfuseWriter, MXFUSE_OK, MXFUSE_TRACK_HAS_CHANNEL_COUNT,
    MXFUSE_TRACK_HAS_CODING_UL, MXFUSE_TRACK_HAS_CONTAINER_UL, MXFUSE_TRACK_HAS_QUANT_BITS,
    MXFUSE_TRACK_HAS_SAMPLING_RATE, MXFUSE_TRACK_HAS_STORED_HEIGHT, MXFUSE_TRACK_HAS_STORED_WIDTH,
};

use crate::error::{Error, Result};
use crate::source::ByteSink;
use crate::types::{ClipSpec, EssenceType, TrackSpec};

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

    let mut err = MxfuseError::default();
    if unsafe { mxfuse_writer_prepare(raw, &mut err) } != MXFUSE_OK {
        unsafe { mxfuse_writer_free(raw) };
        return Err(Error::from_shim(&err));
    }

    Ok(ClipWriter { raw, tracks })
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
    if let Some(ul) = track.picture_coding_ul {
        spec.flags |= MXFUSE_TRACK_HAS_CODING_UL;
        spec.picture_coding_ul = ul;
    }
    spec
}
