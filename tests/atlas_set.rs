//! **Several atlases in one draw list** (docs/design/20261021_03_ATLAS_SET_PLAN.md).
//!
//! An `AtlasSet` lays its atlases out as one texture array and one glyph
//! table; a list draws from them through handles and is lowered against the
//! set. These check the layout of the set, the handles, the tags a list
//! records, and the lowering.

use libmsdf::drawlist::MAX_SET_GLYPHS;
use libmsdf::font::{AtlasLayer, FontAtlas, GlyphEntry, GlyphStyle, MAX_ATLAS_LAYERS};
use libmsdf::{AtlasId, AtlasSet, AtlasSetError};

/// The bundled Roboto Regular and Roboto with Material Symbols atlases, both
/// 400 x 2000, one layer, with layout tables.
const ROBOTO: &[u8] = include_bytes!("../fonts/atlases/roboto-regular-48.atlas");
const MSYMBOLS: &[u8] = include_bytes!("../fonts/atlases/roboto-msymbols-48.atlas");

fn roboto() -> FontAtlas {
    FontAtlas::from_bytes(ROBOTO).expect("Roboto atlas parses")
}

fn msymbols() -> FontAtlas {
    FontAtlas::from_bytes(MSYMBOLS).expect("symbols atlas parses")
}

/// A small atlas of `layers` layers of `w` x `h`, RGB, each layer's texels
/// all `fill + layer`, with one 2 x 2 cell per layer.
fn small(w: u32, h: u32, layers: u16, fill: u8) -> FontAtlas {
    let table: Vec<AtlasLayer> = (0..layers)
        .map(|_| AtlasLayer::new(4, GlyphStyle::Regular))
        .collect();
    let mut atlas = FontAtlas::empty_layered(w, h, 3, &table);
    let per_layer = (w * h * 3) as usize;
    for l in 0..layers as usize {
        atlas.pixel_data[l * per_layer..(l + 1) * per_layer].fill(fill + l as u8);
    }
    for l in 0..layers {
        atlas.insert_entry(GlyphEntry {
            glyph_id: 10 + l,
            atlas_x: 1,
            atlas_y: 1,
            atlas_w: 2,
            atlas_h: 2,
            layer: l,
            advance_x: 0.5,
            baseline_row: 1.5,
            px_per_em: 2.0,
            x_margin: 0.1,
        });
    }
    atlas
}

// ── the set ─────────────────────────────────────────────────────────────

#[test]
fn ids_bases_extent_and_layers_follow_registration() {
    let mut set = AtlasSet::new();
    assert!(set.is_empty());
    let a = set.register(small(4, 4, 1, 0x10)).unwrap();
    let b = set.register(small(8, 2, 2, 0x40)).unwrap();
    let c = set.register(small(2, 6, 1, 0x80)).unwrap();
    assert_eq!([a.index, b.index, c.index], [0, 1, 2]);
    assert!([a, b, c].iter().all(|id| id.set == set.number()));
    assert_eq!(set.len(), 3);
    assert_eq!(set.extent(), [8, 6], "the widest and the tallest");
    assert_eq!(set.layers(), 4);
    assert_eq!(set.glyphs(), 4);
    assert_eq!(set.table_bases(), [0, 1, 3]);
    assert_eq!(set.layer_bases(), [0, 1, 3]);
}

#[test]
fn every_set_has_its_own_number_and_refuses_the_others_ids() {
    let mut one = AtlasSet::new();
    let mut two = AtlasSet::new();
    assert_ne!(one.number(), two.number());
    assert_ne!(one.number(), 0, "0 is no set");
    let a = one.register(small(4, 4, 1, 0)).unwrap();
    let b = two.register(small(4, 4, 1, 0)).unwrap();
    assert!(one.atlas(a).is_some());
    assert!(one.atlas(b).is_none(), "another set's id");
    let past = AtlasId {
        set: one.number(),
        index: 1,
    };
    assert!(one.atlas(past).is_none(), "an index past the end");
    // The default id is the first atlas of whatever set it is read against.
    assert_eq!(one.atlas(AtlasId::default()).unwrap().glyphs.len(), 1);
}

#[test]
fn a_set_past_the_layer_or_glyph_limit_is_refused() {
    let mut set = AtlasSet::new();
    set.register(small(1, 1, MAX_ATLAS_LAYERS as u16, 0))
        .unwrap();
    assert_eq!(
        set.register(small(1, 1, 1, 0)),
        Err(AtlasSetError::TooManyLayers {
            have: MAX_ATLAS_LAYERS as u32,
            adding: 1
        })
    );

    let mut set = AtlasSet::new();
    let mut big = FontAtlas::empty(1, 1, 3);
    for id in 0..u16::MAX {
        big.insert_entry(GlyphEntry {
            glyph_id: id,
            atlas_x: 0,
            atlas_y: 0,
            atlas_w: 1,
            atlas_h: 1,
            layer: 0,
            advance_x: 0.5,
            baseline_row: 0.5,
            px_per_em: 1.0,
            x_margin: 0.0,
        });
    }
    set.register(big).unwrap();
    assert_eq!(set.glyphs(), u16::MAX as u32);
    assert_eq!(
        set.register(small(1, 1, 2, 0)),
        Err(AtlasSetError::TooManyGlyphs {
            have: u16::MAX as u32,
            adding: 2
        }),
        "{MAX_SET_GLYPHS} is the most"
    );
}

#[test]
fn the_merged_table_moves_only_each_entrys_layer() {
    let (a, b) = (small(4, 4, 2, 0x10), small(4, 4, 3, 0x40));
    let (ta, tb) = (a.glyph_table_u32s(), b.glyph_table_u32s());
    let mut set = AtlasSet::new();
    set.register(a).unwrap();
    set.register(b).unwrap();
    let merged = set.glyph_table_u32s();
    assert_eq!(merged.len(), ta.len() + tb.len());
    assert_eq!(&merged[..ta.len()], &ta[..], "the first atlas is unchanged");
    for (m, t) in merged[ta.len()..].chunks(8).zip(tb.chunks(8)) {
        assert_eq!(&m[..7], &t[..7], "everything but the layer");
        assert_eq!(m[7], t[7] + 2, "the layer, after the first atlas's two");
    }
}

#[test]
fn the_merged_texels_put_each_atlas_at_its_layers_top_left() {
    let mut set = AtlasSet::new();
    set.register(small(2, 2, 1, 0x10)).unwrap();
    set.register(small(4, 3, 2, 0x40)).unwrap();
    let [w, h] = set.extent();
    assert_eq!([w, h], [4, 3]);
    let rgba = set.to_rgba_bytes();
    assert_eq!(rgba.len(), (4 * 3 * 4 * 3) as usize);
    let texel = |layer: u32, x: u32, y: u32| {
        let i = (((layer * h + y) * w + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    };
    // The 2 x 2 atlas fills the top left of layer 0, and zeros the rest.
    assert_eq!(texel(0, 1, 1), [0x10, 0x10, 0x10, 255]);
    assert_eq!(texel(0, 2, 0), [0; 4]);
    assert_eq!(texel(0, 0, 2), [0; 4]);
    // The 4 x 3 atlas's two layers fill layers 1 and 2 whole.
    assert_eq!(texel(1, 3, 2), [0x40, 0x40, 0x40, 255]);
    assert_eq!(texel(2, 0, 0), [0x41, 0x41, 0x41, 255]);
}

#[test]
fn a_handle_derefs_to_its_atlas_and_shapes_as_the_atlas_does() {
    let mut set = AtlasSet::new();
    set.register(msymbols()).unwrap();
    let id = set.register(roboto()).unwrap();
    let handle = set.atlas(id).unwrap();
    assert_eq!(handle.id, id);
    assert_eq!(handle.width, 400, "metrics read through the handle");
    let ours = handle
        .shaper(GlyphStyle::Regular)
        .unwrap()
        .shape("Bubble Pop");
    let direct = libmsdf::AtlasShaper::new(&roboto(), GlyphStyle::Regular)
        .unwrap()
        .shape("Bubble Pop");
    assert_eq!(ours, direct);
}
