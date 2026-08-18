use std::io::Cursor;
use std::ops::Range;
use std::sync::atomic::Ordering;

use mxfuse::{open_mxf, CountingSource, Frame, ReadOptions, RecordingSource, TrackKind};

mod common;

fn measure_last_picture_frame(bytes: &[u8], options: ReadOptions) -> (usize, usize) {
    let source = CountingSource::new(Cursor::new(bytes.to_vec()));
    let reads = source.reads.clone();
    let nbytes = source.bytes.clone();
    let mut clip = open_mxf(source, options).unwrap();
    let duration = clip.duration();
    let picture: Vec<_> = clip
        .tracks()
        .iter()
        .filter(|track| track.kind == TrackKind::Picture)
        .cloned()
        .collect();
    clip.select(picture.iter()).unwrap();
    clip.seek(duration.saturating_sub(1)).unwrap();
    let packages = clip.read(1).unwrap();
    assert!(!packages.is_empty(), "last-frame read returned no packages");
    assert!(
        !packages[0].frames.is_empty(),
        "last-frame package had no picture frame"
    );
    drop(clip);
    (reads.load(Ordering::SeqCst), nbytes.load(Ordering::SeqCst))
}

fn essence_range(frame: &Frame) -> Range<u64> {
    let start = u64::try_from(frame.file_position).expect("negative file_position");
    let len = u64::from(frame.kl_size) + frame.data.len() as u64;
    start..start + len
}

fn ranges_intersect(left: &Range<u64>, right: &Range<u64>) -> bool {
    left.start < right.end && right.start < left.end
}

#[test]
fn last_frame_fetch_is_sublinear_in_file_size() {
    let small = common::small_clip();
    let large = common::large_clip();
    assert!(
        large.len() as f64 / small.len() as f64 >= 3.0,
        "large clip ({} bytes) is not ~4x the small clip ({} bytes)",
        large.len(),
        small.len()
    );

    let options = ReadOptions::default();
    let (small_reads, small_bytes) = measure_last_picture_frame(small, options);
    let (large_reads, large_bytes) = measure_last_picture_frame(large, options);

    eprintln!(
        "sublinear: small {} bytes / {} reads over {} file; large {} bytes / {} reads over {} file",
        small_bytes,
        small_reads,
        small.len(),
        large_bytes,
        large_reads,
        large.len()
    );

    assert!(
        (large_bytes as f64) <= (small_bytes as f64) * 1.5,
        "large last-frame fetch ({large_bytes} bytes) exceeded 1.5x small fetch ({small_bytes} bytes) despite a ~4x file"
    );
}

#[test]
fn last_frame_stays_under_five_percent_of_file() {
    let large = common::large_clip();
    let (_reads, bytes) = measure_last_picture_frame(large, ReadOptions::default());
    eprintln!(
        "budget: fetched {bytes} of {} ({:.2}%)",
        large.len(),
        100.0 * bytes as f64 / large.len() as f64
    );
    assert!(
        bytes * 20 < large.len(),
        "last-frame fetch ({bytes} bytes) exceeded 5% of file ({})",
        large.len()
    );
}

#[test]
fn disabled_sound_track_is_never_demanded() {
    let bytes = common::small_clip();
    let mut clip = open_mxf(Cursor::new(bytes.to_vec()), ReadOptions::default()).unwrap();
    let tracks = clip.tracks().to_vec();
    let sound = tracks
        .iter()
        .find(|track| track.kind == TrackKind::Sound)
        .expect("synthetic clip has a sound track")
        .clone();
    clip.select(tracks.iter()).unwrap();
    clip.seek(0).unwrap();

    // Payload only: bmx still peeks at a disabled element's KLV key to skip it.
    let mut sound_payloads = Vec::new();
    loop {
        let packages = clip.read(1).unwrap();
        if packages.is_empty() {
            break;
        }
        for package in packages {
            for frame in package.frames {
                if frame.track_index == sound.index {
                    let klv = essence_range(&frame);
                    let payload = (klv.start + u64::from(frame.kl_size))..klv.end;
                    assert!(
                        payload.start < payload.end,
                        "sound frame at {} had empty payload",
                        frame.file_position
                    );
                    sound_payloads.push(payload);
                }
            }
        }
    }
    drop(clip);
    assert!(
        !sound_payloads.is_empty(),
        "pass one collected no sound essence ranges"
    );

    let recording = RecordingSource::new(Cursor::new(bytes.to_vec())).unwrap();
    let ranges = recording.ranges.clone();
    let mut clip = open_mxf(
        recording,
        ReadOptions {
            read_ahead: 0,
            cache_bytes: 0,
        },
    )
    .unwrap();
    let picture: Vec<_> = clip
        .tracks()
        .iter()
        .filter(|track| track.kind == TrackKind::Picture)
        .cloned()
        .collect();
    clip.select(picture.iter()).unwrap();
    clip.seek(0).unwrap();
    loop {
        let packages = clip.read(1).unwrap();
        if packages.is_empty() {
            break;
        }
        for package in &packages {
            assert!(
                package
                    .frames
                    .iter()
                    .all(|frame| frame.track_index != sound.index),
                "picture-only read returned a sound frame"
            );
        }
    }
    drop(clip);

    let demanded = ranges.lock().expect("recording source lock poisoned");
    let overlap = demanded.iter().find(|demand| {
        sound_payloads
            .iter()
            .any(|payload| ranges_intersect(demand, payload))
    });
    assert!(
        overlap.is_none(),
        "picture-only read demanded sound payload at {overlap:?}"
    );
}
