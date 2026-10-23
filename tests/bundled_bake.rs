//! **The baker on the bundled table** (bundled fonts plan, step 4). The
//! seven-atlas bake with every equivalence check, its reproducibility and
//! the comparisons with the tower defense game's and highbay's committed
//! atlases are run by `libmsdf-bake bundled` and logged under `out/fonts/`;
//! these are the checks cheap enough for every `cargo test`.

#![cfg(feature = "cpu-bake")]

use std::path::{Path, PathBuf};

use libmsdf::bake::{BakeError, Baker};
use libmsdf::font::BundledAtlas;
use libmsdf::{FontAtlas, GlyphStyle};

fn fonts() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts")
}

/// The Roboto Regular bundled atlas IS the committed format-5 fixture, and a
/// second bake is byte-identical.
#[test]
fn the_roboto_regular_bake_is_the_fixture() {
    let a = Baker::bake_bundled(BundledAtlas::RobotoRegular, &fonts()).expect("bakes");
    assert!(
        a == include_bytes!("fixtures/roboto-ascii-48.atlas"),
        "not the fixture"
    );
    let b = Baker::bake_bundled(BundledAtlas::RobotoRegular, &fonts()).expect("bakes again");
    assert!(a == b, "two bakes differ");
    let atlas = FontAtlas::from_bytes(&a).unwrap();
    assert!(
        atlas.coverage.is_empty(),
        "shipped coverage records no CVRG"
    );
}

/// A face that is not the recorded bytes is refused before anything bakes.
#[test]
fn a_changed_face_is_refused_with_both_digests() {
    let dir = std::env::temp_dir().join(format!("libmsdf-facehash-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut bytes = std::fs::read(fonts().join("Roboto-Regular-ascii.ttf")).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(dir.join("Roboto-Regular-ascii.ttf"), &bytes).unwrap();
    let err = Baker::bake_bundled(BundledAtlas::RobotoRegular, &dir).expect_err("refused");
    std::fs::remove_dir_all(&dir).ok();
    match err {
        BakeError::FaceHash {
            expected, found, ..
        } => assert_ne!(expected, found),
        other => panic!("expected FaceHash, got {other}"),
    }
}

/// Without its waiver, Texturina's missing long s fails the bake with
/// exactly that codepoint.
#[test]
fn texturina_without_its_waiver_fails_on_the_long_s() {
    let (ttf, mut opts) = Baker::bundled_options(BundledAtlas::Texturina, &fonts());
    assert_eq!(opts.waive.len(), 1);
    opts.waive.clear();
    match Baker::bake_face(&ttf, &opts) {
        Err(BakeError::MissingText { style, codepoints }) => {
            assert_eq!(style, GlyphStyle::Regular);
            assert_eq!(codepoints, vec!['\u{17f}']);
        }
        other => panic!("expected MissingText, got {:?}", other.map(|b| b.len())),
    }
}

/// **A cell box bakes a face whole without moving its baseline** (per-atlas
/// cell box plan, step 2): Texturina and Grenze Gotisch with section 4.5's
/// boxes report no ink past the cell, carry the box through the file, and
/// put the baseline at the same place in the line box as the committed
/// line-box atlases.
#[test]
fn a_cell_box_bakes_texturina_and_grenze_whole_on_the_same_baseline() {
    use libmsdf::font::CellBox;
    for (atlas, cell) in [
        (
            BundledAtlas::Texturina,
            CellBox {
                above_em: 0.179,
                below_em: -0.073,
            },
        ),
        (
            BundledAtlas::GrenzeGotisch,
            CellBox {
                above_em: 0.075,
                below_em: -0.075,
            },
        ),
    ] {
        let (ttf, mut opts) = Baker::bundled_options(atlas, &fonts());
        opts.cell = cell;
        let (bytes, report) = Baker::bake_face_report(&ttf, &opts).unwrap();
        for face in &report.faces {
            assert!(
                face.ink_outside.is_empty(),
                "{atlas:?}: {:?}",
                face.ink_outside
            );
        }
        let boxed = FontAtlas::from_bytes(&bytes).unwrap();
        assert_eq!(&bytes[4..8], &[5, 0, 2, 0]);
        let c = boxed.cell_box();
        assert!((c.above_em - cell.above_em).abs() < 1e-6, "{atlas:?}");
        assert!((c.below_em - cell.below_em).abs() < 1e-6, "{atlas:?}");

        let committed = std::fs::read(fonts().join("atlases").join(atlas.file_name())).unwrap();
        let line = FontAtlas::from_bytes(&committed).unwrap();
        let line_box_baseline = line.text_baseline_frac().unwrap() as f64;
        let in_line_box = (boxed.text_baseline_frac().unwrap() as f64 * c.cell_em() - c.above_em)
            / libmsdf::font::CELL_EM_RATIO;
        assert!(
            (in_line_box - line_box_baseline).abs() < 1e-5,
            "{atlas:?}: baseline {in_line_box} in the line box, was {line_box_baseline}"
        );
    }
}
