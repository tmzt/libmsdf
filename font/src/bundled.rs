//! **The bundled-atlas table**: which atlases libmsdf can bake from its
//! committed fonts, from which files, with which options, and which cargo
//! features enable each (bundled fonts plan, sections 4.4 and 4.5, with the
//! 13:34 and 13:40 EDT amendments). Compiled without the `bake` feature, so
//! libmsdf's build script and tests can name the files and features without
//! the baker.

use crate::GlyphStyle;

/// The cell size every bundled atlas is baked at.
pub const BUNDLED_ATLAS_GLYPH_SIZE: u32 = 48;
/// The distance range every bundled atlas is baked at, in texels.
pub const BUNDLED_ATLAS_PX_RANGE: f32 = 6.0;

/// One input face of a bundled atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundledFace {
    /// The file under libmsdf's `fonts/`.
    pub file: &'static str,
    /// Its sha256, checked before every bake.
    pub sha256: [u8; 32],
    /// For a subset face, the sha256 of the upstream file it was made from
    /// (checked by `fonts/ofl_subset.py`).
    pub upstream_sha256: Option<[u8; 32]>,
    /// The style it is baked as.
    pub style: GlyphStyle,
}

/// **Every atlas libmsdf bundles.**
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BundledAtlas {
    /// Roboto Regular, one layer.
    RobotoRegular,
    /// Roboto Bold, baked as a face of its own (Regular style), one layer.
    RobotoBold,
    /// Roboto Italic, baked as a face of its own (Regular style), one layer.
    RobotoItalic,
    /// Roboto Regular merged with Material Symbols, one layer.
    RobotoMsymbols,
    /// The merged face with Bold and Italic as style layers, three layers.
    RobotoMsymbolsStyled,
    /// Grenze Gotisch (subset), one 16-column layer.
    GrenzeGotisch,
    /// Texturina (subset), one 16-column layer.
    Texturina,
}

const fn hex(s: &str) -> [u8; 32] {
    let b = s.as_bytes();
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = (nibble(b[2 * i]) << 4) | nibble(b[2 * i + 1]);
        i += 1;
    }
    out
}

const fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => panic!("not a lowercase hex digit"),
    }
}

const ROBOTO_REGULAR: BundledFace = BundledFace {
    file: "Roboto-Regular-ascii.ttf",
    sha256: hex("b7ba6ab7d6dca2d1611e5e3c287f30e40116758928e922b440ce8666db1d6a85"),
    upstream_sha256: None,
    style: GlyphStyle::Regular,
};
const ROBOTO_MSYMBOLS: BundledFace = BundledFace {
    file: "Roboto-Regular-ascii-msymbols.ttf",
    sha256: hex("12f0fe674e25ebb3763c37e838c408f2fd128ce17c393d504e5bfe94e65237de"),
    upstream_sha256: None,
    style: GlyphStyle::Regular,
};
const ROBOTO_BOLD: BundledFace = BundledFace {
    file: "Roboto-Bold-ascii.ttf",
    sha256: hex("f3ed9c770b5fd59cf8d40d4f8e70ae28a0ff5c96fc5271ae7a1653f0d2e6d5eb"),
    upstream_sha256: None,
    style: GlyphStyle::Bold,
};
const ROBOTO_ITALIC: BundledFace = BundledFace {
    file: "Roboto-Italic-ascii.ttf",
    sha256: hex("c385f74a3e81baf200fddf547c5efdd367c6e6c21dc4989c4962d0302e062995"),
    upstream_sha256: None,
    style: GlyphStyle::Italic,
};
const ROBOTO_BOLD_AS_REGULAR: BundledFace = BundledFace {
    style: GlyphStyle::Regular,
    ..ROBOTO_BOLD
};
const ROBOTO_ITALIC_AS_REGULAR: BundledFace = BundledFace {
    style: GlyphStyle::Regular,
    ..ROBOTO_ITALIC
};
const GRENZE_GOTISCH: BundledFace = BundledFace {
    file: "GrenzeGotisch[wght]-subset.ttf",
    sha256: hex("17d705ef35289801832ff0f7563cb042b7818b329a06372b75ed8ad421d15001"),
    upstream_sha256: Some(hex(
        "701b299d8dc002a2b4bea2ff0f1272c0e4081a2835914354804565c410d0c637",
    )),
    style: GlyphStyle::Regular,
};
const TEXTURINA: BundledFace = BundledFace {
    file: "Texturina[opsz,wght]-subset.ttf",
    sha256: hex("f7ec0c7534a0f534964826cef6118475590061f9265c882c3c44e45ea2a0f108"),
    upstream_sha256: Some(hex(
        "478a15d7145cf94565cfc1aeb33596aeb0118c7d86a84d061ddf46de3d7dfda3",
    )),
    style: GlyphStyle::Regular,
};

impl BundledAtlas {
    /// Every bundled atlas.
    pub const ALL: [BundledAtlas; 7] = [
        BundledAtlas::RobotoRegular,
        BundledAtlas::RobotoBold,
        BundledAtlas::RobotoItalic,
        BundledAtlas::RobotoMsymbols,
        BundledAtlas::RobotoMsymbolsStyled,
        BundledAtlas::GrenzeGotisch,
        BundledAtlas::Texturina,
    ];

    /// The atlas file's name (in `OUT_DIR/bundled/`, `out/bundled/`).
    pub const fn file_name(self) -> &'static str {
        match self {
            BundledAtlas::RobotoRegular => "roboto-regular-48.atlas",
            BundledAtlas::RobotoBold => "roboto-bold-48.atlas",
            BundledAtlas::RobotoItalic => "roboto-italic-48.atlas",
            BundledAtlas::RobotoMsymbols => "roboto-msymbols-48.atlas",
            BundledAtlas::RobotoMsymbolsStyled => "roboto-msymbols-styled-48.atlas",
            BundledAtlas::GrenzeGotisch => "grenze-gotisch-48.atlas",
            BundledAtlas::Texturina => "texturina-48.atlas",
        }
    }

    /// The cargo features of libmsdf that, all together, enable this atlas.
    pub const fn features(self) -> &'static [&'static str] {
        match self {
            BundledAtlas::RobotoRegular => &["font-apache-roboto-regular"],
            BundledAtlas::RobotoBold => &["font-apache-roboto-bold"],
            BundledAtlas::RobotoItalic => &["font-apache-roboto-italic"],
            BundledAtlas::RobotoMsymbols => &["font-apache-roboto-msymbols"],
            BundledAtlas::RobotoMsymbolsStyled => &[
                "font-apache-roboto-msymbols",
                "font-apache-roboto-bold",
                "font-apache-roboto-italic",
            ],
            BundledAtlas::GrenzeGotisch => &["font-ofl-grenze-gotisch"],
            BundledAtlas::Texturina => &["font-ofl-texturina"],
        }
    }

    /// The input faces, the Regular-style face first.
    pub const fn faces(self) -> &'static [BundledFace] {
        match self {
            BundledAtlas::RobotoRegular => &[ROBOTO_REGULAR],
            BundledAtlas::RobotoBold => &[ROBOTO_BOLD_AS_REGULAR],
            BundledAtlas::RobotoItalic => &[ROBOTO_ITALIC_AS_REGULAR],
            BundledAtlas::RobotoMsymbols => &[ROBOTO_MSYMBOLS],
            BundledAtlas::RobotoMsymbolsStyled => &[ROBOTO_MSYMBOLS, ROBOTO_BOLD, ROBOTO_ITALIC],
            BundledAtlas::GrenzeGotisch => &[GRENZE_GOTISCH],
            BundledAtlas::Texturina => &[TEXTURINA],
        }
    }

    /// Whether this atlas is one of the two OFL faces (coverage groups, 16
    /// columns) rather than a Roboto one (shipped coverage, 8 columns).
    pub const fn is_ofl(self) -> bool {
        matches!(self, BundledAtlas::GrenzeGotisch | BundledAtlas::Texturina)
    }

    /// The licences each bundled atlas records: `(SPDX, source, styles)`.
    pub const fn licences(self) -> &'static [(&'static str, &'static str, &'static [GlyphStyle])] {
        const R: &[GlyphStyle] = &[GlyphStyle::Regular];
        const RBI: &[GlyphStyle] = &[GlyphStyle::Regular, GlyphStyle::Bold, GlyphStyle::Italic];
        match self {
            BundledAtlas::RobotoRegular | BundledAtlas::RobotoBold | BundledAtlas::RobotoItalic => {
                &[("Apache-2.0", "Roboto", R)]
            }
            BundledAtlas::RobotoMsymbols => &[
                ("Apache-2.0", "Roboto", R),
                ("Apache-2.0", "Material Symbols", R),
            ],
            BundledAtlas::RobotoMsymbolsStyled => &[
                ("Apache-2.0", "Roboto", RBI),
                ("Apache-2.0", "Material Symbols", R),
            ],
            BundledAtlas::GrenzeGotisch => &[("OFL-1.1", "Grenze Gotisch", R)],
            BundledAtlas::Texturina => &[("OFL-1.1", "Texturina", R)],
        }
    }

    /// TEXT codepoints a face of this atlas lacks, waived with a reason.
    pub const fn waivers(self) -> &'static [(GlyphStyle, char, &'static str)] {
        match self {
            BundledAtlas::Texturina => &[(
                GlyphStyle::Regular,
                '\u{17F}',
                "long s (U+017F): Texturina has no glyph; archaic",
            )],
            _ => &[],
        }
    }
}

/// Lowercase hex of a digest.
pub fn hex_of(digest: &[u8; 32]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
