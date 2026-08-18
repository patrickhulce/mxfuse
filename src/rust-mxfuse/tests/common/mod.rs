use std::io::{self, Cursor, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex, OnceLock};

use mxfuse::{write_mxf, ByteSink, ClipSpec, EssenceType, Flavour, Rational, TrackSpec};

// libMXF++ UTF-16 conversion is process-global; serialize writers in this binary.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

pub const PICTURE_PAYLOAD: usize = 4096;
pub const SMALL_EDIT_UNITS: i64 = 2_000;
pub const LARGE_EDIT_UNITS: i64 = 8_000;
pub const EDIT_RATE_NUM: i32 = 25;
pub const EDIT_RATE_DEN: i32 = 1;
pub const PCM_RATE: u32 = 48_000;
pub const PCM_CHANNELS: u32 = 1;
pub const PCM_BITS: u32 = 16;

const CONTAINER_UL: [u8; 16] = [
    0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x01, 0x0d, 0x01, 0x03, 0x01, 0x02, 0x7f, 0x01, 0x01,
];
const CODING_UL: [u8; 16] = [
    0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x01, 0x04, 0x01, 0x02, 0x01, 0x7f, 0x00, 0x00, 0x00,
];

static SMALL: OnceLock<Vec<u8>> = OnceLock::new();
static LARGE: OnceLock<Vec<u8>> = OnceLock::new();

#[derive(Clone)]
struct SharedCursor {
    inner: Arc<Mutex<Cursor<Vec<u8>>>>,
}

impl SharedCursor {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Cursor::new(Vec::new()))),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.inner
            .lock()
            .expect("shared cursor lock poisoned")
            .get_ref()
            .clone()
    }
}

impl ByteSink for SharedCursor {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Write::write(
            &mut *self.inner.lock().expect("shared cursor lock poisoned"),
            buf,
        )
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        Seek::seek(
            &mut *self.inner.lock().expect("shared cursor lock poisoned"),
            pos,
        )
    }

    fn tell(&mut self) -> io::Result<u64> {
        Ok(self
            .inner
            .lock()
            .expect("shared cursor lock poisoned")
            .position())
    }
}

pub fn pcm_bytes_per_edit_unit() -> usize {
    let samples = (PCM_RATE as i64) * (EDIT_RATE_DEN as i64) / (EDIT_RATE_NUM as i64);
    samples as usize * PCM_CHANNELS as usize * (PCM_BITS as usize).div_ceil(8)
}

/// Build an OP1a clip with opaque picture and WAVE_PCM tracks.
pub fn synthetic(edit_units: i64, payload_bytes: usize) -> Vec<u8> {
    let _guard = WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let shared = SharedCursor::new();
    let spec = ClipSpec {
        edit_rate: Rational {
            num: EDIT_RATE_NUM,
            den: EDIT_RATE_DEN,
        },
        flavour: Flavour::DEFAULT,
        duration: Some(edit_units),
        tracks: vec![
            TrackSpec {
                stored_width: Some(64),
                stored_height: Some(32),
                essence_container_ul: Some(CONTAINER_UL),
                coding_ul: Some(CODING_UL),
                ..TrackSpec::new(EssenceType::OPAQUE_PICTURE)
            },
            TrackSpec {
                sampling_rate: Some(PCM_RATE),
                channel_count: Some(PCM_CHANNELS),
                quantization_bits: Some(PCM_BITS),
                ..TrackSpec::new(EssenceType::WAVE_PCM)
            },
        ],
        xml: vec![],
        ..ClipSpec::default()
    };
    let mut writer = write_mxf(shared.clone(), spec).expect("synthetic write_mxf");
    let picture = vec![0x11u8; payload_bytes];
    let audio = vec![0x33u8; pcm_bytes_per_edit_unit()];
    for _ in 0..edit_units {
        writer.write(0, &picture).expect("write picture");
        writer.write(1, &audio).expect("write sound");
    }
    writer.finish().expect("finish synthetic clip");
    shared.into_bytes()
}

pub fn small_clip() -> &'static [u8] {
    SMALL.get_or_init(|| synthetic(SMALL_EDIT_UNITS, PICTURE_PAYLOAD))
}

pub fn large_clip() -> &'static [u8] {
    LARGE.get_or_init(|| synthetic(LARGE_EDIT_UNITS, PICTURE_PAYLOAD))
}
