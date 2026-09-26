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
        let face = ttf_parser::Face::parse(&font_data, 0).map_err(|_| "failed to parse font")?;
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
        self.shape_with_options(
            text,
            rustybuzz::Direction::LeftToRight,
            None,
            None,
            &self.default_features,
        )
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

        let mut rb_features: Vec<rustybuzz::Feature> = features
            .iter()
            .map(|&(tag, val)| rustybuzz::Feature::new(tag, val, ..))
            .collect();

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
#[cfg(test)]
mod uncovered_text_shapes_to_the_placeholder {
    use super::TextShaper;

    fn shaper() -> TextShaper {
        TextShaper::new(crate::font::ROBOTO_REGULAR_ASCII.to_vec()).expect("face parses")
    }

    /// The strings that used to abort a debug build. A user's name is the case
    /// that mattered - a curly quote is a bug in a label, but `José` is a
    /// person, and neither one may take the frame down.
    #[test]
    fn a_curly_quote_and_an_ellipsis_and_a_name_all_shape() {
        for text in [
            "Script \u{201c}Chat\u{201d}",
            "clipped\u{2026}",
            "Jos\u{e9}",
        ] {
            let run = shaper().shape(text);
            assert_eq!(
                run.glyphs.len(),
                text.chars().count(),
                "every character keeps a glyph and an advance in {text:?}"
            );
        }
    }

    /// ...and the count is right, which is what tells the two apart: the curly
    /// quotes are outside coverage and get the box, the accented `e` is INSIDE
    /// it now and must not.
    #[test]
    fn only_the_uncovered_characters_become_the_box() {
        assert_eq!(
            shaper().shape("Script \u{201c}Chat\u{201d}").notdef_count(),
            2
        );
        assert_eq!(shaper().shape("clipped\u{2026}").notdef_count(), 1);
        assert_eq!(shaper().shape("Jos\u{e9} M\u{fc}ller").notdef_count(), 0);
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
