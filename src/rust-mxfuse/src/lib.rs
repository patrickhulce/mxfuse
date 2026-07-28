//! Language-neutral media model and I/O contracts for mxfuse.
//!
//! MXF parsing and encoding intentionally remain unimplemented in this initial
//! scaffold. The public types establish the API that the Python and Node
//! packages adapt for their respective runtimes.

use std::collections::BTreeMap;
use std::fmt;
use std::io::{Read, Seek, Write};

/// Controls how essence frames are exposed by a decoded container.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DecodeMode {
    /// Yield the encoded essence bytes exactly as stored in the container.
    #[default]
    Raw,
    /// Decode known essence formats into pixels and fall back to raw bytes.
    Parsed,
}

/// Extensible metadata attached to a container or track.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Metadata {
    values: BTreeMap<String, String>,
}

impl Metadata {
    pub fn new(values: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            values: values.into_iter().collect(),
        }
    }

    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }
}

/// The representation carried by a frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameKind {
    RawEssence,
    Pixels,
}

/// A single essence frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub kind: FrameKind,
    pub data: Vec<u8>,
}

impl Frame {
    pub fn raw(data: impl Into<Vec<u8>>) -> Self {
        Self {
            kind: FrameKind::RawEssence,
            data: data.into(),
        }
    }

    pub fn pixels(data: impl Into<Vec<u8>>) -> Self {
        Self {
            kind: FrameKind::Pixels,
            data: data.into(),
        }
    }
}

/// A media track and its lazily iterable essence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Track {
    pub id: u32,
    pub codec: Option<String>,
    pub metadata: Metadata,
    frames: Vec<Frame>,
}

impl Track {
    pub fn new(id: u32, codec: Option<String>, metadata: Metadata, frames: Vec<Frame>) -> Self {
        Self {
            id,
            codec,
            metadata,
            frames,
        }
    }

    /// Iterate frames without copying their payloads.
    ///
    /// The backing store is in-memory for now. A future decoder can replace it
    /// with an index into the source handle without changing this public shape.
    pub fn frames(&self) -> impl Iterator<Item = &Frame> {
        self.frames.iter()
    }
}

/// An MXF container with track and container-level metadata.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Container {
    pub mode: DecodeMode,
    pub metadata: Metadata,
    pub tracks: Vec<Track>,
}

impl Container {
    pub fn new(mode: DecodeMode, metadata: Metadata, tracks: Vec<Track>) -> Self {
        Self {
            mode,
            metadata,
            tracks,
        }
    }

    /// Lazily flatten the frames from all tracks.
    pub fn frames(&self) -> impl Iterator<Item = &Frame> {
        self.tracks.iter().flat_map(Track::frames)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    Encode,
    Decode,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Error {
    operation: Operation,
}

impl Error {
    fn scaffold(operation: Operation) -> Self {
        Self { operation }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?} is not implemented in the initial mxfuse scaffold",
            self.operation
        )
    }
}

impl std::error::Error for Error {}

/// Decode an MXF source into a lazily traversable container.
///
/// The generic source is intentionally compatible with local files, in-memory
/// buffers, and remote/random-access adapters.
pub fn decode<R: Read + Seek>(_source: &mut R, _mode: DecodeMode) -> Result<Container, Error> {
    Err(Error::scaffold(Operation::Decode))
}

/// Encode a container to a file-like destination.
pub fn encode<W: Write + Seek>(_container: &Container, _destination: &mut W) -> Result<(), Error> {
    Err(Error::scaffold(Operation::Encode))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_lazily_flattens_track_frames() {
        let tracks = vec![
            Track::new(
                1,
                Some("jpeg2000".into()),
                Metadata::default(),
                vec![Frame::raw([1]), Frame::raw([2])],
            ),
            Track::new(
                2,
                Some("pcm".into()),
                Metadata::default(),
                vec![Frame::raw([3])],
            ),
        ];
        let container = Container::new(DecodeMode::Raw, Metadata::default(), tracks);

        let payloads: Vec<&[u8]> = container
            .frames()
            .map(|frame| frame.data.as_slice())
            .collect();

        assert_eq!(payloads, vec![&[1][..], &[2][..], &[3][..]]);
    }

    #[test]
    fn io_operations_are_explicit_scaffold_seams() {
        let mut source = std::io::Cursor::new(Vec::<u8>::new());
        assert_eq!(
            decode(&mut source, DecodeMode::Raw).unwrap_err().operation,
            Operation::Decode
        );

        let mut destination = std::io::Cursor::new(Vec::<u8>::new());
        assert_eq!(
            encode(&Container::default(), &mut destination)
                .unwrap_err()
                .operation,
            Operation::Encode
        );
    }
}
