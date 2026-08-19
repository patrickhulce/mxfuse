# Vendored third-party source

`bmx/` is the [ebu/bmx](https://github.com/ebu/bmx) **v1.7** source-release
tarball (`bmx-1.7.tar.gz`), with test data, docs, docker, and CI trees removed
to keep the tree small. `deps/libMXF`, `deps/libMXFpp`, `deps/libexpat`,
`deps/uriparser`, and `deps/cmake-git-version-tracking` are included so a
configure is offline and needs no `git`.

Do not treat this as a git submodule. The tree stays pristine. Do not edit
files under `vendor/bmx` to add mxfuse behavior.

`src/mxfuse-sys/build.rs` copies this tree (minus `apps`, `tools`, and `meta`),
applies `patches/` (opaque essence type), and writes the result to the
gitignored `src/mxfuse-sys/generated/bmx` directory. cmake and the shim compile
that generated tree. `cargo package` ships `generated/` via the crate `include`
list so a crates.io source build does not need this directory.
