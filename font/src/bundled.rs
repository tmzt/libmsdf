//! **The bundled-atlas table**: which atlases libmsdf can bake from its
//! committed fonts, from which files, with which options, and which cargo
//! features enable each (bundled fonts plan, sections 4.4 and 4.5, with the
//! 13:34 and 13:40 EDT amendments). Compiled without the `bake` feature, so
//! libmsdf's build script and tests can name the files and features without
//! the baker.

use crate::coverage::{
    COVERAGE_MATH, COVERAGE_SYMBOLS, COVERAGE_TERMINAL, COVERAGE_TERMINAL_EMOJI, COVERAGE_TEXT,
    Coverage,
};
use crate::{ATLAS_COLS, CapPlacement, CellBox, GlyphStyle};

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
#[non_exhaustive]
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
    /// Noto Sans Mono Regular with Bold as a style layer, two 16-column
    /// layers, at the terminal cap placement.
    NotoSansMono,
    /// The terminal symbol page, one 16-column layer, sharing Noto Sans
    /// Mono's baseline.
    TerminalSymbols,
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
    sha256: hex("19f01d3ccd8d1abcc29db790d3e94f68166797e05727dc798b3045086042f9e7"),
    upstream_sha256: Some(hex(
        "701b299d8dc002a2b4bea2ff0f1272c0e4081a2835914354804565c410d0c637",
    )),
    style: GlyphStyle::Regular,
};
const TEXTURINA: BundledFace = BundledFace {
    file: "Texturina[opsz,wght]-subset.ttf",
    sha256: hex("399fd6669e246d50d47ec89904fb4df9efea7597768663b391f3847f4614737e"),
    upstream_sha256: Some(hex(
        "478a15d7145cf94565cfc1aeb33596aeb0118c7d86a84d061ddf46de3d7dfda3",
    )),
    style: GlyphStyle::Regular,
};

const NOTO_SANS_MONO_REGULAR: BundledFace = BundledFace {
    file: "NotoSansMono-Regular-subset.ttf",
    sha256: hex("436c580a03128211bf11c4ef1cb10d2f5ab31473829ee4585f8ebdb75313c37e"),
    upstream_sha256: Some(hex(
        "2cb2adb378a8f574213e23df697050b83c54c27df465a2015552740b2769a081",
    )),
    style: GlyphStyle::Regular,
};
const NOTO_SANS_MONO_BOLD: BundledFace = BundledFace {
    file: "NotoSansMono-Bold-subset.ttf",
    sha256: hex("b77c8dee8ec3fb5e01553b9d4323e1723dc686f77bd9eb7cd31918316c82a06f"),
    upstream_sha256: Some(hex(
        "2cb2adb378a8f574213e23df697050b83c54c27df465a2015552740b2769a081",
    )),
    style: GlyphStyle::Bold,
};

const TERMINAL_SYMBOLS: BundledFace = BundledFace {
    file: "LibmsdfTerminalSymbols-Regular.ttf",
    sha256: hex("0d91232725e03887601f6ea857300ea9c384ab6b89b05e171060246966640cca"),
    upstream_sha256: None,
    style: GlyphStyle::Regular,
};

/// The symbol page's placement: Noto Sans Mono's, with its cap height (717
/// of 1000) given, since the merged face has no `A`.
const TERMINAL_SYMBOLS_CAP: CapPlacement = CapPlacement {
    top_frac: CapPlacement::TERMINAL_TOP_FRAC,
    height_em: Some(0.717),
};

/// Texturina's cell box: 1.406 em, from y −312 to 1094.
const TEXTURINA_CELL: CellBox = CellBox {
    above_em: 0.179,
    below_em: -0.073,
};

/// Grenze Gotisch's cell box: the 1.3 em cell moved up 0.075 em, y −410 to
/// 890.
const GRENZE_GOTISCH_CELL: CellBox = CellBox {
    above_em: 0.075,
    below_em: -0.075,
};

/// The terminal faces' cap placement
/// (`docs/design/20261023_01_TERMINAL_FONTS_PLAN.md`, section 4.2).
const TERMINAL_CAP: CapPlacement = CapPlacement {
    top_frac: CapPlacement::TERMINAL_TOP_FRAC,
    height_em: None,
};

impl BundledAtlas {
    /// Every bundled atlas.
    pub const ALL: [BundledAtlas; 9] = [
        BundledAtlas::RobotoRegular,
        BundledAtlas::RobotoBold,
        BundledAtlas::RobotoItalic,
        BundledAtlas::RobotoMsymbols,
        BundledAtlas::RobotoMsymbolsStyled,
        BundledAtlas::GrenzeGotisch,
        BundledAtlas::Texturina,
        BundledAtlas::NotoSansMono,
        BundledAtlas::TerminalSymbols,
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
            BundledAtlas::NotoSansMono => "noto-sans-mono-48.atlas",
            BundledAtlas::TerminalSymbols => "terminal-symbols-48.atlas",
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
            BundledAtlas::NotoSansMono => &["font-ofl-noto-sans-mono"],
            BundledAtlas::TerminalSymbols => &["font-ofl-noto-terminal-symbols"],
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
            BundledAtlas::NotoSansMono => &[NOTO_SANS_MONO_REGULAR, NOTO_SANS_MONO_BOLD],
            BundledAtlas::TerminalSymbols => &[TERMINAL_SYMBOLS],
        }
    }

    /// Whether this atlas is one of the OFL faces (coverage groups, 16
    /// columns) rather than a Roboto one (shipped coverage, 8 columns).
    pub const fn is_ofl(self) -> bool {
        matches!(
            self,
            BundledAtlas::GrenzeGotisch
                | BundledAtlas::Texturina
                | BundledAtlas::NotoSansMono
                | BundledAtlas::TerminalSymbols
        )
    }

    /// What this atlas covers: the shipped coverage for the Roboto faces,
    /// the terminal groups for the symbol page, the text groups for the
    /// other OFL faces.
    pub fn coverage(self) -> Coverage {
        if self == BundledAtlas::TerminalSymbols {
            Coverage::Groups(vec![COVERAGE_TERMINAL, COVERAGE_TERMINAL_EMOJI])
        } else if self.is_ofl() {
            Coverage::Groups(vec![COVERAGE_TEXT, COVERAGE_SYMBOLS, COVERAGE_MATH])
        } else {
            Coverage::Shipped
        }
    }

    /// Cell columns per layer: 16 for the OFL faces, [`ATLAS_COLS`] (8)
    /// for the Roboto ones.
    pub const fn columns(self) -> u32 {
        if self.is_ofl() { 16 } else { ATLAS_COLS }
    }

    /// Where its cells put the cap height: the terminal placement for the
    /// terminal faces, the default for every other.
    pub const fn cap(self) -> CapPlacement {
        match self {
            BundledAtlas::NotoSansMono => TERMINAL_CAP,
            BundledAtlas::TerminalSymbols => TERMINAL_SYMBOLS_CAP,
            _ => CapPlacement::DEFAULT,
        }
    }

    /// **Where the atlas's cells sit relative to the line box**
    /// (`docs/design/20261023_02_PER_ATLAS_CELL_BOX_PLAN.md`, section 4.5):
    /// [`CellBox::LINE_BOX`] except for the two faces whose ink does not fit
    /// it. Texturina's accents and `ĥ ĺ` rise to 1064 units and `¿` falls to
    /// −282, more than the 1300-unit line box, so its cell is 1.406 em;
    /// Grenze Gotisch's rings rise to 862, so its 1.3 em cell moves up
    /// 0.075 em. Each keeps a texel of margin and its baseline where the
    /// line box puts it.
    pub const fn cell(self) -> CellBox {
        match self {
            BundledAtlas::Texturina => TEXTURINA_CELL,
            BundledAtlas::GrenzeGotisch => GRENZE_GOTISCH_CELL,
            _ => CellBox::LINE_BOX,
        }
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
            BundledAtlas::NotoSansMono => &[(
                "OFL-1.1",
                "Noto Sans Mono",
                &[GlyphStyle::Regular, GlyphStyle::Bold],
            )],
            BundledAtlas::TerminalSymbols => &[(
                "OFL-1.1",
                "Noto Sans Mono, Noto Sans Symbols 2, Noto Sans Symbols, Noto Sans Math, Noto Emoji",
                R,
            )],
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
            BundledAtlas::NotoSansMono => &[
                (
                    GlyphStyle::Regular,
                    '\u{AD}',
                    "soft hyphen (U+00AD): Noto Sans Mono has no glyph; default-ignorable, drawn \
                     as nothing",
                ),
                (
                    GlyphStyle::Bold,
                    '\u{AD}',
                    "soft hyphen (U+00AD): Noto Sans Mono has no glyph; default-ignorable, drawn \
                     as nothing",
                ),
            ],
            _ => &[],
        }
    }
}

/// Lowercase hex of a digest.
pub fn hex_of(digest: &[u8; 32]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
