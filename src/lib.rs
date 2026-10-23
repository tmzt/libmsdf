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

/// The baker (`libmsdf-font`'s `bake` module), with `cpu-bake`.
#[cfg(all(feature = "cpu-bake", not(target_arch = "wasm32")))]
pub use libmsdf_font::bake;
pub mod bundled;
pub mod core;
pub mod drawlist;
/// [`drawlist::Render`]: something that draws itself into a `DrawList`.
pub use drawlist::Render;
/// The font half of libmsdf: the `libmsdf-font` crate, with every path it
/// had as `libmsdf::font` unchanged.
pub mod font {
    pub use libmsdf_font::*;

    #[cfg(feature = "font-ofl-grenze-gotisch")]
    pub use crate::bundled::GRENZE_GOTISCH_ATLAS;
    #[cfg(feature = "font-ofl-noto-sans-mono")]
    pub use crate::bundled::NOTO_SANS_MONO_ATLAS;
    #[cfg(feature = "font-apache-roboto-bold")]
    pub use crate::bundled::ROBOTO_BOLD_ATLAS;
    #[cfg(feature = "font-apache-roboto-italic")]
    pub use crate::bundled::ROBOTO_ITALIC_ATLAS;
    #[cfg(feature = "font-apache-roboto-msymbols")]
    pub use crate::bundled::ROBOTO_MSYMBOLS_ATLAS;
    #[cfg(all(
        feature = "font-apache-roboto-msymbols",
        feature = "font-apache-roboto-bold",
        feature = "font-apache-roboto-italic"
    ))]
    pub use crate::bundled::ROBOTO_MSYMBOLS_STYLED_ATLAS;
    #[cfg(feature = "font-apache-roboto-regular")]
    pub use crate::bundled::ROBOTO_REGULAR_ATLAS;
    #[cfg(feature = "font-ofl-noto-terminal-symbols")]
    pub use crate::bundled::TERMINAL_SYMBOLS_ATLAS;
    #[cfg(feature = "font-ofl-texturina")]
    pub use crate::bundled::TEXTURINA_ATLAS;
}
pub mod gpu;
pub mod pane;

pub use crate::core::{
    Anim, ClipRect, DRAW_TYPE_BEZIER, DRAW_TYPE_BOX, DRAW_TYPE_CIRCLE, DRAW_TYPE_LINE,
    DRAW_TYPE_MSDF_TEXT, DRAW_TYPE_OUTLINE, DRAW_TYPE_PATH_FILL, DRAW_TYPE_SLAB, DRAW_TYPE_TEXT,
    GpuFont, PARAM_BANK_CAPACITY, RenderFrame, SdfDrawCmd,
};
#[cfg(feature = "font-ofl-grenze-gotisch")]
pub use bundled::GRENZE_GOTISCH_ATLAS;
#[cfg(all(feature = "font-ofl-grenze-gotisch", feature = "shaper"))]
pub use bundled::GRENZE_GOTISCH_TTF;
#[cfg(feature = "font-ofl-noto-sans-mono")]
pub use bundled::NOTO_SANS_MONO_ATLAS;
#[cfg(all(feature = "font-apache-roboto-msymbols", feature = "shaper"))]
pub use bundled::ROBOTO_ASCII_MSYMBOLS;
#[cfg(all(feature = "font-apache-roboto-bold", feature = "shaper"))]
pub use bundled::ROBOTO_BOLD_ASCII;
#[cfg(feature = "font-apache-roboto-bold")]
pub use bundled::ROBOTO_BOLD_ATLAS;
#[cfg(all(feature = "font-apache-roboto-italic", feature = "shaper"))]
pub use bundled::ROBOTO_ITALIC_ASCII;
#[cfg(feature = "font-apache-roboto-italic")]
pub use bundled::ROBOTO_ITALIC_ATLAS;
#[cfg(feature = "font-apache-roboto-msymbols")]
pub use bundled::ROBOTO_MSYMBOLS_ATLAS;
#[cfg(all(
    feature = "font-apache-roboto-msymbols",
    feature = "font-apache-roboto-bold",
    feature = "font-apache-roboto-italic"
))]
pub use bundled::ROBOTO_MSYMBOLS_STYLED_ATLAS;
#[cfg(all(feature = "font-apache-roboto-regular", feature = "shaper"))]
pub use bundled::ROBOTO_REGULAR_ASCII;
#[cfg(feature = "font-apache-roboto-regular")]
pub use bundled::ROBOTO_REGULAR_ATLAS;
#[cfg(feature = "font-ofl-noto-terminal-symbols")]
pub use bundled::TERMINAL_SYMBOLS_ATLAS;
#[cfg(all(feature = "font-ofl-noto-terminal-symbols", feature = "shaper"))]
pub use bundled::TERMINAL_SYMBOLS_TTF;
#[cfg(feature = "font-ofl-texturina")]
pub use bundled::TEXTURINA_ATLAS;
#[cfg(all(feature = "font-ofl-texturina", feature = "shaper"))]
pub use bundled::TEXTURINA_TTF;
pub use bundled::{BUNDLED_ATLAS_GLYPH_SIZE, BUNDLED_ATLAS_PX_RANGE};
#[cfg(all(feature = "font-ofl-noto-sans-mono", feature = "shaper"))]
pub use bundled::{NOTO_SANS_MONO_BOLD_TTF, NOTO_SANS_MONO_REGULAR_TTF};
pub use drawlist::{
    AtlasId, AtlasRef, AtlasSet, AtlasSetError, Contour, DrawEffects, DrawList, Elevation,
    PATH_CUBIC_TOLERANCE, PATH_MAX_SEGMENTS, Path, PathBuilder, PathError, PathSegment, SdfFrame,
    SdfInstance, SdfKind, SdfRotate, rotate_rect,
};
pub use font::{
    ATLAS_COLS, ATLAS_FILE_MAJOR, ATLAS_FORMAT_VERSION, ATLAS_HEADER_SIZE, ATLAS_HEADER_SIZE_V5,
    ATLAS_MAGIC, ATLAS_ROWS, ATLAS_VERSION, ATLAS_VERSION_LAYERED, AtlasFileVersion,
    AtlasFormatVersion, AtlasLayer, AtlasLayout, AtlasManager, AtlasRegion, AtlasShaper, CellKey,
    FALLBACK_BASELINE_FRAC, FALLBACK_MAX_INK_DESCENT_EM, FaceLayout, FontAtlas, GlyphEntry,
    GlyphSet, GlyphStyle, HIGHBAY_ICONS, HIGHBAY_ICONS_BLOCK, LICENCE_REGISTRY, LayerNotBaked,
    LicenceRecord, MARKER_ARROW, MARKERS, MAX_ATLAS_LAYERS, MAX_RAW_GLYPH_ID, MSYMBOLS_ICONS,
    NoLayout, OWNED_BLOCKS, RunShaper, SetMetrics, ShapedRun, StyledGlyphError, TEXT_RANGES,
    atlas_capacity, highbay_codepoint, msymbols_codepoint,
};
#[cfg(feature = "shaper")]
pub use font::{
    FontAtlasBuilder, LayoutError, LayoutOptions, NoFaceForStyle, StyledShaper, TextShaper,
    bundled_style_face,
};
pub use gpu::{BlurPass, GpuSdfRenderer, MsdfCompute, blur_params};
pub use pane::{PaneGate, PaneMark};

/// Human-readable crate status, printed by the root `highbay` bin.
pub const PHASE_STATUS: &str =
    "Phase 5: wgpu SDF/MSDF engine (extracted from matter-stream; native + wasm32)";

/// The bundled Roboto Regular face for libmsdf's own unit tests, whatever
/// features are on.
#[cfg(all(test, feature = "shaper"))]
pub(crate) const TEST_FACE: &[u8] = include_bytes!("../fonts/Roboto-Regular-ascii.ttf");
