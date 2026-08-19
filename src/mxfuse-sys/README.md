# mxfuse-sys

Static FFI bindings to a vendored [bmx](https://github.com/ebu/bmx) / libMXF
stack. Downstream users should depend on the `mxfuse` crate from this
repository (not yet published to crates.io).

A git checkout copies `vendor/bmx` and applies `patches/` into the gitignored
`generated/bmx` tree, then cmake builds that tree. `cargo package` ships
`generated/bmx` (already patch-applied) so a crates.io source build does not
need the repo-root vendor directory.
