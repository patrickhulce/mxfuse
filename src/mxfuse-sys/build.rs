use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let vendor_bmx = dunce::canonicalize(manifest_dir.join("../../vendor/bmx"))
        .expect("vendor/bmx is missing; see vendor/README.md");
    let patches_dir = manifest_dir.join("../../patches");

    println!("cargo:rerun-if-changed=shim/mxfuse_shim.cpp");
    println!("cargo:rerun-if-changed=shim/mxfuse_shim.h");
    println!("cargo:rerun-if-changed=shim/uuid_stub.c");
    println!("cargo:rerun-if-changed=shim/uuid/uuid.h");
    println!(
        "cargo:rerun-if-changed={}",
        vendor_bmx.join("CMakeLists.txt").display()
    );
    println!("cargo:rerun-if-changed={}", patches_dir.display());

    let bmx_src = prepare_patched_source(&vendor_bmx, &patches_dir, &out_dir);

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let stub_lib = build_uuid_stub(&out_dir);

    let mut config = cmake::Config::new(&bmx_src);
    config
        .profile("Release")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("BMX_BUILD_LIB_ONLY", "ON")
        .define("BMX_BUILD_TESTING", "OFF")
        .define("BUILD_TESTING", "OFF")
        .define("BMX_BUILD_APPS", "OFF")
        .define("BMX_BUILD_TOOLS", "OFF")
        .define("BMX_BUILD_EXPAT_SOURCE", "ON")
        .define("BMX_BUILD_URIPARSER_SOURCE", "ON")
        .very_verbose(true);

    if target_os == "linux" {
        let include_dir = manifest_dir.join("shim");
        config.define("uuid_lib", stub_lib.display().to_string());
        config.define("uuid_include_dir", include_dir.display().to_string());
    }

    if let Ok(deployment_target) = env::var("MACOSX_DEPLOYMENT_TARGET") {
        config.define("CMAKE_OSX_DEPLOYMENT_TARGET", &deployment_target);
    }

    let dst = config.build();
    add_link_search(&dst);
    add_link_search(&dst.join("lib"));
    add_link_search(&dst.join("lib64"));
    add_link_search(&dst.join("lib").join("Release"));
    add_link_search(&out_dir);

    println!("cargo:rustc-link-lib=static=bmx");
    println!("cargo:rustc-link-lib=static=MXF++");
    println!("cargo:rustc-link-lib=static=MXF");
    println!("cargo:rustc-link-lib=static=uriparser");
    // MSVC libexpat uses OUTPUT_NAME libexpat plus a RELEASE_POSTFIX of MD.
    if target_os == "windows" {
        println!("cargo:rustc-link-lib=static=libexpatMD");
    } else {
        println!("cargo:rustc-link-lib=static=expat");
    }

    if target_os == "linux" {
        println!("cargo:rustc-link-lib=static=uuid_stub");
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-lib=m");
    } else if target_os == "macos" {
        println!("cargo:rustc-link-lib=c++");
    } else if target_os == "windows" {
        println!("cargo:rustc-link-lib=ole32");
    }

    compile_shim(&manifest_dir, &bmx_src, &target_os);
}

fn prepare_patched_source(vendor_bmx: &Path, patches_dir: &Path, out_dir: &Path) -> PathBuf {
    let dest = out_dir.join("bmx-src");
    let stamp_path = out_dir.join("bmx-src.stamp");
    let stamp = source_stamp(vendor_bmx, patches_dir);

    if dest.is_dir()
        && fs::read_to_string(&stamp_path)
            .ok()
            .is_some_and(|existing| existing == stamp)
    {
        return dest;
    }

    if dest.exists() {
        fs::remove_dir_all(&dest).expect("failed to clear previous patched bmx tree");
    }
    copy_dir(vendor_bmx, &dest).expect("failed to copy vendor/bmx into OUT_DIR");
    apply_patches(&dest, patches_dir);
    fs::write(&stamp_path, stamp).expect("failed to write bmx patch stamp");
    dest
}

fn source_stamp(vendor_bmx: &Path, patches_dir: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    vendor_bmx.hash(&mut hasher);
    let mut patches = list_patches(patches_dir);
    patches.sort();
    for path in patches {
        println!("cargo:rerun-if-changed={}", path.display());
        path.hash(&mut hasher);
        fs::read(&path).unwrap_or_default().hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn list_patches(patches_dir: &Path) -> Vec<PathBuf> {
    let mut patches = Vec::new();
    let Ok(entries) = fs::read_dir(patches_dir) else {
        return patches;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("patch") {
            patches.push(path);
        }
    }
    patches
}

fn apply_patches(dest: &Path, patches_dir: &Path) {
    let mut patches = list_patches(patches_dir);
    patches.sort();
    for path in patches {
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        apply_unified_diff(dest, &text)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }
}

fn apply_unified_diff(root: &Path, text: &str) -> Result<(), String> {
    for file_patch in split_file_patches(text) {
        apply_file_patch(root, &file_patch)?;
    }
    Ok(())
}

fn split_file_patches(text: &str) -> Vec<String> {
    let mut files = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if line.starts_with("--- ") && !current.is_empty() {
            files.push(current);
            current = String::new();
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        files.push(current);
    }
    files
}

fn apply_file_patch(root: &Path, patch_text: &str) -> Result<(), String> {
    let plus_line = patch_text
        .lines()
        .find(|line| line.starts_with("+++ "))
        .ok_or_else(|| "patch is missing +++ header".to_string())?;
    let rel = plus_path(plus_line)?;
    let target = root.join(&rel);
    let original = if target.is_file() {
        fs::read_to_string(&target)
            .map_err(|error| format!("read {}: {error}", target.display()))?
    } else {
        String::new()
    };
    let patch = diffy::Patch::from_str(patch_text)
        .map_err(|error| format!("parse patch for {rel}: {error}"))?;
    let updated = diffy::apply(&original, &patch)
        .map_err(|error| format!("apply patch for {rel}: {error}"))?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    fs::write(&target, updated).map_err(|error| format!("write {}: {error}", target.display()))?;
    Ok(())
}

fn plus_path(line: &str) -> Result<String, String> {
    let rest = line
        .strip_prefix("+++ ")
        .ok_or_else(|| "invalid +++ line".to_string())?;
    let path = rest.split_whitespace().next().unwrap_or(rest);
    let trimmed = path
        .strip_prefix("b/")
        .or_else(|| path.strip_prefix("a/"))
        .unwrap_or(path);
    if trimmed == "/dev/null" {
        return Err("+++ path is /dev/null".to_string());
    }
    Ok(trimmed.replace('\\', "/"))
}

fn copy_dir(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn build_uuid_stub(out_dir: &Path) -> PathBuf {
    let mut build = cc::Build::new();
    build
        .file("shim/uuid_stub.c")
        .include("shim")
        .warnings(false)
        .cargo_metadata(false);
    build.compile("uuid_stub");
    let name = if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        "uuid_stub.lib"
    } else {
        "libuuid_stub.a"
    };
    out_dir.join(name)
}

fn compile_shim(manifest_dir: &Path, bmx_src: &Path, target_os: &str) {
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .file("shim/mxfuse_shim.cpp")
        .include(manifest_dir.join("shim"))
        .include(bmx_src.join("include"))
        .include(bmx_src.join("deps/libMXF"))
        .include(bmx_src.join("deps/libMXFpp"))
        .warnings(false);

    if target_os == "windows" {
        build.flag("/EHsc");
        build.define("NOMINMAX", None);
        build.define("_CRT_SECURE_NO_WARNINGS", None);
        build.std("c++14");
    } else {
        build.std("c++11");
        build.flag_if_supported("-fexceptions");
    }

    build.compile("mxfuse_shim");
}

fn add_link_search(path: &Path) {
    if path.is_dir() {
        println!("cargo:rustc-link-search=native={}", path.display());
    }
}
