//! The icons THIS REPO DREW, checked against the SHIPPED FACES and the REAL
//! BAKED ATLAS — the namespace they live in, the metric contract that lets the
//! text path place them, and the SHAPES that came out of the bake.
//!
//! Sibling to `marker.rs`, which does the same for the edge markers, and it is
//! deliberately the same shape: a glyph this repo authors has a `fonts/*.py`
//! script that decides its geometry and a test file that asserts what Rust and
//! the design rely on against the bytes that ship.
//!
//! The silhouette assertions are the unusual half and the point of the file.
//! Table, Props and Graph exist to be told apart AT A GLANCE at 16-28px, so
//! "the glyph is present" is not the property that matters — what matters is
//! that the Table still has three columns under a heavier header rule, that a
//! Props row is still a LABEL and a VALUE rather than one bar, and that the
//! Graph's nodes still dominate its edges. Each of those is a design decision
//! someone could undo in `fonts/icon.py` without any other test noticing, and
//! each is read here off the baked field rather than off the font, because
//! "the outline says so" and "the atlas drew it" are different claims.

use libmsdf::drawlist::{LINE_BOX_RATIO, screen_px_range};
use libmsdf::font::{
    FontAtlas, HIGHBAY_ICONS, HIGHBAY_ICONS_BLOCK, MARKERS, MSYMBOLS_ICONS, OWNED_BLOCKS,
    PRIVATE_USE, ROBOTO_ASCII_MSYMBOLS, ROBOTO_REGULAR_ASCII, TextShaper, highbay_codepoint,
    msymbols_codepoint,
};

const ATLAS_FIXTURE: &[u8] = include_bytes!("fixtures/roboto-ascii-48.atlas");
const PX_RANGE: f32 = 6.0;
/// Roboto's cap height, which `glyph_projection` aligns every cell to.
const CAP_HEIGHT: f32 = 1456.0;

fn atlas() -> FontAtlas {
    FontAtlas::from_bytes(ATLAS_FIXTURE).expect("fixture atlas parses")
}

fn shaper() -> TextShaper {
    TextShaper::new(ROBOTO_REGULAR_ASCII.to_vec()).expect("the shipped face parses")
}

fn codepoint(name: &str) -> char {
    highbay_codepoint(name).unwrap_or_else(|| panic!("{name:?} is not in the manifest"))
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

/// One glyph's cell as a `gs × gs` grid of COVERAGE, not of yes/no.
///
/// The alpha ramp is the shader's, at the cell's own scale (one screen pixel
/// per texel, so `screen_px_range` is just `PX_RANGE`). Thresholded ink would
/// be enough to find a shape but not to compare two stroke WEIGHTS: the
/// Table's header rule is 3.3 texels and its frame 2.3, and both round to
/// three inked rows. Coverage keeps the difference the design is about.
struct Cell {
    gs: usize,
    a: Vec<f32>,
}

impl Cell {
    fn of(atlas: &FontAtlas, name: &str) -> Self {
        let run = shaper().shape(codepoint(name).encode_utf8(&mut [0u8; 4]));
        let e = *atlas
            .get_glyph(run.glyphs[0].glyph_id)
            .unwrap_or_else(|| panic!("{name:?} has no baked cell"));
        let gs = e.atlas_w as usize;
        let a = (0..gs)
            .flat_map(|y| (0..gs).map(move |x| (x, y)))
            .map(|(x, y)| {
                let sd = median_at(atlas, e.atlas_x as u32 + x as u32, e.atlas_y as u32 + y as u32);
                (PX_RANGE * (sd - 0.5) + 0.5).clamp(0.0, 1.0)
            })
            .collect();
        Self { gs, a }
    }

    fn at(&self, x: usize, y: usize) -> f32 {
        self.a[y * self.gs + x]
    }

    fn inked(&self, x: usize, y: usize) -> bool {
        self.at(x, y) > 0.5
    }

    /// Ink bounds as `(x0, y0, x1, y1)`, inclusive.
    fn bounds(&self) -> (usize, usize, usize, usize) {
        let pts: Vec<(usize, usize)> = (0..self.gs)
            .flat_map(|y| (0..self.gs).map(move |x| (x, y)))
            .filter(|&(x, y)| self.inked(x, y))
            .collect();
        assert!(!pts.is_empty(), "the cell baked blank");
        (
            pts.iter().map(|p| p.0).min().unwrap(),
            pts.iter().map(|p| p.1).min().unwrap(),
            pts.iter().map(|p| p.0).max().unwrap(),
            pts.iter().map(|p| p.1).max().unwrap(),
        )
    }

    /// Inclusive `[start, end]` runs of inked texels along row `y`.
    fn runs(&self, y: usize) -> Vec<(usize, usize)> {
        let mut out: Vec<(usize, usize)> = Vec::new();
        for x in 0..self.gs {
            if self.inked(x, y) {
                match out.last_mut() {
                    Some(r) if r.1 + 1 == x => r.1 = x,
                    _ => out.push((x, x)),
                }
            }
        }
        out
    }

    /// Sub-texel thickness of the ink in column `x` over rows `y0..=y1`, as
    /// summed coverage — how thick a stroke *looks*, not how many texels it
    /// happened to light up.
    fn weight_col(&self, x: usize, y0: usize, y1: usize) -> f32 {
        (y0..=y1).map(|y| self.at(x, y)).sum()
    }

    /// The same measure across a row.
    fn weight_row(&self, y: usize, x0: usize, x1: usize) -> f32 {
        (x0..=x1).map(|x| self.at(x, y)).sum()
    }

    /// The row with the most ink in `y0..=y1` — a rounded feature's widest
    /// slice, found rather than guessed at from where its bounds happen to be.
    fn widest_row(&self, y0: usize, y1: usize) -> usize {
        (y0..=y1)
            .max_by_key(|&y| (0..self.gs).filter(|&x| self.inked(x, y)).count())
            .expect("a non-empty band")
    }
}

// ── the carveout, and who owns which half of it ─────────────────────────

/// **The top 256 codepoints of the Private Use Area are the repo's**, split in
/// two blocks that abut exactly and overlap nowhere.
///
/// This is the property that lets `add_shipped_coverage` queue the whole PUA as
/// three declared BLOCKS without knowing an icon set: borrowed glyphs sit far
/// below the line, ours sit above it in two abutting halves, and neither can
/// wander into the other by accident. The bake asserts the same shape before it
/// scans, because a gap between the blocks would be codepoints the face defines
/// and the atlas never queues.
#[test]
fn owned_blocks_are_the_top_of_the_carveout() {
    let (pua_lo, pua_hi) = PRIVATE_USE;
    let (own_lo, own_hi) = OWNED_BLOCKS;
    let (icons_lo, icons_hi) = HIGHBAY_ICONS_BLOCK;
    let (m_lo, m_hi) = MARKERS;

    assert!(own_lo >= pua_lo && own_hi <= pua_hi, "the owned blocks escape the carveout");
    assert_eq!(own_hi, pua_hi, "the owned blocks are the TOP of the carveout");
    // The two halves partition it: icons start at the bottom, markers end at
    // the top, and the boundary between them has no gap and no overlap.
    assert_eq!(icons_lo, own_lo);
    assert_eq!(m_hi, own_hi);
    assert_eq!(
        icons_hi as u32 + 1,
        m_lo as u32,
        "the icon block and the marker block are not contiguous — a gap here is \
         address space nobody owns, and an overlap is two vocabularies on one codepoint",
    );

    // Everything BORROWED sorts below the line, so a vendor codepoint can
    // never land in either of our blocks.
    for &(name, cp) in MSYMBOLS_ICONS {
        assert!(
            cp < own_lo,
            "borrowed icon {name:?} at U+{:04X} is inside the repo's own blocks",
            cp as u32,
        );
    }
    // ...and everything we drew sorts above it, in the right half.
    for &(name, cp) in HIGHBAY_ICONS {
        assert!(
            cp >= icons_lo && cp <= icons_hi,
            "{name:?} at U+{:04X} is outside HIGHBAY_ICONS_BLOCK",
            cp as u32,
        );
    }
}

/// The manifest's two invariants: sorted by name (which
/// [`highbay_codepoint`]'s binary search assumes, and which fails SILENTLY
/// when broken) and one codepoint per name.
#[test]
fn the_manifest_is_sorted_and_one_to_one() {
    let names: Vec<&str> = HIGHBAY_ICONS.iter().map(|&(n, _)| n).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "HIGHBAY_ICONS must be sorted by name");

    let mut cps: Vec<char> = HIGHBAY_ICONS.iter().map(|&(_, c)| c).collect();
    cps.sort_unstable();
    let n = cps.len();
    cps.dedup();
    assert_eq!(cps.len(), n, "two names share a codepoint");

    // Every name resolves to what the manifest says, which is the vacuity pin
    // for the crossing test below.
    for &(name, cp) in HIGHBAY_ICONS {
        assert_eq!(highbay_codepoint(name), Some(cp));
    }
}

/// **A name never crosses between the two vocabularies**, in either direction.
///
/// This is the whole reason there are two resolvers rather than one table.
/// `msymbols_codepoint("table")` answering `Some` would mean the codebase
/// believes Material publishes a `table` icon, which it does not — and
/// `highbay_codepoint("settings")` answering `Some` would mean a Material name
/// silently picked up a mark we drew. Both are the same mistake, and both are
/// what a merged resolver or a fallback between the two would introduce.
#[test]
fn a_name_never_crosses_between_the_two_vocabularies() {
    for &(name, _) in HIGHBAY_ICONS {
        assert_eq!(
            msymbols_codepoint(name),
            None,
            "{name:?} is ours — Material must not claim to publish it",
        );
    }
    for &(name, _) in MSYMBOLS_ICONS {
        assert_eq!(
            highbay_codepoint(name),
            None,
            "{name:?} is Material's — we must not claim to have drawn it",
        );
    }
    // And an unknown name is unknown to BOTH, so a typo stays a reportable
    // missing asset instead of becoming whichever block happens to have it.
    for name in ["", "tabel", "Table", "table_chart", "schema", "\u{f802}"] {
        assert_eq!(highbay_codepoint(name), None, "{name:?} must not resolve");
        assert_eq!(msymbols_codepoint(name), None, "{name:?} must not resolve");
    }
}

// ── the metric contract the font declares and the text path relies on ───

/// **The icon glyphs still have the metrics that let the TEXT path place
/// them**, read off the shipped face bytes.
///
/// `fonts/icon.py` is where the geometry is decided; this is the assertion
/// that a redesign there which forgot what the placement code assumes fails a
/// test rather than drawing the icon at the wrong size, off centre, or with
/// its top edge shorn off. Both faces, because a glyph this repo drew belongs
/// to whichever face is loaded.
#[test]
fn highbay_icon_contract_holds() {
    for (label, bytes) in [("plain", ROBOTO_REGULAR_ASCII), ("merged", ROBOTO_ASCII_MSYMBOLS)] {
        let face = ttf_parser::Face::parse(bytes, 0).expect("the shipped face parses");
        let upem = face.units_per_em() as f32;
        for &(name, ch) in HIGHBAY_ICONS {
            let gid = face
                .glyph_index(ch)
                .unwrap_or_else(|| panic!("{label}: {name:?} is not in the face"));
            let bbox = face.glyph_bounding_box(gid).expect("an icon has an outline");
            let advance = face.glyph_hor_advance(gid).expect("an icon has an advance") as f32;

            assert_eq!(
                advance, upem,
                "{label} {name:?}: an icon advances ONE EM, as the borrowed Material set \
                 does — a different advance puts our icons and theirs on different pitches \
                 in the same row",
            );
            assert_eq!(
                (bbox.x_min as f32 + bbox.x_max as f32, bbox.y_min as f32 + bbox.y_max as f32),
                (upem, upem),
                "{label} {name:?}: ink is not centred on the em square's middle — the \
                 Material set is, so an off-centre icon sits at a different optical height \
                 from its neighbours",
            );

            // The cell has to hold the ink AND `px_range/2` of field around it,
            // or the edge stops antialiasing. Ink centred half an em ABOVE the
            // baseline runs out of room at the TOP, which is the mirror image
            // of the marker block's constraint (see `fonts/icon.py`).
            let em_scale = 48.0 / (upem * LINE_BOX_RATIO);
            let baseline_row = 48.0 * 0.15 + em_scale * CAP_HEIGHT;
            let top_margin = baseline_row - bbox.y_max as f32 * em_scale;
            assert!(
                top_margin >= PX_RANGE / 2.0,
                "{label} {name:?}: only {top_margin:.2}px of cell above the ink, under the \
                 {:.1}px the distance field needs",
                PX_RANGE / 2.0,
            );
            // Horizontally the projection centres the ink, so both side
            // margins are the same one.
            let side_margin = (48.0 - (bbox.x_max - bbox.x_min) as f32 * em_scale) / 2.0;
            assert!(
                side_margin >= PX_RANGE / 2.0,
                "{label} {name:?}: only {side_margin:.2}px of cell beside the ink",
            );
        }
    }
}

/// **The icon PRIMITIVE puts the mark where the caller's box is** —
/// [`DrawList::push_icon`], which is what every icon call site outside a
/// widget tree goes through (the ZUI's toolbar, the selector's inline rename
/// affordance).
///
/// Read off the emitted instance rather than recomputed, because "centred" is
/// the whole contract: a hit region derived from the caller's rect and ink
/// drawn from a second formula is exactly the drift CLAUDE.md item 5 is about.
/// The tolerance is a texel of the 48px cell mapped down to this em, not a
/// fudge — the cell's own `x_margin` is quantized.
#[test]
fn push_icon_centres_the_mark_in_its_box_and_reports_a_miss() {
    let (shaper, atlas) = (shaper(), atlas());
    let rect_pos = [100.0f32, 40.0];
    let rect_size = [40.0f32, 40.0];
    const EM: f32 = 22.0;

    for &(name, ch) in HIGHBAY_ICONS {
        let run = shaper.shape(ch.encode_utf8(&mut [0u8; 4]));
        let mut list = libmsdf::DrawList::new();
        let pen = list
            .push_icon(&run, &atlas, rect_pos, rect_size, EM, PX_RANGE, [1.0; 4])
            .unwrap_or_else(|| panic!("{name:?} drew nothing"));
        assert_eq!(list.instances.len(), 1, "{name:?} is one instance");
        let inst = &list.instances[0];
        assert!(
            matches!(inst.kind, libmsdf::SdfKind::MsdfText { char_count: 1, .. }),
            "{name:?} is one glyph on the text path",
        );

        // The INK is centred horizontally: the pen is the ink's left edge and
        // the cell's margins are symmetric, so pen + ink_w/2 is the mark's own
        // middle.
        let e = atlas.get_glyph(run.glyphs[0].glyph_id).unwrap();
        let ink_w = (e.atlas_h as f32 - 2.0 * e.x_margin) * EM * LINE_BOX_RATIO / e.atlas_h as f32;
        let ink_mid = pen[0] + ink_w * 0.5;
        assert!(
            (ink_mid - (rect_pos[0] + rect_size[0] * 0.5)).abs() < 0.5,
            "{name:?} ink centre {ink_mid} is not the box's {}",
            rect_pos[0] + rect_size[0] * 0.5,
        );
        // ...and vertically, via the baseline the cell declares: an em box
        // centred in the rect puts the baseline half an em below its middle.
        let baseline = pen[1] + e.baseline_row * EM * LINE_BOX_RATIO / e.atlas_h as f32;
        assert!(
            (baseline - (rect_pos[1] + rect_size[1] * 0.5 + EM * 0.5)).abs() < 0.01,
            "{name:?} baseline {baseline} is not half an em below the box's middle",
        );
    }

    // A name the face cannot draw shapes to `.notdef`, and `push_icon` reports
    // the miss rather than drawing the placeholder box — an icon is a
    // developer's asset, not a user's text.
    let mut list = libmsdf::DrawList::new();
    let missing = shaper.shape("\u{F8EF}"); // inside our block, nothing drawn there
    assert_eq!(missing.notdef_count(), 1, "vacuity: that codepoint IS uncovered");
    assert_eq!(
        list.push_icon(&missing, &atlas, rect_pos, rect_size, EM, PX_RANGE, [1.0; 4]),
        None,
    );
    assert!(list.instances.is_empty(), "a missing icon draws nothing at all");
}

/// Each icon shapes, is baked, and its cell has INK — the three separate ways
/// a glyph goes missing, asked together the way `coverage.rs` asks them of
/// text. Both faces, and they must agree about the advance.
#[test]
fn every_highbay_icon_shapes_and_is_baked_with_ink() {
    let (shaper, atlas) = (shaper(), atlas());
    let merged = TextShaper::new(ROBOTO_ASCII_MSYMBOLS.to_vec()).expect("the merged face parses");
    let mut centres: Vec<(&str, usize)> = Vec::new();
    for &(name, ch) in HIGHBAY_ICONS {
        assert!(shaper.covers(ch), "the plain face has no {name:?} glyph");
        assert!(merged.covers(ch), "the merged face has no {name:?} glyph");

        let run = shaper.shape(ch.encode_utf8(&mut [0u8; 4]));
        assert_eq!(run.glyphs.len(), 1, "{name:?} is one glyph");
        assert_eq!(run.notdef_count(), 0);
        assert_eq!(
            merged.shape(ch.encode_utf8(&mut [0u8; 4])).total_advance,
            run.total_advance,
            "the faces disagree about how wide {name:?} is",
        );

        let cell = Cell::of(&atlas, name);
        let (x0, y0, x1, y1) = cell.bounds();
        // **Inside the live box, and not a speck in it.** The box is 16 x 15
        // Material grid units — 24.6 x 23.1 texels at 48px cells — and the
        // ceiling is what makes these a set with the borrowed marks: nothing
        // may reach past it, because past it the distance field is cut off.
        //
        // The FLOOR used to be 20 x 18, which said "every one of them fills the
        // box". That was true of the toolbar trio and is not true of the seven
        // the navigation rail brought (2026-09-17): those were designed
        // together in one 24px box at one stroke weight and are deliberately
        // NOT the same size as each other — the widget is 16.4 x 15.0 of that
        // box where the sparkle is 22.4 x 21.6 — so mapping them through one
        // scale, which is what keeps their stroke one weight, lands them at 15
        // to 23 texels. Stretching each to fill the box would have given the
        // set an optical size the design refuses and made the stroke a
        // different weight in every glyph.
        //
        // So the floor is now "not a speck" rather than "fills the box", and
        // what still makes them a SET is the line below: one centre.
        assert!(
            (15..=26).contains(&(x1 - x0 + 1)) && (15..=26).contains(&(y1 - y0 + 1)),
            "{name:?} inks {}x{} texels, outside the shared live box",
            x1 - x0 + 1,
            y1 - y0 + 1,
        );
        // ...and the four drawn for the TOOLBAR still fill it, which is the
        // half of the old assertion that is still a fact about a design.
        if matches!(name, "graph" | "props" | "screen" | "table") {
            assert!(
                (20..=26).contains(&(x1 - x0 + 1)) && (18..=26).contains(&(y1 - y0 + 1)),
                "{name:?} is one of the toolbar four and no longer fills the live box: {}x{}",
                x1 - x0 + 1,
                y1 - y0 + 1,
            );
        }
        assert!(
            (x0 + x1).abs_diff(cell.gs - 1) <= 1,
            "{name:?} is not centred in its cell: ink spans {x0}..{x1} of {}",
            cell.gs,
        );
        // Vertically the cell is aligned to CAP HEIGHT, not centred, so this
        // is a fact about the three glyphs agreeing rather than about the
        // projection: all of them put their ink centre on the same row, so a
        // Table and a Graph beside each other sit at one height.
        centres.push((name, y0 + y1));
    }
    let (_, first) = centres[0];
    for &(name, c) in &centres {
        assert!(
            c.abs_diff(first) <= 1,
            "{name:?} sits at a different optical height from the rest of the set",
        );
    }
}

// ── the shapes, read off the baked field ────────────────────────────────

/// **Table is a bordered grid of THREE columns under a HEAVIER header rule.**
///
/// Every clause is a decision carried over from the retired `draw_table_glyph`
/// (`fonts/icon.py` is the design now): the border, the three
/// columns, and the rule that is deliberately thicker than the frame so the
/// top row reads as a header. The weight comparison is the one that needs
/// coverage rather than thresholded ink — 3.3 texels and 2.3 texels both light
/// up three rows, and the whole point of the heavier rule is the difference
/// between them.
#[test]
fn the_baked_table_cell_is_a_bordered_grid_with_a_header() {
    let cell = Cell::of(&atlas(), "table");
    let (x0, y0, x1, y1) = cell.bounds();
    let (cx, cy) = ((x0 + x1) / 2, (y0 + y1) / 2);

    // A frame: ink all the way round, and a hollow middle.
    assert!(cell.inked(cx, y0) && cell.inked(cx, y1), "the frame has no top or bottom");
    assert!(cell.inked(x0, cy) && cell.inked(x1, cy), "the frame has no sides");

    // Rows that are inked from edge to edge are the horizontal rules: the top
    // frame, the header rule, the bottom frame — and NOTHING else, or this is
    // a grid with row lines rather than a table with a header.
    let full: Vec<usize> = (y0..=y1)
        .filter(|&y| (x0..=x1).all(|x| cell.inked(x, y)))
        .collect();
    let mut bands: Vec<(usize, usize)> = Vec::new();
    for y in full {
        match bands.last_mut() {
            Some(b) if b.1 + 1 == y => b.1 = y,
            _ => bands.push((y, y)),
        }
    }
    assert_eq!(bands.len(), 3, "expected top frame, header rule, bottom frame; got {bands:?}");

    // In between, a row crosses four strokes: the two frame sides and the two
    // column dividers. Four strokes is three columns.
    let body = (bands[1].1 + bands[2].0) / 2;
    assert_eq!(
        cell.runs(body).len(),
        4,
        "row {body} crosses {:?}, which is not two frame sides and two dividers",
        cell.runs(body),
    );

    // **The header rule is heavier than the frame**, measured through a column
    // four texels in — inside the leftmost table cell, so it crosses the top
    // frame and the rule and no divider.
    let col = x0 + 4;
    let frame_w = cell.weight_col(col, y0 - 1, bands[0].1 + 1);
    let rule_w = cell.weight_col(col, bands[1].0 - 1, bands[1].1 + 1);
    assert!(
        rule_w > frame_w * 1.25,
        "the header rule ({rule_w:.2} texels) is not meaningfully heavier than the frame \
         ({frame_w:.2}) — the top row stops reading as a header",
    );
    // ...and heavier than a column divider, which is the other half of the
    // hierarchy the design states (header 2.0 > frame 1.4 > divider 1.2).
    let divider = cell.runs(body)[1];
    let divider_w = cell.weight_row(body, divider.0 - 1, divider.1 + 1);
    assert!(
        rule_w > divider_w * 1.25,
        "the header rule ({rule_w:.2}) is not heavier than a column divider ({divider_w:.2})",
    );
}

/// **Props is three rows, and every row is a LABEL and a VALUE.**
///
/// The split is the design, and `fonts/icon.py`'s `props` says why: three equal bars
/// is the hamburger mark, and at 28px a viewer reads the silhouette rather
/// than the intent. So this asserts two runs per row with the leading one
/// SHORTER — the exact shape that stops being true if someone "simplifies" the
/// rows into single bars.
#[test]
fn the_baked_props_cell_is_three_split_rows() {
    let cell = Cell::of(&atlas(), "props");
    let (x0, y0, x1, y1) = cell.bounds();

    // Rows with any ink, grouped: exactly three bands, two clear gaps.
    let mut bands: Vec<(usize, usize)> = Vec::new();
    for y in y0..=y1 {
        if (x0..=x1).any(|x| cell.inked(x, y)) {
            match bands.last_mut() {
                Some(b) if b.1 + 1 == y => b.1 = y,
                _ => bands.push((y, y)),
            }
        }
    }
    assert_eq!(bands.len(), 3, "expected three property rows, got {bands:?}");

    for (i, &(a, b)) in bands.iter().enumerate() {
        let mid = (a + b) / 2;
        let runs = cell.runs(mid);
        assert_eq!(
            runs.len(),
            2,
            "row {i} is {:?} — a property row is a LABEL and a VALUE, not one bar",
            runs,
        );
        let (label, value) = (runs[0].1 - runs[0].0 + 1, runs[1].1 - runs[1].0 + 1);
        assert!(
            value > label * 2,
            "row {i}: label {label} texels, value {value} — too close in length to read as \
             a name/value pair rather than a broken bar",
        );
        // The rows are the same length as each other, which is what makes them
        // a sheet rather than a ragged list.
        assert!(
            runs[0].0 <= x0 + 1 && runs[1].1 + 1 >= x1,
            "row {i} spans {}..{}, short of the glyph's {x0}..{x1}",
            runs[0].0,
            runs[1].1,
        );
    }
    // The three rows are evenly pitched.
    let pitch = |i: usize| bands[i + 1].0 as i32 - bands[i].0 as i32;
    assert!((pitch(0) - pitch(1)).abs() <= 1, "the rows are not evenly spaced: {bands:?}");
}

/// **Graph is three discs joined by two edges, and the DISCS dominate.**
///
/// The proportions were settled by rendering (see `fonts/icon.py`): at lighter
/// nodes the mark collapsed into a plain chevron at 16px, because the edges
/// carried the silhouette. So the assertion is not "there are three blobs" but
/// the ratio that fixed it — a disc is several times wider than the edge that
/// leaves it — plus the clear channel down the middle that distinguishes two
/// diverging edges from one stem.
#[test]
fn the_baked_graph_cell_is_three_discs_and_two_edges() {
    let cell = Cell::of(&atlas(), "graph");
    let (x0, y0, x1, y1) = cell.bounds();
    let (cx, third) = ((x0 + x1) / 2, (y1 - y0) / 3);

    // The top of the glyph is ONE mass, centred: the parent. Measured at the
    // widest slice of the disc rather than at its first inked row, which for a
    // round feature is a two-texel cap that says nothing about where it sits.
    let top = cell.runs(cell.widest_row(y0, y0 + third));
    assert_eq!(top.len(), 1, "the top of the glyph is {top:?}, not a single parent node");
    let parent_mid = (top[0].0 + top[0].1) / 2;
    assert!(
        parent_mid.abs_diff(cx) <= 1,
        "the parent node sits at {parent_mid}, not on the glyph's centre {cx}",
    );

    // The bottom is TWO masses, one at each side: the children.
    let bottom = cell.runs(cell.widest_row(y1 - third, y1));
    assert_eq!(bottom.len(), 2, "the bottom of the glyph is {bottom:?}, not two child nodes");
    assert_eq!((bottom[0].0, bottom[1].1), (x0, x1), "the children do not reach the sides");

    // Between them, two edges and a CLEAR CHANNEL down the middle — the half
    // that separates a graph from a stem-and-blobs shape like an anchor.
    let mid = (y0 + y1) / 2;
    let edges = cell.runs(mid);
    assert_eq!(edges.len(), 2, "the middle of the glyph is {edges:?}, not two edges");
    assert!(!cell.inked(cx, mid), "the glyph's centre line is inked at row {mid}");

    // **The nodes dominate the edges.** A child disc is 500 font units across
    // and an edge crosses a row in about 125, so the ratio is around four; at
    // three the mark starts reading as a chevron with thick ends.
    let disc = (bottom[0].1 - bottom[0].0 + 1) as f32;
    let edge = (edges[0].1 - edges[0].0 + 1) as f32;
    assert!(
        disc >= edge * 3.0,
        "a node is {disc} texels across and an edge {edge} — too close for the nodes to \
         carry the silhouette at 16px",
    );
}

// ── the frame a reader has to look at ───────────────────────────────────

/// Render the trio at the sizes it is used at and write it out to be READ.
///
/// `LIBMSDF_DUMP=<dir> cargo test -p libmsdf --test highbay_icons`. Nothing is
/// asserted here: no threshold can answer "do these three read as a set", and
/// the tests above deliberately check the shapes one at a time, which is the
/// one thing that cannot catch a trio that is individually fine and mushy
/// together.
///
/// The rasteriser is `sdf_render.wgsl`'s case `8u` arithmetic, reproduced on
/// the CPU exactly as `text_fidelity.rs` does — the same cell-space mapping,
/// the same `screen_px_range`, the same alpha ramp — so what comes out is the
/// engine's own antialiasing rather than an approximation of it. A 6x
/// nearest-neighbour blow-up is written beside the 1:1 frame because a 16px
/// glyph cannot be judged at page scale.
#[test]
fn dump_the_trio() {
    let Ok(dir) = std::env::var("LIBMSDF_DUMP") else { return };
    std::fs::create_dir_all(&dir).unwrap();
    let atlas = atlas();
    let shaper = shaper();

    const SIZES: [f32; 4] = [16.0, 20.0, 28.0, 48.0];
    const PAD: f32 = 10.0;
    let col_w = SIZES.iter().cloned().fold(0.0f32, f32::max) * LINE_BOX_RATIO + PAD;
    let w = (col_w * HIGHBAY_ICONS.len() as f32 + PAD).ceil() as usize;
    let h = (SIZES.iter().map(|s| s * LINE_BOX_RATIO + PAD).sum::<f32>() + PAD).ceil() as usize;
    let mut img = vec![255u8; w * h];

    let mut top = PAD;
    for &size in &SIZES {
        let line_h = size * LINE_BOX_RATIO;
        for (i, &(_, ch)) in HIGHBAY_ICONS.iter().enumerate() {
            let run = shaper.shape(ch.encode_utf8(&mut [0u8; 4]));
            let e = *atlas.get_glyph(run.glyphs[0].glyph_id).unwrap();
            let scale = e.atlas_h as f32 / line_h; // atlas texels per screen pixel
            let spr = screen_px_range(PX_RANGE, e.atlas_h as f32, size);
            let (left, top_y) = (PAD + i as f32 * col_w, top);
            for py in 0..h {
                for px in 0..w {
                    let acx = (px as f32 + 0.5 - left) * scale;
                    let acy = (py as f32 + 0.5 - top_y) * scale;
                    if acx < 0.0 || acy < 0.0 || acx >= e.atlas_w as f32 || acy >= e.atlas_h as f32
                    {
                        continue;
                    }
                    let sd = bilinear(
                        &atlas,
                        e.atlas_x as f32 + acx + 0.5,
                        e.atlas_y as f32 + acy + 0.5,
                    );
                    let a = (spr * (sd - 0.5) + 0.5).clamp(0.0, 1.0);
                    let p = &mut img[py * w + px];
                    *p = (*p as f32 * (1.0 - a)).round() as u8;
                }
            }
        }
        top += line_h + PAD;
    }

    write_gray(&format!("{dir}/highbay-icons.png"), w, h, &img);
    const Z: usize = 6;
    let mut big = vec![0u8; w * Z * h * Z];
    for y in 0..h * Z {
        for x in 0..w * Z {
            big[y * w * Z + x] = img[(y / Z) * w + x / Z];
        }
    }
    write_gray(&format!("{dir}/highbay-icons-zoom.png"), w * Z, h * Z, &big);
    eprintln!("DUMPED {dir}/highbay-icons.png (+ -zoom)");
}

/// Bilinear tap into the atlas in texel coordinates — `textureSampleLevel`
/// with a linear sampler, as `text_fidelity.rs` reproduces it.
fn bilinear(a: &FontAtlas, u: f32, v: f32) -> f32 {
    let texel = |x: i32, y: i32| {
        let x = x.clamp(0, a.width as i32 - 1) as u32;
        let y = y.clamp(0, a.height as i32 - 1) as u32;
        let o = ((y * a.width + x) * a.channels) as usize;
        [
            a.pixel_data[o] as f32 / 255.0,
            a.pixel_data[o + 1] as f32 / 255.0,
            a.pixel_data[o + 2] as f32 / 255.0,
        ]
    };
    let (fx, fy) = (u - 0.5, v - 0.5);
    let (x0, y0) = (fx.floor() as i32, fy.floor() as i32);
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let mut c = [0.0f32; 3];
    for (i, ci) in c.iter_mut().enumerate() {
        let t = texel(x0, y0)[i] * (1.0 - tx) + texel(x0 + 1, y0)[i] * tx;
        let b = texel(x0, y0 + 1)[i] * (1.0 - tx) + texel(x0 + 1, y0 + 1)[i] * tx;
        *ci = t * (1.0 - ty) + b * ty;
    }
    c[0].min(c[1]).max(c[0].max(c[1]).min(c[2]))
}

fn write_gray(path: &str, w: usize, h: usize, data: &[u8]) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(file, w as u32, h as u32);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(data).unwrap();
}
