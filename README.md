# mxfuse

Rust-first MXF container primitives with Python and Node.js bindings, built on
a statically linked [bmx](https://github.com/ebu/bmx) core.

Essence goes in and essence comes out. A frame is the KLV payload with the key
and length stripped. You can reproduce any Generic Container mapping exactly —
element key, BER length, descriptor class, and every descriptor field — and
read that mapping back: the picture coding UL and the full descriptor survive
a round trip. Pin product info, creation date, generation UID, and package
UMIDs on write. The core is synchronous: one reader or writer per thread.

```bash
cargo add mxfuse
npm i mxfuse
uv add mxfuse
```

Wheels and napi prebuilds are the supported install path. They statically link
bmx, libMXF, libMXF++, expat, and uriparser. On Linux the C++ runtime
(`libstdc++.so.6`, `libgcc_s.so.1`) stays dynamic; both are on the manylinux
allowlist. `cargo add mxfuse` is a source build and needs CMake plus a C++
toolchain. uriparser, expat, and `cmake-git-version-tracking` are pre-vendored,
and libuuid is replaced by a shim-provided `uuid_generate`, so a source build
does not need `git`, network access at configure time, or `uuid-dev`.

## Usage

### Python

```python
from mxfuse import open_mxf, ReadOptions, TrackKind

options = ReadOptions(read_ahead=1 << 20, cache_bytes=64 << 20)

with open("input.mxf", "rb") as f:
    with open_mxf(f, options=options) as clip:
        print(clip.edit_rate, clip.duration)
        for track in clip.tracks:
            print(track.index, track.kind, track.essence_type, track.essence_container_ul)
        clip.select(t for t in clip.tracks if t.kind is TrackKind.PICTURE)
        clip.seek(0)
        for package in clip.read(count=1):
            for frame in package.frames:
                frame.data
                frame.element_key
                frame.file_position
```

A custom byte source is any object that implements `read`, `seek`, `tell`, and
optionally `size`. Regular files infer size via `seek(0, 2)`. Tune `read_ahead`
and `cache_bytes` for remote sources: `make bench` on a 61 MB synthetic OP1a
sought the last picture frame in 41,007 reads (127 KB) with both off, and 3
reads (1.2 MB) with the defaults. A 1 MB window overshoots into neighbouring
interleaved essence; set both to 0 for payload-level track isolation.

```python
from mxfuse import ClipSpec, EssenceType, Flavour, TrackSpec, write_mxf

spec = ClipSpec(
    edit_rate=(25, 1),
    flavour=Flavour.DEFAULT,
    duration=len(images),
    tracks=[
        TrackSpec(EssenceType.UNC_HD_1080P),
        TrackSpec(EssenceType.WAVE_PCM, sampling_rate=48000),
    ],
)

with open("output.mxf", "wb") as f, write_mxf(f, spec) as clip:
    for image, audio in zip(images, audios):
        clip.write_unit(image, audio)
```

A private Generic Container mapping supplies the element key, BER length,
descriptor class, and descriptor fields. On read, `track.coding_ul` and the
picture descriptor identify the mapping.

```python
from mxfuse import ClipSpec, DescriptorKind, EssenceType, PixelComponent, TrackSpec, write_mxf

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
)
```

### Node

```typescript
import { openMxf } from "mxfuse";

const clip = await openMxf("input.mxf", { readAhead: 1 << 20, cacheBytes: 64 << 20 });
const info = await clip.info();
await clip.select(info.tracks.filter((track) => track.kind === "picture"));
await clip.seek(0);
for (const package_ of await clip.read(1)) {
  for (const frame of package_.frames) {
    frame.data;
    frame.elementKey;
    frame.filePosition;
  }
}
await clip.close();
```

Do not share one reader across concurrent tasks.

```typescript
import { EssenceType, Flavour, writeMxf } from "mxfuse";

const writer = await writeMxf("output.mxf", {
  editRate: [25, 1],
  flavour: Flavour.DEFAULT,
  duration: images.length,
  tracks: [
    { essenceType: EssenceType.UNC_HD_1080P },
    { essenceType: EssenceType.WAVE_PCM, samplingRate: 48000 },
  ],
});
for (let i = 0; i < images.length; i++) {
  await writer.write(0, images[i]);
  await writer.write(1, audios[i]);
}
await writer.finish();
```

### Rust

```rust
use mxfuse::{open_mxf, ReadOptions, TrackKind};

let file = std::fs::File::open("input.mxf")?;
let mut clip = open_mxf(file, ReadOptions::default())?;
let picture: Vec<_> = clip
    .tracks()
    .iter()
    .filter(|track| track.kind == TrackKind::Picture)
    .cloned()
    .collect();
clip.select(picture.iter())?;
clip.seek(0)?;
for package in clip.read(1)? {
    for frame in package.frames {
        let _ = (frame.data, frame.element_key, frame.file_position);
    }
}
```

```rust
use mxfuse::{write_mxf, ClipSpec, EssenceType, Flavour, Rational, TrackSpec};

let file = std::fs::File::create("output.mxf")?;
let spec = ClipSpec {
    edit_rate: Rational { num: 25, den: 1 },
    flavour: Flavour::DEFAULT,
    duration: Some(images.len() as i64),
    tracks: vec![
        TrackSpec::new(EssenceType::UNC_HD_1080P),
        TrackSpec {
            sampling_rate: Some(48000),
            ..TrackSpec::new(EssenceType::WAVE_PCM)
        },
    ],
    xml: vec![],
    ..ClipSpec::default()
};
let mut writer = write_mxf(file, spec)?;
for (image, audio) in images.iter().zip(audios.iter()) {
    writer.write(0, image)?;
    writer.write(1, audio)?;
}
writer.finish()?;
```

`file_position` for frame-wrapped essence points at the KLV, with `kl_size`
giving the header length. Clip-wrapped essence points at the sample data and
`kl_size` is 0. Clip-level XML (ST 434 / generic stream) is `ClipSpec.xml` on
write and `clip.xml` on read; it is not an essence track.

## Development

### Prerequisites

- Rust (stable)
- CMake ≥ 3.12 and a C++ toolchain
- [uv](https://docs.astral.sh/uv/)
- [pnpm](https://pnpm.io/) 9+
- Node.js 22+

### Layout

```text
src/
├── mxfuse-sys/      # CMake build of vendored bmx + C++ shim
├── rust-mxfuse/     # Core Rust library
├── python-mxfuse/   # PyO3 extension and Python API
└── node-mxfuse/     # napi-rs extension and TypeScript API
vendor/bmx/          # ebu/bmx v1.7 source release (offline)
patches/             # applied to an OUT_DIR copy of vendor/bmx at build time
```

### Commands

```bash
make              # build, lint, typecheck, test
make build        # build all targets
make test         # run all tests
make fixtures     # generate tests/fixtures/sample_op1a.mxf
make bench        # print read/byte costs across ReadOptions
make examples     # JPEG XL Generic Container round-trip (needs .data/4KProRes.mov)
```
