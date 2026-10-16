//! **`AtlasShaper`: text layout from an atlas alone** (plan section 4.10).
//!
//! A pure-Rust shaper over the layout tables a baked atlas carries
//! ([`crate::layout`]): a `Cmap` lookup, the `Gsub` lookups in order,
//! `Hmtx` advances and `Gpos` pair kerning. It needs no TTF and no font
//! parsing dependency, and produces the same [`ShapedRun`] as
//! [`crate::TextShaper`] (and [`crate::StyledShaper`] for another
//! style), so [`crate::DrawList::push_shaped_text`] and its callers are
//! unchanged.
//!
//! # The contract
//!
//! For any string whose characters are in the face's `Cmap` scope
//! ([`crate::TEXT_RANGES`] and the control codepoints the face maps)
//! or are unmapped by the face, the output equals `TextShaper`'s with the
//! bake's features, glyph for glyph and unit for unit; the bake proves it on
//! a corpus and refuses a face it cannot represent. A character the face
//! maps but the scope leaves out shapes to glyph 0 with the `.notdef`
//! advance, where `TextShaper` returns the face's glyph: both draw the
//! placeholder box (that glyph has no cell), but the advance can differ.

use crate::layout::{ContextSide, FaceLayout, GsubRules};
use crate::shaper::{ShapedGlyph, ShapedRun};
use crate::{FontAtlas, GlyphStyle};

/// **What a caller needs from a shaper**, implemented by
/// [`crate::TextShaper`] and [`AtlasShaper`].
pub trait RunShaper {
    /// Shape `text` into positioned glyphs, in font units.
    fn shape(&self, text: &str) -> ShapedRun;
    /// The face's units per em.
    fn units_per_em(&self) -> u16;
    /// Whether the face maps `ch` to a non-zero glyph.
    fn covers(&self, ch: char) -> bool;
}

/// Why an [`AtlasShaper`] could not be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoLayout {
    /// The atlas carries no layout tables at all (a legacy file, a runtime
    /// atlas, or a bake without `with_layout`).
    NotBaked,
    /// The atlas carries layout tables, but not for this style.
    StyleNotBaked(GlyphStyle),
}

impl core::fmt::Display for NoLayout {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotBaked => write!(
                f,
                "this atlas carries no layout tables - re-bake it with bake_atlas --layout, or \
                 shape with TextShaper"
            ),
            Self::StyleNotBaked(s) => {
                write!(f, "this atlas carries no layout tables for {s:?}")
            }
        }
    }
}

impl std::error::Error for NoLayout {}

/// **A shaper over one style's layout tables.** Owns a copy of the tables
/// (small: a few kilobytes), so it does not borrow the atlas.
#[derive(Debug, Clone)]
pub struct AtlasShaper {
    face: FaceLayout,
}

/// One glyph while shaping.
#[derive(Debug, Clone, Copy)]
struct Slot {
    glyph: u16,
    cluster: u32,
    /// A default ignorable that no substitution has touched.
    ignorable: bool,
}

impl AtlasShaper {
    /// A shaper for `style`, from the layout tables `atlas` carries.
    pub fn new(atlas: &FontAtlas, style: GlyphStyle) -> Result<Self, NoLayout> {
        let layout = atlas.layout.as_ref().ok_or(NoLayout::NotBaked)?;
        let face = layout.face(style).ok_or(NoLayout::StyleNotBaked(style))?;
        Ok(Self { face: face.clone() })
    }

    /// A shaper over explicit tables.
    pub fn from_face(face: FaceLayout) -> Self {
        Self { face }
    }

    /// The style this shaper's glyph ids are addressed in.
    pub fn style(&self) -> GlyphStyle {
        self.face.style
    }

    /// The tables this shaper reads.
    pub fn face(&self) -> &FaceLayout {
        &self.face
    }

    /// **The atlas address of `ch`** in this shaper's style, or `None` when
    /// the `Cmap` does not map it.
    pub fn glyph_id_for_char(&self, ch: char) -> Option<u16> {
        self.raw_glyph(ch)
            .and_then(|g| self.face.style.styled_glyph_id(g))
    }

    fn raw_glyph(&self, ch: char) -> Option<u16> {
        let cp = ch as u32;
        self.face
            .cmap
            .binary_search_by_key(&cp, |c| c.0)
            .ok()
            .map(|i| self.face.cmap[i].1)
    }

    fn advance(&self, g: u16) -> i32 {
        self.face
            .hmtx
            .binary_search_by_key(&g, |h| h.0)
            .map_or(0, |i| self.face.hmtx[i].1 as i32)
    }

    fn kern(&self, left: u16, right: u16) -> i32 {
        let key = ((left as u32) << 16) | right as u32;
        self.face
            .gpos
            .binary_search_by_key(&key, |p| p.0)
            .map_or(0, |i| self.face.gpos[i].1 as i32)
    }

    /// Shape `text` (left to right) into atlas addresses for this style.
    pub fn shape(&self, text: &str) -> ShapedRun {
        // 1. Cmap.
        let mut buf: Vec<Slot> = text
            .char_indices()
            .map(|(b, ch)| Slot {
                glyph: self.raw_glyph(ch).unwrap_or(0),
                cluster: b as u32,
                ignorable: is_default_ignorable(ch),
            })
            .collect();

        // 2. Gsub, lookup by lookup.
        for lookup in &self.face.gsub {
            match &lookup.rules {
                GsubRules::Single(rules) => {
                    for slot in &mut buf {
                        if let Ok(i) = rules.binary_search_by_key(&slot.glyph, |r| r.0) {
                            slot.glyph = rules[i].1;
                            slot.ignorable = false;
                        }
                    }
                }
                GsubRules::Ligature(ligs) => {
                    let mut i = 0;
                    while i < buf.len() {
                        let first = buf[i].glyph;
                        let start = ligs.partition_point(|l| l.components[0] < first);
                        let mut applied = None;
                        for lig in ligs[start..]
                            .iter()
                            .take_while(|l| l.components[0] == first)
                        {
                            if let Some(positions) = match_forward(&buf, i, &lig.components[1..]) {
                                applied = Some((lig.out, positions));
                                break;
                            }
                        }
                        let Some((out, positions)) = applied else {
                            i += 1;
                            continue;
                        };
                        let end = *positions.last().expect("at least two components");
                        merge_clusters(&mut buf, i, end + 1);
                        // The ligature replaces the first component; glyphs
                        // skipped inside the span move after it; the other
                        // components go.
                        let span: Vec<Slot> = buf[i..=end].to_vec();
                        let mut rebuilt = Vec::with_capacity(span.len());
                        rebuilt.push(Slot {
                            glyph: out,
                            cluster: span[0].cluster,
                            ignorable: false,
                        });
                        let mut skipped = 0;
                        for (k, slot) in span.iter().enumerate().skip(1) {
                            if !positions.contains(&(i + k)) {
                                rebuilt.push(*slot);
                                skipped += 1;
                            }
                        }
                        buf.splice(i..=end, rebuilt);
                        i += 1 + skipped;
                    }
                }
                GsubRules::ContextSingle(rules) => {
                    for i in 0..buf.len() {
                        let input = buf[i].glyph;
                        let start = rules.partition_point(|r| r.input < input);
                        let hit = rules[start..].iter().take_while(|r| r.input == input).find(
                            |r| match r.side {
                                ContextSide::Before => matches_before(&buf, i, r.context),
                                ContextSide::After => matches_after(&buf, i, r.context),
                            },
                        );
                        if let Some(r) = hit {
                            buf[i].glyph = r.out;
                            buf[i].ignorable = false;
                        }
                    }
                }
            }
        }

        // 3 and 4. Advances, then kerning against the next non-ignorable.
        let mut advances: Vec<i32> = buf.iter().map(|s| self.advance(s.glyph)).collect();
        for i in 0..buf.len() {
            if let Some(j) = (i + 1..buf.len()).find(|&j| !buf[j].ignorable) {
                advances[i] += self.kern(buf[i].glyph, buf[j].glyph);
            }
        }

        // Default ignorables: zero advance, then hidden (or deleted).
        for (slot, adv) in buf.iter().zip(advances.iter_mut()) {
            if slot.ignorable {
                *adv = 0;
            }
        }
        let mut glyphs: Vec<(Slot, i32)> = buf.into_iter().zip(advances).collect();
        if self.face.hidden_glyph != 0 {
            for (slot, _) in &mut glyphs {
                if slot.ignorable {
                    slot.glyph = self.face.hidden_glyph;
                }
            }
        } else {
            delete_ignorables(&mut glyphs);
        }

        // 5 and 6. Drop an unmapped control character, as TextShaper does;
        // address the glyphs in this style.
        let mut out = Vec::with_capacity(glyphs.len());
        let mut total_advance = 0;
        for (slot, adv) in glyphs {
            if slot.glyph == 0
                && text
                    .get(slot.cluster as usize..)
                    .and_then(|s| s.chars().next())
                    .is_some_and(char::is_control)
            {
                continue;
            }
            out.push(ShapedGlyph {
                glyph_id: self.face.style.styled_glyph_id(slot.glyph).unwrap_or(0),
                x_advance: adv,
                x_offset: 0,
                y_offset: 0,
                cluster: slot.cluster,
            });
            total_advance += adv;
        }
        ShapedRun {
            glyphs: out,
            total_advance,
            units_per_em: self.face.units_per_em,
        }
    }
}

impl RunShaper for AtlasShaper {
    fn shape(&self, text: &str) -> ShapedRun {
        AtlasShaper::shape(self, text)
    }
    fn units_per_em(&self) -> u16 {
        self.face.units_per_em
    }
    fn covers(&self, ch: char) -> bool {
        self.raw_glyph(ch).is_some()
    }
}

/// Match `rest` against the glyphs after `i`, skipping a default ignorable
/// that does not match (HarfBuzz's skipping iterator: an ignorable is
/// "skip maybe"). The positions of every component, `i` first.
fn match_forward(buf: &[Slot], i: usize, rest: &[u16]) -> Option<Vec<usize>> {
    let mut positions = vec![i];
    let mut j = i;
    for &want in rest {
        loop {
            j += 1;
            let slot = buf.get(j)?;
            if slot.glyph == want {
                break;
            }
            if !slot.ignorable {
                return None;
            }
        }
        positions.push(j);
    }
    Some(positions)
}

/// Whether the nearest glyph before `i` that is not a skipped ignorable is
/// `context`.
fn matches_before(buf: &[Slot], i: usize, context: u16) -> bool {
    for slot in buf[..i].iter().rev() {
        if slot.glyph == context {
            return true;
        }
        if !slot.ignorable {
            return false;
        }
    }
    false
}

/// [`matches_before`], after `i`.
fn matches_after(buf: &[Slot], i: usize, context: u16) -> bool {
    for slot in &buf[i + 1..] {
        if slot.glyph == context {
            return true;
        }
        if !slot.ignorable {
            return false;
        }
    }
    false
}

/// HarfBuzz's `merge_clusters(start, end)`: every glyph in the range (grown
/// to whole clusters at both ends) takes the smallest cluster in it.
fn merge_clusters(buf: &mut [Slot], mut start: usize, mut end: usize) {
    if end - start < 2 {
        return;
    }
    let cluster = buf[start..end].iter().map(|s| s.cluster).min().unwrap();
    while end < buf.len() && buf[end - 1].cluster == buf[end].cluster {
        end += 1;
    }
    while start > 0 && buf[start - 1].cluster == buf[start].cluster {
        start -= 1;
    }
    for slot in &mut buf[start..end] {
        slot.cluster = cluster;
    }
}

/// Delete default ignorables when the face has no space glyph to hide them
/// as, merging clusters the way rustybuzz's `delete_glyphs_inplace` does.
fn delete_ignorables(glyphs: &mut Vec<(Slot, i32)>) {
    let mut out: Vec<(Slot, i32)> = Vec::with_capacity(glyphs.len());
    let n = glyphs.len();
    let mut i = 0;
    while i < n {
        let (slot, _) = glyphs[i];
        if slot.ignorable {
            let cluster = slot.cluster;
            if i + 1 < n && glyphs[i + 1].0.cluster == cluster {
                i += 1;
                continue;
            }
            if let Some(last) = out.last().map(|g| g.0.cluster) {
                if cluster < last {
                    for g in out.iter_mut().rev().take_while(|g| g.0.cluster == last) {
                        g.0.cluster = cluster;
                    }
                }
            } else if i + 1 < n {
                let next = glyphs[i + 1].0.cluster;
                let m = cluster.min(next);
                // Merge forward: the next cluster takes the smaller value.
                let mut k = i + 1;
                while k < n && glyphs[k].0.cluster == next {
                    glyphs[k].0.cluster = m;
                    k += 1;
                }
            }
            i += 1;
            continue;
        }
        out.push(glyphs[i]);
        i += 1;
    }
    *glyphs = out;
}

/// **Default_Ignorable_Code_Point, as rustybuzz 0.20.1 tests it**
/// (`hb/unicode.rs`, `CharExt::is_default_ignorable`), copied range for
/// range. U+00AD SOFT HYPHEN is the one inside [`crate::TEXT_RANGES`].
pub fn is_default_ignorable(ch: char) -> bool {
    let c = ch as u32;
    matches!(c,
        0x00AD
        | 0x034F
        | 0x061C
        | 0x17B4..=0x17B5
        | 0x180B..=0x180E
        | 0x200B..=0x200F
        | 0x202A..=0x202E
        | 0x2060..=0x206F
        | 0xFE00..=0xFE0F
        | 0xFEFF
        | 0xFFF0..=0xFFF8
        | 0x1D173..=0x1D17A
        | 0xE0000..=0xE0FFF
    )
}

#[cfg(all(test, feature = "shaper"))]
mod tests {
    use super::*;
    use crate::layout_bake::LayoutOptions;
    use crate::{AtlasLayout, StyledShaper, TextShaper};

    const STEP4: &[&str] = &[
        "Hello",
        "AV",
        "fifty",
        "office",
        "T ",
        " T",
        "f\u{AD}i",
        "T\u{AD} ",
        "0123",
        "a\tb",
        "Jos\u{e9}",
    ];

    fn layout(face: &[u8], style: GlyphStyle) -> FaceLayout {
        FaceLayout::extract_tables(face, style, &LayoutOptions::default()).expect("extracts")
    }

    #[test]
    fn the_bundled_faces_shape_as_textshaper_does() {
        for regular in [crate::TEST_FACE, crate::TEST_FACE_MSYMBOLS] {
            let text = TextShaper::new(regular.to_vec()).unwrap();
            let atlas = AtlasShaper::from_face(layout(regular, GlyphStyle::Regular));
            for s in STEP4 {
                assert_eq!(atlas.shape(s), text.shape(s), "regular {s:?}");
            }
        }
        let styled = StyledShaper::new(crate::TEST_FACE.to_vec())
            .unwrap()
            .with_face(GlyphStyle::Bold, crate::TEST_FACE_BOLD.to_vec())
            .unwrap()
            .with_face(GlyphStyle::Italic, crate::TEST_FACE_ITALIC.to_vec())
            .unwrap();
        for style in [GlyphStyle::Bold, GlyphStyle::Italic] {
            let face = crate::test_style_face(style).unwrap();
            let atlas = AtlasShaper::from_face(layout(face, style));
            assert_eq!(atlas.style(), style);
            for s in STEP4 {
                assert_eq!(
                    atlas.shape(s),
                    styled.shape(style, s).unwrap(),
                    "{style:?} {s:?}"
                );
            }
        }
    }

    #[test]
    fn the_soft_hyphen_cases_are_what_rustybuzz_does() {
        let face = crate::TEST_FACE;
        let atlas = AtlasShaper::from_face(layout(face, GlyphStyle::Regular));
        // f + SHY + i: the fi ligature, then the hidden space glyph, both
        // cluster 0.
        let run = atlas.shape("f\u{AD}i");
        assert_eq!(run.glyphs.len(), 2);
        assert_eq!(run.glyphs[0].cluster, 0);
        assert_eq!(run.glyphs[1].cluster, 0);
        assert_eq!(run.glyphs[1].x_advance, 0);
        // T + SHY + space: T kerns against the space (1222 - 40).
        assert_eq!(atlas.shape("T\u{AD} ").glyphs[0].x_advance, 1182);
    }

    #[test]
    fn a_legacy_atlas_has_no_layout() {
        let legacy = FontAtlas::from_bytes(include_bytes!(
            "../../tests/fixtures/legacy/roboto-ascii-48-v3.atlas"
        ))
        .unwrap();
        assert_eq!(
            AtlasShaper::new(&legacy, GlyphStyle::Regular).err(),
            Some(NoLayout::NotBaked)
        );
        let mut with = legacy.clone();
        with.layout = Some(
            AtlasLayout::from_faces(vec![layout(crate::TEST_FACE, GlyphStyle::Regular)]).unwrap(),
        );
        assert_eq!(
            AtlasShaper::new(&with, GlyphStyle::Bold).err(),
            Some(NoLayout::StyleNotBaked(GlyphStyle::Bold))
        );
        assert!(AtlasShaper::new(&with, GlyphStyle::Regular).is_ok());
    }
}
