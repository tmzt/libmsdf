//! **Layout tables carried in an atlas**: what a face needs to lay out its
//! baked text without the TTF and without a shaping library.
//!
//! An atlas of file-major 5 can carry, per face, five small tables flattened
//! at bake time from the face's own OpenType tables
//! (`docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md`, sections 4.5 to
//! 4.7):
//!
//! * `Head` - units per em, the glyph a default ignorable becomes, vertical
//!   metrics and the features the bake applied;
//! * `Cmap` - codepoint to glyph, over [`crate::font::TEXT_RANGES`] and the
//!   control codepoints the face maps;
//! * `Hmtx` - integer advances of every glyph the shaper can output;
//! * `Gsub` - single, ligature and one-glyph-context substitutions, in the
//!   order rustybuzz applies them;
//! * `Gpos` - pair kerning as integer x-advance adjustments.
//!
//! A first-capital tag (`Cmap`) names libmsdf's simplified flavour of the
//! OpenType table of the same name; nothing about OpenType's byte layout
//! carries over. Nothing here is a program: every table is plain records a
//! reader looks up (plan section 4.4).
//!
//! This module holds the parsed tables and their byte codecs and compiles
//! without the `shaper` feature. [`crate::font::AtlasShaper`] lays text out
//! from them.

use crate::font::GlyphStyle;

/// Units per em a `Head` table may state.
pub const MIN_UNITS_PER_EM: u16 = 16;
/// Units per em a `Head` table may state.
pub const MAX_UNITS_PER_EM: u16 = 16384;
/// The most `Gsub` lookups one face may carry.
pub const MAX_GSUB_LOOKUPS: usize = 65_535;
/// The fewest components a `Gsub` ligature has.
pub const MIN_LIGATURE_COMPONENTS: usize = 2;
/// The most components a `Gsub` ligature has.
pub const MAX_LIGATURE_COMPONENTS: usize = 8;

/// **Every layout table of an atlas**, one [`FaceLayout`] per baked style.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AtlasLayout {
    faces: Vec<FaceLayout>,
}

impl AtlasLayout {
    /// An atlas layout from its faces, kept in [`GlyphStyle`] order. Refuses
    /// two faces of one style.
    pub fn from_faces(mut faces: Vec<FaceLayout>) -> Result<Self, &'static str> {
        faces.sort_by_key(|f| f.style);
        if faces.windows(2).any(|w| w[0].style == w[1].style) {
            return Err("two layout faces have the same style");
        }
        Ok(Self { faces })
    }

    /// The layout of one style's face, if the atlas carries one.
    pub fn face(&self, style: GlyphStyle) -> Option<&FaceLayout> {
        self.faces.iter().find(|f| f.style == style)
    }

    /// Every style with a layout, in [`GlyphStyle`] order.
    pub fn styles(&self) -> Vec<GlyphStyle> {
        self.faces.iter().map(|f| f.style).collect()
    }

    /// Every face's layout, in [`GlyphStyle`] order.
    pub fn faces(&self) -> &[FaceLayout] {
        &self.faces
    }
}

/// **One face's layout tables**, parsed. Glyph ids are the face's RAW ids
/// (at most [`crate::font::MAX_RAW_GLYPH_ID`]); [`crate::font::AtlasShaper`]
/// turns them into atlas addresses with [`GlyphStyle::styled_glyph_id`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaceLayout {
    /// The style this face is baked as.
    pub style: GlyphStyle,
    /// `head.unitsPerEm`; every other value here is in these units.
    pub units_per_em: u16,
    /// The glyph a default ignorable becomes (the face's space glyph); 0 when
    /// the face has no space glyph and ignorables are deleted instead.
    pub hidden_glyph: u16,
    /// `hhea` ascender (positive above the baseline).
    pub ascender: i16,
    /// `hhea` descender (negative below the baseline).
    pub descender: i16,
    /// `hhea` line gap.
    pub line_gap: i16,
    /// `OS/2` sCapHeight (version 2 or later, non-zero), else the top of
    /// `H`'s bounding box, else 0.
    pub cap_height: i16,
    /// `OS/2` sxHeight under the same rule, else the top of `x`'s bounding
    /// box, else 0.
    pub x_height: i16,
    /// Every GSUB and GPOS feature the bake applied that the face has,
    /// ascending and unique.
    pub features: Vec<[u8; 4]>,
    /// `(codepoint, glyph)`, sorted by codepoint, unique, glyph never 0.
    pub cmap: Vec<(u32, u16)>,
    /// `(glyph, advance)`, sorted by glyph, unique.
    pub hmtx: Vec<(u16, u16)>,
    /// Substitution lookups, in the order they are applied.
    pub gsub: Vec<GsubLookup>,
    /// `((left << 16) | right, x_advance)`, sorted by key, unique, never 0.
    pub gpos: Vec<(u32, i16)>,
}

/// One substitution lookup: the feature it came from and its rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GsubLookup {
    /// The feature the lookup came from (the first, if several).
    pub feature: [u8; 4],
    /// Its rules.
    pub rules: GsubRules,
}

/// The rules of one [`GsubLookup`], by kind.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GsubRules {
    /// Kind 1: `(from, to)`, sorted by `from`, unique.
    Single(Vec<(u16, u16)>),
    /// Kind 2: grouped by first component ascending; inside a group, in the
    /// font's order (the first match wins).
    Ligature(Vec<Ligature>),
    /// Kind 3: one input glyph, one context glyph on one side, a single
    /// substitution; sorted by `(input, side, context)`, unique.
    ContextSingle(Vec<ContextSingle>),
}

/// A ligature: its components (2 to 8, the first included) and the glyph
/// they become.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ligature {
    /// The component glyphs, first included.
    pub components: Vec<u16>,
    /// The ligature glyph.
    pub out: u16,
}

/// Which side of the input glyph a [`ContextSingle`]'s context glyph is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContextSide {
    /// The nearest non-ignorable glyph before the input.
    Before = 0,
    /// The nearest non-ignorable glyph after the input.
    After = 1,
}

/// A one-input, one-context-glyph single substitution (`Gsub` kind 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextSingle {
    /// Which side the context glyph is on.
    pub side: ContextSide,
    /// The context glyph.
    pub context: u16,
    /// The glyph that is replaced.
    pub input: u16,
    /// What it is replaced with.
    pub out: u16,
}

// ── Byte codecs ─────────────────────────────────────────────────────────

/// The layout tables of `layout` as table bodies.
pub(crate) fn encode_tables(layout: Option<&AtlasLayout>) -> Vec<crate::font::atlas::TableBody> {
    let _ = layout;
    Vec::new()
}

/// The layout tables among `bodies` (tag, face byte, body), or `None` when
/// there are none.
pub(crate) fn decode_tables(
    bodies: &[(&[u8; 4], u8, &[u8])],
) -> Result<Option<AtlasLayout>, &'static str> {
    let _ = bodies;
    Ok(None)
}
