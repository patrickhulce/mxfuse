use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let bmx_src = dunce::canonicalize(manifest_dir.join("../../vendor/bmx"))
        .expect("vendor/bmx is missing; see vendor/README.md");

    println!("cargo:rerun-if-changed=shim/mxfuse_shim.cpp");
    println!("cargo:rerun-if-changed=shim/mxfuse_shim.h");
    println!("cargo:rerun-if-changed=shim/uuid_stub.c");
    println!("cargo:rerun-if-changed=shim/uuid/uuid.h");
    println!(
        "cargo:rerun-if-changed={}",
        bmx_src.join("CMakeLists.txt").display()
    );

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
