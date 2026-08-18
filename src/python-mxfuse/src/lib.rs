use std::io::{self, SeekFrom};

use mxfuse::{
    ByteSink, ByteSource, ClipSpec, EssenceType, Flavour, Rational, ReadOptions, TrackKind,
    TrackSpec,
};
use pyo3::exceptions::{PyIOError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyIterator, PyList, PyModule};

struct PythonSource {
    handle: Py<PyAny>,
}

impl ByteSource for PythonSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Python::with_gil(|py| {
            let value = self
                .handle
                .bind(py)
                .call_method1("read", (buf.len(),))
                .map_err(py_io_error)?;
            let bytes: Vec<u8> = value.extract().map_err(py_io_error)?;
            if bytes.len() > buf.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "read returned more bytes than requested",
                ));
            }
            buf[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        })
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        seek_python_handle(&self.handle, pos)
    }

    fn size(&mut self) -> io::Result<u64> {
        Python::with_gil(|py| {
            let handle = self.handle.bind(py);
            if handle.hasattr("size").map_err(py_io_error)? {
                let value = handle.call_method0("size").map_err(py_io_error)?;
                return value.extract::<u64>().map_err(py_io_error);
            }
            let current = handle
                .call_method0("tell")
                .and_then(|value| value.extract::<i64>())
                .map_err(py_io_error)?;
            let end = handle
                .call_method1("seek", (0, 2))
                .and_then(|value| value.extract::<i64>())
                .map_err(py_io_error)?;
            handle
                .call_method1("seek", (current, 0))
                .map_err(py_io_error)?;
            u64::try_from(end).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "size returned a negative value")
            })
        })
    }
}

fn seek_python_handle(handle: &Py<PyAny>, position: SeekFrom) -> io::Result<u64> {
    let (offset, whence) = match position {
        SeekFrom::Start(offset) => (
            i64::try_from(offset).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "seek offset exceeds i64")
            })?,
            0,
        ),
        SeekFrom::End(offset) => (offset, 2),
        SeekFrom::Current(offset) => (offset, 1),
    };
    Python::with_gil(|py| {
        let result: i64 = handle
            .bind(py)
            .call_method1("seek", (offset, whence))
            .and_then(|value| value.extract())
            .map_err(py_io_error)?;
        u64::try_from(result).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "seek returned a negative position",
            )
        })
    })
}

fn py_io_error(error: PyErr) -> io::Error {
    io::Error::other(error.to_string())
}

fn mxfuse_error(error: mxfuse::Error) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

#[pyclass(name = "Clip", unsendable)]
struct PyClip {
    inner: Option<mxfuse::Clip>,
}

impl PyClip {
    fn clip(&mut self) -> PyResult<&mut mxfuse::Clip> {
        self.inner
            .as_mut()
            .ok_or_else(|| PyValueError::new_err("clip is closed"))
    }
}

#[pymethods]
impl PyClip {
    #[getter]
    fn edit_rate(&mut self) -> PyResult<(i32, i32)> {
        let rate = self.clip()?.edit_rate();
        Ok((rate.num, rate.den))
    }

    #[getter]
    fn duration(&mut self) -> PyResult<i64> {
        Ok(self.clip()?.duration())
    }

    #[getter]
    fn tracks(&mut self, py: Python<'_>) -> PyResult<Py<PyList>> {
        let tracks = self.clip()?.tracks().to_vec();
        let list = PyList::empty(py);
        for track in tracks {
            list.append(PyTrack {
                index: track.index,
                kind: kind_name(track.kind),
                essence_type: track.essence_type.name().to_string(),
                essence_container_ul: track.essence_container_ul.to_vec(),
                edit_rate: (track.edit_rate.num, track.edit_rate.den),
                duration: track.duration,
            })?;
        }
        Ok(list.unbind())
    }

    fn select(&mut self, py: Python<'_>, tracks: Bound<'_, PyAny>) -> PyResult<()> {
        let indexes = collect_indexes(py, tracks)?;
        let clip = self.clip()?;
        let selected: Vec<mxfuse::Track> = clip
            .tracks()
            .iter()
            .filter(|track| indexes.contains(&track.index))
            .cloned()
            .collect();
        py.allow_threads(|| clip.select(selected.iter()))
            .map_err(mxfuse_error)
    }

    fn seek(&mut self, py: Python<'_>, position: i64) -> PyResult<()> {
        let clip = self.clip()?;
        py.allow_threads(|| clip.seek(position))
            .map_err(mxfuse_error)
    }

    #[pyo3(signature = (count=1))]
    fn read(&mut self, py: Python<'_>, count: u32) -> PyResult<Vec<PyPackage>> {
        let clip = self.clip()?;
        let packages = py
            .allow_threads(|| clip.read(count))
            .map_err(mxfuse_error)?;
        Ok(packages
            .into_iter()
            .map(|package| PyPackage {
                frames: package
                    .frames
                    .into_iter()
                    .map(|frame| PyFrame {
                        data: frame.data,
                        element_key: frame.element_key.to_vec(),
                        file_position: frame.file_position,
                        kl_size: frame.kl_size,
                        position: frame.position,
                    })
                    .collect(),
            })
            .collect())
    }

    fn close(&mut self) {
        self.inner = None;
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __exit__(
        &mut self,
        _exc_type: Bound<'_, PyAny>,
        _exc: Bound<'_, PyAny>,
        _tb: Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.close();
        Ok(false)
    }
}

#[pyclass(name = "Track")]
#[derive(Clone)]
struct PyTrack {
    #[pyo3(get)]
    index: usize,
    #[pyo3(get)]
    kind: String,
    #[pyo3(get)]
    essence_type: String,
    #[pyo3(get)]
    essence_container_ul: Vec<u8>,
    #[pyo3(get)]
    edit_rate: (i32, i32),
    #[pyo3(get)]
    duration: i64,
}

#[pyclass(name = "Frame")]
#[derive(Clone)]
struct PyFrame {
    #[pyo3(get)]
    data: Vec<u8>,
    #[pyo3(get)]
    element_key: Vec<u8>,
    #[pyo3(get)]
    file_position: i64,
    #[pyo3(get)]
    kl_size: u8,
    #[pyo3(get)]
    position: i64,
}

#[pyclass(name = "Package")]
#[derive(Clone)]
struct PyPackage {
    #[pyo3(get)]
    frames: Vec<PyFrame>,
}

#[pyfunction]
#[pyo3(signature = (source, read_ahead=1 << 20, cache_bytes=64 << 20))]
fn open_mxf(
    py: Python<'_>,
    source: Py<PyAny>,
    read_ahead: u32,
    cache_bytes: u32,
) -> PyResult<PyClip> {
    let source = PythonSource { handle: source };
    let options = ReadOptions {
        read_ahead,
        cache_bytes,
    };
    let clip = py
        .allow_threads(|| mxfuse::open_mxf(source, options))
        .map_err(|error| {
            if error.message().contains("failed") {
                PyIOError::new_err(error.to_string())
            } else {
                mxfuse_error(error)
            }
        })?;
    Ok(PyClip { inner: Some(clip) })
}

fn collect_indexes(py: Python<'_>, tracks: Bound<'_, PyAny>) -> PyResult<Vec<usize>> {
    let iter = PyIterator::from_object(&tracks)?;
    let mut indexes = Vec::new();
    for item in iter {
        let item = item?;
        if let Ok(track) = item.extract::<PyRef<PyTrack>>() {
            indexes.push(track.index);
        } else if let Ok(index) = item.extract::<usize>() {
            indexes.push(index);
        } else {
            return Err(PyValueError::new_err(
                "select() expects Track objects or integer indexes",
            ));
        }
    }
    let _ = py;
    Ok(indexes)
}

fn kind_name(kind: TrackKind) -> String {
    match kind {
        TrackKind::Picture => "picture",
        TrackKind::Sound => "sound",
        TrackKind::Data => "data",
        TrackKind::Other => "other",
    }
    .to_string()
}

struct PythonSink {
    handle: Py<PyAny>,
}

impl ByteSink for PythonSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Python::with_gil(|py| {
            let written = self
                .handle
                .bind(py)
                .call_method1("write", (buf,))
                .map_err(py_io_error)?;
            if written.is_none() {
                return Ok(buf.len());
            }
            written.extract::<usize>().map_err(py_io_error)
        })
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        seek_python_handle(&self.handle, pos)
    }

    fn tell(&mut self) -> io::Result<u64> {
        Python::with_gil(|py| {
            self.handle
                .bind(py)
                .call_method0("tell")
                .and_then(|value| value.extract::<u64>())
                .map_err(py_io_error)
        })
    }

    fn is_seekable(&self) -> bool {
        Python::with_gil(|py| {
            let handle = self.handle.bind(py);
            if handle.hasattr("seekable").ok() == Some(true) {
                return handle
                    .call_method0("seekable")
                    .and_then(|value| value.extract::<bool>())
                    .unwrap_or(true);
            }
            handle.hasattr("seek").unwrap_or(false)
        })
    }
}

#[pyclass(name = "Writer", unsendable)]
struct PyWriter {
    inner: Option<mxfuse::ClipWriter>,
}

impl PyWriter {
    fn writer(&mut self) -> PyResult<&mut mxfuse::ClipWriter> {
        self.inner
            .as_mut()
            .ok_or_else(|| PyValueError::new_err("writer is closed"))
    }
}

#[pymethods]
impl PyWriter {
    fn write(&mut self, py: Python<'_>, track_index: usize, data: &[u8]) -> PyResult<()> {
        let writer = self.writer()?;
        py.allow_threads(|| writer.write(track_index, data))
            .map_err(mxfuse_error)
    }

    fn finish(&mut self, py: Python<'_>) -> PyResult<()> {
        let writer = self
            .inner
            .take()
            .ok_or_else(|| PyValueError::new_err("writer is closed"))?;
        py.allow_threads(|| writer.finish()).map_err(mxfuse_error)
    }

    fn close(&mut self) {
        self.inner = None;
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __exit__(
        &mut self,
        py: Python<'_>,
        exc_type: Bound<'_, PyAny>,
        _exc: Bound<'_, PyAny>,
        _tb: Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        if self.inner.is_some() {
            if exc_type.is_none() {
                self.finish(py)?;
            } else {
                self.close();
            }
        }
        Ok(false)
    }
}

#[pyfunction]
fn write_mxf(py: Python<'_>, sink: Py<PyAny>, spec: Bound<'_, PyAny>) -> PyResult<PyWriter> {
    let clip_spec = clip_spec_from_py(py, spec)?;
    let sink = PythonSink { handle: sink };
    let writer = py
        .allow_threads(|| mxfuse::write_mxf(sink, clip_spec))
        .map_err(mxfuse_error)?;
    Ok(PyWriter {
        inner: Some(writer),
    })
}

fn clip_spec_from_py(py: Python<'_>, spec: Bound<'_, PyAny>) -> PyResult<ClipSpec> {
    let edit_rate: (i32, i32) = spec.getattr("edit_rate")?.extract()?;
    let flavour = spec
        .getattr("flavour")
        .ok()
        .map(|value| flavour_from_py(&value))
        .unwrap_or(0);
    let duration = spec.getattr("duration").ok().and_then(|value| {
        if value.is_none() {
            None
        } else {
            value.extract::<i64>().ok()
        }
    });
    let tracks_obj = spec.getattr("tracks")?;
    let iter = PyIterator::from_object(&tracks_obj)?;
    let mut tracks = Vec::new();
    for item in iter {
        tracks.push(track_spec_from_py(item?)?);
    }
    let _ = py;
    Ok(ClipSpec {
        edit_rate: Rational {
            num: edit_rate.0,
            den: edit_rate.1,
        },
        flavour: Flavour(flavour),
        duration,
        tracks,
    })
}

fn track_spec_from_py(track: Bound<'_, PyAny>) -> PyResult<TrackSpec> {
    let essence = track.getattr("essence_type")?;
    let essence_type = if let Ok(value) = essence.extract::<i32>() {
        EssenceType::from_i32(value)
    } else {
        EssenceType::from_i32(essence.getattr("value")?.extract::<i32>()?)
    };
    let mut spec = TrackSpec::new(essence_type);
    spec.sampling_rate = optional_u32(&track, "sampling_rate")?;
    spec.channel_count = optional_u32(&track, "channel_count")?;
    spec.quantization_bits = optional_u32(&track, "quantization_bits")?;
    spec.stored_width = optional_u32(&track, "stored_width")?;
    spec.stored_height = optional_u32(&track, "stored_height")?;
    spec.essence_container_ul = optional_ul(&track, "essence_container_ul")?;
    spec.picture_coding_ul = optional_ul(&track, "picture_coding_ul")?;
    Ok(spec)
}

fn flavour_from_py(value: &Bound<'_, PyAny>) -> i32 {
    if let Ok(flavour) = value.extract::<i32>() {
        return flavour;
    }
    value
        .call_method0("__int__")
        .ok()
        .and_then(|converted| converted.extract::<i32>().ok())
        .unwrap_or(0)
}

fn optional_u32(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<u32>> {
    let value = obj.getattr(name)?;
    if value.is_none() {
        Ok(None)
    } else {
        Ok(Some(value.extract()?))
    }
}

fn optional_ul(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<[u8; 16]>> {
    let value = obj.getattr(name)?;
    if value.is_none() {
        return Ok(None);
    }
    let bytes: Vec<u8> = value.extract()?;
    let array: [u8; 16] = bytes
        .try_into()
        .map_err(|_| PyValueError::new_err(format!("{name} must be 16 bytes")))?;
    Ok(Some(array))
}

#[pymodule]
fn _mxfuse(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(open_mxf, m)?)?;
    m.add_function(wrap_pyfunction!(write_mxf, m)?)?;
    m.add_class::<PyClip>()?;
    m.add_class::<PyWriter>()?;
    m.add_class::<PyTrack>()?;
    m.add_class::<PyFrame>()?;
    m.add_class::<PyPackage>()?;
    Ok(())
}
