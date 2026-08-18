use std::io::{self, Cursor, SeekFrom};

use mxfuse::{
    open_mxf, write_mxf, ByteSink, ClipSpec, EssenceType, Flavour, Rational, ReadOptions,
    TrackKind, TrackSpec,
};

struct NonSeekableSink {
    inner: Cursor<Vec<u8>>,
}

impl ByteSink for NonSeekableSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        std::io::Write::write(&mut self.inner, buf)
    }

    fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "sink is not seekable",
        ))
    }

    fn tell(&mut self) -> io::Result<u64> {
        Ok(self.inner.position())
    }

    fn is_seekable(&self) -> bool {
        false
    }
}

fn unc_frame(fill: u8) -> Vec<u8> {
    vec![fill; 1920 * 1080 * 2]
}

fn pcm_edit_unit(fill: u8) -> Vec<u8> {
    vec![fill; 1920 * 2]
}

fn stock_spec(duration: i64, flavour: Flavour) -> ClipSpec {
    ClipSpec {
        edit_rate: Rational { num: 25, den: 1 },
        flavour,
        duration: Some(duration),
        tracks: vec![
            TrackSpec::new(EssenceType::UNC_HD_1080P),
            TrackSpec {
                sampling_rate: Some(48000),
                channel_count: Some(1),
                quantization_bits: Some(16),
                ..TrackSpec::new(EssenceType::WAVE_PCM)
            },
        ],
    }
}

#[test]
fn write_round_trip_reads_payloads() {
    let picture_a = unc_frame(0x11);
    let picture_b = unc_frame(0x22);
    let audio_a = pcm_edit_unit(0x33);
    let audio_b = pcm_edit_unit(0x44);

    let shared = std::sync::Arc::new(std::sync::Mutex::new(Cursor::new(Vec::new())));
    struct SharedSink(std::sync::Arc<std::sync::Mutex<Cursor<Vec<u8>>>>);
    impl ByteSink for SharedSink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            std::io::Write::write(&mut *self.0.lock().unwrap(), buf)
        }
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            std::io::Seek::seek(&mut *self.0.lock().unwrap(), pos)
        }
        fn tell(&mut self) -> io::Result<u64> {
            Ok(self.0.lock().unwrap().position())
        }
    }

    let mut writer =
        write_mxf(SharedSink(shared.clone()), stock_spec(2, Flavour::DEFAULT)).unwrap();
    writer.write(0, &picture_a).unwrap();
    writer.write(1, &audio_a).unwrap();
    writer.write(0, &picture_b).unwrap();
    writer.write(1, &audio_b).unwrap();
    writer.finish().unwrap();

    let bytes = shared.lock().unwrap().get_ref().clone();
    let mut clip = open_mxf(Cursor::new(bytes), ReadOptions::default()).unwrap();
    assert_eq!(clip.edit_rate(), Rational { num: 25, den: 1 });
    assert_eq!(clip.duration(), 2);
    assert_eq!(clip.tracks().len(), 2);
    assert_eq!(clip.tracks()[0].kind, TrackKind::Picture);
    assert_eq!(clip.tracks()[1].kind, TrackKind::Sound);
    let tracks = clip.tracks().to_vec();
    clip.select(tracks.iter()).unwrap();
    clip.seek(0).unwrap();
    let first = clip.read(1).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].frames.len(), 2);
    assert_eq!(first[0].frames[0].data, picture_a);
    assert_eq!(first[0].frames[1].data, audio_a);
    let second = clip.read(1).unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].frames[0].data, picture_b);
    assert_eq!(second[0].frames[1].data, audio_b);
}

#[test]
fn opaque_round_trip_preserves_container_ul() {
    let container = [
        0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x01, 0x0d, 0x01, 0x03, 0x01, 0x02, 0x7f, 0x01,
        0x01,
    ];
    let coding = [
        0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x01, 0x04, 0x01, 0x02, 0x01, 0x7f, 0x00, 0x00,
        0x00,
    ];
    let frame_a = b"opaque-frame-a".to_vec();
    let frame_b = b"opaque-frame-b".to_vec();

    let shared = std::sync::Arc::new(std::sync::Mutex::new(Cursor::new(Vec::new())));
    struct SharedSink(std::sync::Arc<std::sync::Mutex<Cursor<Vec<u8>>>>);
    impl ByteSink for SharedSink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            std::io::Write::write(&mut *self.0.lock().unwrap(), buf)
        }
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            std::io::Seek::seek(&mut *self.0.lock().unwrap(), pos)
        }
        fn tell(&mut self) -> io::Result<u64> {
            Ok(self.0.lock().unwrap().position())
        }
    }

    let spec = ClipSpec {
        edit_rate: Rational { num: 24, den: 1 },
        flavour: Flavour::DEFAULT,
        duration: Some(2),
        tracks: vec![TrackSpec {
            stored_width: Some(64),
            stored_height: Some(32),
            essence_container_ul: Some(container),
            picture_coding_ul: Some(coding),
            ..TrackSpec::new(EssenceType::OPAQUE_PICTURE)
        }],
    };
    let mut writer = write_mxf(SharedSink(shared.clone()), spec).unwrap();
    writer.write(0, &frame_a).unwrap();
    writer.write(0, &frame_b).unwrap();
    writer.finish().unwrap();

    let bytes = shared.lock().unwrap().get_ref().clone();
    let mut clip = open_mxf(Cursor::new(bytes), ReadOptions::default()).unwrap();
    assert_eq!(clip.tracks().len(), 1);
    assert_eq!(clip.tracks()[0].essence_container_ul, container);
    let tracks = clip.tracks().to_vec();
    clip.select(tracks.iter()).unwrap();
    clip.seek(0).unwrap();
    let first = clip.read(1).unwrap();
    assert_eq!(first[0].frames[0].data, frame_a);
    let second = clip.read(1).unwrap();
    assert_eq!(second[0].frames[0].data, frame_b);
}

#[test]
fn single_pass_writes_to_non_seekable_sink() {
    let picture = unc_frame(0xaa);
    let audio = pcm_edit_unit(0x55);
    let shared = std::sync::Arc::new(std::sync::Mutex::new(Cursor::new(Vec::new())));
    struct SharedNonSeekable(std::sync::Arc<std::sync::Mutex<Cursor<Vec<u8>>>>);
    impl ByteSink for SharedNonSeekable {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            std::io::Write::write(&mut *self.0.lock().unwrap(), buf)
        }
        fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "sink is not seekable",
            ))
        }
        fn tell(&mut self) -> io::Result<u64> {
            Ok(self.0.lock().unwrap().position())
        }
        fn is_seekable(&self) -> bool {
            false
        }
    }

    let mut writer = write_mxf(
        SharedNonSeekable(shared.clone()),
        stock_spec(1, Flavour::SINGLE_PASS),
    )
    .unwrap();
    writer.write(0, &picture).unwrap();
    writer.write(1, &audio).unwrap();
    writer.finish().unwrap();
    assert!(!shared.lock().unwrap().get_ref().is_empty());
}

#[test]
fn single_pass_duration_mismatch_fails() {
    let picture = unc_frame(0xaa);
    let audio = pcm_edit_unit(0x55);
    let mut writer = write_mxf(
        NonSeekableSink {
            inner: Cursor::new(Vec::new()),
        },
        stock_spec(2, Flavour::SINGLE_PASS),
    )
    .unwrap();
    writer.write(0, &picture).unwrap();
    writer.write(1, &audio).unwrap();
    assert!(writer.finish().is_err());
}
