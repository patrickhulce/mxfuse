//! Print how many reads and bytes a seek+read costs across ReadOptions.
//!
//! Usage: probe [path] [seek] [count]
//!
//! Omit `path` to synthesize an 8_000-edit-unit opaque+PCM clip in memory.

use std::env;
use std::io::{self, Cursor, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use mxfuse::{
    open_mxf, write_mxf, ByteSink, ClipSpec, CountingSource, EssenceType, Flavour, Rational,
    ReadOptions, TrackKind, TrackSpec,
};

const MATRIX: &[(u32, u32)] = &[(0, 0), (1 << 20, 0), (0, 64 << 20), (1 << 20, 64 << 20)];

fn main() {
    let mut args = env::args().skip(1);
    let path = args.next();
    let seek_arg = args.next();
    let count: u32 = args
        .next()
        .map(|value| value.parse().expect("count must be an integer"))
        .unwrap_or(1);

    let bytes = match path.as_deref() {
        Some(path) => std::fs::read(path).unwrap_or_else(|error| {
            panic!("failed to read {}: {error}", Path::new(path).display())
        }),
        None => synthetic(8_000, 4096),
    };
    let file_bytes = bytes.len();

    let duration = {
        let clip =
            open_mxf(Cursor::new(bytes.clone()), ReadOptions::default()).expect("open probe clip");
        clip.duration()
    };
    let seek: i64 = match seek_arg {
        Some(value) => value.parse().expect("seek must be an integer"),
        None => duration.saturating_sub(1),
    };

    println!("file_bytes={file_bytes} duration={duration} seek={seek} count={count}");
    println!(
        "{:<12} {:<12} {:<8} {:<12}",
        "read_ahead", "cache_bytes", "reads", "bytes"
    );

    for &(read_ahead, cache_bytes) in MATRIX {
        let source = CountingSource::new(Cursor::new(bytes.clone()));
        let reads = source.reads.clone();
        let nbytes = source.bytes.clone();
        let mut clip = open_mxf(
            source,
            ReadOptions {
                read_ahead,
                cache_bytes,
            },
        )
        .expect("open probe clip");
        let picture: Vec<_> = clip
            .tracks()
            .iter()
            .filter(|track| track.kind == TrackKind::Picture)
            .cloned()
            .collect();
        clip.select(picture.iter()).expect("select picture");
        clip.seek(seek).expect("seek");
        let packages = clip.read(count).expect("read");
        assert!(
            !packages.is_empty(),
            "probe read returned no packages at position {seek}"
        );
        drop(clip);
        println!(
            "{:<12} {:<12} {:<8} {:<12}",
            read_ahead,
            cache_bytes,
            reads.load(Ordering::SeqCst),
            nbytes.load(Ordering::SeqCst)
        );
    }
}

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
        self.inner.lock().expect("lock").get_ref().clone()
    }
}

impl ByteSink for SharedCursor {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Write::write(&mut *self.inner.lock().expect("lock"), buf)
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        Seek::seek(&mut *self.inner.lock().expect("lock"), pos)
    }

    fn tell(&mut self) -> io::Result<u64> {
        Ok(self.inner.lock().expect("lock").position())
    }
}

fn synthetic(edit_units: i64, payload_bytes: usize) -> Vec<u8> {
    let shared = SharedCursor::new();
    let spec = ClipSpec {
        edit_rate: Rational { num: 25, den: 1 },
        flavour: Flavour::DEFAULT,
        duration: Some(edit_units),
        tracks: vec![
            TrackSpec {
                stored_width: Some(64),
                stored_height: Some(32),
                essence_container_ul: Some([
                    0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x01, 0x0d, 0x01, 0x03, 0x01, 0x02,
                    0x7f, 0x01, 0x01,
                ]),
                coding_ul: Some([
                    0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x01, 0x04, 0x01, 0x02, 0x01, 0x7f,
                    0x00, 0x00, 0x00,
                ]),
                ..TrackSpec::new(EssenceType::OPAQUE_PICTURE)
            },
            TrackSpec {
                sampling_rate: Some(48_000),
                channel_count: Some(1),
                quantization_bits: Some(16),
                ..TrackSpec::new(EssenceType::WAVE_PCM)
            },
        ],
        xml: vec![],
        ..ClipSpec::default()
    };
    let mut writer = write_mxf(shared.clone(), spec).expect("synthetic write_mxf");
    let picture = vec![0x11u8; payload_bytes];
    let audio = vec![0x33u8; 1920 * 2];
    for _ in 0..edit_units {
        writer.write(0, &picture).expect("write picture");
        writer.write(1, &audio).expect("write sound");
    }
    writer.finish().expect("finish synthetic clip");
    shared.into_bytes()
}
