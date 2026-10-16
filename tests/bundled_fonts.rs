//! **The bundled-font API, feature by feature**
//! (`docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md`, section 4.11).
//!
//! Every test here is gated on the features it needs, so
//! `cargo test --features fonts-apache-roboto,fonts-ofl-ledger` runs them all
//! and a build with no font feature runs none.

#![cfg(feature = "shaper")]
#![allow(unused_imports)]

use libmsdf::{GlyphStyle, StyledShaper, bundled_style_face};

#[cfg(feature = "font-apache-roboto-regular")]
#[test]
fn the_regular_constant_is_the_committed_face() {
    assert!(libmsdf::ROBOTO_REGULAR_ASCII == include_bytes!("../fonts/Roboto-Regular-ascii.ttf"));
    assert!(libmsdf::font::ROBOTO_REGULAR_ASCII == libmsdf::ROBOTO_REGULAR_ASCII);
    assert_eq!(
        bundled_style_face(GlyphStyle::Regular),
        Some(libmsdf::ROBOTO_REGULAR_ASCII)
    );
}

#[cfg(feature = "font-apache-roboto-bold")]
#[test]
fn the_bold_constant_is_the_committed_face() {
    assert!(libmsdf::ROBOTO_BOLD_ASCII == include_bytes!("../fonts/Roboto-Bold-ascii.ttf"));
    assert_eq!(
        bundled_style_face(GlyphStyle::Bold),
        Some(libmsdf::ROBOTO_BOLD_ASCII)
    );
}

#[cfg(feature = "font-apache-roboto-italic")]
#[test]
fn the_italic_constant_is_the_committed_face() {
    assert!(libmsdf::ROBOTO_ITALIC_ASCII == include_bytes!("../fonts/Roboto-Italic-ascii.ttf"));
    assert_eq!(
        bundled_style_face(GlyphStyle::Italic),
        Some(libmsdf::ROBOTO_ITALIC_ASCII)
    );
}

#[cfg(feature = "font-apache-roboto-msymbols")]
#[test]
fn the_msymbols_constant_is_the_committed_face() {
    assert!(
        libmsdf::ROBOTO_ASCII_MSYMBOLS
            == include_bytes!("../fonts/Roboto-Regular-ascii-msymbols.ttf")
    );
    let shaper = libmsdf::TextShaper::new(libmsdf::ROBOTO_ASCII_MSYMBOLS.to_vec()).unwrap();
    let send = libmsdf::msymbols_codepoint("send").unwrap();
    assert!(shaper.covers(send));
}

/// No bundled face is bold italic, whatever is enabled.
#[test]
fn bold_italic_is_never_bundled() {
    assert!(bundled_style_face(GlyphStyle::BoldItalic).is_none());
}

/// With the regular face alone, the styled shaper carries Regular and refuses
/// Bold rather than substituting.
#[cfg(all(
    feature = "font-apache-roboto-regular",
    not(feature = "font-apache-roboto-bold")
))]
#[test]
fn a_style_whose_feature_is_off_is_refused() {
    assert!(bundled_style_face(GlyphStyle::Bold).is_none());
    let shaper = StyledShaper::bundled(libmsdf::ROBOTO_REGULAR_ASCII.to_vec()).unwrap();
    assert!(shaper.shape(GlyphStyle::Regular, "Tax").is_ok());
    assert_eq!(
        shaper.shape(GlyphStyle::Bold, "Tax").unwrap_err(),
        libmsdf::NoFaceForStyle(GlyphStyle::Bold)
    );
}

/// With every Roboto feature, the bundled styled shaper carries all three.
#[cfg(feature = "fonts-apache-roboto")]
#[test]
fn every_roboto_style_is_bundled() {
    let shaper = StyledShaper::bundled(libmsdf::ROBOTO_REGULAR_ASCII.to_vec()).unwrap();
    assert_eq!(
        shaper.styles(),
        vec![GlyphStyle::Regular, GlyphStyle::Bold, GlyphStyle::Italic]
    );
}
