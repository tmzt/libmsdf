//! **The faces the integration tests read**, straight from `fonts/`, so the
//! tests need no font feature: a test binary is not the library, and the
//! bundled-face constants exist only with their `font-*` features
//! (`docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md`, section 4.11).
//! The names match the library constants they stand in for.

#![allow(dead_code)]

use libmsdf::GlyphStyle;

pub const ROBOTO_REGULAR_ASCII: &[u8] = include_bytes!("../../fonts/Roboto-Regular-ascii.ttf");
pub const ROBOTO_BOLD_ASCII: &[u8] = include_bytes!("../../fonts/Roboto-Bold-ascii.ttf");
pub const ROBOTO_ITALIC_ASCII: &[u8] = include_bytes!("../../fonts/Roboto-Italic-ascii.ttf");
pub const ROBOTO_ASCII_MSYMBOLS: &[u8] =
    include_bytes!("../../fonts/Roboto-Regular-ascii-msymbols.ttf");

/// The face for `style` (Regular, Bold, Italic), as `bundled_style_face`
/// answers with every Roboto feature on.
pub const fn style_face(style: GlyphStyle) -> Option<&'static [u8]> {
    match style {
        GlyphStyle::Regular => Some(ROBOTO_REGULAR_ASCII),
        GlyphStyle::Bold => Some(ROBOTO_BOLD_ASCII),
        GlyphStyle::Italic => Some(ROBOTO_ITALIC_ASCII),
        GlyphStyle::BoldItalic => None,
    }
}

/// `StyledShaper::bundled(regular)` with every Roboto feature on.
#[cfg(feature = "shaper")]
pub fn styled_shaper(regular: &[u8]) -> libmsdf::StyledShaper {
    let mut shaper = libmsdf::StyledShaper::new(regular.to_vec()).expect("regular parses");
    for style in [GlyphStyle::Bold, GlyphStyle::Italic] {
        shaper = shaper
            .with_face(style, style_face(style).unwrap().to_vec())
            .expect("style face parses");
    }
    shaper
}
