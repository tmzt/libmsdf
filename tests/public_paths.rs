//! **Every public font path still resolves** after the font module became
//! the `libmsdf-font` crate (bundled fonts plan, step 3): the font paths
//! listed in VERSIONS.md for 0.2.0 and 0.3.0, and the additions of the atlas
//! layout tables plan (its section 4.12). Compiling is the test.

#![allow(unused_imports)]

// 0.2.0: the `font` module and its crate-root re-exports.
use libmsdf::font::atlas::{FontAtlas as _, SetMetrics as _};
use libmsdf::font::glyph_table::GlyphEntry as _;
use libmsdf::font::manager::{AtlasManager as _, AtlasRegion as _};
use libmsdf::font::outline::{Edge as _, EdgeKind as _, GlyphOutline as _};
use libmsdf::font::packer::ShelfPacker as _;
use libmsdf::font::shaper::{ShapedGlyph as _, ShapedRun as _};
use libmsdf::font::{
    AtlasLayer, AtlasManager, AtlasRegion, CellKey, Edge, EdgeKind, FontAtlas, GlyphEntry,
    GlyphOutline, GlyphSet, GlyphStyle, HIGHBAY_ICONS, MARKERS, MSYMBOLS_ICONS, SetMetrics,
    ShapedGlyph, ShapedRun, ShelfPacker,
};
use libmsdf::{
    ATLAS_COLS, ATLAS_HEADER_SIZE, ATLAS_MAGIC, ATLAS_ROWS, ATLAS_VERSION, ATLAS_VERSION_LAYERED,
    AtlasLayer as _, AtlasManager as _, AtlasRegion as _, CellKey as _, FALLBACK_BASELINE_FRAC,
    FALLBACK_MAX_INK_DESCENT_EM, FontAtlas as _, GlyphEntry as _, GlyphSet as _, GlyphStyle as _,
    HIGHBAY_ICONS as _, HIGHBAY_ICONS_BLOCK, LayerNotBaked, MARKER_ARROW, MARKERS as _,
    MAX_ATLAS_LAYERS, MAX_RAW_GLYPH_ID, MSYMBOLS_ICONS as _, OWNED_BLOCKS, SetMetrics as _,
    ShapedRun as _, StyledGlyphError, TEXT_RANGES, atlas_capacity, highbay_codepoint,
    msymbols_codepoint,
};
#[cfg(feature = "shaper")]
use libmsdf::{
    FontAtlasBuilder, NoFaceForStyle, StyledShaper, TextShaper, bundled_style_face,
    font::extract_outline, font::shaper::TextShaper as _,
};
// The drawlist constant that moved into libmsdf-font.
use libmsdf::drawlist::LINE_BOX_RATIO;

// 0.4.0, atlas layout tables plan section 4.12.
use libmsdf::font::{
    ATLAS_TABLE_FACE_ALL, ContextSide, ContextSingle, GsubLookup, GsubRules, LICENCE_REGISTRY,
    LicenceKind, LicenceTextError, Ligature, MAX_ATLAS_TABLE_BYTES, MAX_ATLAS_TABLES,
    is_default_ignorable, licence_by_key, licence_by_spdx,
};
use libmsdf::{
    ATLAS_FILE_MAJOR, ATLAS_FORMAT_VERSION, ATLAS_HEADER_SIZE_V5, AtlasFileVersion,
    AtlasFormatVersion, AtlasLayout, AtlasShaper, FaceLayout, LicenceRecord, NoLayout, RunShaper,
};
#[cfg(feature = "shaper")]
use libmsdf::{LayoutError, LayoutOptions};

#[test]
fn the_paths_resolve() {
    assert_eq!(LINE_BOX_RATIO, 1.3);
}
