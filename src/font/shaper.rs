//! Text shaping via rustybuzz (Rust port of HarfBuzz).
//!
//! Wraps `rustybuzz::shape()` to produce positioned glyph runs from Unicode
//! text, handling complex scripts, ligatures, kerning, and BiDi.

use unicode_segmentation::UnicodeSegmentation;

/// A single positioned glyph from shaping.
#[derive(Debug, Clone, Copy)]
pub struct ShapedGlyph {
    /// Font-internal glyph ID (u16, matches ttf-parser/OpenType glyph index)
    pub glyph_id: u16,
    /// Horizontal advance in font units
    pub x_advance: i32,
    /// Horizontal offset from current position (kerning, combining marks)
    pub x_offset: i32,
    /// Vertical offset from baseline
    pub y_offset: i32,
    /// Index of the source cluster (character index in original text)
    pub cluster: u32,
}

/// Result of shaping a text run.
#[derive(Debug, Clone)]
pub struct ShapedRun {
    pub glyphs: Vec<ShapedGlyph>,
    /// Total advance width in font units
    pub total_advance: i32,
    /// Font units per em (for converting to pixels: px = units * font_size / upem)
    pub units_per_em: u16,
}

impl ShapedRun {
    /// Convert total advance to pixels at a given font size.
    pub fn advance_px(&self, font_size: f32) -> f32 {
        self.total_advance as f32 * font_size / self.units_per_em as f32
    }

    /// Convert a font-unit value to pixels.
    pub fn to_px(&self, units: i32, font_size: f32) -> f32 {
        units as f32 * font_size / self.units_per_em as f32
    }

    /// **How many characters of this run the face could not draw**, and will
    /// therefore render as the placeholder box.
    ///
    /// The non-aborting successor to the `.notdef` panic
    /// [`TextShaper::shape`] used to carry. A run that contains one of these
    /// is not an error — it is what a user typed, or what a server sent — so
    /// this reports rather than decides, and every caller is free to ignore it.
    /// What it is for is the case the panic was actually right about: a test or
    /// a lint asserting that a string the REPO authored stays inside
    /// [`crate::font::TEXT_RANGES`], without that assertion also being able to
    /// fire on a stranger's name.
    pub fn notdef_count(&self) -> usize {
        self.glyphs.iter().filter(|g| g.glyph_id == 0).count()
    }
}

/// Text shaper backed by rustybuzz.
pub struct TextShaper {
    face_data: Vec<u8>,
    units_per_em: u16,
    default_features: Vec<(ttf_parser::Tag, u32)>,
}

impl TextShaper {
    /// Create a shaper from raw font file data (TTF/OTF).
    pub fn new(font_data: Vec<u8>) -> Result<Self, &'static str> {
        let face = ttf_parser::Face::parse(&font_data, 0)
            .map_err(|_| "failed to parse font")?;
        let units_per_em = face.units_per_em();
        Ok(Self {
            face_data: font_data,
            units_per_em,
            default_features: vec![
                (ttf_parser::Tag::from_bytes(b"lnum"), 1),
                (ttf_parser::Tag::from_bytes(b"pnum"), 1),
            ],
        })
    }

    /// Set the default OpenType features.
    pub fn set_default_features(&mut self, features: Vec<(ttf_parser::Tag, u32)>) {
        self.default_features = features;
    }

    /// Shape a text string into positioned glyphs.
    ///
    /// Uses default left-to-right, Latin script settings and default features.
    pub fn shape(&self, text: &str) -> ShapedRun {
        self.shape_with_options(text, rustybuzz::Direction::LeftToRight, None, None, &self.default_features)
    }

    /// Shape with explicit direction, script, language, and OpenType features.
    /// Features are passed as (tag, value) pairs, e.g. (b"lnum", 1).
    pub fn shape_with_options(
        &self,
        text: &str,
        direction: rustybuzz::Direction,
        script: Option<rustybuzz::Script>,
        language: Option<rustybuzz::Language>,
        features: &[(ttf_parser::Tag, u32)],
    ) -> ShapedRun {
        let face = rustybuzz::Face::from_slice(&self.face_data, 0)
            .expect("face already validated in new()");

        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.set_direction(direction);
        if let Some(s) = script {
            buffer.set_script(s);
        }
        if let Some(l) = language {
            buffer.set_language(l);
        }

        let mut rb_features: Vec<rustybuzz::Feature> = features.iter().map(|&(tag, val)| {
            rustybuzz::Feature::new(tag, val, ..)
        }).collect();

        // Features MUST be sorted by tag for rustybuzz/HarfBuzz
        rb_features.sort_by_key(|f| f.tag);

        let output = rustybuzz::shape(&face, &rb_features, buffer);

        let infos = output.glyph_infos();
        let positions = output.glyph_positions();

        let mut glyphs = Vec::with_capacity(infos.len());
        let mut total_advance = 0i32;

        for (info, pos) in infos.iter().zip(positions.iter()) {
            // **A control character is dropped, not drawn.**
            //
            // It shapes to `.notdef` like anything else the face cannot draw,
            // and `.notdef` is now a visible box - but the box means "this face
            // has no glyph for a character that has one", and a `'\n'` has no
            // glyph in ANY face. Drawing one would put a box in the middle of
            // every multi-line string that reaches a run: tool output, a chat
            // transcript, a pasted paragraph. Dropping it is what every text
            // engine does with C0, and it is the same set the editor's typed
            // input already filters on (`char::is_control`).
            //
            // Dropped rather than zero-advanced so nothing downstream has to
            // special-case an invisible glyph. Safe because glyphs are mapped
            // back to text by `cluster` (a byte offset), never by position -
            // `libhbui::draw`'s caret bounds and `place`'s breaker both.
            //
            // This does NOT make a hard break work: the run simply loses it, so
            // two paragraphs run together on one line. Wrapping a `'\n'` still
            // needs the draw side to shape per paragraph.
            if info.glyph_id == 0
                && text
                    .get(info.cluster as usize..)
                    .and_then(|s| s.chars().next())
                    .is_some_and(char::is_control)
            {
                continue;
            }
            glyphs.push(ShapedGlyph {
                glyph_id: info.glyph_id as u16,
                x_advance: pos.x_advance,
                x_offset: pos.x_offset,
                y_offset: pos.y_offset,
                cluster: info.cluster,
            });
            total_advance += pos.x_advance;
        }

        // **A codepoint this face cannot draw shapes to glyph 0, and glyph 0 is
        // a BOX** ([`crate::font::ROBOTO_REGULAR_ASCII`]). Nothing is checked
        // here, and nothing needs to be.
        //
        // There used to be a debug-only `panic!` on exactly this condition. It
        // was written for AUTHORED text, where a curly quote in a label is a
        // bug in the label, and at the time it was the only thing standing
        // between an em-dash and an invisible gap. The premise - that
        // everything drawn is authored source - was false: the signed-in user's
        // display name, chat transcript rows, raw tool output, synced data
        // cells and the editor buffer all reach this function, and a user
        // called `José` was enough to abort a pane. Shipping with assertions
        // off did not fix that, it only traded the crash back for the gap.
        //
        // Both failures had the same root, which was that a missing glyph drew
        // NOTHING. It draws a box now, so the honest answer is available to
        // every caller without either a panic or a scrubbing pass: authored
        // text that steps outside coverage shows a box in the review frames,
        // and runtime text that does shows a box to the user, which is what a
        // user of any other application would see.
        //
        // A caller that wants the fact rather than the pixels asks
        // [`ShapedRun::notdef_count`].

        ShapedRun {
            glyphs,
            total_advance,
            units_per_em: self.units_per_em,
        }
    }

    /// Get the units-per-em value for this font.
    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// **Can this face draw `ch`?** A cmap lookup — asked BEFORE shaping, by a
    /// caller for whom the placeholder box is the wrong answer.
    ///
    /// That caller is the icon path (see
    /// [`crate::font::msymbols_codepoint`]). Text wants the box: a character
    /// the face has no glyph for is the user's, and a box says so while
    /// leaving the rest of the string readable. An icon does not: a name in
    /// the coverage manifest that the live face turns out not to carry is a
    /// missing ASSET, which Rule 28 says to report, and a box drawn in its
    /// place would disguise the gap as a rendering quirk. Asking here is how
    /// that caller distinguishes the two without reaching for its own font
    /// parser, and without the answer coming from a hard-coded list that can
    /// drift from the face.
    ///
    /// **Glyph 0 gaining an outline did not change this.** The `!= 0` test is
    /// about the cmap, which cannot name glyph 0 at all; a codepoint the face
    /// does not map answers `false` exactly as before.
    pub fn covers(&self, ch: char) -> bool {
        ttf_parser::Face::parse(&self.face_data, 0)
            .ok()
            .and_then(|face| face.glyph_index(ch))
            .is_some_and(|glyph| glyph.0 != 0)
    }

    /// Get grapheme cluster boundaries for a string (for line-breaking).
    pub fn grapheme_indices(text: &str) -> Vec<(usize, &str)> {
        text.grapheme_indices(true).collect()
    }

    /// Resolve a codepoint to a glyph ID using the font's cmap.
    pub fn glyph_id_for_char(&self, ch: char) -> Option<u16> {
        let face = ttf_parser::Face::parse(&self.face_data, 0).ok()?;
        face.glyph_index(ch).map(|id| id.0)
    }

    /// Check if a character is likely an emoji (heuristic).
    pub fn is_emoji(ch: char) -> bool {
        let cp = ch as u32;
        // Emoticons, Misc Symbols, Dingbats, Supplemental Symbols, Flags, etc.
        matches!(cp,
            0x2600..=0x27BF |        // Misc Symbols, Dingbats
            0xFE00..=0xFE0F |        // Variation Selectors
            0x200D |                  // ZWJ
            0x1F000..=0x1FAFF |      // Mahjong, Playing Cards, Emoticons, Transport, etc.
            0xE0020..=0xE007F        // Tags (flag sequences)
        )
    }

    /// Check if a character is CJK.
    pub fn is_cjk(ch: char) -> bool {
        let cp = ch as u32;
        matches!(cp,
            0x4E00..=0x9FFF |        // CJK Unified Ideographs
            0x3400..=0x4DBF |        // CJK Unified Ideographs Extension A
            0x20000..=0x2A6DF |      // CJK Unified Ideographs Extension B
            0x2A700..=0x2B73F |      // Extension C
            0x2B740..=0x2B81F |      // Extension D
            0x2B820..=0x2CEAF |      // Extension E
            0x2CEB0..=0x2EBEF |      // Extension F
            0x30000..=0x3134F |      // Extension G
            0x3000..=0x303F |        // CJK Symbols and Punctuation
            0x3040..=0x309F |        // Hiragana
            0x30A0..=0x30FF |        // Katakana
            0xFF00..=0xFFEF          // Halfwidth and Fullwidth Forms
        )
    }
}

/// **One shaper per [`crate::font::GlyphStyle`]**, and a refusal for a style it has no face
/// for.
///
/// # Why a styled run cannot be shaped by the regular face
///
/// Emphasis is not a property a renderer can add to a shaped run. The bold
/// letterforms live in a different FILE, with their own `cmap`, their own
/// advances, their own kerning and their own ligatures - Roboto's italic `a`
/// is a different letter shape, not a slanted one. So "shape this in bold"
/// means "shape it with the bold face", and the glyph ids that come back are
/// that face's, meaningless against the regular one.
///
/// # It hands back ADDRESSES, not the face's own ids
///
/// Every [`ShapedGlyph::glyph_id`] in a run from [`StyledShaper::shape`] has
/// already been through [`crate::font::GlyphStyle::styled_glyph_id`], so it is an atlas
/// address and [`crate::font::FontAtlas::get_glyph`] takes it directly. Two consequences
/// worth being explicit about:
///
/// * Nothing downstream of shaping needs to learn about styles. The draw list
///   packs a 16-bit glyph id and packs a styled one without noticing.
/// * A regular run is bit-for-bit the run it was before this type existed
///   ([`crate::font::GlyphStyle::Regular`] is prefix 0), so this cannot change what any
///   existing caller draws.
///
/// [`ShapedRun::notdef_count`] still counts, because the placeholder is
/// address 0 in every style.
///
/// # The refusal is the point of the type
///
/// [`StyledShaper::shape`] returns `Err` for a style with no face. It does not
/// fall back to the regular one, for [`crate::font::msymbols_codepoint`]'s
/// reason: upright text where the document said emphasis is a wrong render
/// that looks like a correct one, and the only place the gap can still be
/// reported is here.
pub struct StyledShaper {
    faces: Vec<(crate::font::GlyphStyle, TextShaper)>,
}

/// Why a styled shape came back empty: [`StyledShaper`] has no face for that
/// style. Distinct from [`crate::font::StyledGlyphError`], which is the same
/// question asked of an ATLAS - a caller can have the face and not the cells,
/// or the cells and not the face, and the two are fixed in different places.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoFaceForStyle(pub crate::font::GlyphStyle);

impl core::fmt::Display for NoFaceForStyle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "no {:?} face is loaded - shaping with the regular one would draw upright text \
             where the document said emphasis",
            self.0
        )
    }
}

impl StyledShaper {
    /// A shaper carrying only [`crate::font::GlyphStyle::Regular`], from an explicit face.
    pub fn new(regular: Vec<u8>) -> Result<Self, &'static str> {
        Ok(Self {
            faces: vec![(crate::font::GlyphStyle::Regular, TextShaper::new(regular)?)],
        })
    }

    /// The bundled answer: `regular` plus every style
    /// [`crate::font::bundled_style_face`] ships a face for.
    ///
    /// `regular` is passed in rather than assumed because there are two of them
    /// ([`crate::font::ROBOTO_REGULAR_ASCII`] and
    /// [`crate::font::ROBOTO_ASCII_MSYMBOLS`]) and only the caller knows which
    /// atlas it is drawing against. The style faces are the same for both -
    /// they carry no icons at all, by [`crate::font::ROBOTO_BOLD_ASCII`]'s
    /// argument.
    pub fn bundled(regular: Vec<u8>) -> Result<Self, &'static str> {
        let mut shaper = Self::new(regular)?;
        for &style in crate::font::GlyphStyle::ALL {
            if let Some(face) = crate::font::bundled_style_face(style) {
                if style != crate::font::GlyphStyle::Regular {
                    shaper = shaper.with_face(style, face.to_vec())?;
                }
            }
        }
        Ok(shaper)
    }

    /// Attach a face for `style`. Refuses a style that already has one rather
    /// than picking a winner.
    pub fn with_face(
        mut self,
        style: crate::font::GlyphStyle,
        font_data: Vec<u8>,
    ) -> Result<Self, &'static str> {
        if self.carries(style) {
            return Err("that style already has a face on this shaper");
        }
        self.faces.push((style, TextShaper::new(font_data)?));
        Ok(self)
    }

    /// Whether a face is loaded for `style`. Asked BEFORE shaping by a caller
    /// that wants to degrade deliberately rather than by accident.
    pub fn carries(&self, style: crate::font::GlyphStyle) -> bool {
        self.faces.iter().any(|(s, _)| *s == style)
    }

    /// Every style this shaper can set, in [`crate::font::GlyphStyle::ALL`]
    /// order.
    pub fn styles(&self) -> Vec<crate::font::GlyphStyle> {
        crate::font::GlyphStyle::ALL
            .iter()
            .copied()
            .filter(|&s| self.carries(s))
            .collect()
    }

    /// The face for one style, for a caller that needs the plain
    /// [`TextShaper`] API (coverage, grapheme boundaries, metrics).
    pub fn face(&self, style: crate::font::GlyphStyle) -> Result<&TextShaper, NoFaceForStyle> {
        self.faces
            .iter()
            .find(|(s, _)| *s == style)
            .map(|(_, shaper)| shaper)
            .ok_or(NoFaceForStyle(style))
    }

    /// **Shape `text` in `style`**, with atlas addresses for glyph ids.
    ///
    /// A glyph the face numbers above [`crate::font::MAX_RAW_GLYPH_ID`] cannot
    /// be addressed, and becomes the placeholder (address 0) rather than
    /// another style's cell - the same answer the shaper already gives for a
    /// codepoint the face cannot draw, and countable the same way.
    pub fn shape(
        &self,
        style: crate::font::GlyphStyle,
        text: &str,
    ) -> Result<ShapedRun, NoFaceForStyle> {
        let mut run = self.face(style)?.shape(text);
        for glyph in &mut run.glyphs {
            glyph.glyph_id = style.styled_glyph_id(glyph.glyph_id).unwrap_or(0);
        }
        Ok(run)
    }

    /// **This codepoint, in this style**: the atlas address, or `None` when the
    /// style's face does not cover it.
    ///
    /// The cmap form of [`StyledShaper::shape`], for a caller checking one
    /// character rather than laying out a run.
    pub fn glyph_id_for_char(
        &self,
        style: crate::font::GlyphStyle,
        ch: char,
    ) -> Result<Option<u16>, NoFaceForStyle> {
        Ok(self
            .face(style)?
            .glyph_id_for_char(ch)
            .and_then(|raw| style.styled_glyph_id(raw)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Deterministic fixture: the bundled Roboto ASCII subset (upstream used
    // whatever system font was installed).
    fn roboto() -> Vec<u8> {
        crate::font::ROBOTO_REGULAR_ASCII.to_vec()
    }

    #[test]
    fn shape_basic_latin() {
        let shaper = TextShaper::new(roboto()).unwrap();
        let run = shaper.shape("Hello");

        assert_eq!(run.glyphs.len(), 5);
        assert!(run.total_advance > 0);
        assert!(run.units_per_em > 0);

        // Each glyph should have a valid glyph_id
        for g in &run.glyphs {
            assert!(g.glyph_id > 0, "glyph_id should be non-zero for 'Hello'");
            assert!(g.x_advance > 0, "advance should be positive");
        }
    }

    #[test]
    fn shape_produces_kerning() {
        let shaper = TextShaper::new(roboto()).unwrap();
        // "AV" is a classic kerning pair; Roboto kerns it via GPOS.
        let run = shaper.shape("AV");
        assert_eq!(run.glyphs.len(), 2);
        let a_alone = shaper.shape("A");
        assert!(
            run.glyphs[0].x_advance <= a_alone.glyphs[0].x_advance,
            "AV should kern tighter than A alone"
        );
    }

    #[test]
    fn emoji_detection() {
        assert!(TextShaper::is_emoji('😀'));
        assert!(TextShaper::is_emoji('🎉'));
        assert!(!TextShaper::is_emoji('A'));
        assert!(!TextShaper::is_emoji('好'));
    }

    #[test]
    fn cjk_detection() {
        assert!(TextShaper::is_cjk('好'));
        assert!(TextShaper::is_cjk('中'));
        assert!(TextShaper::is_cjk('あ'));
        assert!(TextShaper::is_cjk('ア'));
        assert!(!TextShaper::is_cjk('A'));
        assert!(!TextShaper::is_cjk('😀'));
    }

    #[test]
    fn grapheme_clusters() {
        let clusters = TextShaper::grapheme_indices("é");
        // 'é' can be 1 or 2 grapheme clusters depending on normalization
        assert!(!clusters.is_empty());
    }
}

/// What used to be `notdef_guard_control`: the same three strings, asserting
/// the opposite outcome now that an uncovered character draws a box instead of
/// aborting a debug build.
///
/// Coverage and the box are checked against the REAL BAKED ATLAS in
/// `tests/coverage.rs`; these only pin the shaper's half.
///
/// **The uncovered examples have moved once already.** A curly quote and an
/// ellipsis were the two here until [`crate::font::TEXT_RANGES`] gained the
/// typographic ten, and they are covered now - so what stands in for them is
/// the ARROW, which this tree's own prose reaches for constantly and which the
/// upstream drop every bundled glyph comes from does not define, and the
/// DAGGER, which that drop does define and which the range list deliberately
/// left out. Two different reasons to be uncovered, and the shaper owes them
/// the same answer.
#[cfg(test)]
mod uncovered_text_shapes_to_the_placeholder {
    use super::TextShaper;

    fn shaper() -> TextShaper {
        TextShaper::new(crate::font::ROBOTO_REGULAR_ASCII.to_vec()).expect("face parses")
    }

    /// The strings that used to abort a debug build. A user's name is the case
    /// that mattered - an uncovered arrow is a bug in a label, but `José` is a
    /// person, and neither one may take the frame down.
    #[test]
    fn an_arrow_and_a_dagger_and_a_name_all_shape() {
        for text in ["Script \u{2192}Chat", "note\u{2020}", "Jos\u{e9}"] {
            let run = shaper().shape(text);
            assert_eq!(
                run.glyphs.len(),
                text.chars().count(),
                "every character keeps a glyph and an advance in {text:?}"
            );
        }
    }

    /// ...and the count is right, which is what tells them apart: the arrow and
    /// the dagger are outside coverage and get the box, while the accented `e`
    /// and the typographic ten are INSIDE it and must not.
    #[test]
    fn only_the_uncovered_characters_become_the_box() {
        assert_eq!(shaper().shape("Script \u{2192}Chat\u{2192}").notdef_count(), 2);
        assert_eq!(shaper().shape("note\u{2020}").notdef_count(), 1);
        assert_eq!(shaper().shape("Jos\u{e9} M\u{fc}ller").notdef_count(), 0);
        // The widening, from the shaper's side: every one of the ten resolves.
        assert_eq!(
            shaper()
                .shape("\u{2013}\u{2014}\u{2018}\u{2019}\u{201c}\u{201d}\u{2022}\u{2026}\u{20ac}\u{2122}")
                .notdef_count(),
            0,
            "a typographic character that used to draw an invisible box still does"
        );
    }

    /// Vacuity pin: a run of plain ASCII has no placeholders at all, so the
    /// counts above are measuring something.
    #[test]
    fn plain_ascii_shapes_fine() {
        let run = shaper().shape("Script 'Chat': clipped...");
        assert!(!run.glyphs.is_empty());
        assert_eq!(run.notdef_count(), 0);
    }
}
