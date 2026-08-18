use std::io::{self, SeekFrom};

use mxfuse::{
    ByteSink, ByteSource, ClipSpec, DescriptorKind, EssenceType, Flavour, Identity, PixelComponent,
    Rational, ReadOptions, Timecode, TrackKind, TrackSpec, XmlMetadata,
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
    fn start_timecode(&mut self) -> PyResult<Option<PyTimecode>> {
        Ok(self.clip()?.start_timecode().map(|tc| PyTimecode {
            hour: tc.hour,
            minute: tc.minute,
            second: tc.second,
            frame: tc.frame,
            drop_frame: tc.drop_frame,
        }))
    }

    #[getter]
    fn tracks(&mut self, py: Python<'_>) -> PyResult<Py<PyList>> {
        let tracks = self.clip()?.tracks().to_vec();
        let list = PyList::empty(py);
        for track in tracks {
            list.append(py_track_from(track))?;
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
                        track_index: frame.track_index,
                    })
                    .collect(),
            })
            .collect())
    }

    fn close(&mut self) {
        self.inner = None;
    }

    #[getter]
    fn xml(&mut self) -> PyResult<Vec<PyXmlMetadata>> {
        let clip = self.clip()?;
        Ok(clip
            .xml()
            .iter()
            .map(|item| PyXmlMetadata {
                data: item.data.clone(),
                scheme_id: item.scheme_id.map(|ul| ul.to_vec()).unwrap_or_default(),
                language: item.language.clone().unwrap_or_default(),
                namespace: item.namespace.clone().unwrap_or_default(),
                mime_type: item.mime_type.clone().unwrap_or_default(),
                is_xml: item.is_xml,
            })
            .collect())
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

#[pyclass(name = "Timecode")]
#[derive(Clone)]
struct PyTimecode {
    #[pyo3(get)]
    hour: i16,
    #[pyo3(get)]
    minute: i16,
    #[pyo3(get)]
    second: i16,
    #[pyo3(get)]
    frame: i16,
    #[pyo3(get)]
    drop_frame: bool,
}

#[pyclass(name = "PixelComponent")]
#[derive(Clone)]
struct PyPixelComponent {
    #[pyo3(get)]
    code: u8,
    #[pyo3(get)]
    depth: u8,
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
    coding_ul: Option<Vec<u8>>,
    #[pyo3(get)]
    descriptor: i32,
    #[pyo3(get)]
    stored_width: Option<u32>,
    #[pyo3(get)]
    stored_height: Option<u32>,
    #[pyo3(get)]
    display_width: Option<u32>,
    #[pyo3(get)]
    display_height: Option<u32>,
    #[pyo3(get)]
    component_depth: Option<u32>,
    #[pyo3(get)]
    subsampling: Option<(u32, u32)>,
    #[pyo3(get)]
    frame_layout: Option<u8>,
    #[pyo3(get)]
    aspect_ratio: Option<(i32, i32)>,
    #[pyo3(get)]
    video_line_map: Option<(i32, i32)>,
    #[pyo3(get)]
    pixel_layout: Vec<PyPixelComponent>,
    #[pyo3(get)]
    color_primaries: Option<Vec<u8>>,
    #[pyo3(get)]
    transfer_characteristic: Option<Vec<u8>>,
    #[pyo3(get)]
    coding_equations: Option<Vec<u8>>,
    #[pyo3(get)]
    sampling_rate: Option<u32>,
    #[pyo3(get)]
    channel_count: Option<u32>,
    #[pyo3(get)]
    quantization_bits: Option<u32>,
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
    #[pyo3(get)]
    track_index: usize,
}

#[pyclass(name = "XmlMetadata")]
#[derive(Clone)]
struct PyXmlMetadata {
    #[pyo3(get)]
    data: Vec<u8>,
    #[pyo3(get)]
    scheme_id: Vec<u8>,
    #[pyo3(get)]
    language: String,
    #[pyo3(get)]
    namespace: String,
    #[pyo3(get)]
    mime_type: String,
    #[pyo3(get)]
    is_xml: bool,
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

fn py_track_from(track: mxfuse::Track) -> PyTrack {
    PyTrack {
        index: track.index,
        kind: kind_name(track.kind),
        essence_type: track.essence_type.name().to_string(),
        essence_container_ul: track.essence_container_ul.to_vec(),
        coding_ul: track.coding_ul.map(|ul| ul.to_vec()),
        descriptor: track.descriptor.as_i32(),
        stored_width: track.stored_width,
        stored_height: track.stored_height,
        display_width: track.display_width,
        display_height: track.display_height,
        component_depth: track.component_depth,
        subsampling: track.subsampling,
        frame_layout: track.frame_layout,
        aspect_ratio: track.aspect_ratio.map(|ratio| (ratio.num, ratio.den)),
        video_line_map: track.video_line_map,
        pixel_layout: track
            .pixel_layout
            .into_iter()
            .map(|item| PyPixelComponent {
                code: item.code,
                depth: item.depth,
            })
            .collect(),
        color_primaries: track.color_primaries.map(|ul| ul.to_vec()),
        transfer_characteristic: track.transfer_characteristic.map(|ul| ul.to_vec()),
        coding_equations: track.coding_equations.map(|ul| ul.to_vec()),
        sampling_rate: track.sampling_rate,
        channel_count: track.channel_count,
        quantization_bits: track.quantization_bits,
        edit_rate: (track.edit_rate.num, track.edit_rate.den),
        duration: track.duration,
    }
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
    let xml = match spec.getattr("xml") {
        Ok(obj) if !obj.is_none() => xml_list_from_py(&obj)?,
        _ => Vec::new(),
    };
    let start_timecode = match spec.getattr("start_timecode") {
        Ok(value) if !value.is_none() => Some(timecode_from_py(&value)?),
        _ => None,
    };
    let timecode_track = match spec.getattr("timecode_track") {
        Ok(value) if !value.is_none() => value.extract::<bool>()?,
        _ => true,
    };
    let system_item = match spec.getattr("system_item") {
        Ok(value) if !value.is_none() => value.extract::<bool>()?,
        _ => false,
    };
    let identity = match spec.getattr("identity") {
        Ok(value) if !value.is_none() => Some(identity_from_py(&value)?),
        _ => None,
    };
    let _ = py;
    Ok(ClipSpec {
        edit_rate: Rational {
            num: edit_rate.0,
            den: edit_rate.1,
        },
        flavour: Flavour(flavour),
        duration,
        tracks,
        xml,
        start_timecode,
        timecode_track,
        system_item,
        identity,
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
    spec.coding_ul = optional_ul(&track, "coding_ul")?;
    spec.element_type = optional_u8(&track, "element_type")?;
    spec.element_llen = optional_u8(&track, "element_llen")?;
    spec.temporal_reordering = optional_bool(&track, "temporal_reordering")?.unwrap_or(false);
    spec.descriptor = optional_descriptor(&track, "descriptor")?;
    spec.component_depth = optional_u32(&track, "component_depth")?;
    spec.subsampling = optional_pair_u32(&track, "subsampling")?;
    spec.frame_layout = optional_u8(&track, "frame_layout")?;
    spec.aspect_ratio = optional_rational(&track, "aspect_ratio")?;
    spec.video_line_map = optional_pair_i32(&track, "video_line_map")?;
    spec.pixel_layout = optional_pixel_layout(&track)?;
    spec.color_primaries = optional_ul(&track, "color_primaries")?;
    spec.transfer_characteristic = optional_ul(&track, "transfer_characteristic")?;
    spec.coding_equations = optional_ul(&track, "coding_equations")?;
    Ok(spec)
}

fn xml_list_from_py(obj: &Bound<'_, PyAny>) -> PyResult<Vec<XmlMetadata>> {
    let iter = PyIterator::from_object(obj)?;
    let mut items = Vec::new();
    for item in iter {
        items.push(xml_from_py(item?)?);
    }
    Ok(items)
}

fn xml_from_py(obj: Bound<'_, PyAny>) -> PyResult<XmlMetadata> {
    let data: Vec<u8> = obj.getattr("data")?.extract()?;
    let mut item = XmlMetadata::new(data);
    item.scheme_id = optional_ul(&obj, "scheme_id")?;
    if let Ok(value) = obj.getattr("language") {
        if !value.is_none() {
            let text: String = value.extract()?;
            if !text.is_empty() {
                item.language = Some(text);
            }
        }
    }
    if let Ok(value) = obj.getattr("namespace") {
        if !value.is_none() {
            let text: String = value.extract()?;
            if !text.is_empty() {
                item.namespace = Some(text);
            }
        }
    }
    Ok(item)
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

fn optional_bool(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<bool>> {
    match obj.getattr(name) {
        Ok(value) if !value.is_none() => Ok(Some(value.extract()?)),
        _ => Ok(None),
    }
}

fn optional_u8(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<u8>> {
    match obj.getattr(name) {
        Ok(value) if !value.is_none() => Ok(Some(value.extract()?)),
        _ => Ok(None),
    }
}

fn optional_descriptor(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<DescriptorKind>> {
    let Ok(value) = obj.getattr(name) else {
        return Ok(None);
    };
    if value.is_none() {
        return Ok(None);
    }
    let number = if let Ok(raw) = value.extract::<i32>() {
        raw
    } else {
        value.getattr("value")?.extract::<i32>()?
    };
    Ok(Some(DescriptorKind::from_i32(number)))
}

fn optional_rational(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<Rational>> {
    match obj.getattr(name) {
        Ok(value) if !value.is_none() => {
            let pair: (i32, i32) = value.extract()?;
            Ok(Some(Rational {
                num: pair.0,
                den: pair.1,
            }))
        }
        _ => Ok(None),
    }
}

fn optional_pair_u32(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<(u32, u32)>> {
    match obj.getattr(name) {
        Ok(value) if !value.is_none() => Ok(Some(value.extract()?)),
        _ => Ok(None),
    }
}

fn optional_pair_i32(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<(i32, i32)>> {
    match obj.getattr(name) {
        Ok(value) if !value.is_none() => Ok(Some(value.extract()?)),
        _ => Ok(None),
    }
}

fn optional_pixel_layout(obj: &Bound<'_, PyAny>) -> PyResult<Option<Vec<PixelComponent>>> {
    let Ok(value) = obj.getattr("pixel_layout") else {
        return Ok(None);
    };
    if value.is_none() {
        return Ok(None);
    }
    let iter = PyIterator::from_object(&value)?;
    let mut items = Vec::new();
    for item in iter {
        let item = item?;
        let code = item.getattr("code")?.extract::<u8>()?;
        let depth = item.getattr("depth")?.extract::<u8>()?;
        items.push(PixelComponent { code, depth });
    }
    Ok(Some(items))
}

fn timecode_from_py(obj: &Bound<'_, PyAny>) -> PyResult<Timecode> {
    Ok(Timecode {
        hour: obj.getattr("hour")?.extract()?,
        minute: obj.getattr("minute")?.extract()?,
        second: obj.getattr("second")?.extract()?,
        frame: obj.getattr("frame")?.extract()?,
        drop_frame: obj.getattr("drop_frame")?.extract()?,
    })
}

fn identity_from_py(obj: &Bound<'_, PyAny>) -> PyResult<Identity> {
    Ok(Identity {
        company_name: optional_string(obj, "company_name")?,
        product_name: optional_string(obj, "product_name")?,
        version_string: optional_string(obj, "version_string")?,
        product_version: match obj.getattr("product_version") {
            Ok(value) if !value.is_none() => Some(value.extract()?),
            _ => None,
        },
        product_uid: optional_ul(obj, "product_uid")?,
        creation_date: match obj.getattr("creation_date") {
            Ok(value) if !value.is_none() => Some(value.extract()?),
            _ => None,
        },
        generation_uid: optional_ul(obj, "generation_uid")?,
        material_package_uid: optional_umid(obj, "material_package_uid")?,
        file_source_package_uid: optional_umid(obj, "file_source_package_uid")?,
    })
}

fn optional_string(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<String>> {
    match obj.getattr(name) {
        Ok(value) if !value.is_none() => {
            let text: String = value.extract()?;
            if text.is_empty() {
                Ok(None)
            } else {
                Ok(Some(text))
            }
        }
        _ => Ok(None),
    }
}

fn optional_umid(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<[u8; 32]>> {
    let Ok(value) = obj.getattr(name) else {
        return Ok(None);
    };
    if value.is_none() {
        return Ok(None);
    }
    let bytes: Vec<u8> = value.extract()?;
    let array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| PyValueError::new_err(format!("{name} must be 32 bytes")))?;
    Ok(Some(array))
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
    m.add_class::<PyXmlMetadata>()?;
    m.add_class::<PyTimecode>()?;
    m.add_class::<PyPixelComponent>()?;
    Ok(())
}
