//! libmsdf — wgpu SDF/MSDF render engine + typography.
//!
//! Extracted from matter-stream's render stack (same author:
//! `matterstream-common` / `-font` / `-ui-gpu` / `-mtd1-format` +
//! `mtd1_to_sdf`), with all VM/skills/card code pruned, plus Highbay
//! additions: the wasm32/WebGPU port, runtime compute-shader MSDF
//! generation, dynamic atlas management, cubic-Bézier arc strokes, and the
//! lo-fi distortion hook (PLAN.md Phase 5).
//!
//! Module map:
//! - [`core`] — `SdfDrawCmd` + SDF eval math, `RenderFrame`, `GpuFont`,
//!   `Rasterizer`, color helpers (zero-dep).
//! - [`font`] — rustybuzz shaping, MSDF atlases (CPU bake native-only;
//!   loading wasm-clean), glyph tables, shelf packing, dynamic atlas
//!   management, outline extraction for compute MSDF.
//! - [`gpu`] — `GpuSdfRenderer` (single SDF fragment pipeline, caller-owned
//!   device) + `MsdfCompute` (runtime MSDF via WGSL compute).
//! - [`drawlist`] — the `DrawList` contract highbay_ui renders through,
//!   plus the compact `Command32` stream lowering.

#![forbid(unsafe_code)]

pub mod core;
pub mod drawlist;
pub mod font;
pub mod gpu;

pub use crate::core::{
    Anim, DRAW_TYPE_BEZIER, DRAW_TYPE_BOX, DRAW_TYPE_CIRCLE, DRAW_TYPE_LINE, DRAW_TYPE_MSDF_TEXT,
    DRAW_TYPE_OUTLINE, DRAW_TYPE_SLAB, DRAW_TYPE_TEXT, GpuFont, RenderFrame, SdfDrawCmd,
};
pub use drawlist::{rotate_rect, DrawList, Elevation, SdfFrame, SdfInstance, SdfKind, SdfRotate};
pub use font::{
    AtlasManager, AtlasRegion, FontAtlas, FontAtlasBuilder, GlyphEntry, ROBOTO_REGULAR_ASCII,
    ShapedRun, TextShaper,
};
pub use gpu::{blur_params, BlurPass, GpuSdfRenderer, MsdfCompute};

/// Human-readable crate status, printed by the root `highbay` bin.
pub const PHASE_STATUS: &str =
    "Phase 5: wgpu SDF/MSDF engine (extracted from matter-stream; native + wasm32)";
