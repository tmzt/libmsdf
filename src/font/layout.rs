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

use crate::font::MAX_RAW_GLYPH_ID;
use crate::font::atlas::TableBody;

/// The five per-face layout tags, in the order a face's tables sort.
pub const LAYOUT_TAGS: [&[u8; 4]; 5] = [b"Cmap", b"Gpos", b"Gsub", b"Head", b"Hmtx"];

/// `Gsub` kind codes.
const KIND_SINGLE: u8 = 1;
const KIND_LIGATURE: u8 = 2;
const KIND_CONTEXT_SINGLE: u8 = 3;

/// A little-endian reader over one table body that refuses to run past it.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], &'static str> {
        let end = self.pos.checked_add(n).ok_or("layout table is truncated")?;
        let out = self
            .data
            .get(self.pos..end)
            .ok_or("layout table is truncated (wrong length)")?;
        self.pos = end;
        Ok(out)
    }
    fn u8(&mut self) -> Result<u8, &'static str> {
        Ok(self.bytes(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, &'static str> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn i16(&mut self) -> Result<i16, &'static str> {
        Ok(self.u16()? as i16)
    }
    fn u32(&mut self) -> Result<u32, &'static str> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn tag(&mut self) -> Result<[u8; 4], &'static str> {
        Ok(self.bytes(4)?.try_into().unwrap())
    }
    fn zero(&mut self, n: usize) -> Result<(), &'static str> {
        if self.bytes(n)?.iter().any(|&b| b != 0) {
            return Err("layout table has non-zero reserved bytes");
        }
        Ok(())
    }
    /// A count of records of `size` bytes, checked against what is left.
    fn count(&mut self, n: usize, size: usize) -> Result<usize, &'static str> {
        if n.checked_mul(size)
            .is_none_or(|b| b > self.data.len() - self.pos)
        {
            return Err("layout table count runs past its body (wrong length)");
        }
        Ok(n)
    }
    fn end(&self) -> Result<(), &'static str> {
        if self.pos != self.data.len() {
            return Err("layout table has bytes past its last record (wrong length)");
        }
        Ok(())
    }
}

fn glyph(g: u16) -> Result<u16, &'static str> {
    if g > MAX_RAW_GLYPH_ID {
        return Err("layout table names a glyph id above MAX_RAW_GLYPH_ID");
    }
    Ok(g)
}

fn strictly_ascending<K: Ord>(
    keys: impl Iterator<Item = K>,
    what: &'static str,
) -> Result<(), &'static str> {
    let mut prev: Option<K> = None;
    for k in keys {
        if prev.as_ref().is_some_and(|p| *p >= k) {
            return Err(what);
        }
        prev = Some(k);
    }
    Ok(())
}

impl FaceLayout {
    /// Every rule a face's tables must keep, checked on read and on write.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(MIN_UNITS_PER_EM..=MAX_UNITS_PER_EM).contains(&self.units_per_em) {
            return Err("Head units_per_em is outside 16..=16384");
        }
        glyph(self.hidden_glyph)?;
        strictly_ascending(
            self.features.iter(),
            "Head features are not ascending and unique",
        )?;
        if self.features.len() > u16::MAX as usize {
            return Err("Head has too many features");
        }
        strictly_ascending(
            self.cmap.iter().map(|c| c.0),
            "Cmap is not sorted by codepoint, or repeats one",
        )?;
        for &(_, g) in &self.cmap {
            if g == 0 {
                return Err("Cmap maps a codepoint to glyph 0");
            }
            glyph(g)?;
        }
        strictly_ascending(
            self.hmtx.iter().map(|h| h.0),
            "Hmtx is not sorted by glyph, or repeats one",
        )?;
        for &(g, _) in &self.hmtx {
            glyph(g)?;
        }
        if self.gsub.len() > MAX_GSUB_LOOKUPS {
            return Err("Gsub has more than 65535 lookups");
        }
        for lookup in &self.gsub {
            match &lookup.rules {
                GsubRules::Single(r) => {
                    strictly_ascending(
                        r.iter().map(|x| x.0),
                        "Gsub single records are not sorted by glyph, or repeat one",
                    )?;
                    for &(a, b) in r {
                        glyph(a)?;
                        glyph(b)?;
                    }
                }
                GsubRules::Ligature(r) => {
                    let mut prev_first = 0u16;
                    for (i, lig) in r.iter().enumerate() {
                        let n = lig.components.len();
                        if !(MIN_LIGATURE_COMPONENTS..=MAX_LIGATURE_COMPONENTS).contains(&n) {
                            return Err("Gsub ligature has fewer than 2 or more than 8 components");
                        }
                        for &c in &lig.components {
                            glyph(c)?;
                        }
                        glyph(lig.out)?;
                        if i > 0 && lig.components[0] < prev_first {
                            return Err(
                                "Gsub ligatures are not grouped by first component ascending",
                            );
                        }
                        prev_first = lig.components[0];
                    }
                    // Grouped: a first component never reappears after another.
                    let mut seen: Vec<u16> = Vec::new();
                    for lig in r {
                        let f = lig.components[0];
                        if seen.last() != Some(&f) {
                            if seen.contains(&f) {
                                return Err(
                                    "Gsub ligatures are not grouped by first component ascending",
                                );
                            }
                            seen.push(f);
                        }
                    }
                }
                GsubRules::ContextSingle(r) => {
                    strictly_ascending(
                        r.iter().map(|x| (x.input, x.side, x.context)),
                        "Gsub context records are not sorted by (input, side, context), or repeat one",
                    )?;
                    for x in r {
                        glyph(x.context)?;
                        glyph(x.input)?;
                        glyph(x.out)?;
                    }
                }
            }
        }
        strictly_ascending(
            self.gpos.iter().map(|p| p.0),
            "Gpos is not sorted by (left, right), or repeats a pair",
        )?;
        for &(key, x) in &self.gpos {
            if x == 0 {
                return Err("Gpos carries a zero adjustment");
            }
            glyph((key >> 16) as u16)?;
            glyph(key as u16)?;
        }
        Ok(())
    }

    fn encode_head(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(20 + 4 * self.features.len());
        b.extend_from_slice(&self.units_per_em.to_le_bytes());
        b.extend_from_slice(&self.hidden_glyph.to_le_bytes());
        for v in [
            self.ascender,
            self.descender,
            self.line_gap,
            self.cap_height,
            self.x_height,
        ] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&(self.features.len() as u16).to_le_bytes());
        b.extend_from_slice(&[0; 4]);
        for f in &self.features {
            b.extend_from_slice(f);
        }
        b
    }

    fn encode_cmap(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(4 + 8 * self.cmap.len());
        b.extend_from_slice(&(self.cmap.len() as u32).to_le_bytes());
        for &(cp, g) in &self.cmap {
            b.extend_from_slice(&cp.to_le_bytes());
            b.extend_from_slice(&g.to_le_bytes());
            b.extend_from_slice(&[0; 2]);
        }
        b
    }

    fn encode_hmtx(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(4 + 4 * self.hmtx.len());
        b.extend_from_slice(&(self.hmtx.len() as u32).to_le_bytes());
        for &(g, a) in &self.hmtx {
            b.extend_from_slice(&g.to_le_bytes());
            b.extend_from_slice(&a.to_le_bytes());
        }
        b
    }

    fn encode_gsub(&self) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&(self.gsub.len() as u16).to_le_bytes());
        b.extend_from_slice(&[0; 2]);
        for lookup in &self.gsub {
            b.extend_from_slice(&lookup.feature);
            let (kind, count) = match &lookup.rules {
                GsubRules::Single(r) => (KIND_SINGLE, r.len()),
                GsubRules::Ligature(r) => (KIND_LIGATURE, r.len()),
                GsubRules::ContextSingle(r) => (KIND_CONTEXT_SINGLE, r.len()),
            };
            b.push(kind);
            b.extend_from_slice(&[0; 3]);
            b.extend_from_slice(&(count as u32).to_le_bytes());
            match &lookup.rules {
                GsubRules::Single(r) => {
                    for &(from, to) in r {
                        b.extend_from_slice(&from.to_le_bytes());
                        b.extend_from_slice(&to.to_le_bytes());
                    }
                }
                GsubRules::Ligature(r) => {
                    for lig in r {
                        b.push(lig.components.len() as u8);
                        b.push(0);
                        b.extend_from_slice(&lig.out.to_le_bytes());
                        for i in 0..MAX_LIGATURE_COMPONENTS {
                            let c = lig.components.get(i).copied().unwrap_or(0);
                            b.extend_from_slice(&c.to_le_bytes());
                        }
                    }
                }
                GsubRules::ContextSingle(r) => {
                    for x in r {
                        b.push(x.side as u8);
                        b.push(0);
                        b.extend_from_slice(&x.context.to_le_bytes());
                        b.extend_from_slice(&x.input.to_le_bytes());
                        b.extend_from_slice(&x.out.to_le_bytes());
                    }
                }
            }
        }
        b
    }

    fn encode_gpos(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(4 + 8 * self.gpos.len());
        b.extend_from_slice(&(self.gpos.len() as u32).to_le_bytes());
        for &(key, x) in &self.gpos {
            b.extend_from_slice(&((key >> 16) as u16).to_le_bytes());
            b.extend_from_slice(&(key as u16).to_le_bytes());
            b.extend_from_slice(&x.to_le_bytes());
            b.extend_from_slice(&[0; 2]);
        }
        b
    }

    /// One face from its five table bodies (`Cmap`, `Gpos`, `Gsub`, `Head`,
    /// `Hmtx`, in [`LAYOUT_TAGS`] order).
    fn decode(style: GlyphStyle, t: [&[u8]; 5]) -> Result<Self, &'static str> {
        let [cmap_b, gpos_b, gsub_b, head_b, hmtx_b] = t;

        let mut r = Reader::new(head_b);
        let units_per_em = r.u16()?;
        let hidden_glyph = r.u16()?;
        let ascender = r.i16()?;
        let descender = r.i16()?;
        let line_gap = r.i16()?;
        let cap_height = r.i16()?;
        let x_height = r.i16()?;
        let n = r.u16()? as usize;
        r.zero(4)?;
        let n = r.count(n, 4)?;
        let mut features = Vec::with_capacity(n);
        for _ in 0..n {
            features.push(r.tag()?);
        }
        r.end()?;

        let mut r = Reader::new(cmap_b);
        let n = r.u32()? as usize;
        let n = r.count(n, 8)?;
        let mut cmap = Vec::with_capacity(n);
        for _ in 0..n {
            let cp = r.u32()?;
            let g = r.u16()?;
            r.zero(2)?;
            cmap.push((cp, g));
        }
        r.end()?;

        let mut r = Reader::new(hmtx_b);
        let n = r.u32()? as usize;
        let n = r.count(n, 4)?;
        let mut hmtx = Vec::with_capacity(n);
        for _ in 0..n {
            hmtx.push((r.u16()?, r.u16()?));
        }
        r.end()?;

        let mut r = Reader::new(gsub_b);
        let n = r.u16()? as usize;
        r.zero(2)?;
        let mut gsub = Vec::with_capacity(n);
        for _ in 0..n {
            let feature = r.tag()?;
            let kind = r.u8()?;
            r.zero(3)?;
            let count = r.u32()? as usize;
            let rules = match kind {
                KIND_SINGLE => {
                    let count = r.count(count, 4)?;
                    let mut v = Vec::with_capacity(count);
                    for _ in 0..count {
                        v.push((r.u16()?, r.u16()?));
                    }
                    GsubRules::Single(v)
                }
                KIND_LIGATURE => {
                    let count = r.count(count, 20)?;
                    let mut v = Vec::with_capacity(count);
                    for _ in 0..count {
                        let len = r.u8()? as usize;
                        r.zero(1)?;
                        let out = r.u16()?;
                        if !(MIN_LIGATURE_COMPONENTS..=MAX_LIGATURE_COMPONENTS).contains(&len) {
                            return Err("Gsub ligature has fewer than 2 or more than 8 components");
                        }
                        let mut components = Vec::with_capacity(len);
                        for i in 0..MAX_LIGATURE_COMPONENTS {
                            let c = r.u16()?;
                            if i < len {
                                components.push(c);
                            } else if c != 0 {
                                return Err("Gsub ligature has non-zero unused components");
                            }
                        }
                        v.push(Ligature { components, out });
                    }
                    GsubRules::Ligature(v)
                }
                KIND_CONTEXT_SINGLE => {
                    let count = r.count(count, 8)?;
                    let mut v = Vec::with_capacity(count);
                    for _ in 0..count {
                        let side = match r.u8()? {
                            0 => ContextSide::Before,
                            1 => ContextSide::After,
                            _ => return Err("Gsub context record has a side other than 0 or 1"),
                        };
                        r.zero(1)?;
                        let context = r.u16()?;
                        let input = r.u16()?;
                        let out = r.u16()?;
                        v.push(ContextSingle {
                            side,
                            context,
                            input,
                            out,
                        });
                    }
                    GsubRules::ContextSingle(v)
                }
                _ => return Err("Gsub lookup has a kind this build does not know"),
            };
            gsub.push(GsubLookup { feature, rules });
        }
        r.end()?;

        let mut r = Reader::new(gpos_b);
        let n = r.u32()? as usize;
        let n = r.count(n, 8)?;
        let mut gpos = Vec::with_capacity(n);
        for _ in 0..n {
            let left = r.u16()? as u32;
            let right = r.u16()? as u32;
            let x = r.i16()?;
            r.zero(2)?;
            gpos.push(((left << 16) | right, x));
        }
        r.end()?;

        let face = FaceLayout {
            style,
            units_per_em,
            hidden_glyph,
            ascender,
            descender,
            line_gap,
            cap_height,
            x_height,
            features,
            cmap,
            hmtx,
            gsub,
            gpos,
        };
        face.validate()?;
        Ok(face)
    }
}

/// The layout tables of `layout` as table bodies, after checking every rule.
pub(crate) fn encode_tables(layout: Option<&AtlasLayout>) -> Result<Vec<TableBody>, &'static str> {
    let mut out = Vec::new();
    let Some(layout) = layout else {
        return Ok(out);
    };
    for face in &layout.faces {
        face.validate()?;
        let style = face.style as u8;
        for (tag, body) in [
            (b"Cmap", face.encode_cmap()),
            (b"Gpos", face.encode_gpos()),
            (b"Gsub", face.encode_gsub()),
            (b"Head", face.encode_head()),
            (b"Hmtx", face.encode_hmtx()),
        ] {
            out.push(TableBody {
                tag: *tag,
                face: style,
                body,
            });
        }
    }
    Ok(out)
}

/// The layout tables among `bodies` (tag, face byte, body), or `None` when
/// there are none. A face with only some of the five is refused.
pub(crate) fn decode_tables(
    bodies: &[(&[u8; 4], u8, &[u8])],
) -> Result<Option<AtlasLayout>, &'static str> {
    let mut faces = Vec::new();
    for &style in GlyphStyle::ALL {
        let mut found: [Option<&[u8]>; 5] = [None; 5];
        for &(tag, face, body) in bodies {
            if let Some(i) = LAYOUT_TAGS.iter().position(|t| *t == tag) {
                if face == crate::font::atlas::ATLAS_TABLE_FACE_ALL {
                    return Err("a layout table belongs to the whole atlas rather than a face");
                }
                if face == style as u8 {
                    found[i] = Some(body);
                }
            }
        }
        match found {
            [None, None, None, None, None] => {}
            [Some(a), Some(b), Some(c), Some(d), Some(e)] => {
                faces.push(FaceLayout::decode(style, [a, b, c, d, e])?);
            }
            _ => return Err("a face carries only some of Head, Cmap, Hmtx, Gsub and Gpos"),
        }
    }
    if faces.is_empty() {
        return Ok(None);
    }
    Ok(Some(AtlasLayout::from_faces(faces)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::FontAtlas;

    pub(crate) fn sample(style: GlyphStyle) -> FaceLayout {
        FaceLayout {
            style,
            units_per_em: 2048,
            hidden_glyph: 3,
            ascender: 1900,
            descender: -500,
            line_gap: 0,
            cap_height: 1456,
            x_height: 1082,
            features: vec![*b"kern", *b"liga", *b"lnum"],
            cmap: vec![(0x20, 3), (0x41, 37), (0x66, 74), (0x69, 77), (0xAD, 16)],
            hmtx: vec![
                (0, 908),
                (3, 508),
                (16, 552),
                (37, 1336),
                (74, 708),
                (77, 486),
                (200, 1100),
            ],
            gsub: vec![
                GsubLookup {
                    feature: *b"lnum",
                    rules: GsubRules::Single(vec![(20, 101), (21, 102)]),
                },
                GsubLookup {
                    feature: *b"liga",
                    rules: GsubRules::Ligature(vec![
                        Ligature {
                            components: vec![74, 74, 77],
                            out: 201,
                        },
                        Ligature {
                            components: vec![74, 77],
                            out: 200,
                        },
                    ]),
                },
                GsubLookup {
                    feature: *b"ccmp",
                    rules: GsubRules::ContextSingle(vec![
                        ContextSingle {
                            side: ContextSide::Before,
                            context: 37,
                            input: 16,
                            out: 17,
                        },
                        ContextSingle {
                            side: ContextSide::After,
                            context: 3,
                            input: 16,
                            out: 17,
                        },
                    ]),
                },
            ],
            gpos: vec![((3 << 16) | 37, -40), ((37 << 16) | 3, -40)],
        }
    }

    fn bytes_of(faces: Vec<FaceLayout>) -> Vec<u8> {
        let mut atlas = FontAtlas::empty(1, 1, 3);
        atlas.layout = Some(AtlasLayout::from_faces(faces).unwrap());
        atlas.to_bytes()
    }

    #[test]
    fn faces_round_trip_through_the_file() {
        let mut empty = sample(GlyphStyle::Bold);
        empty.gsub.clear();
        empty.gpos.clear();
        empty.features.clear();
        let faces = vec![sample(GlyphStyle::Regular), empty];
        let back = FontAtlas::from_bytes(&bytes_of(faces.clone())).expect("loads");
        let layout = back.layout.expect("layout survives");
        assert_eq!(layout.faces(), &faces[..]);
        assert_eq!(layout.styles(), vec![GlyphStyle::Regular, GlyphStyle::Bold]);
        assert!(layout.face(GlyphStyle::Italic).is_none());
    }

    /// Bodies of one sample face, by tag, to corrupt one at a time.
    fn bodies() -> Vec<TableBody> {
        let layout = AtlasLayout::from_faces(vec![sample(GlyphStyle::Regular)]).unwrap();
        encode_tables(Some(&layout)).unwrap()
    }

    fn load(tables: &[TableBody]) -> Result<FontAtlas, &'static str> {
        FontAtlas::from_bytes(&FontAtlas::empty(1, 1, 3).write_v5(tables))
    }

    fn with(tag: &[u8; 4], f: impl Fn(&mut Vec<u8>)) -> Vec<TableBody> {
        let mut t = bodies();
        let b = t.iter_mut().find(|b| &b.tag == tag).unwrap();
        f(&mut b.body);
        t
    }

    #[test]
    fn wrong_length_bodies_are_refused() {
        for tag in LAYOUT_TAGS {
            let err = load(&with(tag, |b| b.push(0))).expect_err("a byte too many");
            assert!(err.contains("wrong length"), "{tag:?}: {err}");
            let err = load(&with(tag, |b| {
                b.pop();
            }))
            .expect_err("a byte short");
            assert!(
                err.contains("wrong length") || err.contains("truncated"),
                "{tag:?}: {err}"
            );
        }
    }

    #[test]
    fn unsorted_duplicate_and_out_of_limit_tables_are_refused() {
        let refuse = |f: &dyn Fn(&mut FaceLayout), why: &str| {
            let mut face = sample(GlyphStyle::Regular);
            f(&mut face);
            assert_eq!(
                face.validate().err().map(|e| e.contains(why)),
                Some(true),
                "{why}"
            );
            let mut atlas = FontAtlas::empty(1, 1, 3);
            atlas.layout = Some(AtlasLayout::from_faces(vec![face]).unwrap());
            assert!(
                atlas.try_to_bytes().is_err(),
                "the encoder refuses too: {why}"
            );
        };
        refuse(&|f| f.cmap.swap(0, 1), "Cmap is not sorted");
        refuse(&|f| f.cmap[1].0 = f.cmap[0].0, "Cmap is not sorted");
        refuse(&|f| f.cmap[0].1 = 0, "glyph 0");
        refuse(&|f| f.hmtx.push((200, 1)), "Hmtx is not sorted");
        refuse(&|f| f.units_per_em = 8, "units_per_em");
        refuse(&|f| f.features.reverse(), "Head features");
        refuse(
            &|f| f.hmtx.push((MAX_RAW_GLYPH_ID + 1, 1)),
            "MAX_RAW_GLYPH_ID",
        );
        refuse(&|f| f.gpos.swap(0, 1), "Gpos is not sorted");
        refuse(&|f| f.gpos[0].1 = 0, "zero adjustment");
        refuse(
            &|f| {
                f.gsub[1].rules = GsubRules::Ligature(vec![Ligature {
                    components: vec![1; 9],
                    out: 2,
                }])
            },
            "more than 8",
        );
        refuse(
            &|f| {
                f.gsub[1].rules = GsubRules::Ligature(vec![
                    Ligature {
                        components: vec![9, 1],
                        out: 2,
                    },
                    Ligature {
                        components: vec![4, 1],
                        out: 3,
                    },
                ])
            },
            "grouped",
        );
        refuse(
            &|f| f.gsub[0].rules = GsubRules::Single(vec![(5, 1), (5, 2)]),
            "single records",
        );
        refuse(
            &|f| {
                if let GsubRules::ContextSingle(r) = &mut f.gsub[2].rules {
                    r.reverse();
                }
            },
            "context records",
        );
        // Raw bytes no in-memory value can produce.
        let err = load(&with(b"Gsub", |b| b[8] = 9)).expect_err("kind 9");
        assert!(err.contains("kind"), "{err}");
        let err = load(&with(b"Cmap", |b| b[10] = 1)).expect_err("reserved");
        assert!(err.contains("reserved"), "{err}");
    }

    #[test]
    fn a_face_with_only_some_tables_is_refused() {
        for skip in LAYOUT_TAGS {
            let t: Vec<TableBody> = bodies().into_iter().filter(|b| &b.tag != skip).collect();
            let err = load(&t).expect_err("four of five");
            assert!(err.contains("only some"), "{skip:?}: {err}");
        }
    }
}
