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

    #[allow(clippy::vec_init_then_push)]
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

/// The host triple, as `libmsdf-bake bundled --hashes` writes it.
fn host() -> &'static str {
    if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_arch = "x86_64", target_os = "macos")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(
        target_arch = "x86_64",
        target_os = "linux",
        target_env = "gnu"
    )) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(all(
        target_arch = "aarch64",
        target_os = "linux",
        target_env = "gnu"
    )) {
        "aarch64-unknown-linux-gnu"
    } else {
        "unknown"
    }
}

/// **Every enabled atlas constant is the reference bake** for this host
/// (`fonts/bundled.sha256`). On a host with no recorded line it prints the
/// digests and passes: a gap to fill, not a pass to rely on.
#[test]
#[allow(clippy::vec_init_then_push)]
fn bundled_atlases_match_the_reference() {
    #[allow(unused_mut)]
    let mut atlases: Vec<(&str, &[u8])> = Vec::new();
    #[cfg(feature = "font-apache-roboto-regular")]
    atlases.push(("roboto-regular-48.atlas", libmsdf::ROBOTO_REGULAR_ATLAS));
    #[cfg(feature = "font-apache-roboto-bold")]
    atlases.push(("roboto-bold-48.atlas", libmsdf::ROBOTO_BOLD_ATLAS));
    #[cfg(feature = "font-apache-roboto-italic")]
    atlases.push(("roboto-italic-48.atlas", libmsdf::ROBOTO_ITALIC_ATLAS));
    #[cfg(feature = "font-apache-roboto-msymbols")]
    atlases.push(("roboto-msymbols-48.atlas", libmsdf::ROBOTO_MSYMBOLS_ATLAS));
    #[cfg(feature = "fonts-apache-roboto")]
    atlases.push((
        "roboto-msymbols-styled-48.atlas",
        libmsdf::ROBOTO_MSYMBOLS_STYLED_ATLAS,
    ));
    #[cfg(feature = "font-ofl-grenze-gotisch")]
    atlases.push(("grenze-gotisch-48.atlas", libmsdf::GRENZE_GOTISCH_ATLAS));
    #[cfg(feature = "font-ofl-texturina")]
    atlases.push(("texturina-48.atlas", libmsdf::TEXTURINA_ATLAS));

    let recorded = include_str!("../fonts/bundled.sha256");
    for (name, bytes) in atlases {
        let digest = libmsdf::font::bundled::hex_of(&sha256(bytes));
        let line = recorded
            .lines()
            .filter(|l| !l.starts_with('#'))
            .map(|l| l.split_whitespace().collect::<Vec<_>>())
            .find(|f| f.len() == 3 && f[1] == name && f[2] == host());
        match line {
            Some(f) => assert_eq!(
                f[0],
                digest,
                "{name} is not the reference bake for {}",
                host()
            ),
            None => eprintln!(
                "no reference bake recorded for {}; record one with `libmsdf-bake bundled \
                 --hashes`: {digest}  {name}",
                host()
            ),
        }
        let atlas = libmsdf::FontAtlas::from_bytes(bytes).expect("the atlas loads");
        assert!(
            atlas.layout.is_some() && !atlas.licences().is_empty(),
            "{name}"
        );
    }
}

/// The Roboto Regular atlas is the committed test fixture, byte for byte.
#[cfg(feature = "font-apache-roboto-regular")]
#[test]
fn the_regular_atlas_is_the_fixture() {
    assert!(libmsdf::ROBOTO_REGULAR_ATLAS == include_bytes!("fixtures/roboto-ascii-48.atlas"));
}

/// Each OFL atlas shapes "Treasury 50G" from its own tables, and records
/// its coverage.
#[cfg(feature = "fonts-ofl-ledger")]
#[test]
fn the_ofl_atlases_shape_from_their_tables() {
    for bytes in [libmsdf::GRENZE_GOTISCH_ATLAS, libmsdf::TEXTURINA_ATLAS] {
        let atlas = libmsdf::FontAtlas::from_bytes(bytes).unwrap();
        assert_eq!((atlas.width, atlas.height), (800, 2000));
        let shaper = libmsdf::AtlasShaper::new(&atlas, GlyphStyle::Regular).unwrap();
        let run = shaper.shape("Treasury 50G \u{141}\u{f3}d\u{17a}");
        assert_eq!(run.notdef_count(), 0);
        assert!(atlas.coverage(GlyphStyle::Regular).is_some());
    }
}

/// sha256, by hand, so this file needs no `cpu-bake`.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[4 * i..4 * i + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    let mut out = [0u8; 32];
    for i in 0..8 {
        out[4 * i..4 * i + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}
