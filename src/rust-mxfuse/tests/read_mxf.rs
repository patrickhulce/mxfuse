use std::fs::File;
use std::io::Cursor;
use std::path::PathBuf;

use mxfuse::{open_mxf, CountingSource, ReadOptions, TrackKind};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/sample_op1a.mxf")
}

fn have_fixture() -> bool {
    fixture_path().is_file()
}

#[test]
fn open_lists_tracks_and_reads_a_frame() {
    if !have_fixture() {
        return;
    }
    let file = File::open(fixture_path()).unwrap();
    let mut clip = open_mxf(file, ReadOptions::default()).unwrap();
    assert!(clip.duration() > 0);
    assert!(!clip.tracks().is_empty());
    let picture: Vec<_> = clip
        .tracks()
        .iter()
        .filter(|track| track.kind == TrackKind::Picture)
        .cloned()
        .collect();
    clip.select(picture.iter()).unwrap();
    clip.seek(0).unwrap();
    let packages = clip.read(1).unwrap();
    assert!(!packages.is_empty());
    assert!(!packages[0].frames.is_empty());
    assert!(!packages[0].frames[0].data.is_empty());
    assert_eq!(packages[0].frames[0].element_key.len(), 16);
}

#[test]
fn read_ahead_amortizes_small_reads() {
    if !have_fixture() {
        return;
    }
    let bytes = std::fs::read(fixture_path()).unwrap();
    let bare = CountingSource::new(Cursor::new(bytes.clone()));
    let bare_reads = bare.reads.clone();
    let cached = CountingSource::new(Cursor::new(bytes));
    let cached_reads = cached.reads.clone();

    let mut clip = open_mxf(
        bare,
        ReadOptions {
            read_ahead: 0,
            cache_bytes: 0,
        },
    )
    .unwrap();
    let pos = clip.duration().saturating_sub(1).min(2);
    clip.seek(pos).unwrap();
    let _ = clip.read(1).unwrap();
    drop(clip);

    let mut clip = open_mxf(
        cached,
        ReadOptions {
            read_ahead: 1 << 20,
            cache_bytes: 8 << 20,
        },
    )
    .unwrap();
    clip.seek(pos).unwrap();
    let _ = clip.read(1).unwrap();
    drop(clip);

    assert!(
        cached_reads.load(std::sync::atomic::Ordering::SeqCst)
            < bare_reads.load(std::sync::atomic::Ordering::SeqCst)
    );
}
