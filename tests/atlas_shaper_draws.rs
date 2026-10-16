//! **An `AtlasShaper` run draws the same `DrawList` as a `TextShaper` run**
//! (atlas layout tables plan, step 4). Lives here rather than beside
//! `AtlasShaper` because `DrawList` is in libmsdf and `AtlasShaper` in
//! `libmsdf-font`.

#![cfg(feature = "shaper")]

mod common;
use common::faces::ROBOTO_REGULAR_ASCII;

use libmsdf::font::{AtlasLayout, FaceLayout, LayoutOptions};
use libmsdf::{AtlasShaper, DrawList, FontAtlas, GlyphStyle, RunShaper, ShapedRun, TextShaper};

#[test]
fn an_atlas_run_draws_the_same_draw_list() {
    let mut atlas =
        FontAtlas::from_bytes(include_bytes!("fixtures/legacy/roboto-ascii-48-v3.atlas")).unwrap();
    let face = FaceLayout::extract(
        ROBOTO_REGULAR_ASCII,
        GlyphStyle::Regular,
        &LayoutOptions::default(),
    )
    .expect("extracts and passes the check");
    atlas.layout = Some(AtlasLayout::from_faces(vec![face]).unwrap());
    let text = TextShaper::new(ROBOTO_REGULAR_ASCII.to_vec()).unwrap();
    let shaper = AtlasShaper::new(&atlas, GlyphStyle::Regular).unwrap();
    let draw = |run: &ShapedRun| {
        let mut list = DrawList::new();
        list.push_shaped_text(run, &atlas, [10.0, 20.0], 18.0, 6.0, [1.0; 4]);
        format!("{list:?}")
    };
    for s in ["The office of fifty T T", "Treasury 50G"] {
        assert_eq!(draw(&shaper.shape(s)), draw(&text.shape(s)), "{s:?}");
    }
    assert!(RunShaper::covers(&shaper, 'A') && !RunShaper::covers(&shaper, '\u{2192}'));
    assert_eq!(RunShaper::units_per_em(&shaper), 2048);
}
