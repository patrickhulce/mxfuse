//! Language-neutral MXF reader built on a statically linked bmx core.
//!
//! Essence goes in and essence comes out. A frame is the KLV payload with the
//! key and length stripped. The core is synchronous: one reader per thread.

mod error;
mod reader;
mod source;
mod types;

pub use error::{Error, Result};
pub use reader::{open_mxf, Clip};
pub use source::{ByteSource, CountingSource, ReadAhead};
pub use types::{EssenceType, Frame, Package, Rational, ReadOptions, Track, TrackKind};
