//! **Emphasis: a second FACE in one atlas, addressed by style.**
//!
//! `GlyphStyle` exists because a glyph id is a face's private numbering and two
//! faces in one atlas collide by construction. It is not a hypothetical
//! collision: `a` is glyph 66 in Roboto Regular, glyph 66 in Roboto Bold and
//! glyph 66 in Roboto Italic, so an atlas keyed by raw glyph id can hold
//! exactly one of the three - which is why nothing here could be done by baking
//! more cells and hoping.
//!
//! Two things are proved, and the second matters more than the first:
//!
//! * a styled run can address a styled glyph, and an atlas without that style
//!   REFUSES rather than answering with the upright glyph at the same id;
//! * **nothing that already draws has moved.** `GlyphStyle::Regular` is prefix
//!   0, so every address in the shipped atlas is the number it always was, and
//!   that is checked against the committed bytes rather than argued.

use libmsdf::font::{
    CellKey, FontAtlas, FontAtlasBuilder, GlyphSet, GlyphStyle, MAX_RAW_GLYPH_ID,
    ROBOTO_ASCII_MSYMBOLS, ROBOTO_BOLD_ASCII, ROBOTO_ITALIC_ASCII, ROBOTO_REGULAR_ASCII,
    StyledGlyphError, StyledShaper, TEXT_RANGES, TextShaper, atlas_capacity, bundled_style_face,
};

/// The same committed artifact `coverage.rs` reads: the plain face, baked at
/// 48px, with no styled cell in it. It is the regression bar in file form.
const ATLAS_FIXTURE: &[u8] = include_bytes!("fixtures/roboto-ascii-48.atlas");

fn atlas() -> FontAtlas {
    FontAtlas::from_bytes(ATLAS_FIXTURE).expect("fixture atlas parses")
}

fn styled_shaper() -> StyledShaper {
    StyledShaper::bundled(ROBOTO_REGULAR_ASCII.to_vec()).expect("the bundled faces parse")
}

// ── the faces ───────────────────────────────────────────────────────────

/// **A style face covers exactly what the regular face covers.**
///
/// `TEXT_RANGES` is "the TEXT coverage of the bundled faces, and there is
/// exactly one of it". A bold cut that covered less would put the placeholder
/// box inside an emphasized word for a codepoint the same sentence renders fine
/// unemphasized - a coverage cliff that appears only under emphasis, which is
/// the hardest kind to find.
#[test]
fn every_style_face_covers_the_declared_text_ranges() {
    for &style in GlyphStyle::ALL {
        let Some(face) = bundled_style_face(style) else {
            continue;
        };
        let shaper = TextShaper::new(face.to_vec()).expect("bundled face parses");
        let mut checked = 0;
        for &(lo, hi) in TEXT_RANGES {
            for cp in (lo as u32)..=(hi as u32) {
                let ch = char::from_u32(cp).expect("a scalar value");
                assert!(
                    shaper.covers(ch),
                    "{style:?} does not cover U+{cp:04X}, which TEXT_RANGES declares"
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 201, "vacuity: TEXT_RANGES is not the range it was");
    }
}

/// **The placeholder is style-INVARIANT, and the bytes say so.**
///
/// Every style addresses glyph 0 as cell 0, so a bold run that meets an
/// uncovered codepoint draws the same box the regular path draws. The other
/// half of that has to be true of the FACES: a styled `.notdef` with an outline
/// would be a glyph nothing can ever reach, and would quietly invite a second
/// placeholder cell into a later bake.
#[test]
fn the_placeholder_is_style_invariant() {
    for &style in GlyphStyle::ALL {
        assert_eq!(
            style.styled_glyph_id(0),
            Some(0),
            "{style:?} does not address the placeholder as cell 0"
        );
    }
    for (style, face) in [
        (GlyphStyle::Bold, ROBOTO_BOLD_ASCII),
        (GlyphStyle::Italic, ROBOTO_ITALIC_ASCII),
    ] {
        let parsed = ttf_parser::Face::parse(face, 0).expect("bundled face parses");
        assert_eq!(
            parsed.glyph_bounding_box(ttf_parser::GlyphId(0)),
            None,
            "the {style:?} face draws its own .notdef - fonts/style.py must drop it"
        );
    }
    // ...and the regular face still draws one, or the box is gone entirely.
    let regular = ttf_parser::Face::parse(ROBOTO_REGULAR_ASCII, 0).expect("parses");
    assert!(regular.glyph_bounding_box(ttf_parser::GlyphId(0)).is_some());
}

/// The style faces are the SIBLINGS of the regular one: same em square, same
/// declared line metrics, same cap height. Emphasis that changed the line box
/// would reflow the paragraph around it.
#[test]
fn a_style_face_shares_the_regular_faces_line_box() {
    let regular = ttf_parser::Face::parse(ROBOTO_REGULAR_ASCII, 0).expect("parses");
    for (style, face) in [
        (GlyphStyle::Bold, ROBOTO_BOLD_ASCII),
        (GlyphStyle::Italic, ROBOTO_ITALIC_ASCII),
    ] {
        let f = ttf_parser::Face::parse(face, 0).expect("parses");
        assert_eq!(f.units_per_em(), regular.units_per_em(), "{style:?} upem");
        assert_eq!(f.ascender(), regular.ascender(), "{style:?} ascender");
        assert_eq!(f.descender(), regular.descender(), "{style:?} descender");
        assert_eq!(
            libmsdf::font::cap_height(&f),
            libmsdf::font::cap_height(&regular),
            "{style:?} cap height differs, so its cells would be projected to a different \
             baseline row and emphasis would not sit on the prose's own baseline"
        );
    }
}

/// Every face this repo bakes fits the 14 bits a prefixed address leaves,
/// with room measured in orders of magnitude rather than in glyphs.
#[test]
fn every_bundled_face_fits_the_address_space() {
    for face in [
        ROBOTO_REGULAR_ASCII,
        ROBOTO_ASCII_MSYMBOLS,
        ROBOTO_BOLD_ASCII,
        ROBOTO_ITALIC_ASCII,
    ] {
        let f = ttf_parser::Face::parse(face, 0).expect("parses");
        assert!(
            f.number_of_glyphs() <= MAX_RAW_GLYPH_ID,
            "{} glyphs is past MAX_RAW_GLYPH_ID",
            f.number_of_glyphs()
        );
    }
}

// ── the address ─────────────────────────────────────────────────────────

/// **The join is lossless**, at the extremes of both halves - the property the
/// whole scheme rests on, since a lost bit would alias one face's glyph onto
/// another's cell rather than fail.
#[test]
fn a_styled_glyph_id_round_trips() {
    for &style in GlyphStyle::ALL {
        for raw in [1u16, 2, 66, 255, 256, 8191, 8192, MAX_RAW_GLYPH_ID] {
            let addr = style.styled_glyph_id(raw).expect("inside the address space");
            assert_eq!(
                GlyphStyle::split_glyph_id(addr),
                (style, raw),
                "{style:?} glyph {raw} did not survive the round trip"
            );
        }
        // Glyph 0 is the one deliberate exception, and it is not a loss: the
        // placeholder is one glyph, addressed as cell 0 from every style.
        assert_eq!(GlyphStyle::split_glyph_id(0), (GlyphStyle::Regular, 0));
    }
}

/// A glyph id too wide to prefix is REFUSED, not truncated. Truncation would
/// hand back an address inside another style's block, which draws the wrong
/// letter in the wrong weight and cannot be noticed from the output.
#[test]
fn a_glyph_id_too_wide_to_prefix_is_refused() {
    for &style in GlyphStyle::ALL {
        assert_eq!(style.styled_glyph_id(MAX_RAW_GLYPH_ID + 1), None);
        assert_eq!(style.styled_glyph_id(u16::MAX), None);
    }
}

/// **A regular address IS the raw glyph id.** The regression bar in one line:
/// prefix 0 means every lookup that exists today is bit-for-bit the lookup it
/// was, and it is checked over every glyph the shipped atlas actually contains
/// rather than over a sample.
#[test]
fn regular_addresses_are_the_glyph_ids_they_always_were() {
    let atlas = atlas();
    assert!(atlas.glyphs.len() > 200, "vacuity: the fixture came back empty");
    for entry in &atlas.glyphs {
        assert_eq!(
            GlyphStyle::Regular.styled_glyph_id(entry.glyph_id),
            Some(entry.glyph_id),
            "glyph {} moved under the regular prefix",
            entry.glyph_id
        );
        assert_eq!(
            atlas.styled_glyph(GlyphStyle::Regular, entry.glyph_id),
            Ok(entry),
            "the styled lookup and the plain one disagree about glyph {}",
            entry.glyph_id
        );
    }
}

/// **Every styled set sorts after every unstyled one**, which is what makes a
/// styled bake an append rather than a repack. Stated over the type rather than
/// over one bake, so a set added in the wrong place fails here and not in a
/// rendered frame six waves later.
#[test]
fn every_styled_set_sorts_after_every_unstyled_one() {
    let (styled, unstyled): (Vec<GlyphSet>, Vec<GlyphSet>) = GlyphSet::ALL
        .iter()
        .partition(|s| s.style() != GlyphStyle::Regular);
    assert_eq!(unstyled.len(), 6, "the unstyled sets are not the six that shipped");
    assert_eq!(styled.len(), 6, "three styles, two sets each");
    for &u in &unstyled {
        for &s in &styled {
            assert!(u < s, "{u:?} does not sort before {s:?}");
            // ...and the join agrees, since the join is what the sort uses.
            assert!(
                CellKey::new(u, u16::MAX).bits() < CellKey::new(s, 0).bits(),
                "a maximal glyph id in {u:?} outranked the first cell of {s:?}"
            );
        }
    }
}

/// Each style's two sets are its own, and no set is claimed twice.
#[test]
fn a_set_belongs_to_exactly_one_style() {
    let mut seen: Vec<GlyphSet> = Vec::new();
    for &style in GlyphStyle::ALL {
        for set in [style.text_set(), style.shaped_set()] {
            assert_eq!(set.style(), style, "{set:?} does not name {style:?} back");
            assert!(!seen.contains(&set), "{set:?} belongs to two styles");
            seen.push(set);
        }
    }
    for &set in GlyphSet::ALL {
        assert!(
            seen.contains(&set) || set.style() == GlyphStyle::Regular,
            "{set:?} is styled but is not either set of its style"
        );
    }
}

// ── the refusal ─────────────────────────────────────────────────────────

/// **The shipped atlas refuses bold**, and it refuses it for a glyph id it
/// certainly has a cell for.
///
/// Glyph 66 is `a` in all three faces. An atlas keyed by raw glyph id would
/// hand back the upright `a` for a bold request and every frame would look
/// plausible - the exact substitution this API exists to make impossible.
#[test]
fn the_shipped_atlas_refuses_a_style_it_does_not_carry() {
    let atlas = atlas();
    let a = TextShaper::new(ROBOTO_REGULAR_ASCII.to_vec())
        .expect("parses")
        .glyph_id_for_char('a')
        .expect("the face draws an 'a'");
    assert!(atlas.get_glyph(a).is_some(), "vacuity: the fixture has no cell for 'a'");

    for style in [GlyphStyle::Bold, GlyphStyle::Italic, GlyphStyle::BoldItalic] {
        assert_eq!(
            atlas.styled_glyph(style, a).unwrap_err(),
            StyledGlyphError::StyleNotBaked(style),
            "the fixture answered a {style:?} request"
        );
        assert!(!atlas.carries_style(style));
    }
    assert!(atlas.carries_style(GlyphStyle::Regular));
    assert_eq!(atlas.styles(), vec![GlyphStyle::Regular]);
}

/// An already-prefixed id passed as a raw one is refused rather than prefixed
/// twice - a programming error, and one whose second prefix would land on a
/// real cell of a real style.
#[test]
fn an_address_passed_as_a_raw_glyph_id_is_refused() {
    let atlas = atlas();
    let addr = GlyphStyle::Bold.styled_glyph_id(66).expect("addressable");
    assert_eq!(
        atlas.styled_glyph(GlyphStyle::Regular, addr).unwrap_err(),
        StyledGlyphError::NotARawGlyphId(addr)
    );
}

/// A runtime-populated atlas ([`FontAtlas::empty`] plus appends) has no face
/// and no set header. It is regular by construction, and every other style is a
/// refusal rather than whatever glyph happens to sit at that id.
#[test]
fn a_runtime_atlas_carries_only_the_regular_style() {
    let atlas = FontAtlas::empty(64, 64, 3);
    assert!(atlas.carries_style(GlyphStyle::Regular));
    assert_eq!(atlas.styles(), vec![GlyphStyle::Regular]);
    assert_eq!(
        atlas.styled_glyph(GlyphStyle::Bold, 66).unwrap_err(),
        StyledGlyphError::StyleNotBaked(GlyphStyle::Bold)
    );
    // Regular is carried, and this atlas simply has no cells yet.
    assert_eq!(
        atlas.styled_glyph(GlyphStyle::Regular, 66).unwrap_err(),
        StyledGlyphError::NoCell(GlyphStyle::Regular, 66)
    );
}

/// **A styled cell generated at RUNTIME is addressed like a baked one.**
///
/// The compute-MSDF path appends cells to an atlas with no set header, and a
/// style's baked cells are ~290 KiB gzipped against a 13 KiB face - so
/// generating them on demand is the option a browser is most likely to want.
/// It needs no second address: the entry is filed under the styled glyph id,
/// and the same lookup finds it.
#[test]
fn a_runtime_appended_styled_cell_is_addressable() {
    let mut atlas = FontAtlas::empty(64, 64, 3);
    let address = GlyphStyle::Bold.styled_glyph_id(66).expect("addressable");
    assert!(!atlas.carries_style(GlyphStyle::Bold), "nothing appended yet");

    atlas.insert_entry(libmsdf::GlyphEntry {
        glyph_id: address,
        atlas_x: 0,
        atlas_y: 0,
        atlas_w: 16,
        atlas_h: 16,
        layer: 0,
        advance_x: 0.536,
        baseline_row: 11.15,
        px_per_em: 12.3,
        x_margin: 2.4,
    });
    assert!(atlas.carries_style(GlyphStyle::Bold));
    assert_eq!(
        atlas.styled_glyph(GlyphStyle::Bold, 66).map(|e| e.glyph_id),
        Ok(address)
    );
    // ...and it did not make the atlas claim a style it has no cells for.
    assert_eq!(
        atlas.styled_glyph(GlyphStyle::Italic, 66).unwrap_err(),
        StyledGlyphError::StyleNotBaked(GlyphStyle::Italic)
    );
    // The regular id 66 is still a MISS rather than the bold cell: the two are
    // different addresses and the runtime path did not blur them.
    assert_eq!(
        atlas.styled_glyph(GlyphStyle::Regular, 66).unwrap_err(),
        StyledGlyphError::NoCell(GlyphStyle::Regular, 66)
    );
}

/// **The shaper refuses a style it has no face for**, and `BoldItalic` is the
/// live case: addressable, bundled with nothing, and a caller that meets nested
/// emphasis gets an error it can report instead of upright text.
#[test]
fn the_shaper_refuses_a_style_it_has_no_face_for() {
    let shaper = styled_shaper();
    assert_eq!(
        shaper.styles(),
        vec![GlyphStyle::Regular, GlyphStyle::Bold, GlyphStyle::Italic]
    );
    assert!(!shaper.carries(GlyphStyle::BoldItalic));
    assert_eq!(
        shaper.shape(GlyphStyle::BoldItalic, "bold italic").unwrap_err(),
        libmsdf::font::NoFaceForStyle(GlyphStyle::BoldItalic)
    );
    assert!(shaper.glyph_id_for_char(GlyphStyle::BoldItalic, 'a').is_err());
    assert!(bundled_style_face(GlyphStyle::BoldItalic).is_none());
}

// ── shaping ─────────────────────────────────────────────────────────────

/// **A regular run through the styled shaper is the plain run**, glyph for
/// glyph and advance for advance. The one API a consumer would migrate to must
/// not change a single frame on the way.
#[test]
fn a_regular_run_is_unchanged_by_going_through_the_styled_shaper() {
    let plain = TextShaper::new(ROBOTO_REGULAR_ASCII.to_vec()).expect("parses");
    let styled = styled_shaper();
    for text in ["Hello", "AV", "fifty officiel", "Jos\u{e9} M\u{fc}ller", "0123 {{ x }}"] {
        let a = plain.shape(text);
        let b = styled.shape(GlyphStyle::Regular, text).expect("regular is loaded");
        assert_eq!(a.total_advance, b.total_advance, "{text:?} advance");
        assert_eq!(a.units_per_em, b.units_per_em);
        assert_eq!(a.glyphs.len(), b.glyphs.len(), "{text:?} glyph count");
        for (x, y) in a.glyphs.iter().zip(b.glyphs.iter()) {
            assert_eq!(x.glyph_id, y.glyph_id, "{text:?}");
            assert_eq!(x.x_advance, y.x_advance, "{text:?}");
            assert_eq!(x.cluster, y.cluster, "{text:?}");
        }
    }
}

/// **A bold run is a DIFFERENT run**: different glyphs, different advances, and
/// every id prefixed into bold's block. If a wave ever wires the regular face
/// in behind the bold style, the ids stay in the regular block and this fails.
#[test]
fn a_bold_run_is_addressed_in_bolds_own_block() {
    let shaper = styled_shaper();
    let text = "The quick brown fox";
    let regular = shaper.shape(GlyphStyle::Regular, text).expect("loaded");
    let bold = shaper.shape(GlyphStyle::Bold, text).expect("loaded");

    assert_eq!(regular.glyphs.len(), bold.glyphs.len(), "the same characters");
    for g in &bold.glyphs {
        let (style, raw) = GlyphStyle::split_glyph_id(g.glyph_id);
        assert_eq!(style, GlyphStyle::Bold, "a bold glyph is not in bold's block");
        assert!(raw != 0, "bold shaped a covered character to the placeholder");
    }
    assert_ne!(
        regular.total_advance, bold.total_advance,
        "bold measures the same as regular - the same face is behind both"
    );
    // The raw ids are the SAME small integers in both faces, which is the whole
    // reason the prefix exists: without it these two runs are one run.
    let raw = |g: &libmsdf::font::ShapedGlyph| GlyphStyle::split_glyph_id(g.glyph_id).1;
    assert_eq!(
        regular.glyphs.iter().map(raw).collect::<Vec<_>>(),
        bold.glyphs.iter().map(raw).collect::<Vec<_>>(),
        "vacuity: the two faces do not even number their glyphs alike"
    );
}

/// **A ligature is per-FACE, and that is why every style needs its own shaped
/// set.** `fi` folds to one glyph in both faces, at ids that mean different
/// letters in the other one - so baking the regular superset and addressing it
/// from a bold run would draw the wrong shape, not a missing one.
#[test]
fn a_bold_ligature_is_a_bold_glyph() {
    let shaper = styled_shaper();
    let regular = shaper.shape(GlyphStyle::Regular, "fi").expect("loaded");
    let bold = shaper.shape(GlyphStyle::Bold, "fi").expect("loaded");
    assert_eq!(regular.glyphs.len(), 1, "Roboto folds fi into one glyph");
    assert_eq!(bold.glyphs.len(), 1, "so does the bold cut");

    let (r, b) = (
        GlyphStyle::split_glyph_id(regular.glyphs[0].glyph_id).1,
        GlyphStyle::split_glyph_id(bold.glyphs[0].glyph_id).1,
    );
    assert_ne!(
        r, b,
        "the two faces happen to number their fi ligature alike - this test proves nothing \
         about which face was shaped with"
    );
    // Neither ligature is nameable through a cmap, which is what puts them in a
    // shaped set rather than a text one.
    let bold_face = shaper.face(GlyphStyle::Bold).expect("loaded");
    assert!(
        (0x20u32..=0xFF).all(|cp| char::from_u32(cp)
            .and_then(|c| bold_face.glyph_id_for_char(c))
            != Some(b)),
        "the bold fi ligature is reachable through the cmap after all"
    );
}

// ── the bake ────────────────────────────────────────────────────────────

/// **A styled queue APPENDS.** Every cell the shipped coverage lays down keeps
/// its position when bold and italic are queued behind it - checked as a prefix
/// of the cell order, which is the order the bake writes cells in.
///
/// This is the property that lets a consumer re-bake with emphasis and treat
/// any moved frame as a real finding.
#[test]
fn a_styled_queue_appends_to_the_shipped_one() {
    let mut builder = FontAtlasBuilder::new(ROBOTO_ASCII_MSYMBOLS.to_vec(), 48, 6.0);
    builder.add_shipped_coverage();
    let shipped = builder.cell_order();

    builder
        .add_styled_coverage(GlyphStyle::Bold, ROBOTO_BOLD_ASCII.to_vec())
        .expect("the bold face parses");
    builder
        .add_styled_coverage(GlyphStyle::Italic, ROBOTO_ITALIC_ASCII.to_vec())
        .expect("the italic face parses");
    let with_styles = builder.cell_order();

    assert_eq!(
        with_styles[..shipped.len()],
        shipped[..],
        "queueing a style moved a cell that had already been baked"
    );
    for key in &with_styles[shipped.len()..] {
        assert_ne!(
            key.set().style(),
            GlyphStyle::Regular,
            "an unstyled cell landed after the styled ones"
        );
    }
    assert_eq!(
        builder.styles(),
        vec![GlyphStyle::Regular, GlyphStyle::Bold, GlyphStyle::Italic]
    );
}

/// The builder refuses a second face for a style, and refuses to be handed a
/// regular one - both cases where the alternative is silently picking a winner
/// for cells that are already queued.
#[test]
fn the_builder_refuses_a_second_face_for_a_style() {
    let mut builder = FontAtlasBuilder::new(ROBOTO_REGULAR_ASCII.to_vec(), 16, 2.0);
    builder
        .add_styled_coverage(GlyphStyle::Bold, ROBOTO_BOLD_ASCII.to_vec())
        .expect("first bold face");
    assert!(
        builder
            .add_styled_coverage(GlyphStyle::Bold, ROBOTO_ITALIC_ASCII.to_vec())
            .is_err()
    );
    assert!(
        builder
            .add_styled_coverage(GlyphStyle::Regular, ROBOTO_BOLD_ASCII.to_vec())
            .is_err()
    );
    assert!(
        builder
            .add_styled_coverage(GlyphStyle::Italic, b"not a font".to_vec())
            .is_err()
    );
}

/// **One style does not fit beside the shipped coverage in today's atlas**, and
/// this is the measurement rather than an estimate.
///
/// 234 cells are spoken for by the merged face and the grid holds 320, so 86
/// are free; a style is 215 (201 declared codepoints, 14 shaped forms beyond
/// them). The binding constraint is `ATLAS_COLS`, not `ATLAS_ROWS`: the texture
/// is 400x2000 inside a 2048x2048 floor, so it is 8 columns wide because
/// widening it moves every glyph's `u` and re-blesses every frame - a one-time
/// cost, not a limit. At 16 columns the same 40 rows hold 640 cells.
///
/// Written as a test so the number is checked and so raising the capacity is
/// forced to come back through this comment.
#[test]
fn a_style_does_not_fit_beside_the_shipped_coverage_today() {
    let mut builder = FontAtlasBuilder::new(ROBOTO_ASCII_MSYMBOLS.to_vec(), 48, 6.0);
    builder.add_shipped_coverage();
    let shipped = builder.cell_order().len();
    builder
        .add_styled_coverage(GlyphStyle::Bold, ROBOTO_BOLD_ASCII.to_vec())
        .expect("the bold face parses");
    let with_bold = builder.cell_order().len();

    assert_eq!(shipped, 234, "the shipped coverage is not the 234 cells it was");
    assert_eq!(with_bold - shipped, 215, "a style is not the 215 cells it was");
    assert_eq!(atlas_capacity(), 320);
    assert!(
        with_bold > atlas_capacity(),
        "a style now fits: re-read ATLAS_ROWS and ATLAS_COLS, and say which one moved"
    );
}

/// **An atlas naming a set this build has never heard of is refused cleanly.**
///
/// The styled sets were added past `BorrowedIcons` with no format version bump,
/// which is only sound because this refusal already existed: an older libmsdf
/// meeting an atlas baked with bold stops here instead of reading a bold cell
/// as a text one. The same path now guards whatever the set after these is.
#[test]
fn an_atlas_from_a_newer_libmsdf_is_refused() {
    let mut atlas = FontAtlas::empty(4, 4, 3);
    atlas.sets = vec![libmsdf::SetMetrics {
        set: GlyphSet::BoldItalicShapedText,
        glyph_count: 0,
        baseline_frac: 0.696875,
        px_per_em_frac: 0.769231,
        ascent_em: 0.92773,
        descent_em: -0.24414,
        line_gap_em: 0.0,
        underline_pos_em: -0.07324,
        underline_thickness_em: 0.04883,
        ink_descent_em: -0.227051,
    }];
    let mut bytes = atlas.to_bytes();
    // The last set this build has, plus one: what a future libmsdf writes.
    assert_eq!(bytes[libmsdf::ATLAS_HEADER_SIZE], GlyphSet::BoldItalicShapedText as u8);
    assert!(FontAtlas::from_bytes(&bytes).is_ok(), "the set this build does have");
    bytes[libmsdf::ATLAS_HEADER_SIZE] += 1;
    let err = FontAtlas::from_bytes(&bytes).expect_err("a newer set must not load");
    assert!(err.contains("newer libmsdf"), "{err}");
}

// ── the bake, for real (native + msdfgen) ───────────────────────────────

/// The CPU bake is native-only and feature-gated (`cpu-bake`), so these are the
/// tests that cannot run under a plain `cargo test`:
///
/// ```text
/// CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm cargo test --features cpu-bake
/// ```
#[cfg(all(feature = "cpu-bake", not(target_arch = "wasm32")))]
mod baked {
    use super::*;

    /// Small cells and a small range: this is a test about ADDRESSES and cell
    /// placement, and 300 cells of msdfgen at 48px would be a minute of it.
    const GS: u32 = 16;
    const PX_RANGE: f64 = 2.0;

    fn regular_only() -> FontAtlas {
        let mut b = FontAtlasBuilder::new(ROBOTO_REGULAR_ASCII.to_vec(), GS, PX_RANGE);
        b.add_ascii();
        b.add_glyph(GlyphSet::Placeholder, 0);
        b.build().expect("regular bake")
    }

    fn regular_and_bold() -> FontAtlas {
        let mut b = FontAtlasBuilder::new(ROBOTO_REGULAR_ASCII.to_vec(), GS, PX_RANGE);
        b.add_ascii();
        b.add_glyph(GlyphSet::Placeholder, 0);
        b.add_styled_coverage(GlyphStyle::Bold, ROBOTO_BOLD_ASCII.to_vec())
            .expect("the bold face parses");
        b.build().expect("styled bake")
    }

    /// The texels of one cell, for comparing two bakes at the pixel level.
    ///
    /// **Delegated to the atlas rather than written out here**, because a cell
    /// address gained a second term - the LAYER - and this used to compute the
    /// first one only. A layer-blind read of a bold cell returns plausible
    /// bytes from the regular layer instead of failing, so the comparison and
    /// the shader have to agree about where a cell is by construction.
    fn cell_pixels(a: &FontAtlas, glyph_id: u16) -> Vec<u8> {
        a.cell_texels(glyph_id).expect("a cell for this glyph")
    }

    /// **Adding bold moved nothing** - not an entry, not a cell coordinate, not
    /// a texel. The strongest form of the regression bar: the two atlases are
    /// compared where it actually matters, in the pixels the shader samples.
    #[test]
    fn a_styled_bake_appends_and_moves_nothing() {
        let (plain, styled) = (regular_only(), regular_and_bold());
        assert_eq!(plain.width, styled.width, "the texture width moved");
        assert_eq!(plain.height, styled.height, "the texture height moved");
        // The style arrives as a LAYER, which is what keeps it from competing
        // for the 320 cells the regular coverage is packed into.
        assert_eq!(plain.layer_count(), 1);
        assert_eq!(styled.layer_count(), 2);
        assert_eq!(
            plain.pixel_data,
            styled.pixel_data[..styled.layer_offset(1)],
            "layer 0's texels changed when bold was added"
        );
        assert!(plain.glyphs.len() >= 96, "vacuity: the plain bake is nearly empty");
        assert!(styled.glyphs.len() > plain.glyphs.len() + 190, "bold did not arrive");

        for entry in &plain.glyphs {
            let after = styled
                .get_glyph(entry.glyph_id)
                .unwrap_or_else(|| panic!("glyph {} lost its cell", entry.glyph_id));
            assert_eq!(entry, after, "glyph {} changed", entry.glyph_id);
            assert_eq!(
                cell_pixels(&plain, entry.glyph_id),
                cell_pixels(&styled, entry.glyph_id),
                "glyph {}'s texels changed",
                entry.glyph_id
            );
        }
        // The regular set header is untouched as well - the styled sets are
        // rows added after it, not a re-measurement of it.
        for set in [GlyphSet::Placeholder, GlyphSet::Text] {
            assert_eq!(plain.set_metrics(set), styled.set_metrics(set), "{set:?}");
        }
    }

    /// **The bold cell is a bold OUTLINE**, not a copy of the regular one at a
    /// second address. Compared as texels, because that is the only difference
    /// that reaches a frame.
    #[test]
    fn a_bold_cell_holds_the_bold_faces_outline() {
        let atlas = regular_and_bold();
        let shaper = styled_shaper();
        let a_regular = shaper
            .glyph_id_for_char(GlyphStyle::Regular, 'a')
            .expect("loaded")
            .expect("covered");
        let a_bold_raw = GlyphStyle::split_glyph_id(
            shaper
                .glyph_id_for_char(GlyphStyle::Bold, 'a')
                .expect("loaded")
                .expect("covered"),
        )
        .1;
        assert_eq!(
            a_regular, a_bold_raw,
            "vacuity: the two faces disagree about 'a''s glyph id, so nothing here is proving \
             the prefix does any work"
        );

        let bold = atlas
            .styled_glyph(GlyphStyle::Bold, a_bold_raw)
            .expect("bold is baked here");
        assert_ne!(
            bold.glyph_id, a_regular,
            "the bold entry is filed under the regular address"
        );
        assert_ne!(
            cell_pixels(&atlas, bold.glyph_id),
            cell_pixels(&atlas, a_regular),
            "the bold 'a' and the regular 'a' are the same texels"
        );
        assert!(
            bold.advance_x < atlas.get_glyph(a_regular).unwrap().advance_x,
            "the bold 'a' has the regular advance - it was baked from the regular face"
        );
    }

    /// **The bold set's metrics are BOLD's**, measured off the cells that were
    /// baked from that face. `SetMetrics` promised this shape before there was
    /// a second face to prove it with:
    /// *"a set fed from a second face would land here with its own baseline and
    /// the header would say so without any further change."*
    #[test]
    fn the_bold_set_carries_the_bold_faces_own_metrics() {
        let atlas = regular_and_bold();
        let text = atlas.set_metrics(GlyphSet::Text).expect("a Text set");
        let bold = atlas.set_metrics(GlyphSet::BoldText).expect("a BoldText set");

        // The deepest ink of the bold face over the ranges its set was baked
        // from - measured off the outlines here, so this compares the header
        // against the face rather than against a remembered number. It is a
        // Latin-1 glyph rather than an ASCII one, which is exactly why the
        // range has to be the set's own coverage and not "printable ASCII".
        let deepest = {
            let f = ttf_parser::Face::parse(ROBOTO_BOLD_ASCII, 0).expect("parses");
            let upem = f.units_per_em() as f32;
            TEXT_RANGES
                .iter()
                .flat_map(|&(lo, hi)| (lo as u32)..=(hi as u32))
                .filter_map(|cp| f.glyph_index(char::from_u32(cp).unwrap()))
                .filter_map(|g| f.glyph_bounding_box(g))
                .map(|bb| bb.y_min as f32 / upem)
                .fold(0.0f32, f32::min)
        };
        assert!(
            (bold.ink_descent_em - deepest).abs() < 1e-6,
            "BoldText reports {} and the bold face reaches {deepest}",
            bold.ink_descent_em,
        );
        assert_ne!(
            bold.ink_descent_em, text.ink_descent_em,
            "vacuity: the two faces reach the same depth, so this measures nothing"
        );
        // The baseline is deliberately the SAME: both faces share a cap height,
        // so emphasis sits on the prose's own baseline rather than beside it.
        assert_eq!(bold.baseline_frac, text.baseline_frac);
        assert_eq!(bold.px_per_em_frac, text.px_per_em_frac);
        assert_eq!(atlas.styles(), vec![GlyphStyle::Regular, GlyphStyle::Bold]);
    }

    /// **A bold run draws**: every glyph of a shaped bold sentence, ligature
    /// included, has a cell in the styled atlas - and none of them is the
    /// placeholder.
    #[test]
    fn every_glyph_of_a_bold_run_has_a_cell() {
        let atlas = regular_and_bold();
        let shaper = styled_shaper();
        let run = shaper
            .shape(GlyphStyle::Bold, "The quick brown fox jumps over fifty officials")
            .expect("loaded");
        assert_eq!(run.notdef_count(), 0, "a covered character shaped to the box");
        for g in &run.glyphs {
            assert!(
                atlas.get_glyph(g.glyph_id).is_some(),
                "bold glyph {} has no cell - the shaped set is short",
                g.glyph_id
            );
            let (style, raw) = GlyphStyle::split_glyph_id(g.glyph_id);
            assert_eq!(style, GlyphStyle::Bold);
            assert!(atlas.styled_glyph(GlyphStyle::Bold, raw).is_ok());
        }
        // ...and the italic the same atlas does NOT carry is still refused.
        assert!(matches!(
            atlas.styled_glyph(GlyphStyle::Italic, 66),
            Err(StyledGlyphError::StyleNotBaked(GlyphStyle::Italic))
        ));
    }
}
