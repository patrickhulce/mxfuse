# Vendored third-party source

`bmx/` is the [ebu/bmx](https://github.com/ebu/bmx) **v1.7** source-release
tarball (`bmx-1.7.tar.gz`), with test data, docs, docker, and CI trees removed
to keep the tree small. `deps/libMXF`, `deps/libMXFpp`, `deps/libexpat`,
`deps/uriparser`, and `deps/cmake-git-version-tracking` are included so a
configure is offline and needs no `git`.

Do not treat this as a git submodule. The files are ordinary tracked source so
`cargo package` can include them.

The tree stays pristine. `src/mxfuse-sys/build.rs` copies it into `$OUT_DIR`
and applies the unified diffs under `patches/` (opaque essence type) before
cmake runs. Do not edit files under `vendor/bmx` to add mxfuse behavior.
