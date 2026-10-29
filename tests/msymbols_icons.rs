//! **The core set of Material Symbols in the merged Roboto face**
//! (`docs/design/20261029_01_MATERIAL_SYMBOLS_CORE_SET_PLAN.md`, section 5).
//!
//! `MSYMBOLS_ICONS` is the data file `fonts/msymbols-icons.txt`, line for
//! line; every icon is in the face and in both merged atlases; and every
//! glyph those atlases carry lies inside its cell, with a texel to spare.
//! That last check is the one the shipped-coverage bake report does not make
//! (section 3.5): the line-box cell clipped 25 of the 71 icons at the top.

#![cfg(all(feature = "font-apache-roboto-msymbols", feature = "shaper"))]

use libmsdf::font::{BundledAtlas, CellBox};
use libmsdf::{
    AtlasShaper, FontAtlas, GlyphStyle, MSYMBOLS_ICONS, ROBOTO_ASCII_MSYMBOLS, highbay_codepoint,
    msymbols_codepoint,
};

const MANIFEST: &str = include_str!("../fonts/msymbols-icons.txt");
const GROUPS: [&str; 5] = ["navigation", "actions", "media", "status", "things"];

/// `(name, codepoint, group)` of every non-comment line of the manifest file.
fn manifest() -> Vec<(&'static str, char, &'static str)> {
    MANIFEST
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let parts: Vec<&str> = l.split_whitespace().collect();
            assert_eq!(parts.len(), 3, "malformed line `{l}`");
            let hex = parts[1].strip_prefix("U+").expect("U+XXXX");
            let cp = char::from_u32(u32::from_str_radix(hex, 16).unwrap()).unwrap();
            (parts[0], cp, parts[2])
        })
        .collect()
}

#[test]
fn the_constant_is_the_manifest_file() {
    let file: Vec<(&str, char)> = manifest().iter().map(|&(n, c, _)| (n, c)).collect();
    assert_eq!(MSYMBOLS_ICONS, file.as_slice());
}

#[test]
fn seventy_one_sorted_one_to_one_below_the_owned_blocks() {
    let m = manifest();
    assert_eq!(m.len(), 71);
    assert_eq!(MSYMBOLS_ICONS.len(), 71);
    for w in m.windows(2) {
        assert!(
            w[0].0.as_bytes() < w[1].0.as_bytes(),
            "`{}` is not before `{}`",
            w[0].0,
            w[1].0
        );
    }
    let mut cps: Vec<char> = m.iter().map(|&(_, c, _)| c).collect();
    cps.sort_unstable();
    cps.dedup();
    assert_eq!(cps.len(), 71, "a codepoint repeats");
    for &(name, cp, group) in &m {
        assert!(
            ('\u{E000}'..='\u{F7FF}').contains(&cp),
            "{name} U+{:04X} is outside U+E000..U+F7FF",
            cp as u32
        );
        assert!(GROUPS.contains(&group), "{name}: unknown group `{group}`");
    }
}

/// Section 3.2, written out here rather than read from the file, so the set
/// is stated twice and a slip in either is caught.
const CORE_SET: &[(&str, u32)] = &[
    // navigation
    ("arrow_back", 0xE5C4),
    ("arrow_forward", 0xE5C8),
    ("arrow_upward", 0xE5D8),
    ("arrow_downward", 0xE5DB),
    ("close", 0xE5CD),
    ("menu", 0xE5D2),
    ("more_vert", 0xE5D4),
    ("more_horiz", 0xE5D3),
    ("expand_more", 0xE5CF),
    ("expand_less", 0xE5CE),
    ("chevron_left", 0xE5CB),
    ("chevron_right", 0xE5CC),
    ("home", 0xE9B2),
    ("open_in_new", 0xE89E),
    // actions
    ("add", 0xE145),
    ("remove", 0xE15B),
    ("check", 0xE668),
    ("done_all", 0xE877),
    ("edit", 0xF097),
    ("delete", 0xE92E),
    ("content_copy", 0xE14D),
    ("content_paste", 0xE14F),
    ("share", 0xE80D),
    ("search", 0xEF7A),
    ("refresh", 0xE5D5),
    ("undo", 0xE166),
    ("redo", 0xE15A),
    ("download", 0xF090),
    ("upload", 0xF09B),
    ("send", 0xE163),
    ("attach_file", 0xE226),
    ("link", 0xE250),
    ("filter_list", 0xE152),
    ("sort", 0xE164),
    ("tune", 0xE429),
    ("settings", 0xE8B8),
    ("logout", 0xE9BA),
    // media and voice
    ("mic", 0xE31D),
    ("mic_off", 0xE02B),
    ("play_arrow", 0xE037),
    ("pause", 0xE034),
    ("stop", 0xE047),
    ("replay", 0xE042),
    ("skip_next", 0xE044),
    ("volume_up", 0xE050),
    ("volume_off", 0xE04F),
    ("record_voice_over", 0xE91F),
    ("graphic_eq", 0xE1B8),
    // status
    ("info", 0xE88E),
    ("help", 0xE8FD),
    ("warning", 0xF083),
    ("report", 0xF052),
    ("check_circle", 0xF0BE),
    ("cancel", 0xE888),
    ("lock", 0xE899),
    ("visibility", 0xE8F4),
    ("visibility_off", 0xE8F5),
    ("notifications", 0xE7F5),
    ("schedule", 0xEFD6),
    ("timer", 0xE425),
    ("history", 0xE8B3),
    ("star", 0xF09A),
    ("bookmark", 0xE8E7),
    // things
    ("person", 0xF0D3),
    ("account_circle", 0xF20B),
    ("chat", 0xE0C9),
    ("mail", 0xE159),
    ("description", 0xE873),
    ("lightbulb", 0xE90F),
    ("code", 0xE86F),
    ("library_books", 0xE02F),
];

#[test]
fn the_core_set_resolves() {
    assert_eq!(CORE_SET.len(), 71);
    for &(name, cp) in CORE_SET {
        assert_eq!(
            msymbols_codepoint(name),
            char::from_u32(cp),
            "{name} does not resolve to U+{cp:04X}"
        );
    }
    assert_eq!(msymbols_codepoint("content_copy"), Some('\u{E14D}'));
    // `error` is declared only at U+F8B6, in the owned blocks; `folder` is
    // the drawn icon's name.
    assert_eq!(msymbols_codepoint("error"), None);
    assert_eq!(msymbols_codepoint("folder"), None);
    assert!(highbay_codepoint("folder").is_some());
}

/// The atlases to check: the merged one, and the styled one when its
/// features are on.
fn atlases() -> Vec<(BundledAtlas, &'static [u8])> {
    #[allow(unused_mut)]
    let mut v = vec![(BundledAtlas::RobotoMsymbols, libmsdf::ROBOTO_MSYMBOLS_ATLAS)];
    #[cfg(all(
        feature = "font-apache-roboto-bold",
        feature = "font-apache-roboto-italic"
    ))]
    v.push((
        BundledAtlas::RobotoMsymbolsStyled,
        libmsdf::ROBOTO_MSYMBOLS_STYLED_ATLAS,
    ));
    v
}

/// Shaped through `TextShaper` on the merged face, whose glyph ids are the
/// atlases' Regular ids. Not through `AtlasShaper`: an atlas's layout `cmap`
/// covers `TEXT_RANGES` only (`LayoutOptions::default`), so it has never
/// mapped the Private Use Area; `an_atlas_shaper_maps_no_icon` pins that.
#[test]
fn every_icon_is_in_the_face_and_both_atlases() {
    let shaper = libmsdf::TextShaper::new(ROBOTO_ASCII_MSYMBOLS.to_vec()).unwrap();
    for (which, bytes) in atlases() {
        let atlas = FontAtlas::from_bytes(bytes).unwrap();
        for &(name, cp) in MSYMBOLS_ICONS {
            assert!(shaper.covers(cp), "the face does not cover {name}");
            let run = shaper.shape(cp.encode_utf8(&mut [0; 4]));
            assert_eq!(run.glyphs.len(), 1, "{which:?}: {name} is not one glyph");
            let gid = run.glyphs[0].glyph_id;
            assert_ne!(gid, 0, "{which:?}: {name} shapes to the placeholder");
            assert!(
                atlas.get_glyph(gid).is_some(),
                "{which:?}: {name} (glyph {gid}) has no cell"
            );
        }
    }
}

/// The atlas's own layout tables shape text only: an icon codepoint gives
/// the placeholder through `AtlasShaper`, as it did before this release.
/// Icons are shaped with the face (`TextShaper`), as the README shows.
#[test]
fn an_atlas_shaper_maps_no_icon() {
    let atlas = FontAtlas::from_bytes(libmsdf::ROBOTO_MSYMBOLS_ATLAS).unwrap();
    let shaper = AtlasShaper::new(&atlas, GlyphStyle::Regular).unwrap();
    for &(name, cp) in MSYMBOLS_ICONS {
        let run = shaper.shape(cp.encode_utf8(&mut [0; 4]));
        assert!(
            run.glyphs.iter().all(|g| g.glyph_id == 0),
            "{name} now maps through the atlas layout: update the README and this test"
        );
    }
    assert_ne!(shaper.shape("A").glyphs[0].glyph_id, 0);
}

/// The faces of a bundled atlas's styles, beside the style.
fn styled_faces(which: BundledAtlas) -> Vec<(GlyphStyle, &'static [u8])> {
    #[allow(unused_mut)]
    let mut v = vec![(GlyphStyle::Regular, ROBOTO_ASCII_MSYMBOLS)];
    #[cfg(all(
        feature = "font-apache-roboto-bold",
        feature = "font-apache-roboto-italic"
    ))]
    if which == BundledAtlas::RobotoMsymbolsStyled {
        v.push((GlyphStyle::Bold, libmsdf::ROBOTO_BOLD_ASCII));
        v.push((GlyphStyle::Italic, libmsdf::ROBOTO_ITALIC_ASCII));
    }
    let _ = which;
    v
}

#[test]
fn every_carried_glyph_fits_the_cell() {
    let mut over = Vec::new();
    let mut checked = 0;
    for (which, bytes) in atlases() {
        let atlas = FontAtlas::from_bytes(bytes).unwrap();
        let cell: CellBox = which.cell();
        assert!(!cell.is_line_box(), "{which:?} has no cell box");
        for (style, face_bytes) in styled_faces(which) {
            let face = ttf_parser::Face::parse(face_bytes, 0).unwrap();
            let upem = face.units_per_em() as f64;
            let (bottom, top) = which.cap().window(&face, cell);
            let width = cell.cell_em() * upem;
            // One texel: 2662.4 / 48 = 55.47 units at the 48 px cell.
            let texel = cell.cell_em() * upem / atlas.cell_px().unwrap() as f64;
            for gid in 0..face.number_of_glyphs() {
                if atlas.styled_glyph(style, gid).is_err() {
                    continue;
                }
                let Some(b) = face.glyph_bounding_box(ttf_parser::GlyphId(gid)) else {
                    continue;
                };
                checked += 1;
                let (y_min, y_max) = (b.y_min as f64, b.y_max as f64);
                if y_max > top - texel {
                    over.push(format!(
                        "{which:?} {style:?} glyph {gid}: top {y_max} is {:.1} units past {:.1}",
                        y_max - (top - texel),
                        top - texel
                    ));
                }
                if y_min < bottom + texel {
                    over.push(format!(
                        "{which:?} {style:?} glyph {gid}: bottom {y_min} is {:.1} units past {:.1}",
                        (bottom + texel) - y_min,
                        bottom + texel
                    ));
                }
                let w = (b.x_max - b.x_min) as f64;
                if w > width {
                    over.push(format!(
                        "{which:?} {style:?} glyph {gid}: {w} units wide, the cell is {width:.1}"
                    ));
                }
            }
        }
    }
    assert!(checked > 300, "only {checked} glyphs checked");
    assert!(
        over.is_empty(),
        "{} glyphs past their cell:\n{}",
        over.len(),
        over.join("\n")
    );
}
