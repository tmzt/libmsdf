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
