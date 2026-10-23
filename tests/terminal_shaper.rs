//! **The terminal shaper and its draw call**
//! (`docs/design/20261023_01_TERMINAL_FONTS_PLAN.md`, step 5, with the
//! 02:32 EDT amendment's wide forms).

#![cfg(all(
    feature = "font-ofl-noto-sans-mono",
    feature = "font-ofl-noto-terminal-symbols"
))]

use libmsdf::font::{TERMINAL_WIDE_FORM, TerminalError, TerminalLine, TerminalSource};
use libmsdf::{AtlasSet, DrawList, FontAtlas, GlyphStyle, TerminalShaper};

fn atlases() -> (FontAtlas, FontAtlas) {
    (
        FontAtlas::from_bytes(libmsdf::NOTO_SANS_MONO_ATLAS).unwrap(),
        FontAtlas::from_bytes(libmsdf::TERMINAL_SYMBOLS_ATLAS).unwrap(),
    )
}

fn shaper() -> TerminalShaper {
    let (text, symbols) = atlases();
    TerminalShaper::new(&text, &symbols).expect("the pair agrees")
}

/// `(source, column, columns)` per segment.
fn layout(line: &TerminalLine) -> Vec<(TerminalSource, u32, u32)> {
    line.segments
        .iter()
        .map(|s| (s.source, s.column, (s.run.total_advance / 600) as u32))
        .collect()
}

/// The column of the first glyph of the last segment.
fn last_column(line: &TerminalLine) -> u32 {
    line.segments.last().unwrap().column
}

#[test]
fn the_cell_and_the_pitch() {
    let t = shaper();
    assert_eq!(t.cell_em(), 0.6);
    assert_eq!(t.line_pitch_em(), libmsdf::drawlist::LINE_BOX_RATIO);
}

#[test]
fn a_line_takes_each_character_from_its_atlas() {
    use TerminalSource::{Symbols, Text};
    let line = shaper().line("\u{2502} \u{280B} a\u{2705}b", GlyphStyle::Regular);
    assert_eq!(
        layout(&line),
        [
            (Symbols, 0, 1),
            (Text, 1, 1),
            (Symbols, 2, 1),
            (Text, 3, 2),
            (Symbols, 5, 2),
            (Text, 7, 1)
        ]
    );
    assert_eq!(line.columns, 8);
    for seg in &line.segments {
        assert_eq!(seg.run.notdef_count(), 0, "{seg:?}");
        assert!(seg.run.glyphs.iter().all(|g| g.x_advance % 600 == 0));
    }
}

#[test]
fn a_narrow_emoji_is_two_cells_only_with_vs16() {
    let t = shaper();
    for (s, x_at) in [
        ("\u{26A0}x", 1),
        ("\u{26A0}\u{FE0E}x", 1),
        ("\u{26A0}\u{FE0F}x", 2),
        ("\u{27A1}x", 1),
        ("\u{27A1}\u{FE0F}x", 2),
        ("\u{2705}x", 2),
        ("\u{2705}\u{FE0E}x", 2),
        ("\u{2705}\u{FE0F}x", 2),
        ("\u{2714}\u{FE0F}x", 1),
    ] {
        let line = t.line(s, GlyphStyle::Regular);
        assert_eq!(last_column(&line), x_at, "{s:?}: {:?}", layout(&line));
        assert_eq!(line.segments.last().unwrap().source, TerminalSource::Text);
        assert_eq!(line.columns, x_at + 1, "{s:?}");
    }
}

/// The wide-form table is the symbol face's: each form is mapped, two cells
/// wide, beside its one-cell base.
#[test]
fn the_wide_forms_are_the_faces() {
    let (_, symbols) = atlases();
    let page = libmsdf::AtlasShaper::new(&symbols, GlyphStyle::Regular).unwrap();
    for &(base, form) in TERMINAL_WIDE_FORM {
        assert!(
            (libmsdf::font::TERMINAL_WIDE_FORMS.0..=libmsdf::font::TERMINAL_WIDE_FORMS.1)
                .contains(&form)
        );
        assert_eq!(page.shape(&base.to_string()).total_advance, 600, "{base:?}");
        assert_eq!(
            page.shape(&form.to_string()).total_advance,
            1200,
            "{form:?}"
        );
        assert_ne!(page.glyph_id_for_char(base), page.glyph_id_for_char(form));
    }
}

/// A spinner's frames keep everything after them in place.
#[test]
fn braille_spinner_frames_do_not_move_the_text() {
    let t = shaper();
    let mut layouts = Vec::new();
    for frame in
        "\u{280B}\u{2819}\u{2839}\u{2838}\u{283C}\u{2834}\u{2826}\u{2827}\u{2807}\u{280F}".chars()
    {
        let line = t.line(&format!("{frame} Working"), GlyphStyle::Regular);
        let text = line.segments.last().unwrap();
        layouts.push((layout(&line), text.run.glyphs[1].x_offset, line.columns));
    }
    assert!(layouts.windows(2).all(|w| w[0] == w[1]), "{layouts:?}");
}

/// Italic draws upright as Regular and BoldItalic as Bold; Bold is its own.
#[test]
fn styles_fall_back_without_slanting() {
    let t = shaper();
    let ids = |style| {
        let line = t.line("Mono 50", style);
        line.segments[0]
            .run
            .glyphs
            .iter()
            .map(|g| g.glyph_id)
            .collect::<Vec<_>>()
    };
    let (regular, bold) = (ids(GlyphStyle::Regular), ids(GlyphStyle::Bold));
    assert_ne!(regular, bold);
    assert_eq!(ids(GlyphStyle::Italic), regular);
    assert_eq!(ids(GlyphStyle::BoldItalic), bold);
    // Symbols are style-invariant.
    let sym = |style| t.line("\u{2500}", style).segments[0].run.glyphs[0].glyph_id;
    assert_eq!(sym(GlyphStyle::Bold), sym(GlyphStyle::Regular));
}

#[test]
fn atlases_that_disagree_are_refused() {
    let (text, symbols) = atlases();
    // A different upem: the Roboto fixture (2048) as the page.
    let roboto = FontAtlas::from_bytes(include_bytes!("fixtures/roboto-ascii-48.atlas")).unwrap();
    match TerminalShaper::new(&text, &roboto) {
        Err(TerminalError::Mismatch { what, .. }) => assert_eq!(what, "units per em"),
        other => panic!("expected a units-per-em mismatch, got {other:?}"),
    }
    // A different baseline.
    let mut moved = symbols.clone();
    for set in &mut moved.sets {
        set.baseline_frac += 0.01;
    }
    match TerminalShaper::new(&text, &moved) {
        Err(TerminalError::Mismatch { what, .. }) => assert_eq!(what, "baseline"),
        other => panic!("expected a baseline mismatch, got {other:?}"),
    }
    // No layout tables.
    let mut bare = symbols;
    bare.layout = None;
    assert_eq!(
        TerminalShaper::new(&text, &bare).unwrap_err(),
        TerminalError::NoLayout(TerminalSource::Symbols)
    );
}

/// The draw call pushes each segment from its atlas at its column.
#[test]
fn a_line_draws_each_segment_from_its_atlas() {
    let (text, symbols) = atlases();
    let t = TerminalShaper::new(&text, &symbols).unwrap();
    let mut set = AtlasSet::new();
    let text_id = set.register(text).unwrap();
    let page_id = set.register(symbols).unwrap();
    assert_eq!(
        set.extent(),
        [800, 2000],
        "same-width atlases need no padding"
    );
    let line = t.line("\u{256D}\u{2500} ok \u{2500}\u{256E}", GlyphStyle::Regular);
    let mut list = DrawList::new();
    let width = list.push_terminal_line(
        &line,
        set.atlas(text_id).unwrap(),
        set.atlas(page_id).unwrap(),
        [10.0, 20.0],
        20.0,
        6.0,
        [1.0; 4],
    );
    assert_eq!(width, line.columns as f32 * 12.0);
    let ids: Vec<_> = (0..line.segments.len())
        .map(|i| list.instance_atlas(i))
        .collect();
    let want: Vec<_> = line
        .segments
        .iter()
        .map(|s| match s.source {
            TerminalSource::Text => text_id,
            TerminalSource::Symbols => page_id,
        })
        .collect();
    assert_eq!(ids, want);
}
