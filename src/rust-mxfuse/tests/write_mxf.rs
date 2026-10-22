use std::io::{self, Cursor, SeekFrom};
use std::sync::Mutex;

use mxfuse::{
    open_mxf, write_mxf, ByteSink, ClipSpec, DescriptorKind, EssenceType, Flavour, Identity,
    PixelComponent, Rational, ReadOptions, Timecode, TrackKind, TrackSpec, XmlMetadata,
};

// libMXF++ keeps UTF-16 conversion state in process-global storage, so two
// writers in one test binary cannot run at the same time.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

fn exclusive_write() -> std::sync::MutexGuard<'static, ()> {
    WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

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
        xml: vec![],
        ..ClipSpec::default()
    }
}

#[test]
fn write_round_trip_reads_payloads() {
    let _guard = exclusive_write();
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
    let _guard = exclusive_write();
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
            coding_ul: Some(coding),
            ..TrackSpec::new(EssenceType::OPAQUE_PICTURE)
        }],
        xml: vec![],
        ..ClipSpec::default()
    };
    let mut writer = write_mxf(SharedSink(shared.clone()), spec).unwrap();
    writer.write(0, &frame_a).unwrap();
    writer.write(0, &frame_b).unwrap();
    writer.finish().unwrap();

    let bytes = shared.lock().unwrap().get_ref().clone();
    let mut clip = open_mxf(Cursor::new(bytes), ReadOptions::default()).unwrap();
    assert_eq!(clip.tracks().len(), 1);
    assert_eq!(clip.tracks()[0].essence_container_ul, container);
    assert_eq!(clip.tracks()[0].coding_ul, Some(coding));
    let tracks = clip.tracks().to_vec();
    clip.select(tracks.iter()).unwrap();
    clip.seek(0).unwrap();
    let first = clip.read(1).unwrap();
    assert_eq!(first[0].frames[0].data, frame_a);
    assert_eq!(first[0].frames[0].track_index, 0);
    let second = clip.read(1).unwrap();
    assert_eq!(second[0].frames[0].data, frame_b);
}

#[test]
fn opaque_data_coding_ul_round_trips() {
    let _guard = exclusive_write();
    let container = [
        0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x0d, 0x0d, 0x01, 0x03, 0x01, 0x02, 0x70, 0x03,
        0x00,
    ];
    let coding = [
        0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x0d, 0x04, 0x01, 0x02, 0x02, 0x70, 0x00, 0x00,
        0x00,
    ];
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
        tracks: vec![TrackSpec {
            essence_container_ul: Some(container),
            coding_ul: Some(coding),
            descriptor: Some(DescriptorKind::GenericData),
            ..TrackSpec::new(EssenceType::OPAQUE_DATA)
        }],
        duration: Some(1),
        ..stock_spec(1, Flavour::DEFAULT)
    };
    let mut writer = write_mxf(SharedSink(shared.clone()), spec).unwrap();
    writer.write(0, b"meta").unwrap();
    writer.finish().unwrap();

    let bytes = shared.lock().unwrap().get_ref().clone();
    let clip = open_mxf(Cursor::new(bytes), ReadOptions::default()).unwrap();
    assert_eq!(clip.tracks()[0].essence_container_ul, container);
    assert_eq!(clip.tracks()[0].coding_ul, Some(coding));
    assert_eq!(clip.tracks()[0].descriptor, DescriptorKind::GenericData);
}

#[test]
fn single_pass_writes_to_non_seekable_sink() {
    let _guard = exclusive_write();
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
    let _guard = exclusive_write();
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

#[test]
fn xml_round_trip_preserves_payload() {
    let _guard = exclusive_write();
    let xml = b"<clip xmlns=\"urn:x-mxfuse:test\">hello</clip>".to_vec();
    let frame_a = b"opaque-frame-a".to_vec();
    let frame_b = b"opaque-frame-b".to_vec();
    let container = [
        0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x01, 0x0d, 0x01, 0x03, 0x01, 0x02, 0x7f, 0x01,
        0x01,
    ];
    let coding = [
        0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x01, 0x04, 0x01, 0x02, 0x01, 0x7f, 0x00, 0x00,
        0x00,
    ];

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
            coding_ul: Some(coding),
            ..TrackSpec::new(EssenceType::OPAQUE_PICTURE)
        }],
        xml: vec![XmlMetadata {
            data: xml.clone(),
            scheme_id: None,
            language: Some("en".to_string()),
            namespace: Some("urn:x-mxfuse:test".to_string()),
            mime_type: None,
            is_xml: true,
            text_encoding: mxfuse::TextEncoding::Utf8,
        }],
        ..ClipSpec::default()
    };
    let mut writer = write_mxf(SharedSink(shared.clone()), spec).unwrap();
    writer.write(0, &frame_a).unwrap();
    writer.write(0, &frame_b).unwrap();
    writer.finish().unwrap();

    let bytes = shared.lock().unwrap().get_ref().clone();
    let mut clip = open_mxf(Cursor::new(bytes), ReadOptions::default()).unwrap();
    assert_eq!(clip.xml().len(), 1);
    assert_eq!(clip.xml()[0].data, xml);
    assert!(clip.xml()[0].is_xml);
    assert_eq!(clip.xml()[0].text_encoding, mxfuse::TextEncoding::Utf8);
    let tracks = clip.tracks().to_vec();
    clip.select(tracks.iter()).unwrap();
    clip.seek(0).unwrap();
    let packages = clip.read(2).unwrap();
    assert_eq!(packages.len(), 2);
    assert_eq!(packages[0].frames[0].data, frame_a);
    assert_eq!(packages[1].frames[0].data, frame_b);
    assert_eq!(packages[0].frames[0].track_index, 0);
}

const JXL_CONTAINER: [u8; 16] = [
    0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x0d, 0x0d, 0x01, 0x03, 0x01, 0x02, 0x70, 0x01, 0x00,
];
const JXL_CODING: [u8; 16] = [
    0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x0d, 0x04, 0x01, 0x02, 0x02, 0x70, 0x00, 0x00, 0x00,
];
const JXL_META_CONTAINER: [u8; 16] = [
    0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x0d, 0x0d, 0x01, 0x03, 0x01, 0x02, 0x70, 0x03, 0x00,
];
const JXL_PICTURE_KEY: [u8; 16] = [
    0x06, 0x0e, 0x2b, 0x34, 0x01, 0x02, 0x01, 0x01, 0x0d, 0x01, 0x03, 0x01, 0x15, 0x01, 0x70, 0x00,
];

#[test]
fn jxl_mapping_writes_rgba_descriptor_and_element_key() {
    let _guard = exclusive_write();
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
        tracks: vec![
            TrackSpec {
                stored_width: Some(96),
                stored_height: Some(64),
                essence_container_ul: Some(JXL_CONTAINER),
                coding_ul: Some(JXL_CODING),
                element_type: Some(0x70),
                element_llen: Some(8),
                temporal_reordering: true,
                descriptor: Some(DescriptorKind::Rgba),
                frame_layout: Some(0),
                aspect_ratio: Some(Rational { num: 16, den: 9 }),
                video_line_map: Some((1, 0)),
                pixel_layout: Some(vec![
                    PixelComponent {
                        code: b'R',
                        depth: 32,
                    },
                    PixelComponent {
                        code: b'G',
                        depth: 32,
                    },
                    PixelComponent {
                        code: b'B',
                        depth: 32,
                    },
                ]),
                ..TrackSpec::new(EssenceType::OPAQUE_PICTURE)
            },
            TrackSpec {
                essence_container_ul: Some(JXL_META_CONTAINER),
                element_type: Some(0x70),
                descriptor: Some(DescriptorKind::GenericData),
                ..TrackSpec::new(EssenceType::OPAQUE_DATA)
            },
        ],
        xml: vec![XmlMetadata {
            data: b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><jxlmxf-exr-meta xmlns=\"http://jxlmxf/exr-meta/1\" version=\"1\"/>".to_vec(),
            language: Some("en".to_string()),
            namespace: Some("http://jxlmxf/exr-meta/1".to_string()),
            scheme_id: Some(mxfuse::XmlScheme::from_name("urn:x-jxlmxf:exr-header-metadata:1")),
            ..XmlMetadata::new(Vec::new())
        }],
        system_item: true,
        ..ClipSpec::default()
    };
    let mut writer = write_mxf(SharedSink(shared.clone()), spec).unwrap();
    writer.write(0, b"jxl-frame-a").unwrap();
    writer.write(1, b"meta-a").unwrap();
    writer.write(0, b"jxl-frame-b").unwrap();
    writer.write(1, b"meta-b").unwrap();
    writer.finish().unwrap();

    let bytes = shared.lock().unwrap().get_ref().clone();
    let mut clip = open_mxf(Cursor::new(bytes), ReadOptions::default()).unwrap();
    assert_eq!(clip.tracks().len(), 2);
    let picture = &clip.tracks()[0];
    assert_eq!(picture.essence_container_ul, JXL_CONTAINER);
    assert_eq!(picture.coding_ul, Some(JXL_CODING));
    assert_eq!(picture.descriptor, DescriptorKind::Rgba);
    assert_eq!(picture.stored_width, Some(96));
    assert_eq!(picture.stored_height, Some(64));
    assert_eq!(picture.aspect_ratio, Some(Rational { num: 16, den: 9 }));
    assert_eq!(picture.video_line_map, Some((1, 0)));
    assert_eq!(
        picture.pixel_layout,
        vec![
            PixelComponent {
                code: b'R',
                depth: 32
            },
            PixelComponent {
                code: b'G',
                depth: 32
            },
            PixelComponent {
                code: b'B',
                depth: 32
            },
        ]
    );
    assert_eq!(
        clip.xml()[0].namespace.as_deref(),
        Some("http://jxlmxf/exr-meta/1")
    );
    assert!(clip.xml()[0].matches_scheme("urn:x-jxlmxf:exr-header-metadata:1"));
    assert!(!clip.xml()[0].matches_scheme("urn:x-jxlmxf:singularity-metadata:1"));
    let tracks = clip.tracks().to_vec();
    clip.select(tracks.iter()).unwrap();
    clip.seek(0).unwrap();
    let first = clip.read(1).unwrap();
    assert_eq!(first[0].frames[0].element_key, JXL_PICTURE_KEY);
    assert_eq!(first[0].frames[0].kl_size, 24);
    assert_eq!(first[0].frames[0].data, b"jxl-frame-a");
}

#[test]
fn start_timecode_and_identity_round_trip() {
    let _guard = exclusive_write();
    let spec = ClipSpec {
        start_timecode: Some(Timecode {
            hour: 1,
            minute: 2,
            second: 3,
            frame: 4,
            drop_frame: false,
        }),
        identity: Some(Identity {
            company_name: Some("mxfuse".to_string()),
            product_name: Some("mxfuse-test".to_string()),
            version_string: Some("0.1.0".to_string()),
            product_version: Some((0, 1, 0, 0, 0)),
            product_uid: Some([0x11; 16]),
            creation_date: Some((2026, 1, 2, 3, 4, 5, 6)),
            generation_uid: Some([0x22; 16]),
            material_package_uid: Some([0x33; 32]),
            file_source_package_uid: Some([0x44; 32]),
        }),
        ..stock_spec(1, Flavour::DEFAULT)
    };
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
    let mut writer = write_mxf(SharedSink(shared.clone()), spec).unwrap();
    writer.write(0, &unc_frame(0x11)).unwrap();
    writer.write(1, &pcm_edit_unit(0x33)).unwrap();
    writer.finish().unwrap();
    let bytes = shared.lock().unwrap().get_ref().clone();
    let clip = open_mxf(Cursor::new(bytes), ReadOptions::default()).unwrap();
    assert_eq!(
        clip.start_timecode(),
        Some(Timecode {
            hour: 1,
            minute: 2,
            second: 3,
            frame: 4,
            drop_frame: false,
        })
    );
}

#[test]
fn clip_name_round_trips() {
    let _guard = exclusive_write();
    let named = ClipSpec {
        clip_name: Some("shot_001 fixed_p0_rgc_grain".to_string()),
        ..stock_spec(1, Flavour::DEFAULT)
    };
    let unnamed = stock_spec(1, Flavour::DEFAULT);
    for (spec, expected) in [
        (named, Some("shot_001 fixed_p0_rgc_grain")),
        (unnamed, None),
    ] {
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
        let mut writer = write_mxf(SharedSink(shared.clone()), spec).unwrap();
        writer.write(0, &unc_frame(0x11)).unwrap();
        writer.write(1, &pcm_edit_unit(0x33)).unwrap();
        writer.finish().unwrap();
        let bytes = shared.lock().unwrap().get_ref().clone();
        let clip = open_mxf(Cursor::new(bytes), ReadOptions::default()).unwrap();
        assert_eq!(clip.name(), expected);
    }
}

#[test]
fn clip_name_too_long_is_rejected() {
    let _guard = exclusive_write();
    let spec = ClipSpec {
        clip_name: Some("x".repeat(256)),
        ..stock_spec(1, Flavour::DEFAULT)
    };
    let result = write_mxf(Cursor::new(Vec::new()), spec);
    assert!(result.is_err());
}

#[test]
fn xml_scheme_round_trips_and_rejects_bad_payloads() {
    let _guard = exclusive_write();
    let container = [
        0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x01, 0x0d, 0x01, 0x03, 0x01, 0x02, 0x7f, 0x01,
        0x01,
    ];
    let xml = b"<clip xmlns=\"urn:x-mxfuse:test\">hello</clip>".to_vec();
    let urn = "urn:x-jxlmxf:exr-header-metadata:1";
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

    let spec_for = |scheme: Option<mxfuse::XmlScheme>, data: Vec<u8>| ClipSpec {
        edit_rate: Rational { num: 24, den: 1 },
        flavour: Flavour::DEFAULT,
        duration: Some(1),
        tracks: vec![TrackSpec {
            stored_width: Some(64),
            stored_height: Some(32),
            essence_container_ul: Some(container),
            ..TrackSpec::new(EssenceType::OPAQUE_PICTURE)
        }],
        xml: vec![XmlMetadata {
            data,
            scheme_id: scheme,
            language: Some("en".to_string()),
            namespace: Some("urn:x-mxfuse:test".to_string()),
            ..XmlMetadata::new(Vec::new())
        }],
        ..ClipSpec::default()
    };

    let write = |scheme: Option<mxfuse::XmlScheme>, data: Vec<u8>| {
        let sink = std::sync::Arc::new(std::sync::Mutex::new(Cursor::new(Vec::new())));
        let mut writer = write_mxf(SharedSink(sink.clone()), spec_for(scheme, data))?;
        writer.write(0, b"frame")?;
        writer.finish()?;
        let bytes = sink.lock().unwrap().get_ref().clone();
        Ok::<_, mxfuse::Error>(bytes)
    };

    let first = write(Some(mxfuse::XmlScheme::from_name(urn)), xml.clone()).unwrap();
    let second = write(Some(mxfuse::XmlScheme::from_name(urn)), xml.clone()).unwrap();
    let other = write(
        Some(mxfuse::XmlScheme::from_name(
            "urn:x-jxlmxf:singularity-metadata:1",
        )),
        xml.clone(),
    )
    .unwrap();
    let defaulted = write(None, xml.clone()).unwrap();
    let ul_bytes = {
        let clip = open_mxf(Cursor::new(first.clone()), ReadOptions::default()).unwrap();
        match clip.xml()[0].scheme_id.clone().unwrap() {
            mxfuse::XmlScheme::Ul(ul) => ul,
            mxfuse::XmlScheme::Name(_) => panic!("read scheme must be a UL"),
        }
    };

    let clip = open_mxf(Cursor::new(first), ReadOptions::default()).unwrap();
    assert_eq!(clip.xml()[0].data, xml);
    assert!(clip.xml()[0].matches_scheme(urn));
    assert!(!clip.xml()[0].matches_scheme("urn:x-jxlmxf:singularity-metadata:1"));
    assert!(XmlMetadata::is_scheme_match(urn, &ul_bytes));
    assert!(XmlMetadata {
        scheme_id: Some(mxfuse::XmlScheme::from_name(urn)),
        ..XmlMetadata::new(xml.clone())
    }
    .matches_scheme(urn));

    let clip2 = open_mxf(Cursor::new(second), ReadOptions::default()).unwrap();
    assert_eq!(
        clip2.xml()[0]
            .scheme_id
            .as_ref()
            .and_then(|s| s.to_ul().ok()),
        Some(ul_bytes)
    );
    let other_ul = open_mxf(Cursor::new(other), ReadOptions::default())
        .unwrap()
        .xml()[0]
        .scheme_id
        .as_ref()
        .and_then(|s| s.to_ul().ok());
    assert_ne!(other_ul, Some(ul_bytes));
    let default_ul = open_mxf(Cursor::new(defaulted), ReadOptions::default())
        .unwrap()
        .xml()[0]
        .scheme_id
        .as_ref()
        .and_then(|s| s.to_ul().ok());
    assert_ne!(default_ul, Some(ul_bytes));

    let raw_ul = [0x11u8; 16];
    let from_ul = write(Some(mxfuse::XmlScheme::Ul(raw_ul)), xml.clone()).unwrap();
    let clip_ul = open_mxf(Cursor::new(from_ul), ReadOptions::default()).unwrap();
    assert_eq!(
        clip_ul.xml()[0]
            .scheme_id
            .as_ref()
            .and_then(|s| s.to_ul().ok()),
        Some(raw_ul)
    );

    let malformed = write(Some(mxfuse::XmlScheme::from_name(urn)), b"<clip>".to_vec());
    let err = malformed.unwrap_err().to_string();
    assert!(err.contains("malformed xml"));
    assert!(err.contains(urn));
    assert!(err.contains("urn:x-mxfuse:test"));

    let mut bom = b"\xef\xbb\xbf".to_vec();
    bom.extend_from_slice(&xml);
    let bom_err = write(Some(mxfuse::XmlScheme::from_name(urn)), bom)
        .unwrap_err()
        .to_string();
    assert!(bom_err.contains("BOM"));
    assert!(bom_err.contains(urn));
}
