//! **Shaping from an atlas alone** (atlas layout tables plan, step 6).
//!
//! This file needs no `shaper` feature: it is compiled and run by
//! `cargo test --no-default-features --test atlas_only`, where neither
//! rustybuzz nor ttf-parser is built. It loads the committed format-5 fixture
//! and lays text out with `AtlasShaper` from the tables it carries.

use libmsdf::{AtlasFileVersion, AtlasShaper, FontAtlas, GlyphStyle, RunShaper};

const ATLAS_FIXTURE: &[u8] = include_bytes!("fixtures/roboto-ascii-48.atlas");

#[test]
fn the_fixture_shapes_without_a_ttf() {
    let atlas = FontAtlas::from_bytes(ATLAS_FIXTURE).expect("the fixture loads");
    assert_eq!(
        atlas.file_version(),
        AtlasFileVersion::Split(libmsdf::ATLAS_FORMAT_VERSION)
    );
    let shaper = AtlasShaper::new(&atlas, GlyphStyle::Regular).expect("it carries layout");
    let run = shaper.shape("fifty Tax");
    // "fi" is one ligature glyph: fi, f, t, y, space, T, a, x.
    assert_eq!(run.glyphs.len(), 8, "{run:?}");
    assert_eq!(run.notdef_count(), 0);
    assert_eq!(run.units_per_em, 2048);
    for g in &run.glyphs {
        assert!(
            atlas.get_glyph(g.glyph_id).is_some(),
            "glyph {} has a cell",
            g.glyph_id
        );
    }
    // T kerns against the space before it and the a after it.
    assert!(run.total_advance > 0);
    assert!(shaper.covers('T') && !shaper.covers('\u{2192}'));

    let licences = atlas.licences();
    assert_eq!(licences.len(), 1);
    assert_eq!(licences[0].spdx, "Apache-2.0");
    assert_eq!(licences[0].source, "Roboto");
}
