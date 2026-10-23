//! **The terminal faces** (`docs/design/20261023_01_TERMINAL_FONTS_PLAN.md`):
//! the cap placement that fits Noto Sans Mono's accents in the cell.

#![cfg(feature = "cpu-bake")]

use std::path::{Path, PathBuf};

use libmsdf::bake::{BakeOptions, BakeReport, Baker, FaceReport};
use libmsdf::font::coverage::{COVERAGE_MATH, COVERAGE_SYMBOLS, COVERAGE_TEXT, Coverage};
use libmsdf::font::{CapPlacement, GlyphStyle};

fn fonts() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts")
}

/// Noto Sans Mono Regular with the text groups and no layout tables, at
/// `cap`: the cheapest bake that lays every text cell out.
fn bake_mono(cap: CapPlacement) -> BakeReport {
    let mut opts = BakeOptions::new(48, 6.0);
    opts.coverage = Coverage::Groups(vec![COVERAGE_TEXT, COVERAGE_SYMBOLS, COVERAGE_MATH]);
    opts.columns = 16;
    opts.waive = vec![(GlyphStyle::Regular, '\u{AD}', "soft hyphen")];
    opts.licences = vec![libmsdf::bake::LicenceSpec {
        spdx: "OFL-1.1".into(),
        source: "Noto Sans Mono".into(),
        styles: vec![GlyphStyle::Regular],
    }];
    opts.cap = cap;
    let ttf = fonts().join("NotoSansMono-Regular-subset.ttf");
    Baker::bake_face_report(&ttf, &opts).expect("bakes").1
}

fn regular(report: &BakeReport) -> &FaceReport {
    report
        .faces
        .iter()
        .find(|f| f.style == GlyphStyle::Regular)
        .expect("a Regular face")
}

/// At the default placement the accented capitals pass the cell top (plan,
/// section 3.3); at the terminal placement nothing passes the window.
#[test]
fn the_terminal_placement_fits_noto_sans_mono() {
    let default = bake_mono(CapPlacement::DEFAULT);
    let clipped = &regular(&default).ink_outside;
    // 65, the accented capitals and ĥ ĺ. Section 3.3's 166 also counted the
    // upstream face's box, block and geometric glyphs, which this atlas does
    // not carry (CHANGES.md, step 2).
    assert_eq!(
        clipped.len(),
        65,
        "{} glyphs pass the default window",
        clipped.len()
    );
    assert!(clipped.iter().all(|o| o.below == 0 && o.wider == 0));
    let worst = clipped.iter().map(|o| o.above).max().unwrap();
    assert_eq!(worst, 83, "the ring of Å passes 912 by 83 units");

    let terminal = bake_mono(CapPlacement {
        top_frac: CapPlacement::TERMINAL_TOP_FRAC,
        height_em: None,
    });
    assert_eq!(regular(&terminal).ink_outside, vec![]);
    assert!(!format!("{terminal}").contains("ink past"), "{terminal}");
}
