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

/// [`ROBOTO_REGULAR_ASCII`] with a Material Symbols icon subset merged in at
/// the icons' own Private Use Area codepoints.
///
/// **One face, so an icon is a glyph on the text path** — same cmap, same
/// shaper, same atlas bake. A caller resolves an icon by shaping its
/// codepoint, exactly as it resolves a letter; there is no second font to keep
/// in step and no parallel "icon renderer".
///
/// The ASCII half is byte-for-byte Roboto's, layout tables included: dropping
/// `GPOS`/`GSUB` in the merge would have taken kerning and the fi/fl/ffi/ffl
/// ligatures with it and shifted every text advance in every frame. The icon
/// half is the variable Material Symbols Outlined face instanced at its
/// default location (FILL 0, GRAD 0, opsz 24, wght 400), subset to the names
/// below, and scaled from 960 to Roboto's 2048 upem.
///
/// Declared coverage, and deliberately small — a name outside this list has no
/// glyph, which is a missing-asset error for the caller to report rather than
/// substitute:
///
/// ```text
/// menu U+E5D2   more_vert U+E5D4   send U+E163   chat U+E0C9   person U+F0D3
/// home U+E9B2   search U+EF7A      library_books U+E02F        settings U+E8B8
/// ```
///
/// Material Symbols is © Google, licensed Apache-2.0 — see
/// `fonts/LICENSE-MaterialSymbols.txt`.
pub const ROBOTO_ASCII_MSYMBOLS: &[u8] =
    include_bytes!("../../fonts/Roboto-Regular-ascii-msymbols.ttf");

/// The icon half of [`ROBOTO_ASCII_MSYMBOLS`]: every declared name, and the
/// codepoint it is drawn at.
///
/// **This is the coverage manifest, and there is exactly one of it.** It lives
/// beside the bytes because it is a fact about the bake, not about any
/// renderer: a second copy in a consumer is a copy that can disagree with the
/// font about which names exist. A caller resolves a name here and shapes the
/// codepoint; a name that is absent has no glyph, and that is a missing-asset
/// error for the caller to report rather than substitute.
///
/// Sorted by name so [`msymbols_codepoint`] can binary-search it, and so the
/// list reads as a list.
pub const MSYMBOLS_ICONS: &[(&str, char)] = &[
    ("chat", '\u{E0C9}'),
    ("home", '\u{E9B2}'),
    ("library_books", '\u{E02F}'),
    ("menu", '\u{E5D2}'),
    ("more_vert", '\u{E5D4}'),
    ("person", '\u{F0D3}'),
    ("search", '\u{EF7A}'),
    ("send", '\u{E163}'),
    ("settings", '\u{E8B8}'),
];

/// The codepoint [`ROBOTO_ASCII_MSYMBOLS`] draws `name` at, or `None` when the
/// bake declares no coverage for it.
///
/// `None` is the whole point of the return type: it is the missing-asset
/// answer, and a caller that turns it into a fallback shape has defeated the
/// reason this is fallible.
pub fn msymbols_codepoint(name: &str) -> Option<char> {
    MSYMBOLS_ICONS
        .binary_search_by_key(&name, |&(n, _)| n)
        .ok()
        .map(|i| MSYMBOLS_ICONS[i].1)
}
