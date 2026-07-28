use std::io::Cursor;

use napi::{Error, Status};
use napi_derive::napi;

#[napi(js_name = "decodeScaffold")]
pub fn decode_scaffold(mode: String) -> napi::Result<()> {
    let mode = match mode.as_str() {
        "raw" => mxfuse::DecodeMode::Raw,
        "parsed" => mxfuse::DecodeMode::Parsed,
        _ => {
            return Err(Error::new(
                Status::InvalidArg,
                "mode must be 'raw' or 'parsed'",
            ));
        }
    };
    let mut source = Cursor::new(Vec::<u8>::new());
    mxfuse::decode(&mut source, mode)
        .map(|_| ())
        .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))
}

#[napi(js_name = "encodeScaffold")]
pub fn encode_scaffold() -> napi::Result<()> {
    let container = mxfuse::Container::default();
    let mut destination = Cursor::new(Vec::<u8>::new());
    mxfuse::encode(&container, &mut destination)
        .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))
}
