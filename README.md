# mxfuse

Rust-first MXF container primitives with Python and Node.js bindings, built on
a statically linked [bmx](https://github.com/ebu/bmx) core.

Essence goes in and essence comes out. A frame is the KLV payload with the key
and length stripped. The core is synchronous: one reader per thread.

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
optionally `size`. Regular files infer size via `seek(0, 2)`.

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

`file_position` for frame-wrapped essence points at the KLV, with `kl_size`
giving the header length. Clip-wrapped essence points at the sample data and
`kl_size` is 0.

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
```

### Commands

```bash
make              # build, lint, typecheck, test
make build        # build all targets
make test         # run all tests
make fixtures     # generate tests/fixtures/sample_op1a.mxf
```
