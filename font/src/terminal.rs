//! **Terminal lines on a fixed cell grid**
//! (`docs/design/20261023_01_TERMINAL_FONTS_PLAN.md`, section 4.7 and the
//! 02:32 EDT amendment).
//!
//! A terminal line takes each character from one of two atlases that share
//! a cell and a baseline: the text atlas (Noto Sans Mono) when its `Cmap`
//! maps the character, otherwise the symbol page, otherwise the text atlas's
//! placeholder. [`TerminalShaper::line`] splits the line into runs of one
//! source, shapes each with that atlas's [`AtlasShaper`], and snaps every
//! advance to whole cells, so a column is a column whatever the face's
//! metrics. The symbol page's advances are one cell or two, so the advance
//! is the width: no Unicode width table is needed.
//!
//! Box drawing joins only when lines are [`TerminalShaper::line_pitch_em`]
//! apart, the 1.3 em line box the symbol page's box and block glyphs are
//! remapped onto.

use crate::{AtlasShaper, FontAtlas, GlyphStyle, LINE_BOX_RATIO, ShapedRun, is_default_ignorable};

/// The block the wide forms of the narrow curated emoji are mapped in, in
/// the Supplementary Private Use Area-A, outside libmsdf's own BMP private
/// blocks.
pub const TERMINAL_WIDE_FORMS: (char, char) = ('\u{F0000}', '\u{F00FF}');

/// **The wide forms**, as (base, wide form): a base followed by U+FE0F is
/// drawn as its wide form, two cells; otherwise it is one cell. The same
/// pairs as `fonts/terminal_symbols.py`'s `WIDE_FORMS`.
pub const TERMINAL_WIDE_FORM: &[(char, char)] =
    &[('\u{26A0}', '\u{F0000}'), ('\u{27A1}', '\u{F0001}')];

/// VARIATION SELECTOR-16, the emoji presentation selector.
const VS16: char = '\u{FE0F}';

/// Which atlas a segment's glyphs are in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerminalSource {
    /// The text atlas.
    Text,
    /// The symbol page.
    Symbols,
}

/// A run of glyphs from one atlas, starting at a column.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalSegment {
    /// The atlas the run's glyphs are in.
    pub source: TerminalSource,
    /// The column the run starts at.
    pub column: u32,
    /// The run, its advances snapped to whole cells (font units).
    pub run: ShapedRun,
}

/// One laid-out line.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalLine {
    /// Its runs, left to right.
    pub segments: Vec<TerminalSegment>,
    /// The columns the line takes.
    pub columns: u32,
    /// The cell advance, in em.
    pub cell_em: f32,
}

/// Why two atlases cannot make a terminal.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TerminalError {
    /// An atlas carries no layout tables, or not for Regular.
    NoLayout(TerminalSource),
    /// The atlases disagree on something they must share.
    Mismatch {
        /// What: `"units per em"`, `"cell advance"` or `"baseline"`.
        what: &'static str,
        /// The text atlas's value.
        text: f32,
        /// The symbol page's value.
        symbols: f32,
    },
}

impl core::fmt::Display for TerminalError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TerminalError::NoLayout(source) => {
                write!(f, "the {source:?} atlas carries no Regular layout tables")
            }
            TerminalError::Mismatch {
                what,
                text,
                symbols,
            } => write!(
                f,
                "the atlases disagree on {what}: {text} in the text atlas, {symbols} on the \
                 symbol page"
            ),
        }
    }
}

impl std::error::Error for TerminalError {}

/// **A shaper for terminal lines** over a text atlas and a symbol page.
#[derive(Debug, Clone)]
pub struct TerminalShaper {
    /// One per style the text atlas carries, Regular first.
    text: Vec<AtlasShaper>,
    symbols: AtlasShaper,
    /// The cell advance in font units.
    cell: i32,
    upem: u16,
}

impl TerminalShaper {
    /// A shaper over `text` (Noto Sans Mono, `NOTO_SANS_MONO_ATLAS`) and
    /// `symbols` (the symbol page, `TERMINAL_SYMBOLS_ATLAS`). Refuses atlases
    /// without layout tables, and atlases that disagree on units per em, the
    /// cell advance (the text face's `0`, against the symbol page's `─`) or
    /// the baseline.
    pub fn new(text: &FontAtlas, symbols: &FontAtlas) -> Result<Self, TerminalError> {
        let regular = AtlasShaper::new(text, GlyphStyle::Regular)
            .map_err(|_| TerminalError::NoLayout(TerminalSource::Text))?;
        let page = AtlasShaper::new(symbols, GlyphStyle::Regular)
            .map_err(|_| TerminalError::NoLayout(TerminalSource::Symbols))?;
        let mut shapers = vec![regular];
        for &style in GlyphStyle::ALL {
            if style != GlyphStyle::Regular
                && let Ok(s) = AtlasShaper::new(text, style)
            {
                shapers.push(s);
            }
        }
        let (tu, su) = (shapers[0].shape("0"), page.shape("\u{2500}"));
        if tu.units_per_em != su.units_per_em {
            return Err(TerminalError::Mismatch {
                what: "units per em",
                text: tu.units_per_em as f32,
                symbols: su.units_per_em as f32,
            });
        }
        let (tc, sc) = (tu.total_advance, su.total_advance);
        if tc <= 0 || tc != sc {
            return Err(TerminalError::Mismatch {
                what: "cell advance",
                text: tc as f32,
                symbols: sc as f32,
            });
        }
        let (tb, sb) = (
            text.text_baseline_frac().unwrap_or(f32::NAN),
            symbols.text_baseline_frac().unwrap_or(f32::NAN),
        );
        if (tb - sb).abs() > 1e-6 || tb.is_nan() {
            return Err(TerminalError::Mismatch {
                what: "baseline",
                text: tb,
                symbols: sb,
            });
        }
        Ok(Self {
            text: shapers,
            symbols: page,
            cell: tc,
            upem: tu.units_per_em,
        })
    }

    /// The cell advance in em (0.6 for Noto Sans Mono).
    pub fn cell_em(&self) -> f32 {
        self.cell as f32 / self.upem as f32
    }

    /// The line pitch at which box drawing joins: [`LINE_BOX_RATIO`] em.
    pub fn line_pitch_em(&self) -> f32 {
        LINE_BOX_RATIO
    }

    /// The text shaper for `style`: the style itself when the text atlas
    /// carries it, otherwise without the slant (Italic as Regular,
    /// BoldItalic as Bold), otherwise Regular.
    fn text_shaper(&self, style: GlyphStyle) -> &AtlasShaper {
        let find = |s: GlyphStyle| self.text.iter().find(|t| t.style() == s);
        let upright = match style {
            GlyphStyle::BoldItalic => GlyphStyle::Bold,
            _ => GlyphStyle::Regular,
        };
        find(style)
            .or_else(|| find(upright))
            .unwrap_or(&self.text[0])
    }

    /// **Lay `text` out in `style`** on the cell grid.
    pub fn line(&self, text: &str, style: GlyphStyle) -> TerminalLine {
        let shaper = self.text_shaper(style);
        // A narrow curated emoji followed by U+FE0F becomes its wide form;
        // the selector stays, with zero width.
        let chars: Vec<char> = text.chars().collect();
        let mut prepared = String::with_capacity(text.len());
        for (i, &ch) in chars.iter().enumerate() {
            let wide = TERMINAL_WIDE_FORM
                .iter()
                .find(|(base, _)| *base == ch)
                .filter(|_| chars.get(i + 1) == Some(&VS16))
                .map(|&(_, form)| form)
                .filter(|&form| self.symbols.glyph_id_for_char(form).is_some());
            prepared.push(wide.unwrap_or(ch));
        }

        let mapped = |s: &AtlasShaper, ch: char| s.glyph_id_for_char(ch).is_some_and(|g| g != 0);
        let mut runs: Vec<(TerminalSource, String)> = Vec::new();
        for ch in prepared.chars() {
            let source = if is_default_ignorable(ch) {
                runs.last().map_or(TerminalSource::Text, |r| r.0)
            } else if mapped(shaper, ch) {
                TerminalSource::Text
            } else if mapped(&self.symbols, ch) {
                TerminalSource::Symbols
            } else {
                TerminalSource::Text
            };
            match runs.last_mut() {
                Some((s, run)) if *s == source => run.push(ch),
                _ => runs.push((source, ch.to_string())),
            }
        }

        let mut segments = Vec::with_capacity(runs.len());
        let mut column = 0u32;
        for (source, s) in runs {
            let mut run = match source {
                TerminalSource::Text => shaper.shape(&s),
                TerminalSource::Symbols => self.symbols.shape(&s),
            };
            let mut cells = 0u32;
            for g in &mut run.glyphs {
                let n = ((g.x_advance as f32 / self.cell as f32).round() as i32).max(0);
                g.x_advance = n * self.cell;
                cells += n as u32;
            }
            run.total_advance = cells as i32 * self.cell;
            segments.push(TerminalSegment {
                source,
                column,
                run,
            });
            column += cells;
        }
        TerminalLine {
            segments,
            columns: column,
            cell_em: self.cell_em(),
        }
    }
}
