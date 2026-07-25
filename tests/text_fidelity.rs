//! Small-text MSDF fidelity: the properties that make a 12px `t` keep its
//! crossbar. Pure CPU — no GPU adapter, no `cpu-bake` (msdfgen) needed; it
//! reads the checked-in baked atlas and reproduces the shader's sampling.
//!
//! These guard a bug that was invisible to every other test: the atlas baked
//! into a 1-bit mask (msdfgen's `Framing::range` is in SHAPE units, so passing
//! the atlas-texel `px_range` straight through collapsed the field to ~0.07
//! texels), and the render shader used that atlas-space range as if it were a
//! screen-space one. Text still *drew*, and still read — only the hairlines
//! silently dropped, so "created" rendered as "creale".

use libmsdf::drawlist::{LINE_BOX_RATIO, screen_px_range};
use libmsdf::font::{FontAtlas, TextShaper};

const ATLAS_FIXTURE: &[u8] = include_bytes!("fixtures/roboto-ascii-48.atlas");
/// Distance range the fixture was baked with, in atlas texels (`bake_atlas`
/// default — keep in step with `highbay/src/scene.rs`'s `PX_RANGE`).
const PX_RANGE: f32 = 6.0;
/// The chat sheet's CLI-row size — the smallest *prose* style that ships.
const CLI_FONT_SIZE: f32 = 12.0;

fn atlas() -> FontAtlas {
    FontAtlas::from_bytes(ATLAS_FIXTURE).expect("fixture atlas parses")
}

fn median(r: f32, g: f32, b: f32) -> f32 {
    r.min(g).max(r.max(g).min(b))
}

/// Bilinear tap into the atlas, in texel coordinates (texel centres at
/// integer + 0.5) — what `textureSampleLevel` with a linear sampler does.
fn sample(a: &FontAtlas, u: f32, v: f32) -> f32 {
    let texel = |x: i32, y: i32| {
        let x = x.clamp(0, a.width as i32 - 1) as usize;
        let y = y.clamp(0, a.height as i32 - 1) as usize;
        let o = (y * a.width as usize + x) * a.channels as usize;
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
        let top = texel(x0, y0)[i] * (1.0 - tx) + texel(x0 + 1, y0)[i] * tx;
        let bot = texel(x0, y0 + 1)[i] * (1.0 - tx) + texel(x0 + 1, y0 + 1)[i] * tx;
        *ci = top * (1.0 - ty) + bot * ty;
    }
    median(c[0], c[1], c[2])
}

/// One column of glyph coverage, exactly as `sdf_render.wgsl` case `8u`
/// computes it: map the screen pixel into the glyph's atlas cell, take the
/// median of the MSDF channels, and convert the signed distance to alpha
/// through the SCREEN-space distance range.
///
/// `cell_col` picks which atlas column of the cell to walk down; `line_top` is
/// the sub-pixel position of the run's line box.
fn alpha_column(a: &FontAtlas, gid: u16, cell_col: f32, font_size: f32, line_top: f32) -> Vec<f32> {
    let e = a.get_glyph(gid).expect("glyph is in the atlas");
    let line_h = font_size * LINE_BOX_RATIO;
    let scale = e.atlas_h as f32 / line_h; // atlas texels per screen pixel
    let spr = screen_px_range(PX_RANGE, e.atlas_h as f32, font_size);
    (0..line_h.ceil() as i32 + 1)
        .map(|j| {
            let acy = (j as f32 + 0.5 - line_top) * scale;
            if !(0.0..e.atlas_h as f32).contains(&acy) {
                return 0.0;
            }
            let sd = sample(
                a,
                e.atlas_x as f32 + cell_col + 0.5,
                e.atlas_y as f32 + acy + 0.5,
            );
            (spr * (sd - 0.5) + 0.5).clamp(0.0, 1.0)
        })
        .collect()
}

/// The atlas must hold a real distance ramp, not a 1-bit mask: walking down a
/// column through the `t` crossbar, consecutive texels must differ by roughly
/// one texel of distance (`1/px_range` in normalized units), never jumping
/// from "fully outside" to "fully inside".
#[test]
fn baked_field_is_a_usable_distance_ramp() {
    let a = atlas();
    let sh = TextShaper::new(libmsdf::ROBOTO_REGULAR_ASCII.to_vec()).unwrap();
    let gid = sh.glyph_id_for_char('t').unwrap();
    let e = a.get_glyph(gid).unwrap();

    // Column 20 of the 48px cell sits on the left flank of the crossbar,
    // clear of the stem; rows 10..20 cross it.
    let col: Vec<f32> = (10..20)
        .map(|row| sample(&a, e.atlas_x as f32 + 20.5, (e.atlas_y + row) as f32 + 0.5))
        .collect();

    let steps: Vec<f32> = col.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
    let biggest = steps.iter().cloned().fold(0.0f32, f32::max);
    // One texel of travel is 1/px_range in normalized units; allow 2× for the
    // corner where two edges meet. A 1-bit mask steps by ~1.0 here.
    assert!(
        biggest < 2.0 / PX_RANGE,
        "field steps by {biggest:.3} between adjacent texels — that is a mask, \
         not a distance field (column: {col:?})"
    );
    // ...and it must actually resolve the crossbar: some texel inside, some out.
    assert!(col.iter().any(|&v| v > 0.5), "crossbar interior missing: {col:?}");
    assert!(col.iter().any(|&v| v < 0.5), "crossbar surroundings missing: {col:?}");
}

/// The regression itself: at the chat sheet's 12px CLI-row size the `t`
/// crossbar is 0.069em ≈ 0.83 screen px — thinner than a pixel. Analytic
/// coverage must therefore hand it to *some* row at *every* sub-pixel phase;
/// with a mask atlas (or an atlas-space alpha ramp) the phases where no pixel
/// centre lands inside it render nothing at all, which is what turned
/// "created" into "creale".
#[test]
fn thin_crossbar_survives_every_subpixel_phase() {
    let a = atlas();
    let sh = TextShaper::new(libmsdf::ROBOTO_REGULAR_ASCII.to_vec()).unwrap();
    let gid = sh.glyph_id_for_char('t').unwrap();
    let e = a.get_glyph(gid).unwrap();
    let line_h = CLI_FONT_SIZE * LINE_BOX_RATIO;
    let scale = e.atlas_h as f32 / line_h;

    // Where the crossbar sits, in screen pixels below the line box top: atlas
    // rows ~13.5..16.0 of the 48px cell.
    let bar_top = 13.45 / scale;
    let bar_bottom = 15.99 / scale;
    let thickness = bar_bottom - bar_top;
    assert!(
        (0.6..1.0).contains(&thickness),
        "expected a sub-pixel crossbar at {CLI_FONT_SIZE}px, got {thickness:.2}px"
    );

    for step in 0..16 {
        let phase = step as f32 / 16.0;
        let col = alpha_column(&a, gid, 20.0, CLI_FONT_SIZE, phase);
        // Ink landing on the crossbar's rows (its centre ±1 pixel).
        let centre = phase + 0.5 * (bar_top + bar_bottom);
        let ink: f32 = col
            .iter()
            .enumerate()
            .filter(|(j, _)| ((*j as f32 + 0.5) - centre).abs() < 1.6)
            .map(|(_, v)| v)
            .sum();
        assert!(
            ink > 0.6 * thickness,
            "phase {phase:.3}: crossbar coverage {ink:.3} — expected ≈{thickness:.2} \
             (the stroke's true area). Column: {col:?}"
        );
    }
}

/// Coverage: every glyph the shaper can emit for printable ASCII — ligature
/// substitutions included — must be in the atlas. A glyph that is missing
/// falls back to table index 0 and draws NOTHING, so `fifty` silently renders
/// as `fty`; only shaping the pairs finds it, because `fi` is a GSUB `liga`
/// substitution that never appears when the alphabet is shaped one char at a
/// time.
#[test]
fn every_glyph_the_shaper_emits_for_ascii_is_baked() {
    let a = atlas();
    let sh = TextShaper::new(libmsdf::ROBOTO_REGULAR_ASCII.to_vec()).unwrap();
    let printable: Vec<char> = (0x20u8..=0x7e).map(|b| b as char).collect();

    let mut missing: Vec<(String, u16)> = Vec::new();
    let probe = |s: String, missing: &mut Vec<(String, u16)>| {
        for g in sh.shape(&s).glyphs {
            if a.glyph_table_index(g.glyph_id).is_none() {
                missing.push((s.clone(), g.glyph_id));
            }
        }
    };
    for &x in &printable {
        for &y in &printable {
            probe([x, y].iter().collect(), &mut missing);
        }
    }
    // Three-glyph ligatures (ffi / ffl) need a triple.
    for &x in &printable {
        for &y in &printable {
            probe(['f', x, y].iter().collect(), &mut missing);
        }
    }
    missing.sort();
    missing.dedup_by_key(|(_, gid)| *gid);
    assert!(
        missing.is_empty(),
        "shaped ASCII produces glyphs the atlas has no cell for (they render \
         blank): {missing:?}"
    );
}

/// The documented antialiasing floor, and the shipped styles that must clear
/// it: the ZUI's smallest text is the 8px nav-graph label at minimum zoom.
#[test]
fn antialiasing_floor_covers_the_smallest_shipped_style() {
    let a = atlas();
    assert_eq!(a.cell_px(), Some(48.0));
    let floor = a.min_antialiased_font_size(PX_RANGE);
    assert!(
        (floor - 48.0 / (1.3 * PX_RANGE)).abs() < 1e-4,
        "floor formula changed: {floor}"
    );
    assert!(floor <= 8.0, "8px graph labels are below the atlas floor ({floor:.2}px)");
    // And the floor is exactly where the screen-space ramp bottoms out.
    assert!((screen_px_range(PX_RANGE, 48.0, floor) - 1.0).abs() < 1e-4);
    assert!(screen_px_range(PX_RANGE, 48.0, CLI_FONT_SIZE) > 1.0);
}
