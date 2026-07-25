//! Build-time WGSL validation (carried over from matter-stream's
//! matterstream-ui-gpu/build.rs).
//!
//! `include_str!` loads the shader source into the binary but doesn't touch
//! its contents — WGSL errors would only surface when wgpu parses the string
//! at runtime. This script runs the same naga front-end + validator that wgpu
//! uses so bad shader edits fail `cargo build` instead.

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let shaders = [
        manifest_dir.join("src/gpu/sdf_render.wgsl"),
        manifest_dir.join("src/gpu/msdf_compute.wgsl"),
        manifest_dir.join("src/gpu/blur.wgsl"),
    ];

    for path in &shaders {
        println!("cargo:rerun-if-changed={}", path.display());
        let src = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));

        let module = match naga::front::wgsl::parse_str(&src) {
            Ok(m) => m,
            Err(e) => {
                let msg = e.emit_to_string(&src);
                panic!("WGSL parse error in {}:\n{}", path.display(), msg);
            }
        };

        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        if let Err(e) = validator.validate(&module) {
            panic!("WGSL validation error in {}:\n{:#?}", path.display(), e);
        }
    }
}
