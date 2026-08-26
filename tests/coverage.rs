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

// ── The set header: the face's metrics, and every cell agreeing with them ──

/// The face the fixture atlas was baked from.
fn face() -> ttf_parser::Face<'static> {
    ttf_parser::Face::parse(ROBOTO_REGULAR_ASCII, 0).expect("the shipped face parses")
}

/// **The baked baseline is the FACE's baseline**, recomputed here from the
/// font rather than copied from the file.
///
/// `glyph_projection` aligns the cap band to `CAP_TOP_FRAC` of the cell, so the
/// baseline lands at `CAP_TOP_FRAC + cap_height / (upem * CELL_EM_RATIO)` -
/// 0.696875 for Roboto, whose `'A'` tops out at 1456 of 2048 units. Anything
/// else in the header means the bake and the face have come apart.
///
/// Note what this number is NOT: `hhea`'s `ascender / (ascender - descender)`
/// is 0.7917 for this face, and that is a different quantity - where a line
/// box's ascent sits, not where THIS atlas's cells put their baseline. Reading
/// one for the other is a mistake worth 0.09 of a cell.
#[test]
fn the_text_baseline_is_the_faces_cap_projection() {
    let face = face();
    let upem = face.units_per_em() as f64;
    let expected =
        (libmsdf::font::CAP_TOP_FRAC + libmsdf::font::cap_height(&face) / (upem * libmsdf::font::CELL_EM_RATIO)) as f32;

    let atlas = atlas();
    let baked = atlas.text_baseline_frac().expect("the shipped atlas carries a Text set");
    assert!(
        (baked - expected).abs() < 1e-6,
        "header says {baked}, the face says {expected}"
    );
    assert!(
        (baked - 0.696875).abs() < 1e-6,
        "Roboto's cap projection is 0.696875 of the cell, not {baked}"
    );
}

/// **Every cell agrees with its set's header.**
///
/// This is the regression guard, and it is written over ALL cells on purpose:
/// the underline bug was one cell in 211 disagreeing with the other 210, and
/// `atlas.glyphs.first()` happening to be that one. A whitespace cell used to
/// carry `0.75 * cell` while every cell with ink carried 0.6969, and because a
/// space draws nothing, no rendered frame could show it. Only a test that reads
/// the metric rather than the pixels can.
#[test]
fn no_cell_disagrees_with_its_sets_baseline() {
    let atlas = atlas();
    let header = atlas.text_baseline_frac().expect("a Text set");
    for e in &atlas.glyphs {
        let cell = e.baseline_row / e.atlas_h as f32;
        assert!(
            (cell - header).abs() < 1e-6,
            "glyph {} is baked at {cell} of its cell, the header says {header}",
            e.glyph_id
        );
    }
}

/// **The em metrics are the face's own tables**, not remembered numbers.
///
/// `libhbui` deliberately rules a roomier underline than Roboto asks for at
/// 12px. That is a decision it is entitled to make - but it can only be stated
/// as a decision if the face's actual value is available to compare against,
/// which is what these fields are for.
#[test]
fn the_em_metrics_are_read_from_the_face() {
    let face = face();
    let upem = face.units_per_em() as f32;
    let atlas = atlas();
    let m = atlas.set_metrics(GlyphSet::Text).expect("a Text set");

    let underline = face.underline_metrics().expect("Roboto has a post table");
    assert!((m.underline_pos_em - underline.position as f32 / upem).abs() < 1e-6);
    assert!((m.underline_thickness_em - underline.thickness as f32 / upem).abs() < 1e-6);
    // Roboto-Regular: post underlinePosition -150/2048, thickness 100/2048.
    assert!((m.underline_pos_em + 0.07324).abs() < 1e-4, "{}", m.underline_pos_em);
    assert!((m.underline_thickness_em - 0.04883).abs() < 1e-4);

    assert!((m.ascent_em - face.ascender() as f32 / upem).abs() < 1e-6);
    assert!((m.descent_em - face.descender() as f32 / upem).abs() < 1e-6);
    assert!((m.line_gap_em - face.line_gap() as f32 / upem).abs() < 1e-6);
    assert!(m.descent_em < 0.0, "descent is negative below the baseline");

    // One em is the font size, and this is the field that says why: a cell is
    // `CELL_EM_RATIO` em tall and is drawn to `LINE_BOX_RATIO` x the size.
    let em_px = m.px_per_em_frac * libmsdf::drawlist::LINE_BOX_RATIO * 12.0;
    assert!((em_px - 12.0).abs() < 1e-3, "one em at 12px came out {em_px}");
}

/// **The ink descent is MEASURED from the outlines, and the face's declared
/// descender is not a bound on it in either direction.**
///
/// This is the finding the field exists for, so it is asserted rather than
/// written in a comment. Roboto-Regular, upem 2048, `hhea` descender -500:
///
/// * `Text` reaches -495 (`U+00A7`), five units SHALLOWER than declared.
/// * `Markers` reaches -512 (`U+F8F0`), twelve units DEEPER than declared.
///
/// One face, one declaration, two sets that miss it in opposite directions. A
/// consumer that used `descent_em` as a clearance would be needlessly low on
/// text and actually crossed on markers.
#[test]
fn the_ink_descent_is_the_deepest_outline_of_its_own_set() {
    let face = face();
    let upem = face.units_per_em() as f32;
    let atlas = atlas();

    let text = atlas.set_metrics(GlyphSet::Text).expect("a Text set");
    assert!(
        (text.ink_descent_em - -495.0 / upem).abs() < 1e-6,
        "Text ink descent came out {} em",
        text.ink_descent_em
    );
    // The deepest Text glyph, found rather than assumed: whatever the set's
    // minimum is, some glyph of the set has to reach exactly it.
    let section = face.glyph_index('\u{00A7}').expect("the face draws a section sign");
    let deepest = face.glyph_bounding_box(section).expect("it has an outline").y_min;
    assert_eq!(deepest, -495);

    let markers = atlas.set_metrics(GlyphSet::Markers).expect("a Markers set");
    assert!(
        (markers.ink_descent_em - -512.0 / upem).abs() < 1e-6,
        "Markers ink descent came out {} em",
        markers.ink_descent_em
    );

    // The two misses, in opposite directions, against one declaration.
    assert!(
        text.ink_descent_em > text.descent_em,
        "Text ink {} is not shallower than the declared {}",
        text.ink_descent_em,
        text.descent_em
    );
    assert!(
        markers.ink_descent_em < markers.descent_em,
        "Markers ink {} is not deeper than the declared {}",
        markers.ink_descent_em,
        markers.descent_em
    );

    // The consumer-facing flip, and the accessor `libhbui` actually calls.
    assert_eq!(text.max_ink_descent_em(), -text.ink_descent_em);
    assert_eq!(atlas.text_max_ink_descent_em(), Some(495.0 / upem));

    // Not every set has ink below the baseline, and the measurement says so
    // rather than clamping: the owned icons bottom out ABOVE it.
    let icons = atlas.set_metrics(GlyphSet::OwnedIcons).expect("an OwnedIcons set");
    assert!(icons.ink_descent_em > 0.0, "owned icons: {}", icons.ink_descent_em);
    assert!(icons.max_ink_descent_em() < 0.0, "so the depth is negative");
}

/// **The set the underline reads is not the deepest set in the file**, which is
/// the whole reason the metric is per-set.
///
/// Measured face-wide, the answer is `Markers`' -512 - a glyph that never
/// appears in a text run, dragging every underline in the app a quarter-pixel
/// lower at 48px for nothing. A rule that clears `Text` clears the text.
#[test]
fn the_face_wide_minimum_is_deeper_than_the_text_set() {
    let atlas = atlas();
    let face_wide = atlas
        .sets
        .iter()
        .map(|m| m.ink_descent_em)
        .fold(f32::INFINITY, f32::min);
    let text = atlas.set_metrics(GlyphSet::Text).expect("a Text set").ink_descent_em;
    assert!(
        face_wide < text,
        "face-wide {face_wide} is not deeper than Text {text} - if a re-bake made \
         them equal, the per-set argument is still sound but this test no longer \
         demonstrates it"
    );
}

/// **A run of text can also carry SHAPED forms**, and the rule is placed from
/// `Text` alone - so `ShapedText` must not be deeper than `Text`, or a ligature
/// would poke through a rule sized for the letters around it.
///
/// Checked rather than assumed: the ligatures and GSUB forms in the shipped
/// bake bottom out at -20/2048, nowhere near the letters. If a future face
/// brought in a descending form, this is where it would be caught.
#[test]
fn no_shaped_form_is_deeper_than_the_text_set() {
    let atlas = atlas();
    let text = atlas.set_metrics(GlyphSet::Text).expect("a Text set").ink_descent_em;
    let shaped = atlas.set_metrics(GlyphSet::ShapedText).expect("a ShapedText set");
    assert!(
        shaped.ink_descent_em >= text,
        "a shaped form reaches {} em, past the Text set's {text} em that the rule is \
         placed from - `underline_rect` would be crossed by it",
        shaped.ink_descent_em
    );
}

/// Every set in the shipped bake declares itself, and the counts are the real
/// cell counts.
#[test]
fn every_baked_set_is_in_the_header() {
    let atlas = atlas();
    assert!(!atlas.sets.is_empty(), "a baked atlas always has a set header");
    let counted: usize = atlas.sets.iter().map(|s| s.glyph_count as usize).sum();
    assert_eq!(counted, atlas.glyphs.len());
    for set in [GlyphSet::Placeholder, GlyphSet::Text, GlyphSet::ShapedText, GlyphSet::Markers] {
        assert!(atlas.set_metrics(set).is_some(), "{set:?} has cells but no header");
    }
    // The plain face borrows nothing, so it has no BorrowedIcons cells - and
    // therefore no header entry for them. An absent set is `None`, not zero.
    assert_eq!(atlas.set_metrics(GlyphSet::BorrowedIcons), None);
}
