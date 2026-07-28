use std::io::Cursor;

use pyo3::exceptions::PyNotImplementedError;
use pyo3::prelude::*;

#[pyfunction]
fn decode_scaffold(mode: &str) -> PyResult<()> {
    let mode = match mode {
        "raw" => mxfuse::DecodeMode::Raw,
        "parsed" => mxfuse::DecodeMode::Parsed,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "mode must be 'raw' or 'parsed'",
            ));
        }
    };
    let mut source = Cursor::new(Vec::<u8>::new());
    mxfuse::decode(&mut source, mode)
        .map(|_| ())
        .map_err(|error| PyNotImplementedError::new_err(error.to_string()))
}

#[pyfunction]
fn encode_scaffold() -> PyResult<()> {
    let container = mxfuse::Container::default();
    let mut destination = Cursor::new(Vec::<u8>::new());
    mxfuse::encode(&container, &mut destination)
        .map_err(|error| PyNotImplementedError::new_err(error.to_string()))
}

#[pymodule]
fn _mxfuse(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(decode_scaffold, m)?)?;
    m.add_function(wrap_pyfunction!(encode_scaffold, m)?)?;
    Ok(())
}
