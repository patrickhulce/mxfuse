# `mxfuse`

Today we're proud to release `mxfuse`, a rust-python-node library that enables effortless MXF creation and consumption. Built on top of the [bmx](https://github.com/ebu/bmx) C++ reference implementation, `mxfuse` gives each language a conventional, low-level handle on MXF structure — partitions, index tables, essence container ULs, and raw frame payloads — without asking you to write C++.

With `mxfuse`, you can extract a single frame of a remote 500 GB IMF by reading only a few MB. 
With `mxfuse`, you can wrap a clip in your company's proprietary image codec. 
With `mxfuse`, you can reproduce any Generic Container mapping exactly — element key, BER length, descriptor class, and the writable descriptor fields. 
With `mxfuse`, you can read that mapping back: the coding UL and the descriptor survive a round trip. 
With `mxfuse`, you can pin product info, creation date, generation UID, and package UMIDs so Identification and package identity stay under your control. 
With `mxfuse`, you can stream a well-formed OP1a file to ffplay in a single pass.


## Limitations

- **OP1a, frame-wrapped only** for v1. No AS-02, IMF flavour, RDD 9, D-10, Avid OP-Atom, or clip wrapping as a write target.
- **No sub-descriptors.** A private mapping that needs a registered sub-descriptor set (JPEG 2000, JPEG XS, or a future `JXLPictureSubDescriptor`) cannot write or read those items yet.
- **Display and sampled geometry follow stored width/height** on write. They are not independently settable.
- **Essence in, essence out.** No image codec decode or encode. A "frame" is the KLV payload with the key and length stripped.
- **Synchronous core.** One reader per thread. Sharing a single reader across threads requires external locking; bmx mutates position, frame buffers, and index caches on every `Read()`.
- **Single-pass streamable write** (`Flavour.SINGLE_PASS`) requires a known duration, a constant-bytes-per-element codec, partition interval 0, and no timed text. JPEG 2000 and ProRes are typically variable-frame-size and will not produce a closed-complete header in one pass.
- **`WAVE_PCM` is pinned to 48 kHz** in OP1a.
- **Clip-wrapped essence without a complete index table** needs a constant edit unit size that bmx already recognizes (DV, uncompressed, VC-3, WAVE PCM). Unknown codecs in that configuration will not read.
- **`cargo add mxfuse` needs CMake and a C++ toolchain.** Wheels and napi prebuilds are the supported path and statically link the bmx stack (bmx, libMXF, libMXF++, expat, uriparser). On Linux the C++ runtime stays dynamic — `libstdc++.so.6` and `libgcc_s.so.1` are on the manylinux allowlist. A source build does not need `git`, network access at configure time, or `uuid-dev`: those dependencies are vendored or replaced by a shim-provided `uuid_generate`.
- **bmx and libMXF are BSD-3-Clause.** `mxfuse` is MIT; the combined binary carries both.

## Usage

Getting started is easy. `mxfuse` is a single package available via the cargo, npm, and pypi package registries.

```bash
cargo add mxfuse
npm i mxfuse
uv add mxfuse
```

### Python

#### Read an MXF File

```python
from mxfuse import open_mxf, ReadOptions, TrackKind

options = ReadOptions(read_ahead=1 << 20, cache_bytes=64 << 20)

with open("input.mxf", "rb") as f:
    with open_mxf(f, options=options) as clip:
        print(clip.edit_rate, clip.duration)

        for track in clip.tracks:
            print(track.index, track.kind, track.essence_type, track.essence_container_ul)

        # Must precede reading: unselected tracks are never fetched.
        clip.select(t for t in clip.tracks if t.kind is TrackKind.PICTURE)

        clip.seek(1000)
        for package in clip.read(count=1):
            for frame in package.frames:
                frame.data            # essence payload, KL stripped
                frame.element_key     # KLV key
                frame.file_position   # for building an offset map
```

A custom byte source — an S3 range-reader, an HTTP client, a memory buffer — is any object that implements `read`, `seek`, `tell`, and `size` (regular files may omit `size`; it is inferred via `seek(0, 2)`). `open_mxf` wraps it in libMXF's `MXFFile` vtable — eleven function pointers, of which `read`/`seek`/`tell`/`size` are the ones a source must implement — and hands it to `MXFFileReader::Open`. Tune `read_ahead` and `cache_bytes`: without them, index-driven access issues one tiny KLV-header read after another. `make bench` on a 61 MB synthetic OP1a (8,000 edit units, Apple M5 Max) sought the last picture frame in 41,007 reads totaling 127 KB; the same access with the default 1 MB read-ahead and 64 MB cache is 3 reads totaling 1.2 MB. A 1 MB window will also pull neighbouring interleaved sound bytes — set both knobs to 0 when you need payload-level track isolation.

`frame.file_position` for frame-wrapped essence points at the KLV, with `kl_size` giving the header length. Clip-wrapped essence points at the sample data and `kl_size` is 0.

#### Write an MXF File

bmx requires every track to be created, then `PrepareWrite()`, then `WriteSamples(track_index, ...)` in lockstep. Duration for a streamable file must be declared before the first sample. A generator that yields container metadata mid-stream does not map onto that lifecycle.

```python
from mxfuse import ClipSpec, TrackSpec, EssenceType, Flavour, write_mxf

spec = ClipSpec(
    edit_rate=(25, 1),
    flavour=Flavour.SINGLE_PASS,   # streamable; requires duration
    duration=len(images),
    tracks=[
        TrackSpec(EssenceType.UNC_HD_1080P),
        TrackSpec(EssenceType.WAVE_PCM, sampling_rate=48000),
    ],
)

with open("output.mxf", "wb") as f, write_mxf(f, spec) as clip:
    for image, audio in zip(images, audios):
        clip.write(0, image)
        clip.write(1, audio)
```

`Flavour.SINGLE_PASS` writes a closed-complete header up front and never seeks backward, so the destination can be a pipe or any other non-seekable sink. Write fewer or more samples than `duration` and `CompleteWrite` fails. Omit the flavour (or pick a variable-frame-size codec) and the writer will seek back to finish the header — fine for a regular file, fatal for a pipe.

#### Write a private codec

Reading a proprietary essence type needs no patch: unrecognized picture/sound/data degrades to a generic type and the frame bytes come through unchanged. Writing one is the reason `mxfuse` vendors a patched bmx. The patch adds `EssenceType.OPAQUE_PICTURE` (and matching sound/data variants) so you supply the container UL, coding UL, element key, BER length, descriptor class, and every descriptor field yourself:

```python
from mxfuse import (
    ClipSpec,
    DescriptorKind,
    EssenceType,
    PixelComponent,
    TrackSpec,
    XmlMetadata,
    write_mxf,
)

spec = ClipSpec(
    edit_rate=(24, 1),
    duration=len(images),
    system_item=True,
    tracks=[
        TrackSpec(
            EssenceType.OPAQUE_PICTURE,
            essence_container_ul="060e2b340401010d0d01030102700100",
            coding_ul="060e2b340401010d0401020270000000",
            stored_width=4096,
            stored_height=2160,
            element_type=0x70,
            element_llen=8,
            temporal_reordering=True,
            descriptor=DescriptorKind.RGBA,
            aspect_ratio=(16, 9),
            video_line_map=(1, 0),
            pixel_layout=(
                PixelComponent(code=ord("R"), depth=32),
                PixelComponent(code=ord("G"), depth=32),
                PixelComponent(code=ord("B"), depth=32),
            ),
        ),
    ],
    xml=[XmlMetadata(data=b"<clip xmlns='urn:example'>hello</clip>")],
)

with open("output.mxf", "wb") as f, write_mxf(f, spec) as clip:
    for image in images:
        clip.write(0, image)
```

You do not fork bmx. The opaque type lives in `mxfuse`'s tracked patch set; prebuilt artifacts already include it. On read, `track.coding_ul` and the picture descriptor identify the mapping — a JPEG XL clip is no longer indistinguishable from any other opaque picture.

#### 1:1 bmx surface

`mxfuse.bmx` is a thin binding over the real C++ classes — not the `mxf2raw` / `raw2bmx` command-line apps, and not a fictional `BMXMetadata` type. Use it when the high-level `open_mxf` / `write_mxf` façade hides a knob you need (`SetReadLimits`, `GetHeaderMetadata`, flavour flags, descriptor mutation between `PrepareHeaderMetadata` and `PrepareWrite`).

```python
from mxfuse.bmx import MXFFileReader, ClipWriter, EssenceType, HeaderMetadata

reader = MXFFileReader.open(path)
print(reader.edit_rate, reader.duration, reader.num_track_readers)
reader.set_read_limits()
reader.seek(0)
while True:
    n = reader.read(1)
    if n == 0:
        break
    track = reader.track_reader(0)
    frame = track.frame_buffer.last_frame(pop=True)
    if frame is not None and not frame.is_empty:
        consume(frame.bytes, frame.size)

writer = ClipWriter.open_new_op1a(flavour=0, file=out, frame_rate=(24, 1))
writer.create_track(EssenceType.UNC_HD_1080P)
writer.create_track(EssenceType.WAVE_PCM)
writer.prepare_write()
# ... WriteSamples per track, then CompleteWrite
header: HeaderMetadata = writer.header_metadata
```

## FAQ

### Why bmx rather than FFmpeg or a pure-Rust MXF implementation?

bmx exists to produce and consume specification-compliant MXF, including IMF essence components. It already resolves the material-package → file-source-package → essence-container chain, normalizes edit rates across video and audio, and exposes index-driven random access. FFmpeg treats MXF as one more container and will not give you private-UL write, descriptor-level mutation, or a first-class custom byte source. A from-scratch Rust MXF stack would reimplement the bulk of what bmx's C++ layer adds on top of libMXF — years of work — to reach the same place. Wrapping bmx is the shortest path to a correct file.

### How does a remote or S3 source work?

libMXF's `MXFFile` is a C vtable of eleven function pointers (`close`, `read`, `write`, `get_char`, `put_char`, `eof`, `seek`, `tell`, `is_seekable`, `size`, `free_sys_data`). An S3 range-reader (or any other byte source) implements the read-side slots and plugs in directly, and bmx's index-driven reader seeks to the requested edit unit instead of scanning from the top. `open_mxf` wraps your handle in that vtable and hands it to `MXFFileReader::Open`.

### How do private codecs work if bmx has a closed catalogue?

Reading unknown essence already works unpatched — bmx degrades it to generic picture/sound/data and still returns the raw bytes. Writing it is the part bmx cannot do out of the box: `EssenceType` is a closed enum and `OP1ATrack::Create` is a hardcoded switch. `mxfuse` vendors a materialized `vendor/bmx` tree plus a small tracked patch set under `patches/` that adds one opaque essence type; you consume a prebuilt wheel or napi binary and never fork. The patch is intended for upstream to `ebu/bmx`.

### How is the vendored patch set kept current?

`vendor/bmx` is a materialized ebu/bmx v1.7 tree (not a git submodule), so `cargo package` can include it and a source build does not need `git`. `build.rs` copies that tree into `$OUT_DIR` and applies `patches/*.patch` with the `diffy` crate — enum value, descriptor helper, OP1a track class, factory switch, CMake. CI applies the same patches to a pristine copy and fails if any patch rejects. The intent is to upstream the opaque type to `ebu/bmx` and then delete the patch.

### What does "async" actually mean if the core is synchronous?

bmx itself is synchronous and has no internal locking. `mxfuse` is async-friendly, not async-native: blocking bmx calls run on a thread pool, and the byte-source callback bridges to your async fetcher. libMXF's `read` callback is `uint32_t (*read)(MXFFileSysData*, uint8_t*, uint32_t)` — you cannot `await` inside it. When bmx asks for bytes, the callback blocks that worker until your async fetcher (S3, HTTP, a pipe) completes and copies into the provided buffer. From Python/Node/Rust async code this looks like `await clip.read(...)`; from bmx's point of view it is a normal synchronous read. Do not share one reader across tasks.

### Does mxfuse decode images?

No. Essence goes in and essence comes out. bmx has bitstream parsers, not image decoders, so `mxfuse` will not turn JPEG 2000 or ProRes into pixels. A "frame" is the KLV payload with the key and length stripped.

### How is `BMXException` kept from unwinding across FFI?

`Open()` returns an error code. Almost everything else (`Read`, `Seek`, `SetReadLimits`, `WriteSamples`, `CompleteWrite`) throws `BMXException`. Unwinding across the Rust/C++ boundary is undefined behavior. Every shim entry point is wrapped in `try`/`catch(...)` and converted to a Rust `Error` (and from there to `NotImplementedError` / a JS exception / a Rust `Result`). There is no `extern "C"` surface in bmx, so the shim is hand-written C++ compiled with `cc`/`cxx`, not `bindgen`.

### Why does file ownership matter for a custom byte source?

`MXFFileReader` and `OP1AFile` take ownership of the `mxfpp::File*` you pass in and `delete` it in the destructor, which closes the underlying `MXFFile` and calls your `close` / `free_sys_data`. The high-level `open_mxf` / `write_mxf` API hides this: the Python/Node/Rust handle stays alive for the lifetime of the clip, and dropping the clip is what closes the source. If you use `mxfuse.bmx` directly, do not also close or free the file you handed over.

### What is the performance story for a 500 GB remote file?

bmx is index-driven. Seeking to an edit unit and reading it touches the header, the index, and the essence KLV for the selected tracks — not the rest of the file. `make bench` (Apple M5 Max) on a 61 MB synthetic OP1a of 8,000 edit units sought the last picture frame in 41,007 reads totaling 127 KB with both knobs off, and in 3 reads totaling 1.2 MB (2% of the file) with the default 1 MB `read_ahead` and 64 MB `cache_bytes`. The same access on a 4×-smaller clip fetched 1.13× fewer bytes, so cost is not proportional to file size. The 41,007 tiny reads are what a remote source has to amortize. Disable unneeded tracks *before* `read`. A 1 MB read-ahead window will pull neighbouring interleaved sound bytes; deselected *payloads* are still never demanded, and the isolation proof runs with both knobs at 0. With those knobs, a single frame of a 500 GB IMF is a handful of range requests totaling a few MB.
