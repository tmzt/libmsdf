//! **The icon manifest is stated twice, and this is what makes the two agree.**
//!
//! `fonts/icon.py`'s `ICONS` list decides which glyphs are drawn and at which
//! codepoint - it is where the geometry is authored and the face is baked.
//! `src/font/mod.rs`'s [`HIGHBAY_ICONS`] restates the same `(name, codepoint)`
//! pairs so Rust can resolve a name. Nothing compiled either against the other.
//!
//! # Why the duplication is not a mistake to remove
//!
//! `icon.py` records the reason the codepoints are written down rather than
//! derived: they used to be `ICON_BASE + index`, "which is only correct while
//! the list is never inserted into - adding `screen` alphabetically would have
//! renumbered `table` from U+F802 to U+F803 - silently, in a face that had
//! already shipped". So both sides hold a deliberate, hand-maintained fact.
//! What was missing was anything that notices when they stop matching.
//!
//! # What drift would look like without this
//!
//! A codepoint present in Rust and absent from the bake resolves to a glyph the
//! atlas never drew, and the atlas renders an uncovered cell as a **visible
//! tofu box** with no finding anywhere - the same silent class the `.hbdef`
//! artifacts had before `hb-pack --mode compile` gave them a producer. A name
//! added to `icon.py` and not to Rust is quieter still: `highbay_codepoint`
//! answers `None`, which callers are told to report as a MISSING ASSET, so a
//! forgotten line is indistinguishable from an unshipped one.
//!
//! # Why a test rather than codegen
//!
//! Four entries, changed about once a year, in two files of one crate. Codegen
//! buys the same guarantee and costs a build script that parses Python; the
//! defect here is not that a human maintains two lists, it is that nothing
//! checked them. If the set grows past a handful, generate `HIGHBAY_ICONS` from
//! `ICONS` and delete this file - the check becomes structural at that point,
//! which is strictly better.

use std::collections::BTreeMap;
use std::path::Path;

const ATLAS_FIXTURE: &[u8] = include_bytes!("fixtures/roboto-ascii-48.atlas");

use libmsdf::{
    FontAtlas, HIGHBAY_ICONS, MARKERS, MARKER_ARROW, MSYMBOLS_ICONS, ROBOTO_ASCII_MSYMBOLS,
    ROBOTO_REGULAR_ASCII, TextShaper, highbay_codepoint, msymbols_codepoint,
};

/// The `ICONS = [...]` list out of `fonts/icon.py`, as `(name, codepoint)`.
///
/// Parsed rather than imported because one side is Python. The parse is
/// deliberately narrow - it reads the bracketed block after `ICONS = [` and
/// takes the first two fields of each tuple - so a change to the file's SHAPE
/// fails loudly here instead of silently matching nothing (a codepoint that is
/// not a bare `0x....` panics by name rather than being skipped). The
/// `entries_were_actually_found` assertion below is the guard against a parse
/// that quietly reads zero.
fn baker_manifest() -> BTreeMap<String, u32> {
    icons_list("fonts/icon.py")
}

/// The `ICONS = [("name", 0x....), ...]` list out of one of `fonts/`'s bakers.
///
/// Two scripts declare a manifest in exactly this shape - `icon.py` for the
/// glyphs this repo DRAWS and `msymbols.py` for the ones it BORROWS - and both
/// are restated in Rust, so both need the same check. The parse stays narrow on
/// purpose (see above); each caller has its own `*_were_actually_found` guard.
fn icons_list(rel: &str) -> BTreeMap<String, u32> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    let start = text
        .find("\nICONS = [")
        .unwrap_or_else(|| panic!("no `ICONS = [` in {} - the baker's manifest moved or was renamed", path.display()));
    let body = &text[start..];
    let end = body
        .find("\n]")
        .unwrap_or_else(|| panic!("`ICONS = [` in {} is never closed", path.display()));

    let mut out = BTreeMap::new();
    for line in body[..end].lines() {
        let line = line.trim();
        let Some(inner) = line.strip_prefix('(') else {
            continue;
        };
        let mut fields = inner.split(',');
        let (Some(name), Some(code)) = (fields.next(), fields.next()) else {
            continue;
        };
        let name = name.trim().trim_matches('"').trim_matches('\'');
        // `icon.py`'s tuples carry a third field (the draw function), so the
        // codepoint is bare there; `msymbols.py`'s are pairs, so the closing
        // paren rides along on the last field. One `trim_end_matches` reads
        // both, and anything else still reaches the `0x....` panic below.
        let code = code.trim().trim_end_matches(')').trim();
        let Some(hex) = code.strip_prefix("0x").or_else(|| code.strip_prefix("0X")) else {
            panic!("{name}'s codepoint in icon.py is `{code}`, not the `0x....` this parse reads");
        };
        let value = u32::from_str_radix(hex, 16)
            .unwrap_or_else(|e| panic!("{name}'s codepoint `{code}` is not hex: {e}"));
        out.insert(name.to_string(), value);
    }
    out
}

#[test]
fn entries_were_actually_found() {
    // Without this, every assertion below passes vacuously the day the parse
    // stops matching - which is the failure mode a hand-rolled parse has.
    let baked = baker_manifest();
    assert!(
        baked.len() >= 4,
        "parsed only {} entries from icon.py - the parse broke, and every other \
         test in this file would have passed by reading nothing",
        baked.len()
    );
}

#[test]
fn every_icon_rust_resolves_is_one_the_baker_draws() {
    let baked = baker_manifest();
    let mut wrong = Vec::new();
    for &(name, ch) in HIGHBAY_ICONS {
        match baked.get(name) {
            None => wrong.push(format!(
                "`{name}` is in HIGHBAY_ICONS and NOT in icon.py's ICONS - Rust \
                 resolves it to U+{:04X}, a cell the face never drew, and the \
                 atlas renders an uncovered cell as a visible tofu box",
                ch as u32
            )),
            Some(&code) if code != ch as u32 => wrong.push(format!(
                "`{name}` is U+{:04X} in Rust and U+{code:04X} in icon.py - one \
                 of the two resolves to a glyph the other never drew",
                ch as u32
            )),
            Some(_) => {}
        }
    }
    assert!(
        wrong.is_empty(),
        "{} icon(s) disagree between the baker and Rust:\n  {}\n\n\
         `fonts/icon.py`'s ICONS decides what is drawn and where; \
         `src/font/mod.rs`'s HIGHBAY_ICONS restates it so a name can be \
         resolved. They are two hand-maintained statements of one fact - fix \
         whichever is wrong, and note that a codepoint already shipped must \
         not be renumbered (icon.py says why).",
        wrong.len(),
        wrong.join("\n  ")
    );
}

#[test]
fn every_icon_the_baker_draws_is_one_rust_can_resolve() {
    let baked = baker_manifest();
    let missing: Vec<String> = baked
        .iter()
        .filter(|(name, _)| highbay_codepoint(name).is_none())
        .map(|(name, code)| format!("`{name}` (U+{code:04X})"))
        .collect();
    assert!(
        missing.is_empty(),
        "{} icon(s) are baked into the face and unreachable from Rust: {}\n\n\
         `highbay_codepoint` answers `None` for them, which callers are told to \
         report as a MISSING ASSET - so a line forgotten in HIGHBAY_ICONS is \
         indistinguishable from a glyph nobody drew.",
        missing.len(),
        missing.join(", ")
    );
}

/// The `MARKERS = { 0x....: (...) }` dict out of `fonts/marker.py`, as
/// codepoints. Keyed by codepoint there rather than by name, because a marker
/// is not a name a developer types - `MARKER_ARROW`'s doc says so: it is
/// "geometry the renderer reaches for itself".
fn baked_markers() -> Vec<u32> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts/marker.py");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let start = text.find("\nMARKERS = {").unwrap_or_else(|| {
        panic!("no `MARKERS = {{` in {} - the baker's dict moved or was renamed", path.display())
    });
    let body = &text[start..];
    let end = body.find("\n}").unwrap_or_else(|| panic!("`MARKERS = {{` is never closed"));
    let mut out = Vec::new();
    for line in body[..end].lines() {
        let line = line.trim();
        let Some(hex) = line.strip_prefix("0x").or_else(|| line.strip_prefix("0X")) else {
            continue;
        };
        let Some((digits, _)) = hex.split_once(':') else { continue };
        if let Ok(v) = u32::from_str_radix(digits.trim(), 16) {
            out.push(v);
        }
    }
    out
}

#[test]
fn markers_were_actually_found() {
    let baked = baked_markers();
    assert!(
        !baked.is_empty(),
        "parsed no markers from marker.py - the parse broke, and the assertion \
         below would have passed by reading nothing"
    );
}

/// **Every marker Rust names is one the baker draws**, and it sits in the block.
///
/// `tests/marker.rs` already checks `MARKER_ARROW` against the SHIPPED ATLAS,
/// which is the stronger artifact-level check. This is the half that one cannot
/// make: a `marker.py` edited and not yet re-baked leaves the atlas agreeing
/// with Rust while the SOURCE disagrees with both, and the next bake would
/// silently move a glyph.
#[test]
fn every_marker_rust_names_is_one_the_baker_draws() {
    let baked = baked_markers();
    let (lo, hi) = MARKERS;
    let arrow = MARKER_ARROW as u32;
    assert!(
        baked.contains(&arrow),
        "MARKER_ARROW is U+{arrow:04X} and marker.py bakes {:?} - Rust names a \
         cell the face never drew, which the atlas renders as a visible tofu box",
        baked.iter().map(|c| format!("U+{c:04X}")).collect::<Vec<_>>()
    );
    for code in baked {
        assert!(
            (lo as u32..=hi as u32).contains(&code),
            "marker.py bakes U+{code:04X}, outside MARKERS \
             (U+{:04X}..=U+{:04X}) - below it is the icon block, and a marker \
             outside its own range collides with a name an app can ask for",
            lo as u32,
            hi as u32
        );
    }
}

/// **What renders is the ATLAS, and reaching it takes two hops.**
///
/// The checks above compare Rust to what `icon.py`/`marker.py` INTEND. This
/// asks what actually ships, and it must follow the whole path, because the
/// codepoint is not the atlas offset:
///
/// 1. **codepoint -> glyph id**, through the face's cmap. A miss here means the
///    bake never ran, or the face was swapped for one without this repo's
///    blocks.
/// 2. **glyph id -> baked cell**, through the atlas. A miss HERE is the one
///    that draws a tofu box, and the first hop cannot see it: the atlas is a
///    BOUNDED subset - `FontAtlasBuilder::build` refuses a build over
///    `atlas_capacity()` - so a glyph can sit in the face and never reach a
///    cell.
///
/// Checking only the cmap would claim to catch the tofu box and miss exactly
/// the case that causes it.
///
/// `tests/marker.rs` asserts more for `MARKER_ARROW` - it rasterises the cell
/// and finds INK, which catches a cell that exists and is blank. This is the
/// cheap total version over every name.
#[test]
fn every_name_rust_resolves_reaches_a_baked_cell() {
    let face = TextShaper::new(ROBOTO_REGULAR_ASCII.to_vec()).expect("the shipped face parses");
    let atlas = FontAtlas::from_bytes(ATLAS_FIXTURE).expect("the shipped atlas parses");

    let mut broken = Vec::new();
    let mut check = |label: String, ch: char| match face.glyph_id_for_char(ch) {
        None => broken.push(format!(
            "{label} (U+{:04X}) has NO GLYPH in the face - the bake did not run, \
             or the face was replaced",
            ch as u32
        )),
        Some(gid) if atlas.get_glyph(gid).is_none() => broken.push(format!(
            "{label} (U+{:04X}) is glyph {gid} in the face and has NO CELL in the \
             atlas - this is the tofu box, and a cmap check cannot see it",
            ch as u32
        )),
        Some(_) => {}
    };

    for &(name, ch) in HIGHBAY_ICONS {
        check(format!("`{name}`"), ch);
    }
    check("MARKER_ARROW".to_string(), MARKER_ARROW);

    assert!(
        broken.is_empty(),
        "{} name(s) Rust resolves do not reach a baked cell:\n  {}\n\n\
         Every call site drawing one gets a tofu box and no finding. Re-bake \
         (`fonts/icon.py`, `fonts/marker.py` - their headers carry the \
         invocation), and if the atlas refused the glyph, `atlas_capacity()` is \
         the ceiling it hit.",
        broken.len(),
        broken.join("\n  ")
    );
}

// ── the BORROWED set: the same duplication, one file over ───────────────

/// The `ICONS = [...]` list out of `fonts/msymbols.py`, as `(name, codepoint)`.
///
/// The borrowed manifest is stated twice for the same reason the drawn one is,
/// and the codepoints are hand-maintained for a sharper reason: they are
/// Material's DECLARED ones, not whichever alias the source face answers to.
/// `check` is drawn at both `U+E5CA` and `U+E668` and only `U+E668` is
/// published; `edit` answers to five and only `U+F097` is. So neither side can
/// derive its value from the font, and nothing compiled either against the
/// other until this.
fn msymbols_manifest() -> BTreeMap<String, u32> {
    icons_list("fonts/msymbols.py")
}

#[test]
fn msymbols_entries_were_actually_found() {
    let baked = msymbols_manifest();
    assert!(
        baked.len() >= 13,
        "parsed only {} entries from msymbols.py - the parse broke, and every \
         other borrowed-set test in this file would have passed by reading nothing",
        baked.len()
    );
}

/// **The merge script and Rust name the same icons at the same codepoints.**
///
/// Both directions, because the two failures are different and both are quiet.
/// A name in Rust and not in `msymbols.py` resolves to a codepoint the merged
/// face never received a glyph for, and the atlas draws an uncovered cell as a
/// visible tofu box with no finding. A name in `msymbols.py` and not in Rust is
/// quieter still: `msymbols_codepoint` answers `None`, which callers are told to
/// report as a MISSING ASSET, so a forgotten line looks exactly like an icon
/// nobody merged.
#[test]
fn the_borrowed_manifest_and_the_merge_script_agree() {
    let baked = msymbols_manifest();
    let mut wrong = Vec::new();
    for &(name, ch) in MSYMBOLS_ICONS {
        match baked.get(name) {
            None => wrong.push(format!(
                "`{name}` is in MSYMBOLS_ICONS and NOT in msymbols.py's ICONS - Rust \
                 resolves it to U+{:04X}, which the merge never gave the face a glyph for",
                ch as u32
            )),
            Some(&code) if code != ch as u32 => wrong.push(format!(
                "`{name}` is U+{:04X} in Rust and U+{code:04X} in msymbols.py - one of \
                 the two is not the codepoint Material declares",
                ch as u32
            )),
            Some(_) => {}
        }
    }
    for (name, code) in &baked {
        if msymbols_codepoint(name).is_none() {
            wrong.push(format!(
                "`{name}` (U+{code:04X}) is merged into the face and unreachable from \
                 Rust - `msymbols_codepoint` answers `None`, which reads as a missing asset"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} borrowed icon(s) disagree between msymbols.py and Rust:\n  {}\n\n\
         `fonts/msymbols.py`'s ICONS decides what is merged and where; \
         `src/font/mod.rs`'s MSYMBOLS_ICONS restates it so a name can be resolved. \
         Re-bake with the invocation in msymbols.py's header after fixing whichever \
         is wrong.",
        wrong.len(),
        wrong.join("\n  ")
    );
}

/// **A borrowed name reaches a glyph WITH AN OUTLINE**, in the face that ships
/// it.
///
/// `covers` - the gate every icon call site is told to ask first - answers from
/// the `cmap` alone, so it cannot tell a merged glyph from a `cmap` entry
/// pointing at an empty one. That is not hypothetical for this set: the merge
/// takes outlines from an upstream face by NAME and writes them under a
/// codepoint, and a mapping written without its glyph would pass `covers`,
/// shape without a notdef, and draw nothing at all - worse than the tofu box,
/// because nothing is visible to notice.
///
/// The advance is checked with it: the borrowed set advances one em, which is
/// what puts a Material icon and one of ours on the same pitch in a row
/// (`highbay_icon_contract_holds` asserts our half against the same number).
#[test]
fn every_borrowed_name_has_an_outline_and_a_one_em_advance() {
    let face = ttf_parser::Face::parse(ROBOTO_ASCII_MSYMBOLS, 0).expect("the merged face parses");
    let upem = face.units_per_em();
    let mut broken = Vec::new();
    for &(name, ch) in MSYMBOLS_ICONS {
        let Some(gid) = face.glyph_index(ch) else {
            broken.push(format!("`{name}` (U+{:04X}) has NO GLYPH in the merged face", ch as u32));
            continue;
        };
        match face.glyph_bounding_box(gid) {
            None => broken.push(format!(
                "`{name}` (U+{:04X}) is glyph {} and has NO OUTLINE - it would \
                 shape cleanly and draw nothing",
                ch as u32,
                gid.0
            )),
            Some(bb) if bb.x_max <= bb.x_min || bb.y_max <= bb.y_min => broken.push(format!(
                "`{name}` (U+{:04X}) has an empty bounding box {bb:?}",
                ch as u32
            )),
            Some(_) => {}
        }
        match face.glyph_hor_advance(gid) {
            Some(a) if a == upem => {}
            other => broken.push(format!(
                "`{name}` (U+{:04X}) advances {other:?}, not the one em ({upem}) the \
                 set is placed on",
                ch as u32
            )),
        }
    }
    assert!(broken.is_empty(), "{} borrowed icon(s):\n  {}", broken.len(), broken.join("\n  "));

    // And the PLAIN face carries none of them - that is what "merged" means,
    // and it is the vacuity pin for the loop above having read the right face.
    let plain = ttf_parser::Face::parse(ROBOTO_REGULAR_ASCII, 0).expect("the plain face parses");
    for &(name, ch) in MSYMBOLS_ICONS {
        assert!(
            plain.glyph_index(ch).is_none(),
            "`{name}` (U+{:04X}) is in the PLAIN face - only the merged one borrows",
            ch as u32
        );
    }
}


// ── and what the bake actually DRAWS ────────────────────────────────────

/// **Render the borrowed set out to be READ**, at the sizes it is used at.
///
/// ```text
/// LIBMSDF_DUMP=<dir> CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm \
///   cargo test -p libmsdf --features cpu-bake --test icons_agree_with_the_baker
/// ```
///
/// Nothing is asserted: the property here is "does `check` read as a bare tick
/// and do the four chevrons point where their names say", and no threshold
/// answers that. The tests above can tell that a glyph exists, has an outline
/// and lands in a cell; a glyph that is present, inked and pointing the WRONG
/// WAY passes every one of them.
///
/// It bakes rather than reading a fixture because `tests/fixtures` holds the
/// PLAIN face's atlas and the borrowed half is only in the merged one - hence
/// the `cpu-bake` gate. The rasteriser is `sdf_render.wgsl`'s case `8u`
/// arithmetic on the CPU, exactly as `highbay_icons.rs`'s `dump_the_trio`
/// reproduces it, and a 6x nearest-neighbour blow-up is written beside the 1:1
/// frame because a 16px glyph cannot be judged at page scale.
#[cfg(all(feature = "cpu-bake", not(target_arch = "wasm32")))]
#[test]
fn dump_the_borrowed_set() {
    use libmsdf::drawlist::{LINE_BOX_RATIO, screen_px_range};
    use libmsdf::font::FontAtlasBuilder;

    let Ok(dir) = std::env::var("LIBMSDF_DUMP") else { return };
    std::fs::create_dir_all(&dir).unwrap();

    const PX_RANGE: f32 = 6.0;
    let mut b = FontAtlasBuilder::new(ROBOTO_ASCII_MSYMBOLS.to_vec(), 48, PX_RANGE as f64);
    b.add_shipped_coverage();
    let atlas = b.build().expect("the merged coverage bakes");
    let shaper = TextShaper::new(ROBOTO_ASCII_MSYMBOLS.to_vec()).expect("the merged face parses");

    const SIZES: [f32; 3] = [16.0, 28.0, 48.0];
    const PAD: f32 = 10.0;
    let col_w = SIZES.iter().cloned().fold(0.0f32, f32::max) * LINE_BOX_RATIO + PAD;
    let w = (col_w * MSYMBOLS_ICONS.len() as f32 + PAD).ceil() as usize;
    let h = (SIZES.iter().map(|s| s * LINE_BOX_RATIO + PAD).sum::<f32>() + PAD).ceil() as usize;
    let mut img = vec![255u8; w * h];

    let mut top = PAD;
    for &size in &SIZES {
        let line_h = size * LINE_BOX_RATIO;
        for (i, &(_, ch)) in MSYMBOLS_ICONS.iter().enumerate() {
            let run = shaper.shape(ch.encode_utf8(&mut [0u8; 4]));
            let e = *atlas.get_glyph(run.glyphs[0].glyph_id).expect("a borrowed icon has a cell");
            let scale = e.atlas_h as f32 / line_h;
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
                    let sd = bilinear(&atlas, e.atlas_x as f32 + acx + 0.5, e.atlas_y as f32 + acy + 0.5);
                    let a = (spr * (sd - 0.5) + 0.5).clamp(0.0, 1.0);
                    let p = &mut img[py * w + px];
                    *p = (*p as f32 * (1.0 - a)).round() as u8;
                }
            }
        }
        top += line_h + PAD;
    }

    write_gray(&format!("{dir}/msymbols-icons.png"), w, h, &img);
    const Z: usize = 6;
    let mut big = vec![0u8; w * Z * h * Z];
    for y in 0..h * Z {
        for x in 0..w * Z {
            big[y * w * Z + x] = img[(y / Z) * w + x / Z];
        }
    }
    write_gray(&format!("{dir}/msymbols-icons-zoom.png"), w * Z, h * Z, &big);
    eprintln!(
        "DUMPED {dir}/msymbols-icons.png (+ -zoom); columns are {}",
        MSYMBOLS_ICONS.iter().map(|&(n, _)| n).collect::<Vec<_>>().join(" ")
    );
}

/// Bilinear tap into the atlas in texel coordinates - `textureSampleLevel` with
/// a linear sampler, as `highbay_icons.rs` and `text_fidelity.rs` reproduce it.
#[cfg(all(feature = "cpu-bake", not(target_arch = "wasm32")))]
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

#[cfg(all(feature = "cpu-bake", not(target_arch = "wasm32")))]
fn write_gray(path: &str, w: usize, h: usize, data: &[u8]) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(file, w as u32, h as u32);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(data).unwrap();
}
