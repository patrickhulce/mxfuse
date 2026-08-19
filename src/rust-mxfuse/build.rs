use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest_dir.join("../..");
    for name in ["LICENSE", "THIRD_PARTY_NOTICES.md"] {
        let src = root.join(name);
        if src.is_file() {
            fs::copy(&src, manifest_dir.join(name)).unwrap_or_else(|err| {
                panic!("failed to copy {} into mxfuse: {err}", src.display());
            });
        }
    }
}
