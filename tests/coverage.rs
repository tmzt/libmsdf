//! What the shipped faces can draw, checked against the REAL BAKED ATLAS.
//!
//! **That is the point of this file.** Every other text test in the repo builds
//! its atlas with `FontAtlas::empty`, which has no cells at all — so it can
//! assert that a glyph id came out of the shaper, and it cannot notice that
//! nothing is baked for it. That blind spot is exactly where the invisible-gap
//! bug lived: coverage is a fact about the FACE and the BAKE together, and only
//! one of the two was ever being read.
//!
//! So these read `fixtures/roboto-ascii-48.atlas`, the same bytes
//! `src/assets/` and `crates/libhbui/assets/` ship, and ask of every codepoint:
//! does it shape, does it have a cell, and does that cell have ink in it.

use libmsdf::font::{
    CellKey,
    FontAtlas, FontAtlasBuilder, GlyphSet, HIGHBAY_ICONS_BLOCK, MARKERS, MSYMBOLS_ICONS,
    PRIVATE_USE, ROBOTO_ASCII_MSYMBOLS, ROBOTO_REGULAR_ASCII, TEXT_RANGES, TextShaper,
    msymbols_codepoint,
};
use libmsdf::{ATLAS_COLS, ATLAS_ROWS, DrawList, SdfKind, atlas_capacity};

const ATLAS_FIXTURE: &[u8] = include_bytes!("fixtures/roboto-ascii-48.atlas");
const PX_RANGE: f32 = 6.0;

fn atlas() -> FontAtlas {
    FontAtlas::from_bytes(ATLAS_FIXTURE).expect("fixture atlas parses")
}

fn shaper() -> TextShaper {
    TextShaper::new(ROBOTO_REGULAR_ASCII.to_vec()).expect("the shipped face parses")
}

/// The MSDF median at one texel — above 0.5 is inside the glyph.
fn median_at(a: &FontAtlas, x: u32, y: u32) -> f32 {
    let o = ((y * a.width + x) * a.channels) as usize;
    let (r, g, b) = (
        a.pixel_data[o] as f32 / 255.0,
        a.pixel_data[o + 1] as f32 / 255.0,
        a.pixel_data[o + 2] as f32 / 255.0,
    );
    r.min(g).max(r.max(g).min(b))
}

/// Does this glyph's atlas cell contain any ink?
fn cell_has_ink(a: &FontAtlas, glyph_id: u16) -> bool {
    let e = a.get_glyph(glyph_id).expect("glyph has a cell");
    (0..e.atlas_h as u32)
        .flat_map(|dy| (0..e.atlas_w as u32).map(move |dx| (dx, dy)))
        .any(|(dx, dy)| median_at(a, e.atlas_x as u32 + dx, e.atlas_y as u32 + dy) > 0.5)
}

// ── coverage ────────────────────────────────────────────────────────────

/// **Every declared codepoint shapes to a real glyph WITH A BAKED CELL.**
///
/// The whole contract in one loop, and it reads both halves: `TEXT_RANGES` says
/// what is covered, the face has to agree, and the atlas has to have baked it.
#[test]
fn every_declared_codepoint_has_a_glyph_and_a_cell() {
    let (shaper, atlas) = (shaper(), atlas());
    let mut checked = 0;
    for &(lo, hi) in TEXT_RANGES {
        for cp in (lo as u32)..=(hi as u32) {
            let ch = char::from_u32(cp).expect("range is scalar values");
            let run = shaper.shape(ch.encode_utf8(&mut [0u8; 4]));
            assert_eq!(run.notdef_count(), 0, "U+{cp:04X} {ch:?} is not covered");
            let gid = run.glyphs[0].glyph_id;
            assert!(
                atlas.get_glyph(gid).is_some(),
                "U+{cp:04X} {ch:?} shapes to glyph {gid}, which the bake has no cell for \
                 — it would draw the placeholder box instead of itself"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 95 + 96, "printable ASCII plus Latin-1 Supplement");
}

/// The letters actually have INK. A cell can exist and be blank — that is what
/// `space` is — so "has a cell" alone would pass on an atlas of empty squares.
#[test]
fn latin1_letters_are_drawn_not_blank() {
    let (shaper, atlas) = (shaper(), atlas());
    for ch in ['\u{e9}', '\u{fc}', '\u{f1}', '\u{e5}', '\u{df}', '\u{c7}', '\u{d8}'] {
        let gid = shaper.shape(ch.encode_utf8(&mut [0u8; 4])).glyphs[0].glyph_id;
        assert!(cell_has_ink(&atlas, gid), "{ch:?} bakes to a blank cell");
    }
    // Vacuity pin: the two space characters in the ranges are blank, so `ink`
    // is measuring something rather than answering yes to everything.
    for ch in [' ', '\u{a0}'] {
        let gid = shaper.shape(ch.encode_utf8(&mut [0u8; 4])).glyphs[0].glyph_id;
        assert!(!cell_has_ink(&atlas, gid), "{ch:?} should bake blank");
    }
}

/// A name a user might actually have, end to end.
#[test]
fn a_users_name_survives_the_whole_path() {
    let (shaper, atlas) = (shaper(), atlas());
    let run = shaper.shape("Jos\u{e9} \u{c5}ngstr\u{f6}m-M\u{fc}ller");
    assert_eq!(run.notdef_count(), 0);
    for g in &run.glyphs {
        assert!(atlas.get_glyph(g.glyph_id).is_some(), "glyph {} unbaked", g.glyph_id);
    }
}

// ── the placeholder ─────────────────────────────────────────────────────

/// **Glyph 0 is baked, and it is a HOLLOW BOX.**
///
/// Read off the baked cell rather than the font: the failure this replaces was
/// a glyph that existed in the face and had no cell, so asking the face proves
/// nothing. Ink on all four edges of the ink bounds and none in the middle is
/// what distinguishes the box from a blank cell, from a solid block, and from
/// Roboto's own crossed box (whose centre is inked where the X meets).
#[test]
fn glyph_zero_is_a_hollow_box() {
    let atlas = atlas();
    let e = atlas.get_glyph(0).expect("the bake queues glyph 0 explicitly");
    let (ox, oy, w, h) = (
        e.atlas_x as u32,
        e.atlas_y as u32,
        e.atlas_w as u32,
        e.atlas_h as u32,
    );
    let inside = |x: u32, y: u32| median_at(&atlas, ox + x, oy + y) > 0.5;

    let ink: Vec<(u32, u32)> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| inside(x, y))
        .collect();
    assert!(!ink.is_empty(), "glyph 0 baked blank — a missing character would draw NOTHING");
    let (x0, x1) = (
        ink.iter().map(|p| p.0).min().unwrap(),
        ink.iter().map(|p| p.0).max().unwrap(),
    );
    let (y0, y1) = (
        ink.iter().map(|p| p.1).min().unwrap(),
        ink.iter().map(|p| p.1).max().unwrap(),
    );
    let (cx, cy) = ((x0 + x1) / 2, (y0 + y1) / 2);

    assert!(inside(cx, y0), "no ink on the top edge");
    assert!(inside(cx, y1), "no ink on the bottom edge");
    assert!(inside(x0, cy), "no ink on the left edge");
    assert!(inside(x1, cy), "no ink on the right edge");
    assert!(
        !inside(cx, cy),
        "the middle of the box is inked — that is a filled block or a crossed box, \
         not the hollow frame a placeholder has to be to read at 12px"
    );
}

/// **An uncovered character lands on that cell**, which is the whole reason it
/// exists. Shaper and atlas checked together: glyph 0 out, glyph 0's cell back.
#[test]
fn uncovered_characters_resolve_to_the_placeholder_cell() {
    let (shaper, atlas) = (shaper(), atlas());
    // A curly quote and an ellipsis (the authored-source case the old panic was
    // written for), an em-dash, a CJK ideograph and an emoji (arbitrary runtime
    // data, which is the case that broke it).
    for ch in ['\u{201c}', '\u{2026}', '\u{2014}', '\u{597d}', '\u{1f389}'] {
        let run = shaper.shape(ch.encode_utf8(&mut [0u8; 4]));
        assert_eq!(run.notdef_count(), 1, "{ch:?} unexpectedly covered");
        assert_eq!(run.glyphs[0].glyph_id, 0);
        assert!(cell_has_ink(&atlas, 0));
    }
}

/// ...and the DRAW LIST agrees. The two above could both hold while the emitter
/// still pointed the run at table index 0, which for every shipped atlas used to
/// be the space — the invisible gap by a second route.
///
/// # The vacuity pin moved, and why it had to
///
/// This used to assert `notdef_idx != 0` first, so that matching it meant
/// something. That is no longer possible: the glyph table is ordered by GLYPH
/// ID and glyph 0 is the lowest there is, so the placeholder is table index 0
/// in every atlas that bakes it. Two things follow. The emitter's last-resort
/// arm (`None => (0, ..)`, for an atlas with no glyph 0 at all) now lands on
/// the placeholder wherever there is one, which is strictly better than
/// "whatever was packed first". And a stuck zero is no longer distinguishable
/// from the right answer by looking at this glyph alone — so the run carries a
/// SECOND glyph, and the pin is that the second one comes back as itself.
#[test]
fn the_emitter_points_an_uncovered_run_at_the_placeholder() {
    let (shaper, atlas) = (shaper(), atlas());
    let notdef_idx = atlas.glyph_table_index(0).expect("glyph 0 is in the table") as u32;

    let mut list = DrawList::new();
    // An em-dash (uncovered) followed by an 'A' (covered).
    list.push_shaped_text(&shaper.shape("\u{2014}A"), &atlas, [0.0, 0.0], 16.0, PX_RANGE, [1.0; 4]);
    let frame = list.lower();
    let SdfKind::MsdfText { char_start, char_count, .. } = list.instances[0].kind else {
        panic!("expected a text instance");
    };
    assert_eq!(char_count, 2);
    let packed = |i: u32| frame.char_buffer[(char_start + i) as usize] >> 16;
    assert_eq!(packed(0), notdef_idx, "the em-dash was emitted as some other cell");

    // Vacuity pin: the buffer is carrying real indices, not zeros. 'A' is a
    // covered glyph, so it must come back as its OWN cell and not as the
    // placeholder's.
    let a_idx = atlas
        .glyph_table_index(shaper.shape("A").glyphs[0].glyph_id)
        .expect("'A' is baked") as u32;
    assert_ne!(a_idx, notdef_idx, "vacuity: 'A' must not resolve to the placeholder");
    assert_eq!(packed(1), a_idx, "'A' was emitted as some other cell");
}

/// A glyph the FACE carries but the BAKE skipped also lands on the box, rather
/// than on whatever happened to be packed first.
#[test]
fn an_unbaked_glyph_falls_back_to_the_placeholder() {
    let atlas = atlas();
    let notdef_idx = atlas.glyph_table_index(0).expect("glyph 0 is in the table") as u32;
    let unbaked = (0..u16::MAX)
        .find(|&g| atlas.get_glyph(g).is_none())
        .expect("some glyph id is unbaked");

    let mut list = DrawList::new();
    let run = libmsdf::font::ShapedRun {
        glyphs: vec![libmsdf::font::ShapedGlyph {
            glyph_id: unbaked,
            x_advance: 1000,
            x_offset: 0,
            y_offset: 0,
            cluster: 0,
        }],
        total_advance: 1000,
        units_per_em: 2048,
    };
    list.push_shaped_text(&run, &atlas, [0.0, 0.0], 16.0, PX_RANGE, [1.0; 4]);
    let frame = list.lower();
    assert_eq!(frame.char_buffer[0] >> 16, notdef_idx);
}

// ── the icon path stays distinct (Rule 28) ──────────────────────────────

/// **A missing ICON must NOT get the box.** The two paths are kept apart by
/// WHERE the name is resolved: an unknown icon name never becomes a codepoint,
/// so it never reaches the shaper and cannot pick the placeholder up on the way
/// through. If `msymbols_codepoint` ever started answering `Some`, a typo'd
/// icon name would silently draw a box where an icon belongs.
#[test]
fn an_unknown_icon_name_never_becomes_a_codepoint() {
    for name in ["sned", "", "settings2", "chat_bubble", "\u{e5d2}"] {
        assert_eq!(msymbols_codepoint(name), None, "{name:?} must not resolve");
    }
    // Vacuity pin: the declared ones do resolve, so the assertion above is not
    // passing because everything answers `None`.
    for &(name, cp) in MSYMBOLS_ICONS {
        assert_eq!(msymbols_codepoint(name), Some(cp));
    }
}

/// The icon path's second gate: a manifest entry the LIVE FACE does not carry.
/// `covers` answers from the cmap, so glyph 0 gaining an outline changed
/// nothing here — an uncovered codepoint is still `false`, and the caller still
/// reports a missing asset instead of shaping a box.
#[test]
fn covers_still_says_no_for_an_uncovered_codepoint() {
    let icons = TextShaper::new(ROBOTO_ASCII_MSYMBOLS.to_vec()).expect("the merged face parses");
    for &(_, cp) in MSYMBOLS_ICONS {
        assert!(icons.covers(cp), "declared icon U+{:04X} is not in the face", cp as u32);
    }
    // Every kind of miss: an undeclared PUA codepoint, and ordinary text the
    // face cannot draw. Both answer `false` even though shaping either one
    // would now hand back a perfectly drawable box.
    for ch in ['\u{e000}', '\u{f8ff}', '\u{201c}', '\u{2026}'] {
        assert!(!icons.covers(ch), "U+{:04X} should not be covered", ch as u32);
        assert_eq!(icons.shape(ch.encode_utf8(&mut [0u8; 4])).notdef_count(), 1);
    }
}

// ── the private-use carveout ────────────────────────────────────────────

/// **Our glyphs and borrowed text cannot collide**, by construction.
///
/// The reason `TEXT_RANGES` can be widened without auditing the icon set:
/// everything this repo owns lives in `PRIVATE_USE`, which Unicode promises no
/// standard character will ever enter. A future widening that reached into it
/// would fail here rather than silently reassigning an icon's cell.
#[test]
fn text_coverage_never_reaches_the_private_use_carveout() {
    let (lo, hi) = PRIVATE_USE;
    for &(a, b) in TEXT_RANGES {
        assert!(b < lo || a > hi, "text range U+{:04X}..U+{:04X} overlaps the carveout", a as u32, b as u32);
    }
    for &(name, cp) in MSYMBOLS_ICONS {
        assert!(cp >= lo && cp <= hi, "icon {name:?} sits outside the carveout at U+{:04X}", cp as u32);
    }
}

/// The two bundled faces LAY TEXT OUT identically. `libhbui` ships one and
/// `highbay_ui` the other, and a run measured against either has to come out
/// the same width or the two surfaces disagree about where anything sits.
///
/// Advances, not glyph ids: each face is only ever used with its own atlas, and
/// the merged one carries nine icon glyphs ahead of the Latin-1 block, so the
/// same character is a different glyph NUMBER in each. Asserting the numbers
/// would be asserting the bake order, which is not the property that matters.
#[test]
fn both_bundled_faces_draw_text_the_same_way() {
    let plain = shaper();
    let merged = TextShaper::new(ROBOTO_ASCII_MSYMBOLS.to_vec()).expect("the merged face parses");
    for &(lo, hi) in TEXT_RANGES {
        for cp in (lo as u32)..=(hi as u32) {
            let ch = char::from_u32(cp).unwrap();
            let s = ch.encode_utf8(&mut [0u8; 4]).to_string();
            let (a, b) = (plain.shape(&s), merged.shape(&s));
            assert_eq!(a.total_advance, b.total_advance, "U+{cp:04X} advance differs");
            assert_eq!(a.notdef_count(), b.notdef_count(), "U+{cp:04X} coverage differs");
        }
    }
    // Kerned pairs and ligatures too, which is where a merge or a widening
    // would show up if it had disturbed GPOS/GSUB.
    for probe in ["AV", "To", "fi", "ffl", "Jos\u{e9}", "M\u{fc}ller"] {
        assert_eq!(
            plain.shape(probe).total_advance,
            merged.shape(probe).total_advance,
            "{probe:?} lays out differently in the two faces"
        );
    }
    // The kerning and ligature tables survived the merge and the widening.
    assert!(plain.shape("AV").glyphs[0].x_advance < plain.shape("A").glyphs[0].x_advance);
    assert_eq!(plain.shape("fi").glyphs.len(), 1, "the fi ligature is gone");
}

// ── control characters are not missing glyphs ───────────────────────────

/// **A control character draws NOTHING, and must not draw the box.**
///
/// The box says "this face has no glyph for this character". No face has one
/// for `'\n'` or `'\t'`: they are not missing, they have no visual form. Left
/// as `.notdef` they would put a box in the middle of every multi-line string
/// that reaches a run — tool output, a chat transcript, a pasted paragraph —
/// which is exactly the visible corruption the placeholder exists to prevent
/// elsewhere.
#[test]
fn control_characters_leave_no_glyph_and_no_width() {
    let shaper = shaper();
    for text in ["one\ntwo", "one\ttwo", "one\r\ntwo", "one\u{0}two"] {
        let run = shaper.shape(text);
        let plain = shaper.shape("onetwo");
        assert_eq!(run.notdef_count(), 0, "{text:?} drew a placeholder box");
        assert_eq!(run.glyphs.len(), plain.glyphs.len(), "{text:?} kept a glyph");
        assert_eq!(run.total_advance, plain.total_advance, "{text:?} kept width");
    }
    // Vacuity pin: a NON-control character the face cannot draw is still kept
    // and still gets the box, so the rule above is about control characters
    // rather than about `.notdef`.
    let em_dash = shaper.shape("one\u{2014}two");
    assert_eq!(em_dash.notdef_count(), 1);
    assert_eq!(em_dash.glyphs.len(), 7);
}

/// Clusters still address the ORIGINAL string after a control character is
/// dropped, which is what everything downstream maps glyphs back through
/// (`libhbui::draw`'s caret bounds, `place`'s breaker) — never by position.
#[test]
fn dropping_a_control_character_does_not_disturb_clusters() {
    let run = shaper().shape("ab\ncd");
    let clusters: Vec<u32> = run.glyphs.iter().map(|g| g.cluster).collect();
    assert_eq!(clusters, vec![0, 1, 3, 4]);
}

// ── the reserved grid ───────────────────────────────────────────────────

/// **The shipped atlas is the pinned size, with the headroom that was
/// reserved** — a fact about the BYTES, which is the only place the
/// reservation can be checked (`ATLAS_ROWS` is a number in a source file until
/// something bakes against it).
///
/// It fails in both directions on purpose. Too small means a re-bake fitted
/// the texture to its contents again, and every `v` in every frame moved with
/// it. Too full means the next glyph will not fit, which is a decision about
/// which GPUs we support (`ATLAS_ROWS`' own doc) rather than something to
/// discover from a bake error.
#[test]
fn the_shipped_atlas_is_pinned_and_has_the_reserved_headroom() {
    let a = atlas();
    let padded = 48 + 2;
    assert_eq!(
        (a.width, a.height),
        (ATLAS_COLS * padded, ATLAS_ROWS * padded),
        "the shipped atlas is not the pinned grid - was it baked before ATLAS_ROWS existed?"
    );
    // Vacuity pin: the pin is doing work, i.e. the contents really are smaller
    // than the reservation. If these were equal the assert above would pass for
    // the wrong reason.
    let rows_used = a.glyphs.len().div_ceil(ATLAS_COLS as usize);
    assert!(
        rows_used < ATLAS_ROWS as usize,
        "{} glyphs fill all {ATLAS_ROWS} reserved rows - nothing is pinned any more",
        a.glyphs.len()
    );
    assert!(
        a.glyphs.len() <= atlas_capacity(),
        "{} glyphs, capacity {}",
        a.glyphs.len(),
        atlas_capacity()
    );
    // The budget as written down in `ATLAS_ROWS`: ~200 text cells and a symbol
    // set Tim sized at ~16. A bake that has drifted far from that is the
    // moment to re-read the reasoning, not to raise the number.
    assert!(
        a.glyphs.len() < 260,
        "{} glyphs is well past the recorded budget - see ATLAS_ROWS",
        a.glyphs.len()
    );
}

// ── the declared cell order ─────────────────────────────────────────────

/// The queue the shipped atlas was baked from, for either bundled face.
fn shipped_queue(face: &[u8]) -> FontAtlasBuilder {
    let mut builder = FontAtlasBuilder::new(face.to_vec(), 48, PX_RANGE as f64);
    builder.add_shipped_coverage();
    builder
}

/// **The bytes on disk are in the order the code says they are.**
///
/// `GlyphSet` declares the layout and `FontAtlasBuilder::cell_order` computes
/// it; this reads the layout back off the ARTIFACT — cells in raster order,
/// which is the only order the texture actually has — and holds the two
/// against each other. Without it the declaration is a comment: the atlas is
/// baked by hand and committed, so a bake done from a different order would
/// ship, load, and render, with nothing to say the order had drifted.
///
/// It also pins the claim `cell_order` makes about geometry, that entry *n* is
/// the glyph in cell *n* of the grid. A caller reasoning about where a glyph
/// will land needs that to be true and cannot see the packer.
#[test]
fn the_shipped_atlas_is_laid_out_in_the_declared_cell_order() {
    let a = atlas();
    let declared = shipped_queue(ROBOTO_REGULAR_ASCII).cell_order();
    assert_eq!(declared.len(), a.glyphs.len(), "the bake queued a different number of glyphs");
    assert!(declared.len() > 200, "vacuity: the queue came back nearly empty");

    // The artifact's own order: cells left to right, top to bottom.
    let mut by_cell: Vec<&libmsdf::GlyphEntry> = a.glyphs.iter().collect();
    by_cell.sort_by_key(|e| (e.atlas_y, e.atlas_x));

    let padded = 48 + 2;
    for (cell, (&key, entry)) in declared.iter().zip(by_cell.iter()).enumerate() {
        let (set, glyph_id) = (key.set(), key.glyph_id());
        assert_eq!(
            entry.glyph_id, glyph_id,
            "cell {cell} holds glyph {} and the declared order puts {glyph_id} ({set:?}) there \
             - the committed atlas was baked from a different order than the one \
             `GlyphSet` states",
            entry.glyph_id
        );
        // ...and cell `n` is where `cell_order`'s doc says it is.
        assert_eq!(
            (entry.atlas_x as u32, entry.atlas_y as u32),
            (
                1 + (cell as u32 % ATLAS_COLS) * padded,
                1 + (cell as u32 / ATLAS_COLS) * padded
            ),
            "cell {cell} is not at grid position ({}, {})",
            cell % ATLAS_COLS as usize,
            cell / ATLAS_COLS as usize
        );
    }
    // The sets really are laid down in blocks, and in the declared sequence.
    let sets: Vec<GlyphSet> = declared.iter().map(|k| k.set()).collect();
    assert!(sets.windows(2).all(|w| w[0] <= w[1]), "the sets are interleaved");
    assert_eq!(sets[0], GlyphSet::Placeholder, "cell 0 is not the placeholder box");
}

/// **The two bundled faces bake to the SAME cells**, right up to the borrowed
/// icons the merged one adds on the end.
///
/// `libhbui` ships the merged atlas and `highbay_ui` the plain one, and this is
/// what makes a cell in one comparable to a cell in the other. It falls out of
/// `GlyphSet::BorrowedIcons` being last: the vendor half is the only part of
/// the merged face that is not in the plain one, so appending it disturbs
/// nothing. Under the old codepoint scan a borrowed `U+E0xx` sorted below our
/// `U+F8xx`, so the merged bake pushed Latin-1 and the placeholder 13 cells
/// along and the two atlases agreed about almost nothing.
///
/// Compared through CODEPOINTS, because the glyph ids differ between the faces
/// — that is the whole reason the set id, and not the glyph id, is what makes
/// the order ours.
#[test]
fn the_two_bundled_faces_share_a_cell_layout() {
    let (plain_face, merged_face) = (shaper(), TextShaper::new(ROBOTO_ASCII_MSYMBOLS.to_vec()).unwrap());
    let (plain, merged) = (
        shipped_queue(ROBOTO_REGULAR_ASCII).cell_order(),
        shipped_queue(ROBOTO_ASCII_MSYMBOLS).cell_order(),
    );
    let cell_of = |order: &[CellKey], gid: u16| {
        order.iter().position(|k| k.glyph_id() == gid)
    };

    let mut checked = 0;
    let ranges = TEXT_RANGES
        .iter()
        .copied()
        .chain([HIGHBAY_ICONS_BLOCK, MARKERS]);
    for (lo, hi) in ranges {
        for cp in (lo as u32)..=(hi as u32) {
            let ch = char::from_u32(cp).unwrap();
            let (Some(pg), Some(mg)) = (plain_face.glyph_id_for_char(ch), merged_face.glyph_id_for_char(ch))
            else {
                continue;
            };
            assert_eq!(
                cell_of(&plain, pg),
                cell_of(&merged, mg),
                "U+{cp:04X} is glyph {pg} in the plain face and {mg} in the merged one, \
                 and the two bakes put it in different cells"
            );
            checked += 1;
        }
    }
    assert!(checked > 190, "vacuity: only {checked} codepoints were comparable");

    // The merged face's extra cells are exactly its borrowed icons, and they
    // are all at the END — after every cell the plain face has.
    assert_eq!(merged.len(), plain.len() + MSYMBOLS_ICONS.len());
    for (cell, key) in merged.iter().enumerate() {
        let set = key.set();
        assert_eq!(
            set == GlyphSet::BorrowedIcons,
            cell >= plain.len(),
            "cell {cell} of the merged bake is {set:?}"
        );
    }
}
