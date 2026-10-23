//! **The bundled fonts**: every face libmsdf can compile in, each behind its
//! own cargo feature, off by default, whose name carries the face's licence
//! (`docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md`, section 4.4).
//! The TTF constants are defined in `libmsdf-font` (`libmsdf::font::faces`)
//! and re-exported here and at the crate root.
//!
//! With no font feature enabled the compiled crate contains no font data and
//! is licensed purely under MIT OR Apache-2.0 (README.md, "Licensing").

#[cfg(all(feature = "font-ofl-grenze-gotisch", feature = "shaper"))]
pub use crate::font::faces::GRENZE_GOTISCH_TTF;
#[cfg(all(feature = "font-apache-roboto-msymbols", feature = "shaper"))]
pub use crate::font::faces::ROBOTO_ASCII_MSYMBOLS;
#[cfg(all(feature = "font-apache-roboto-bold", feature = "shaper"))]
pub use crate::font::faces::ROBOTO_BOLD_ASCII;
#[cfg(all(feature = "font-apache-roboto-italic", feature = "shaper"))]
pub use crate::font::faces::ROBOTO_ITALIC_ASCII;
#[cfg(all(feature = "font-apache-roboto-regular", feature = "shaper"))]
pub use crate::font::faces::ROBOTO_REGULAR_ASCII;
#[cfg(all(feature = "font-ofl-texturina", feature = "shaper"))]
pub use crate::font::faces::TEXTURINA_TTF;
#[cfg(all(feature = "font-ofl-noto-sans-mono", feature = "shaper"))]
pub use crate::font::faces::{NOTO_SANS_MONO_BOLD_TTF, NOTO_SANS_MONO_REGULAR_TTF};

// ── The bundled atlases ─────────────────────────────────────────────────
//
// Baked by `libmsdf-bake bundled --out fonts/atlases --hashes` and
// committed: the fallback of the bundled fonts plan's section 4.10, taken
// because the build-time bake did not meet switch criterion 2 (CHANGES.md).
// Each is format 5:0.1.0 with layout and licence tables; never hand-edited
// (AGENTS.md, "Bundled fonts and atlases").

/// Roboto Regular as the Regular style, one layer (the same bake as
/// `tests/fixtures/roboto-ascii-48.atlas`).
#[cfg(feature = "font-apache-roboto-regular")]
pub const ROBOTO_REGULAR_ATLAS: &[u8] = include_bytes!("../fonts/atlases/roboto-regular-48.atlas");
/// Roboto Bold baked as a face of its own (Regular style), one layer: its
/// glyph ids are the face's raw ids, so it pairs with
/// `TextShaper::new(ROBOTO_BOLD_ASCII)` or `AtlasShaper::new(&atlas,
/// GlyphStyle::Regular)`.
#[cfg(feature = "font-apache-roboto-bold")]
pub const ROBOTO_BOLD_ATLAS: &[u8] = include_bytes!("../fonts/atlases/roboto-bold-48.atlas");
/// Roboto Italic baked as a face of its own (Regular style), one layer.
#[cfg(feature = "font-apache-roboto-italic")]
pub const ROBOTO_ITALIC_ATLAS: &[u8] = include_bytes!("../fonts/atlases/roboto-italic-48.atlas");
/// Roboto Regular merged with Material Symbols, one layer.
#[cfg(feature = "font-apache-roboto-msymbols")]
pub const ROBOTO_MSYMBOLS_ATLAS: &[u8] =
    include_bytes!("../fonts/atlases/roboto-msymbols-48.atlas");
/// The merged face with Bold and Italic as style layers (three layers), for
/// `StyledShaper` and styled `AtlasShaper`s.
#[cfg(all(
    feature = "font-apache-roboto-msymbols",
    feature = "font-apache-roboto-bold",
    feature = "font-apache-roboto-italic"
))]
pub const ROBOTO_MSYMBOLS_STYLED_ATLAS: &[u8] =
    include_bytes!("../fonts/atlases/roboto-msymbols-styled-48.atlas");
/// Grenze Gotisch (subset) as the Regular style, one 16-column layer
/// (800 x 2000 texels).
#[cfg(feature = "font-ofl-grenze-gotisch")]
pub const GRENZE_GOTISCH_ATLAS: &[u8] = include_bytes!("../fonts/atlases/grenze-gotisch-48.atlas");
/// Texturina (subset) as the Regular style, one 16-column layer
/// (800 x 2000 texels).
#[cfg(feature = "font-ofl-texturina")]
pub const TEXTURINA_ATLAS: &[u8] = include_bytes!("../fonts/atlases/texturina-48.atlas");
/// The terminal face: Noto Sans Mono Regular, with Bold as a style layer
/// (two 16-column layers, 800 x 2000 texels each), at the terminal cap
/// placement (`docs/design/20261023_01_TERMINAL_FONTS_PLAN.md`, section 4.3).
/// Bold is `GlyphStyle::Bold`: `AtlasShaper::new(&atlas, GlyphStyle::Bold)`.
#[cfg(feature = "font-ofl-noto-sans-mono")]
pub const NOTO_SANS_MONO_ATLAS: &[u8] = include_bytes!("../fonts/atlases/noto-sans-mono-48.atlas");

pub use crate::font::{BUNDLED_ATLAS_GLYPH_SIZE, BUNDLED_ATLAS_PX_RANGE};
