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

/// The OFL faces: the committed subsets, their copyright lines, their
/// Modified Version marks, no hinting, and text that shapes to real glyphs
/// (bundled fonts plan, step 2 with its 13:34 and 13:40 EDT amendments).
#[cfg(any(feature = "font-ofl-grenze-gotisch", feature = "font-ofl-texturina"))]
mod ofl {
    use libmsdf::TextShaper;

    fn faces() -> Vec<(&'static str, &'static [u8], &'static str)> {
        let mut v = Vec::new();
        #[cfg(feature = "font-ofl-grenze-gotisch")]
        v.push((
            "Grenze Gotisch",
            libmsdf::GRENZE_GOTISCH_TTF,
            "Copyright 2020 The Grenze Gotisch Project Authors \
             (https://github.com/Omnibus-Type/Grenze-Gotisch)",
        ));
        #[cfg(feature = "font-ofl-texturina")]
        v.push((
            "Texturina",
            libmsdf::TEXTURINA_TTF,
            "Copyright 2020 The Texturina Project Authors \
             (https://github.com/Omnibus-Type/Texturina)",
        ));
        v
    }

    fn name(face: &ttf_parser::Face, id: u16) -> Option<String> {
        face.names()
            .into_iter()
            .filter(|n| n.name_id == id)
            .find_map(|n| n.to_string())
    }

    #[test]
    fn the_subsets_carry_their_provenance() {
        for (family, data, copyright) in faces() {
            let face = ttf_parser::Face::parse(data, 0).unwrap();
            assert_eq!(name(&face, 0).as_deref(), Some(copyright), "{family}");
            assert!(
                name(&face, 5)
                    .unwrap()
                    .ends_with("; libmsdf subset (U+0020-017F, punctuation)"),
                "{family}"
            );
            assert!(
                name(&face, 10).unwrap().starts_with("Subset by libmsdf"),
                "{family}"
            );
            for tag in [b"prep", b"fpgm", b"cvt ", b"gasp", b"DSIG"] {
                let tag = ttf_parser::Tag::from_bytes(tag);
                assert!(face.raw_face().table(tag).is_none(), "{family} keeps {tag}");
            }
        }
    }

    #[test]
    fn european_text_shapes_to_real_glyphs() {
        for (family, data, _) in faces() {
            let shaper = TextShaper::new(data.to_vec()).unwrap();
            for s in [
                "Treasury 50G",
                "\u{141}\u{f3}d\u{17a}",
                "\u{152}uvre",
                "\u{160}\u{165}astn\u{fd}",
                "\u{2039} 25 \u{2030} \u{203a}",
                "x\u{b2} H\u{2082}O",
                "\u{bd} \u{215b}",
                "2\u{2212}5 \u{2264} 7",
                "\u{2116} 5",
                "5 \u{20ac} \u{20bd}",
            ] {
                let run = shaper.shape(s);
                // Grenze Gotisch has no ⅛ (U+215B); the MATH group skips it.
                let allowed = if family == "Grenze Gotisch" && s.contains('\u{215b}') {
                    1
                } else {
                    0
                };
                assert!(run.notdef_count() <= allowed, "{family} {s:?}: {run:?}");
            }
        }
    }
}
