//! Language-neutral MXF reader and writer built on a statically linked bmx core.
//!
//! Essence goes in and essence comes out. A frame is the KLV payload with the
//! key and length stripped. The core is synchronous: one reader or writer per
//! thread.

mod error;
mod reader;
mod source;
mod types;
mod writer;

pub use error::{Error, Result};
pub use reader::{open_mxf, Clip};
pub use source::{ByteSink, ByteSource, CountingSource, ReadAhead};
pub use types::{
    ClipSpec, EssenceType, Flavour, Frame, Package, Rational, ReadOptions, Track, TrackKind,
    TrackSpec,
};
pub use writer::{write_mxf, ClipWriter};
