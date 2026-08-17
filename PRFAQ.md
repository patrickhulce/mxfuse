# `mxfuse`

Today we're proud to release `mxfuse`, a rust-python-node library that enables effortless MXF creation and consumption. Built on top of the bmx C++ reference implementation, `mxfuse` enables low-level access to MXF encoding details, private image codecs, and async streaming benefits all while conforming to the MXF specification and each language's respective conventions. With `mxfuse`, you can extract just a single frame of a remote S3 500GB IMF by reading only a few MB. With `mxfuse`, you can encode a clip in your company's proprietary image compression codec without heavy-handed C++ implementations or resorting to a fork of bmx. With `mxfuse`, you can stream the results of an unreadable container to ffplay and visualize complex image data in real-time.

## Limitations

- OP1A Only
- Frame-Wrapped Only

## Usage

Getting started is easy, `mxfuse` is a single package available via the cargo, npm, and pypi package registries.

```bash
cargo add mxfuse
npm i mxfuse
uv add mxfuse
```

### Python

#### Read an MXF File

```python
from mxfuse import demux, ContainerMetadata, TrackMetadata, DemuxOptions, DecodeMode

options = DemuxOptions(
    frame_buffer=8,
    decode_mode=DecodeMode.RAW,
)

with open("input.mxf") as f:
    # Parse the initial structure of the container
    container = demux(src=f, options=options)
    assert isinstance(container.metadata, ContainerMetadata)

    # Inspect the different available tracks
    for track in container.tracks:
        assert isinstance(track.metadata, TrackMetadata)
        
    # Use the frames generator to only read the bytes off `f` that we need
    for frame in container.frames:
        # Read the essence for each track associated with the frame
        for content in frame.read(container.tracks)
            assert isinstance(content, (TrackContentPicture,TrackContentAudio,TrackContentSystem,TrackContentData))
            assert isinstance(content.ul, bytes)
            assert isinstance(content.data, bytes)
```

#### Write an MXF File

```python
from mxfuse import mux, ContainerMetadata, MXFrame, MXFChunk


def mxf_writer() -> Generator[MXFChunk]:
    # Write the important container-wide metadata
    yield ContainerMetadata(fps=...)

    # Write each frame out
    for image, audio, metadata in zip(frames, audios, timeline):
        yield TrackContentPicture.from_essence(data=image)
        yield TrackContentAudio.from_pcm(samples=audio)
        yield TrackContentData.from_essence(data=metadata)

with open("output.mxf") as f:
    mux(src=mxf_writer, dst=f)

```

#### 1:1 BMX API

```python
from mxfuse import mxf2raw, raw2mxf
from mxfuse.bmx import BMXMetadata

raw_iterator = mxf2raw(open('path/to/file'))
mxf_byte_chunk_iterator = raw2mxf(raw_iterator, metadata=BMXMetadata())
```
