use std::fs::File;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// A synchronous random-access byte source.
///
/// libMXF's `read` callback cannot `await`, so every source is blocking.
/// Language façades that expose async I/O run these calls on a worker thread.
pub trait ByteSource: Send {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64>;
    fn size(&mut self) -> io::Result<u64>;
}

impl ByteSource for File {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Read::read(self, buf)
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        Seek::seek(self, pos)
    }

    fn size(&mut self) -> io::Result<u64> {
        self.metadata().map(|metadata| metadata.len())
    }
}

impl ByteSource for Cursor<Vec<u8>> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Read::read(self, buf)
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        Seek::seek(self, pos)
    }

    fn size(&mut self) -> io::Result<u64> {
        Ok(self.get_ref().len() as u64)
    }
}

/// A synchronous byte sink for writing MXF.
///
/// `SINGLE_PASS` flavour never seeks backward, so a non-seekable sink
/// (pipe, socket) is valid when duration is known and the codec is CBE.
pub trait ByteSink: Send {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize>;
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64>;
    fn tell(&mut self) -> io::Result<u64>;
    fn is_seekable(&self) -> bool {
        true
    }
}

impl ByteSink for File {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Write::write(self, buf)
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        Seek::seek(self, pos)
    }

    fn tell(&mut self) -> io::Result<u64> {
        Seek::seek(self, SeekFrom::Current(0))
    }
}

impl ByteSink for Cursor<Vec<u8>> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Write::write(self, buf)
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        Seek::seek(self, pos)
    }

    fn tell(&mut self) -> io::Result<u64> {
        Ok(self.position())
    }
}

impl ByteSource for Cursor<&[u8]> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Read::read(self, buf)
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        Seek::seek(self, pos)
    }

    fn size(&mut self) -> io::Result<u64> {
        Ok(self.get_ref().len() as u64)
    }
}

/// A byte source that records how many `read` calls were issued.
pub struct CountingSource<S> {
    inner: S,
    pub reads: Arc<AtomicUsize>,
    pub bytes: Arc<AtomicUsize>,
}

impl<S: ByteSource> CountingSource<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            reads: Arc::new(AtomicUsize::new(0)),
            bytes: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn read_count(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }

    pub fn byte_count(&self) -> usize {
        self.bytes.load(Ordering::SeqCst)
    }
}

/// A byte source that records the file-offset range of every `read`.
///
/// Position is tracked across `seek` so each served slice can be logged as
/// `[pos, pos + n)`. The log is published through an `Arc` so tests can
/// inspect demand after `open_mxf` takes ownership of the source.
pub struct RecordingSource<S> {
    inner: S,
    pos: u64,
    pub ranges: Arc<Mutex<Vec<Range<u64>>>>,
}

impl<S: ByteSource> RecordingSource<S> {
    pub fn new(mut inner: S) -> io::Result<Self> {
        let pos = inner.seek(SeekFrom::Current(0))?;
        Ok(Self {
            inner,
            pos,
            ranges: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn ranges(&self) -> Vec<Range<u64>> {
        self.ranges
            .lock()
            .expect("recording source lock poisoned")
            .clone()
    }

    pub fn bytes_read(&self) -> u64 {
        self.ranges()
            .iter()
            .map(|range| range.end - range.start)
            .sum()
    }
}

impl<S: ByteSource> ByteSource for RecordingSource<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.ranges
                .lock()
                .expect("recording source lock poisoned")
                .push(self.pos..self.pos + n as u64);
            self.pos += n as u64;
        }
        Ok(n)
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.pos = self.inner.seek(pos)?;
        Ok(self.pos)
    }

    fn size(&mut self) -> io::Result<u64> {
        self.inner.size()
    }
}

impl<S: ByteSource> ByteSource for CountingSource<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let n = self.inner.read(buf)?;
        self.bytes.fetch_add(n, Ordering::SeqCst);
        Ok(n)
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }

    fn size(&mut self) -> io::Result<u64> {
        self.inner.size()
    }
}

/// Read-ahead window modelled on bmx's HTTP file reader.
///
/// A short read pulls a window; a seek that lands inside the window does not
/// issue another request.
pub struct ReadAhead<S> {
    inner: S,
    window: usize,
    buf: Vec<u8>,
    buf_pos: u64,
    buf_valid: usize,
    pos: u64,
    inner_pos: Option<u64>,
}

impl<S: ByteSource> ReadAhead<S> {
    pub fn inner(&self) -> &S {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    pub fn new(mut inner: S, window: usize) -> io::Result<Self> {
        let pos = inner.seek(SeekFrom::Start(0))?;
        Ok(Self {
            inner,
            window,
            buf: Vec::new(),
            buf_pos: 0,
            buf_valid: 0,
            pos,
            inner_pos: Some(pos),
        })
    }

    fn fill(&mut self, min_len: usize) -> io::Result<usize> {
        let want = self.window.max(min_len);
        if self.inner_pos != Some(self.pos) {
            self.inner.seek(SeekFrom::Start(self.pos))?;
            self.inner_pos = Some(self.pos);
        }
        self.buf.resize(want, 0);
        let n = self.inner.read(&mut self.buf)?;
        self.buf_valid = n;
        self.buf_pos = self.pos;
        self.inner_pos = Some(self.pos + n as u64);
        Ok(n)
    }
}

impl<S: ByteSource> ByteSource for ReadAhead<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.window == 0 {
            if self.inner_pos != Some(self.pos) {
                self.inner.seek(SeekFrom::Start(self.pos))?;
                self.inner_pos = Some(self.pos);
            }
            let n = self.inner.read(buf)?;
            self.pos += n as u64;
            self.inner_pos = Some(self.pos);
            return Ok(n);
        }

        if self.pos >= self.buf_pos && self.pos < self.buf_pos + self.buf_valid as u64 {
            let offset = (self.pos - self.buf_pos) as usize;
            let available = self.buf_valid - offset;
            let n = available.min(buf.len());
            buf[..n].copy_from_slice(&self.buf[offset..offset + n]);
            self.pos += n as u64;
            return Ok(n);
        }

        let n = self.fill(buf.len())?;
        let take = n.min(buf.len());
        buf[..take].copy_from_slice(&self.buf[..take]);
        self.pos += take as u64;
        Ok(take)
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::Current(delta) => {
                let signed = i128::from(self.pos) + i128::from(delta);
                if signed < 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "seek before start of file",
                    ));
                }
                signed as u64
            }
            SeekFrom::End(delta) => {
                let size = self.inner.size()?;
                let signed = i128::from(size) + i128::from(delta);
                if signed < 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "seek before start of file",
                    ));
                }
                signed as u64
            }
        };
        self.pos = target;
        Ok(target)
    }

    fn size(&mut self) -> io::Result<u64> {
        self.inner.size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_ahead_serves_from_the_window() {
        let data: Vec<u8> = (0..64).collect();
        let counting = CountingSource::new(Cursor::new(data.clone()));
        let mut source = ReadAhead::new(counting, 16).unwrap();

        let mut first = [0u8; 4];
        assert_eq!(source.read(&mut first).unwrap(), 4);
        assert_eq!(&first, &[0, 1, 2, 3]);
        assert_eq!(source.inner().read_count(), 1);

        let mut second = [0u8; 4];
        assert_eq!(source.read(&mut second).unwrap(), 4);
        assert_eq!(&second, &[4, 5, 6, 7]);
        assert_eq!(source.inner().read_count(), 1);

        source.seek(SeekFrom::Start(2)).unwrap();
        let mut third = [0u8; 2];
        assert_eq!(source.read(&mut third).unwrap(), 2);
        assert_eq!(&third, &[2, 3]);
        assert_eq!(source.inner().read_count(), 1);
    }

    #[test]
    fn read_ahead_refills_after_a_miss() {
        let data: Vec<u8> = (0..64).collect();
        let counting = CountingSource::new(Cursor::new(data));
        let mut source = ReadAhead::new(counting, 8).unwrap();

        let mut buf = [0u8; 2];
        source.read(&mut buf).unwrap();
        source.seek(SeekFrom::Start(40)).unwrap();
        source.read(&mut buf).unwrap();
        assert_eq!(source.inner().read_count(), 2);
        assert_eq!(&buf, &[40, 41]);
    }

    #[test]
    fn recording_source_logs_served_ranges() {
        let data: Vec<u8> = (0..32).collect();
        let mut source = RecordingSource::new(Cursor::new(data)).unwrap();
        let mut buf = [0u8; 4];
        assert_eq!(source.read(&mut buf).unwrap(), 4);
        source.seek(SeekFrom::Start(10)).unwrap();
        assert_eq!(source.read(&mut buf).unwrap(), 4);
        assert_eq!(source.ranges(), vec![0..4, 10..14]);
        assert_eq!(source.bytes_read(), 8);
    }
}
