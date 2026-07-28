# mxfuse

Rust-first MXF container primitives with Python and Node.js bindings.

This initial scaffold defines a shared API for:

- `encode(container, destination)` and `decode(source, mode)`
- open-like I/O handles that can be backed by files, memory, S3, or another
  random-access adapter
- `Container`, `Track`, `Metadata`, and `Frame` types
- raw mode, which exposes encoded essence frames
- parsed mode, which will decode known essence formats to pixels and fall back
  to raw frames
- lazy frame iteration at both track and container level

The Rust core owns the domain model and accepts generic `Read + Seek` /
`Write + Seek` handles. Python and TypeScript provide idiomatic handle
protocols. Actual MXF parsing and encoding are deliberately left as explicit
`NotImplemented` seams for the next milestone.

## Prerequisites

- Rust (stable)
- [uv](https://docs.astral.sh/uv/)
- [pnpm](https://pnpm.io/) 9+
- Node.js 22+

## Layout

```text
src/
├── rust-mxfuse/     # Core Rust library and domain model
├── python-mxfuse/   # PyO3 extension and Python API
└── node-mxfuse/     # napi-rs extension and TypeScript API
```

## Commands

```bash
make              # build, lint, typecheck, test
make build        # build all targets
make test         # run all tests
```

## API sketches

```rust
let mut source = std::fs::File::open("example.mxf")?;
let container = mxfuse::decode(&mut source, mxfuse::DecodeMode::Raw)?;
for frame in container.frames() {
    consume(frame);
}
```

```python
from mxfuse import DecodeMode, decode

with open("example.mxf", "rb") as source:
    container = decode(source, mode=DecodeMode.RAW)
    for frame in container.frames():
        consume(frame)
```

```typescript
import { decode } from "mxfuse";

const container = await decode(remoteSource, { mode: "parsed" });
for (const frame of container.frames()) {
  consume(frame);
}
```
