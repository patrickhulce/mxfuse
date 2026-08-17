use std::fs::File;
use std::io::{self, Cursor, SeekFrom};
use std::sync::{Arc, Mutex};

use mxfuse::{ByteSource, ReadOptions, TrackKind};
use napi::bindgen_prelude::*;
use napi_derive::napi;

#[napi(object)]
pub struct NativeReadOptions {
    pub read_ahead: Option<u32>,
    pub cache_bytes: Option<u32>,
}

#[napi(object)]
pub struct NativeTrack {
    pub index: u32,
    pub kind: String,
    pub essence_type: String,
    pub essence_container_ul: Buffer,
    pub edit_rate_num: i32,
    pub edit_rate_den: i32,
    pub duration: i64,
}

#[napi(object)]
pub struct NativeFrame {
    pub data: Buffer,
    pub element_key: Buffer,
    pub file_position: i64,
}

#[napi(object)]
pub struct NativePackage {
    pub frames: Vec<NativeFrame>,
}

#[napi(object)]
pub struct NativeClipInfo {
    pub edit_rate_num: i32,
    pub edit_rate_den: i32,
    pub duration: i64,
    pub tracks: Vec<NativeTrack>,
}

struct BufferSource {
    cursor: Cursor<Vec<u8>>,
}

impl ByteSource for BufferSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        std::io::Read::read(&mut self.cursor, buf)
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        std::io::Seek::seek(&mut self.cursor, pos)
    }

    fn size(&mut self) -> io::Result<u64> {
        Ok(self.cursor.get_ref().len() as u64)
    }
}

#[napi]
pub struct NativeClip {
    inner: Arc<Mutex<Option<mxfuse::Clip>>>,
}

impl NativeClip {
    async fn with_clip<T, F>(&self, func: F) -> napi::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut mxfuse::Clip) -> mxfuse::Result<T> + Send + 'static,
    {
        let inner = Arc::clone(&self.inner);
        napi::tokio::task::spawn_blocking(move || {
            let mut guard = inner
                .lock()
                .map_err(|_| Error::from_reason("clip lock poisoned"))?;
            let clip = guard
                .as_mut()
                .ok_or_else(|| Error::from_reason("clip is closed"))?;
            func(clip).map_err(|error| Error::from_reason(error.to_string()))
        })
        .await
        .map_err(|error| Error::from_reason(error.to_string()))?
    }
}

#[napi]
impl NativeClip {
    #[napi]
    pub async fn info(&self) -> napi::Result<NativeClipInfo> {
        self.with_clip(|clip| {
            Ok(NativeClipInfo {
                edit_rate_num: clip.edit_rate().num,
                edit_rate_den: clip.edit_rate().den,
                duration: clip.duration(),
                tracks: clip
                    .tracks()
                    .iter()
                    .map(|track| NativeTrack {
                        index: track.index as u32,
                        kind: kind_name(track.kind).to_string(),
                        essence_type: track.essence_type.name().to_string(),
                        essence_container_ul: Buffer::from(track.essence_container_ul.to_vec()),
                        edit_rate_num: track.edit_rate.num,
                        edit_rate_den: track.edit_rate.den,
                        duration: track.duration,
                    })
                    .collect(),
            })
        })
        .await
    }

    #[napi]
    pub async fn select(&self, indexes: Vec<u32>) -> napi::Result<()> {
        self.with_clip(move |clip| {
            let selected: Vec<mxfuse::Track> = clip
                .tracks()
                .iter()
                .filter(|track| indexes.contains(&(track.index as u32)))
                .cloned()
                .collect();
            clip.select(selected.iter())
        })
        .await
    }

    #[napi]
    pub async fn seek(&self, position: i64) -> napi::Result<()> {
        self.with_clip(move |clip| clip.seek(position)).await
    }

    #[napi]
    pub async fn read(&self, count: u32) -> napi::Result<Vec<NativePackage>> {
        self.with_clip(move |clip| {
            let packages = clip.read(count)?;
            Ok(packages
                .into_iter()
                .map(|package| NativePackage {
                    frames: package
                        .frames
                        .into_iter()
                        .map(|frame| NativeFrame {
                            data: Buffer::from(frame.data),
                            element_key: Buffer::from(frame.element_key.to_vec()),
                            file_position: frame.file_position,
                        })
                        .collect(),
                })
                .collect())
        })
        .await
    }

    #[napi]
    pub async fn close(&self) -> napi::Result<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| Error::from_reason("clip lock poisoned"))?;
        *guard = None;
        Ok(())
    }
}

#[napi]
pub async fn open_mxf_from_buffer(
    data: Buffer,
    options: Option<NativeReadOptions>,
) -> napi::Result<NativeClip> {
    let source = BufferSource {
        cursor: Cursor::new(data.to_vec()),
    };
    open_native(source, options)
}

#[napi]
pub async fn open_mxf_from_path(
    path: String,
    options: Option<NativeReadOptions>,
) -> napi::Result<NativeClip> {
    let file = File::open(&path).map_err(|error| Error::from_reason(error.to_string()))?;
    open_native(file, options)
}

fn open_native<S: ByteSource + 'static>(
    source: S,
    options: Option<NativeReadOptions>,
) -> napi::Result<NativeClip> {
    let options = options.unwrap_or(NativeReadOptions {
        read_ahead: None,
        cache_bytes: None,
    });
    let chosen = ReadOptions {
        read_ahead: options.read_ahead.unwrap_or(1 << 20),
        cache_bytes: options.cache_bytes.unwrap_or(64 << 20),
    };
    let clip =
        mxfuse::open_mxf(source, chosen).map_err(|error| Error::from_reason(error.to_string()))?;
    Ok(NativeClip {
        inner: Arc::new(Mutex::new(Some(clip))),
    })
}

fn kind_name(kind: TrackKind) -> &'static str {
    match kind {
        TrackKind::Picture => "picture",
        TrackKind::Sound => "sound",
        TrackKind::Data => "data",
        TrackKind::Other => "other",
    }
}
