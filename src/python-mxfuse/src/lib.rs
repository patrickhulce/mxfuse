use std::io::{self, SeekFrom};

use mxfuse::{ByteSource, ReadOptions, TrackKind};
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

#[pymodule]
fn _mxfuse(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(open_mxf, m)?)?;
    m.add_class::<PyClip>()?;
    m.add_class::<PyTrack>()?;
    m.add_class::<PyFrame>()?;
    m.add_class::<PyPackage>()?;
    Ok(())
}
