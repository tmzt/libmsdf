//! Font shaping, MSDF atlas generation, and glyph table management.
//!
//! Extracted from matter-stream's `matterstream-font` (same author), with
//! atlas management (dynamic append + eviction) and glyph-outline extraction
//! (for the compute-shader MSDF path) added for Highbay.
//!
//! - **`shaper`** — rustybuzz (HarfBuzz port) text shaping
//! - **`atlas`** — Multi-Channel Signed Distance Field (MSDF) atlas
//!   (loading is wasm-clean; CPU baking via msdfgen is native-only)
//! - **`glyph_table`** — GPU-uploadable glyph metrics table
//! - **`packer`** — shelf bin packer for atlas cells
//! - **`manager`** — dynamic atlas region allocation + eviction hooks
//! - **`outline`** — glyph outline → colored edge list (compute MSDF input)

pub mod atlas;
pub mod glyph_table;
pub mod manager;
pub mod outline;
pub mod packer;
pub mod shaper;

pub use atlas::{FontAtlas, FontAtlasBuilder, GlyphProjection, glyph_projection};
pub use glyph_table::GlyphEntry;
pub use manager::{AtlasManager, AtlasRegion};
pub use outline::{Edge, EdgeKind, GlyphOutline, extract_outline};
pub use packer::ShelfPacker;
pub use shaper::{ShapedGlyph, ShapedRun, TextShaper};

/// Bundled Roboto Regular, subset to printable ASCII (U+0020–U+007E).
/// Roboto is © The Roboto Project Authors, licensed Apache-2.0 — see
/// `fonts/LICENSE-Roboto.txt`. Used as the deterministic test fixture and
/// as the default face for the M3 demo theme.
pub const ROBOTO_REGULAR_ASCII: &[u8] = include_bytes!("../../fonts/Roboto-Regular-ascii.ttf");
