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

// ── the list ────────────────────────────────────────────────────────────

use libmsdf::{DrawList, SdfInstance, SdfKind};

const WHITE: [f32; 4] = [1.0; 4];

/// A set of the symbols atlas (index 0) and Roboto (index 1).
fn two() -> (AtlasSet, AtlasId, AtlasId) {
    let mut set = AtlasSet::new();
    let sym = set.register(msymbols()).unwrap();
    let text = set.register(roboto()).unwrap();
    (set, sym, text)
}

/// A one-glyph run of the symbols atlas's last cell (an icon), addressed by
/// glyph id: the layout tables map text only.
fn icon_run(atlas: &FontAtlas) -> libmsdf::ShapedRun {
    let glyph_id = atlas.glyphs.last().expect("the atlas has cells").glyph_id;
    libmsdf::ShapedRun {
        glyphs: vec![libmsdf::font::ShapedGlyph {
            glyph_id,
            x_advance: 2048,
            x_offset: 0,
            y_offset: 0,
            cluster: 0,
        }],
        total_advance: 2048,
        units_per_em: 2048,
    }
}

/// Text from Roboto, a check icon from the symbols atlas, a panel, then more
/// Roboto: all through handles.
fn mixed(set: &AtlasSet, sym: AtlasId, text: AtlasId) -> DrawList {
    let (sym, text) = (set.atlas(sym).unwrap(), set.atlas(text).unwrap());
    let roboto = text.shaper(GlyphStyle::Regular).unwrap();
    let mut list = DrawList::new();
    list.push_shaped_text_with(&roboto.shape("Name"), text, [10.0, 10.0], 24.0, 4.0, WHITE);
    let icon = list.push_icon_with(
        &icon_run(&sym),
        sym,
        [100.0, 10.0],
        [32.0, 32.0],
        32.0,
        4.0,
        WHITE,
    );
    assert!(icon.is_some(), "the icon drew");
    list.push(SdfInstance {
        kind: SdfKind::Box,
        position: [0.0, 0.0],
        size: [5.0, 5.0],
        color: WHITE,
        anim: 0,
    });
    list.push_shaped_text_with(&roboto.shape("fish"), text, [10.0, 50.0], 24.0, 4.0, WHITE);
    list
}

#[test]
fn the_handle_calls_record_their_atlas_and_everything_else_the_default() {
    let (set, sym, text) = two();
    let list = mixed(&set, sym, text);
    let ids: Vec<AtlasId> = (0..4).map(|i| list.instance_atlas(i)).collect();
    assert_eq!(ids, [text, sym, AtlasId::default(), text]);
    assert_eq!(list.instance_atlas(99), AtlasId::default(), "past the end");

    // The plain calls record the default.
    let mut plain = DrawList::new();
    let atlas = roboto();
    let run = libmsdf::AtlasShaper::new(&atlas, GlyphStyle::Regular)
        .unwrap()
        .shape("Name");
    plain.push_shaped_text(&run, &atlas, [0.0, 0.0], 24.0, 4.0, WHITE);
    assert_eq!(plain.instance_atlas(0), AtlasId::default());

    // A handle call draws exactly what the plain call draws.
    let mut with = DrawList::new();
    with.push_shaped_text_with(&run, set.atlas(text).unwrap(), [0.0, 0.0], 24.0, 4.0, WHITE);
    assert_eq!(with.instances, plain.instances);
    assert_eq!(with.chars(), plain.chars());
}

#[test]
fn append_from_keeps_the_sources_atlases() {
    let (set, sym, text) = two();
    let kept = mixed(&set, sym, text);
    let mut list = DrawList::new();
    list.push(SdfInstance {
        kind: SdfKind::Box,
        position: [0.0, 0.0],
        size: [1.0, 1.0],
        color: WHITE,
        anim: 0,
    });
    list.append_from(&kept);
    let ids: Vec<AtlasId> = (0..5).map(|i| list.instance_atlas(i)).collect();
    assert_eq!(
        ids,
        [AtlasId::default(), text, sym, AtlasId::default(), text]
    );
    list.clear();
    assert_eq!(list.instance_atlas(0), AtlasId::default());
}

#[test]
fn lower_with_moves_each_atlases_glyphs_to_its_place_in_the_table() {
    let (set, sym, text) = two();
    let list = mixed(&set, sym, text);
    let frame = list.lower_with(&set);
    let base = set.table_bases()[1];
    assert!(base > 0, "Roboto follows the symbols atlas");
    // The first and last instances are Roboto's; the second is the icon,
    // from the symbols atlas at base 0.
    let ranges: Vec<(usize, usize, AtlasId)> = list
        .instances
        .iter()
        .enumerate()
        .filter_map(|(i, inst)| match inst.kind {
            SdfKind::MsdfText {
                char_start,
                char_count,
                ..
            } => Some((
                char_start as usize,
                (char_start + char_count) as usize,
                list.instance_atlas(i),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(ranges.len(), 3);
    for (start, end, id) in ranges {
        let add = if id == text { base } else { 0 };
        for i in start..end {
            let (was, now) = (list.chars()[i], frame.char_buffer[i]);
            assert_eq!(now >> 16, (was >> 16) + add, "the glyph index");
            assert_eq!(now & 0xFFFF, was & 0xFFFF, "the advance delta");
        }
    }
    assert_eq!(frame.draws.len(), 4, "every instance is drawn");
}

#[test]
fn a_list_of_plain_calls_lowers_with_a_set_exactly_as_with_lower() {
    let atlas = roboto();
    let run = libmsdf::AtlasShaper::new(&atlas, GlyphStyle::Regular)
        .unwrap()
        .shape("Bubble Pop");
    let mut list = DrawList::new();
    list.push_shaped_text(&run, &atlas, [3.0, 4.0], 20.0, 4.0, WHITE);
    list.push_line([0.0, 0.0], [9.0, 9.0], 2.0, WHITE);
    let mut set = AtlasSet::new();
    set.register(roboto()).unwrap();
    let (a, b) = (list.lower(), list.lower_with(&set));
    assert_eq!(a.char_buffer, b.char_buffer);
    assert_eq!(a.param_bank, b.param_bank);
    assert_eq!(a.draws.len(), b.draws.len());
    for (x, y) in a.draws.iter().zip(&b.draws) {
        assert_eq!(format!("{x:?}"), format!("{y:?}"));
    }
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "lower_with(&set)")]
fn lower_refuses_text_from_a_handle_in_debug() {
    let (set, sym, text) = two();
    mixed(&set, sym, text).lower();
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "did not hand out")]
fn lowering_against_another_set_panics_in_debug() {
    let (set, sym, text) = two();
    let (other, _, _) = two();
    mixed(&set, sym, text).lower_with(&other);
}

#[cfg(not(debug_assertions))]
#[test]
fn lowering_against_another_set_drops_that_text_in_release() {
    let (set, sym, text) = two();
    let (other, _, _) = two();
    let list = mixed(&set, sym, text);
    let frame = list.lower_with(&other);
    // Only the panel is this set's (default id); the three runs are not.
    assert_eq!(frame.draws.len(), 1);
    assert_eq!(frame.char_buffer, list.chars(), "nothing rewritten");
}

/// **One list, two atlases, one renderer** draws each piece exactly as a
/// renderer holding only that piece's atlas does. Roboto is registered
/// first and the symbols atlas second, so the icon's glyph index and its
/// layer are both moved by the set.
///
/// ```text
/// CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm cargo test --features gpu-tests --test atlas_set
/// ```
#[cfg(feature = "gpu-tests")]
mod on_the_gpu {
    use super::*;
    use libmsdf::gpu::GpuSdfRenderer;

    const W: u32 = 160;
    const H: u32 = 96;
    /// The icon's box, right of the text; everything left of it is text.
    const ICON_X: u32 = 96;

    fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            ..Default::default()
        }))
        .ok()?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
    }

    /// Draw with `draw` into a fresh W x H target and read it back.
    fn frame(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        draw: impl FnOnce(&wgpu::TextureView),
    ) -> Vec<u8> {
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d {
                width: W,
                height: H,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        draw(&target.create_view(&Default::default()));
        let padded_row = (W * 4).next_multiple_of(256);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rb"),
            size: (padded_row * H) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: W,
                height: H,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(std::iter::once(enc.finish()));
        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let data = slice.get_mapped_range().unwrap();
        let mut img = Vec::with_capacity((W * H * 4) as usize);
        for row in 0..H {
            let start = (row * padded_row) as usize;
            img.extend_from_slice(&data[start..start + (W * 4) as usize]);
        }
        drop(data);
        readback.unmap();
        img
    }

    /// A renderer holding only `atlas`, drawing `list` the usual way.
    fn alone(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        atlas: &FontAtlas,
        list: &DrawList,
    ) -> Vec<u8> {
        let r = GpuSdfRenderer::new_with_msdf_layers(
            device,
            wgpu::TextureFormat::Rgba8Unorm,
            atlas.width,
            atlas.height,
            atlas.layer_count(),
        );
        r.upload_msdf_atlas(queue, atlas.width, atlas.height, &atlas.to_rgba_bytes());
        r.upload_glyph_table(queue, &atlas.glyph_table_u32s());
        frame(device, queue, |view| {
            r.render_draw_list(device, queue, view, W, H, 1.0, list, 0.0)
        })
    }

    #[test]
    fn one_renderer_draws_both_atlases_as_two_renderers_do() {
        let Some((device, queue)) = gpu() else {
            eprintln!("SKIP: no GPU adapter");
            return;
        };
        let (rob, sym) = (roboto(), msymbols());
        let mut set = AtlasSet::new();
        let text = set.register(roboto()).unwrap();
        let icon = set.register(msymbols()).unwrap();
        assert_eq!(set.layer_bases(), [0, 1]);
        let shaper = set
            .atlas(text)
            .unwrap()
            .shaper(GlyphStyle::Regular)
            .unwrap();
        let (name, fish, check) = (shaper.shape("Name"), shaper.shape("fish"), icon_run(&sym));
        let icon_box = ([ICON_X as f32 + 8.0, 10.0], [32.0, 32.0]);

        // One list, through the handles.
        let mut both = DrawList::new();
        let (t, i) = (set.atlas(text).unwrap(), set.atlas(icon).unwrap());
        both.push_shaped_text_with(&name, t, [10.0, 10.0], 24.0, 4.0, WHITE);
        both.push_icon_with(&check, i, icon_box.0, icon_box.1, 32.0, 4.0, WHITE)
            .expect("the icon drew");
        both.push_shaped_text_with(&fish, t, [10.0, 50.0], 24.0, 4.0, WHITE);
        let r = GpuSdfRenderer::new_with_atlas_set(&device, wgpu::TextureFormat::Rgba8Unorm, &set);
        r.upload_atlas_set(&queue, &set);
        let together = frame(&device, &queue, |view| {
            r.render_draw_list_in(&device, &queue, view, W, H, 1.0, &both, &set, 0.0)
        });

        // The same pieces, each by a renderer of its own atlas.
        let mut words = DrawList::new();
        words.push_shaped_text(&name, &rob, [10.0, 10.0], 24.0, 4.0, WHITE);
        words.push_shaped_text(&fish, &rob, [10.0, 50.0], 24.0, 4.0, WHITE);
        let mut mark = DrawList::new();
        mark.push_icon(&check, &sym, icon_box.0, icon_box.1, 32.0, 4.0, WHITE)
            .expect("the icon drew alone");
        let (words, mark) = (
            alone(&device, &queue, &rob, &words),
            alone(&device, &queue, &sym, &mark),
        );

        let ink = |img: &[u8]| img.chunks(4).filter(|p| p[0] > 128).count();
        assert!(ink(&words) > 50 && ink(&mark) > 50, "both pieces drew ink");
        for y in 0..H {
            for x in 0..W {
                let i = ((y * W + x) * 4) as usize;
                let want = if x >= ICON_X { &mark } else { &words };
                assert_eq!(
                    &together[i..i + 4],
                    &want[i..i + 4],
                    "pixel ({x}, {y}) differs from the {} drawn alone",
                    if x >= ICON_X { "icon" } else { "text" }
                );
            }
        }
    }
}
