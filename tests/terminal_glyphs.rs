//! **Every glyph Claude Code's interface prints draws from the terminal
//! atlases** (`docs/design/20261023_02_PER_ATLAS_CELL_BOX_PLAN.md`, 13:23 EDT
//! amendment, section 5).
//!
//! The list is Claude Code 2.1.309's interface glyph table and both of its
//! spinners, read from that release: the starburst spinner `· ✢ ✳ ✶ ✻ ✽`
//! (with `*` for `✳` on some terminals), the Braille spinner, the box
//! corners and tree branches, the status and tool-call marks, the thinking
//! animation `∴ ∷ ∵`, and the rest. A new release that prints a glyph not
//! listed here needs a coverage change, not just a longer list.

#![cfg(all(
    feature = "font-ofl-noto-sans-mono",
    feature = "font-ofl-noto-terminal-symbols"
))]

use libmsdf::font::TerminalSource;
use libmsdf::{FontAtlas, GlyphStyle, TerminalShaper};

/// Claude Code 2.1.309's interface glyphs and spinner frames.
const CLAUDE_CODE_2_1_309: &[char] = &[
    '*', '·', '×', '–', '—', '•', '※', '←', '↑', '→', '↓', '↯', '↳', '↻', '∙', '∴', '∵', '∷', '⌕',
    '⎿', '⏎', '⏵', '⏸', '⏺', '⑂', '─', '│', '┃', '└', '├', '┬', '┴', '╭', '╮', '╯', '╰', '╱', '█',
    '▎', '▸', '◆', '◇', '◈', '◉', '○', '◌', '◎', '●', '◐', '⚠', '✔', '✕', '✢', '✦', '✳', '✶', '✻',
    '✽', '⠇', '⠋', '⠏', '⠙', '⠦', '⠧', '⠴', '⠸', '⠹', '⠼', '⠿', '⧉',
];

/// The starburst spinner's frames, `*` included.
const STARBURST: &[char] = &['·', '✢', '✳', '✶', '✻', '✽', '*'];

fn shaper() -> TerminalShaper {
    let text = FontAtlas::from_bytes(libmsdf::NOTO_SANS_MONO_ATLAS).unwrap();
    let page = FontAtlas::from_bytes(libmsdf::TERMINAL_SYMBOLS_ATLAS).unwrap();
    TerminalShaper::new(&text, &page).unwrap()
}

#[test]
fn the_list_is_the_release_s_seventy() {
    assert_eq!(CLAUDE_CODE_2_1_309.len(), 70);
    let mut sorted = CLAUDE_CODE_2_1_309.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 70, "no glyph is listed twice");
}

/// Each glyph shapes with no placeholder, from the text atlas or the symbol
/// page.
#[test]
fn every_claude_code_glyph_draws_from_an_atlas() {
    let t = shaper();
    let mut missing = Vec::new();
    for &ch in CLAUDE_CODE_2_1_309 {
        let line = t.line(&ch.to_string(), GlyphStyle::Regular);
        let notdef: usize = line.segments.iter().map(|s| s.run.notdef_count()).sum();
        if notdef > 0 || line.columns == 0 {
            missing.push(format!("U+{:04X} {ch}", ch as u32));
        }
    }
    assert!(missing.is_empty(), "drawn as the placeholder: {missing:?}");
}

/// The starburst frames are one cell each, so the spinner never moves the
/// text after it; `✨` is two, like the other curated emoji; `✔︎` with its
/// text-presentation selector stays one.
#[test]
fn spinner_frames_and_sparkle_widths() {
    let t = shaper();
    for &ch in STARBURST {
        let line = t.line(&format!("{ch} Working"), GlyphStyle::Regular);
        assert_eq!(line.columns, 9, "{ch}");
    }
    let sparkle = t.line("\u{2728}", GlyphStyle::Regular);
    assert_eq!(sparkle.columns, 2);
    assert_eq!(sparkle.segments[0].source, TerminalSource::Symbols);
    assert_eq!(sparkle.segments[0].run.notdef_count(), 0);
    let check = t.line("\u{2714}\u{FE0E}", GlyphStyle::Regular);
    assert_eq!(check.columns, 1);
}

/// `Ș ș Ț ț` are in the text atlas, Regular and Bold.
#[test]
fn romanian_comma_below_is_in_the_text_atlas() {
    let t = shaper();
    for style in [GlyphStyle::Regular, GlyphStyle::Bold] {
        let line = t.line("\u{218}\u{219}\u{21A}\u{21B}", style);
        assert_eq!(line.segments.len(), 1, "{style:?}");
        assert_eq!(line.segments[0].source, TerminalSource::Text);
        assert_eq!(line.segments[0].run.notdef_count(), 0, "{style:?}");
        assert_eq!(line.columns, 4);
    }
}

/// **The coding agents' interface glyphs**
/// (`docs/design/20261028_01_CODING_AGENT_GLYPHS_PLAN.md`, sections 3.1 and
/// 5), each with its width in cells: the 77 codepoints no bundled atlas
/// carried before 0.6.2, from a survey of Claude Code 2.1.312, Codex
/// 0.162.1, Gemini CLI 0.38.2 and Antigravity 1.4.7, and flankr's own ⊙ ⦿.
///
/// By the plan's rows:
/// 1. interface marks: Codex `⋮ ⓘ ‖`, Antigravity `⋯ ⟳ ⊠ ⛁`, Gemini CLI
///    `⊷ ⌐`, Claude Code `ℹ ≡`;
/// 2. Claude Code's key hints `⌦ ⇞ ⇟ ␣`;
/// 3. Claude Code's control pictures U+2400-241F, `␡` and `␤`;
/// 4. Claude Code's `figures` fallbacks `☰ ⌂ ⬢ ♥ ♪ ☺ Ⓘ ⓞ ⓧ`;
/// 5. Gemini CLI's text-presentation `✋ ⏲ ⛔`;
/// 6. Gemini CLI's emoji;
/// 7. flankr's `⊙ ⦿` (the subagent dropdown and its radio dots).
const CODING_AGENTS_2026_10_28: &[(char, u32)] = &[
    // 1. Interface marks.
    ('\u{22EE}', 1),
    ('\u{22EF}', 1),
    ('\u{22B7}', 1),
    ('\u{27F3}', 1),
    ('\u{22A0}', 1),
    ('\u{26C1}', 1),
    ('\u{2139}', 1),
    ('\u{24D8}', 1),
    ('\u{2261}', 1),
    ('\u{2310}', 1),
    ('\u{2016}', 1),
    // 2. Key hints.
    ('\u{2326}', 1),
    ('\u{21DE}', 1),
    ('\u{21DF}', 1),
    ('\u{2423}', 1),
    // 3. Control pictures.
    ('\u{2400}', 1),
    ('\u{2401}', 1),
    ('\u{2402}', 1),
    ('\u{2403}', 1),
    ('\u{2404}', 1),
    ('\u{2405}', 1),
    ('\u{2406}', 1),
    ('\u{2407}', 1),
    ('\u{2408}', 1),
    ('\u{2409}', 1),
    ('\u{240A}', 1),
    ('\u{240B}', 1),
    ('\u{240C}', 1),
    ('\u{240D}', 1),
    ('\u{240E}', 1),
    ('\u{240F}', 1),
    ('\u{2410}', 1),
    ('\u{2411}', 1),
    ('\u{2412}', 1),
    ('\u{2413}', 1),
    ('\u{2414}', 1),
    ('\u{2415}', 1),
    ('\u{2416}', 1),
    ('\u{2417}', 1),
    ('\u{2418}', 1),
    ('\u{2419}', 1),
    ('\u{241A}', 1),
    ('\u{241B}', 1),
    ('\u{241C}', 1),
    ('\u{241D}', 1),
    ('\u{241E}', 1),
    ('\u{241F}', 1),
    ('\u{2421}', 1),
    ('\u{2424}', 1),
    // 4. Claude Code's `figures` fallbacks; ☰ is two cells (Unicode 16).
    ('\u{2630}', 2),
    ('\u{2302}', 1),
    ('\u{2B22}', 1),
    ('\u{2665}', 1),
    ('\u{266A}', 1),
    ('\u{263A}', 1),
    ('\u{24BE}', 1),
    ('\u{24DE}', 1),
    ('\u{24E7}', 1),
    // 5. Text presentation; ✋ and ⛔ are East Asian Width W.
    ('\u{270B}', 2),
    ('\u{23F2}', 1),
    ('\u{26D4}', 2),
    // 6. Gemini CLI's emoji; 🛠 is narrow (two only with U+FE0F).
    ('\u{1F7E2}', 2),
    ('\u{1F7E1}', 2),
    ('\u{1F534}', 2),
    ('\u{1F504}', 2),
    ('\u{1F4AD}', 2),
    ('\u{1F9E0}', 2),
    ('\u{1F6E0}', 1),
    ('\u{1F9D1}', 2),
    ('\u{1F4BB}', 2),
    ('\u{1F4CE}', 2),
    ('\u{1F6A7}', 2),
    ('\u{1F514}', 2),
    ('\u{1F4C2}', 2),
    ('\u{1F510}', 2),
    // 7. flankr's empty and filled circles.
    ('\u{2299}', 1),
    ('\u{29BF}', 1),
];

#[test]
fn the_list_is_the_survey_s_seventy_seven() {
    assert_eq!(CODING_AGENTS_2026_10_28.len(), 77);
    let mut sorted: Vec<char> = CODING_AGENTS_2026_10_28.iter().map(|&(c, _)| c).collect();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 77, "no glyph is listed twice");
}

/// Each codepoint, alone, is one segment from the symbol page, not the
/// placeholder, and takes its listed columns.
#[test]
fn every_coding_agent_glyph_draws_from_the_page() {
    let t = shaper();
    let mut wrong = Vec::new();
    for &(ch, cells) in CODING_AGENTS_2026_10_28 {
        let line = t.line(&ch.to_string(), GlyphStyle::Regular);
        let ok = line.segments.len() == 1
            && line.segments[0].source == TerminalSource::Symbols
            && line.segments[0].run.glyphs.len() == 1
            && line.segments[0].run.notdef_count() == 0
            && line.columns == cells;
        if !ok {
            wrong.push(format!(
                "U+{:04X} {ch}: {} segments, {} columns (want {cells})",
                ch as u32,
                line.segments.len(),
                line.columns
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// The column `x` starts at in `line`.
fn column_of_x(t: &TerminalShaper, line: &str) -> u32 {
    let line = t.line(line, GlyphStyle::Regular);
    let last = line.segments.last().expect("a segment");
    assert_eq!(
        last.source,
        TerminalSource::Text,
        "x is from the text atlas"
    );
    assert_eq!(last.run.glyphs.len(), 1);
    assert_eq!(line.columns, last.column + 1);
    last.column
}

/// The variation selectors and the two-cell symbols (coding-agent glyphs
/// plan, sections 3.3 and 5): 🛠 is one cell, two with U+FE0F; ℹ and ♥
/// stay one cell whatever follows (the terminal fonts plan's 02:32 EDT
/// rule); ✋ ⛔ ☰ are two.
#[test]
fn selectors_and_widths() {
    let t = shaper();
    assert_eq!(column_of_x(&t, "\u{1F6E0}x"), 1);
    assert_eq!(column_of_x(&t, "\u{1F6E0}\u{FE0E}x"), 1);
    assert_eq!(column_of_x(&t, "\u{1F6E0}\u{FE0F}x"), 2);
    assert_eq!(column_of_x(&t, "\u{2139}\u{FE0F}x"), 1);
    assert_eq!(column_of_x(&t, "\u{2665}\u{FE0F}x"), 1);
    for ch in ['\u{270B}', '\u{26D4}', '\u{2630}'] {
        assert_eq!(column_of_x(&t, &format!("{ch}x")), 2, "{ch}");
    }
    let circles = t.line("\u{2299} \u{29BF} \u{22EE}", GlyphStyle::Regular);
    assert_eq!(circles.columns, 5);
    let notdef: usize = circles.segments.iter().map(|s| s.run.notdef_count()).sum();
    assert_eq!(notdef, 0);
}
