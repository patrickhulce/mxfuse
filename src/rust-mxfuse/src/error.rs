use std::fmt;
use std::io;

use mxfuse_sys::MxfuseError;

/// An error produced by the bmx-backed reader or a byte source.
#[derive(Debug)]
pub struct Error {
    message: String,
}

impl Error {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub(crate) fn from_shim(err: &MxfuseError) -> Self {
        let bytes = err
            .message
            .iter()
            .take_while(|b| **b != 0)
            .map(|b| *b as u8)
            .collect::<Vec<_>>();
        let message = String::from_utf8_lossy(&bytes).into_owned();
        if message.is_empty() {
            Self::new("bmx operation failed")
        } else {
            Self { message }
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::new(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
