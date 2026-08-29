//! **The atlas is a texture ARRAY, and a layer is `(point size, style)`.**
//!
//! Wave K put a style in the top two bits of the glyph id and then could not
//! bake one: the shipped coverage is 224 cells of a 320-cell grid and a style
//! needs 205. The grid was the atlas's budget. It is now one LAYER's budget,
//! selected per glyph from the glyph TABLE, and the four things that had to
//! stay true are the four things this file checks:
//!
//! * **Nothing that exists moved.** The layer index went into two bytes of
//!   `GlyphEntry` that were already pad and into `g1.w`, which was the literal
//!   `0` and was read nowhere - so a single-layer atlas is still written at
//!   `ATLAS_VERSION`, and the committed fixture round-trips byte for byte.
//! * **A styled bake fits.** `add_shipped_coverage` plus both bundled style
//!   faces is 634 cells, which no single grid holds and three layers do.
//! * **A layer the atlas lacks is REFUSED**, never answered with layer 0 - the
//!   same rule `StyledGlyphError` enforces one axis over, for the same reason.
//! * **An older file loads, and a newer one is refused cleanly.**

use libmsdf::font::{
    ATLAS_VERSION, ATLAS_VERSION_LAYERED, AtlasLayer, FontAtlas, GlyphEntry, GlyphStyle,
    LayerNotBaked, MAX_ATLAS_LAYERS,
};

/// The same committed artifact `coverage.rs` and `styled_faces.rs` read: the
/// plain face at 48px, baked before layers existed. It is the regression bar
/// in file form.
const ATLAS_FIXTURE: &[u8] = include_bytes!("fixtures/roboto-ascii-48.atlas");

fn atlas() -> FontAtlas {
    FontAtlas::from_bytes(ATLAS_FIXTURE).expect("fixture atlas parses")
}

// ── nothing that exists moved ───────────────────────────────────────────

/// **A file baked before layers existed round-trips byte for byte.**
///
/// The strongest available form of "no atlas moved", and it needs no bake: the
/// committed bytes are parsed and re-serialized, and the result is compared to
/// the file. A version bump, a widened header, a widened entry or a second
/// layer written where there was one would each show up here as a length or a
/// byte, and none of them may.
#[test]
fn the_committed_atlas_round_trips_byte_for_byte() {
    let atlas = atlas();
    let out = atlas.to_bytes();
    assert_eq!(out.len(), ATLAS_FIXTURE.len(), "the serialized length moved");
    assert!(out == ATLAS_FIXTURE, "the serialized bytes moved");
    assert_eq!(
        u32::from_le_bytes(out[4..8].try_into().unwrap()),
        ATLAS_VERSION,
        "a single-layer atlas must still be written at the version it always was"
    );
}

/// **The shipped atlas is one layer, and every cell is in it.**
///
/// `layer: 0` is both "the first layer" and "this file predates layers", and
/// the point of putting the index in what was pad is that those two are the
/// same bytes meaning the same thing.
#[test]
fn the_shipped_atlas_is_one_layer_of_48px_regular() {
    let atlas = atlas();
    assert!(atlas.glyphs.len() > 200, "vacuity: the fixture came back empty");
    assert_eq!(atlas.layers, vec![AtlasLayer::new(48, GlyphStyle::Regular)]);
    assert_eq!(atlas.layer_count(), 1);
    for e in &atlas.glyphs {
        assert_eq!(e.layer, 0, "glyph {} claims a layer this atlas has not got", e.glyph_id);
    }
    // ...and the pixel data is exactly one layer of it.
    assert_eq!(atlas.layer_offset(1), atlas.pixel_data.len());
}

/// **The layer reaches the GPU through `g1.w`, and through nothing else.**
///
/// The draw list still packs a bare 16-bit glyph id: it never learns that
/// textures are plural. So the whole GPU-side change is one word of the table
/// that used to be the constant zero, and for the shipped atlas it still is.
#[test]
fn the_layer_travels_in_the_glyph_table_and_the_draw_list_never_learns() {
    let atlas = atlas();
    let table = atlas.glyph_table_u32s();
    assert_eq!(table.len(), atlas.glyphs.len() * 8);
    for (i, e) in atlas.glyphs.iter().enumerate() {
        assert_eq!(table[i * 8], e.glyph_id as u32);
        assert_eq!(table[i * 8 + 7], 0, "g1.w must still be 0 for a single-layer atlas");
    }

    // ...and a cell in layer 3 says 3 there, with every other word untouched.
    let mut e = atlas.glyphs[10];
    let before = e.to_gpu_u32s();
    e.layer = 3;
    let after = e.to_gpu_u32s();
    assert_eq!(&before[..7], &after[..7], "the layer perturbed another word");
    assert_eq!(after[7], 3);
}

// ── a layer the atlas lacks ─────────────────────────────────────────────

/// **A layer that is not baked is REFUSED**, and layer 0 is not offered.
///
/// Falling back would always "work": layer 0 exists in every atlas. It would
/// also always be wrong - 48px regular cells where the caller asked for 16px
/// bold is upright text where the document says emphasis, at a size nobody
/// asked for. Both axes are checked, because a caller can be wrong about
/// either one on its own.
#[test]
fn a_layer_this_atlas_lacks_is_refused_rather_than_substituted() {
    let atlas = atlas();
    assert_eq!(atlas.layer_index(AtlasLayer::new(48, GlyphStyle::Regular)), Ok(0));

    for missing in [
        AtlasLayer::new(48, GlyphStyle::Bold),        // right size, wrong cut
        AtlasLayer::new(48, GlyphStyle::Italic),
        AtlasLayer::new(48, GlyphStyle::BoldItalic),
        AtlasLayer::new(16, GlyphStyle::Regular),     // right cut, wrong size
        AtlasLayer::new(0, GlyphStyle::Regular),
    ] {
        assert_eq!(
            atlas.layer_index(missing),
            Err(LayerNotBaked(missing)),
            "{missing} was answered instead of refused"
        );
    }

    // The message names the layer AND says that layer 0 is not a substitute,
    // because the whole failure mode is a caller quietly taking one.
    let err = atlas.layer_index(AtlasLayer::new(16, GlyphStyle::Bold)).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("16px"), "{msg}");
    assert!(msg.contains("Bold"), "{msg}");
    assert!(msg.contains("layer 0 is not a"), "{msg}");
}

// ── what shares a layer ─────────────────────────────────────────────────

/// **Text and the Private Use Area share ONE layer at a size**, which is the
/// whole reason the merged face exists: a run mixing prose with an icon must
/// not swap textures mid-draw, and text beside an icon is the common case.
///
/// Stated over the type rather than over a bake, so a set filed under the
/// wrong style fails here instead of as a mid-run texture swap nobody profiles
/// for.
#[test]
fn text_and_the_private_use_area_share_a_layer_at_a_size() {
    use libmsdf::font::GlyphSet;
    let at = |set: GlyphSet| AtlasLayer::new(16, set.style());
    let text = at(GlyphSet::Text);
    for set in [
        GlyphSet::Placeholder,
        GlyphSet::Text,
        GlyphSet::ShapedText,
        GlyphSet::Markers,
        GlyphSet::OwnedIcons,
        GlyphSet::BorrowedIcons,
    ] {
        assert_eq!(at(set), text, "{set:?} would need a texture swap beside prose");
    }
    // ...and no styled set joins them, because a style is what a layer is FOR.
    for set in [
        GlyphSet::BoldText,
        GlyphSet::BoldShapedText,
        GlyphSet::ItalicText,
        GlyphSet::ItalicShapedText,
        GlyphSet::BoldItalicText,
        GlyphSet::BoldItalicShapedText,
    ] {
        assert_ne!(at(set), text, "{set:?} shares the regular layer");
    }
    // A layer is `(point size, style)`: the same cut at another size is
    // another layer, so nothing above holds across sizes.
    assert_ne!(AtlasLayer::new(48, GlyphStyle::Regular), text);
}

/// **The Private Use Area is NOT duplicated per style.** An icon has no
/// weight, the outlines would be identical, and a copy per style would spend
/// cells on a distinction no caller can make. A bold run containing an icon
/// takes one layer change, which is the ordinary cost of any layer change.
#[cfg(all(feature = "cpu-bake", not(target_arch = "wasm32")))]
#[test]
fn a_style_carries_text_only_and_never_a_second_copy_of_the_icons() {
    use libmsdf::font::{
        CellKey, FontAtlasBuilder, GlyphSet, ROBOTO_ASCII_MSYMBOLS, ROBOTO_BOLD_ASCII,
    };
    let mut b = FontAtlasBuilder::new(ROBOTO_ASCII_MSYMBOLS.to_vec(), 16, 2.0);
    b.add_shipped_coverage();
    b.add_styled_coverage(GlyphStyle::Bold, ROBOTO_BOLD_ASCII.to_vec())
        .expect("the bold face parses");
    let order: Vec<CellKey> = b.cell_order();

    let icons = |style: GlyphStyle| {
        order
            .iter()
            .filter(|k| {
                k.set().style() == style
                    && matches!(
                        k.set(),
                        GlyphSet::Markers | GlyphSet::OwnedIcons | GlyphSet::BorrowedIcons
                    )
            })
            .count()
    };
    assert!(icons(GlyphStyle::Regular) > 0, "vacuity: the merged face queued no icons");
    assert_eq!(icons(GlyphStyle::Bold), 0, "the icons were duplicated into the bold layer");
}

// ── the file format ─────────────────────────────────────────────────────

/// Two layers of 4x4, filled with distinguishable texels: the smallest thing
/// that can tell a layer-aware reader from a layer-blind one.
fn two_layer_atlas() -> FontAtlas {
    let layers = [
        AtlasLayer::new(4, GlyphStyle::Regular),
        AtlasLayer::new(4, GlyphStyle::Bold),
    ];
    let mut atlas = FontAtlas::empty_layered(4, 4, 3, &layers);
    // Layer 0 all 0x11, layer 1 all 0x99.
    let per_layer = 4 * 4 * 3;
    atlas.pixel_data[..per_layer].fill(0x11);
    atlas.pixel_data[per_layer..].fill(0x99);
    let cell = |glyph_id: u16, layer: u16| GlyphEntry {
        glyph_id,
        atlas_x: 0,
        atlas_y: 0,
        atlas_w: 4,
        atlas_h: 4,
        layer,
        advance_x: 0.5,
        baseline_row: 2.5,
        px_per_em: 3.0,
        x_margin: 0.6,
    };
    atlas.insert_entry(cell(66, 0));
    atlas.insert_entry(cell(GlyphStyle::Bold.styled_glyph_id(66).unwrap(), 1));
    atlas
}

/// **A multi-layer atlas is written at v4 and read back whole** - the layer
/// table, every entry's layer, and every layer's pixels.
#[test]
fn a_layered_atlas_round_trips() {
    let atlas = two_layer_atlas();
    let bytes = atlas.to_bytes();
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        ATLAS_VERSION_LAYERED,
        "more than one layer must not be written at the single-layer version"
    );

    let back = FontAtlas::from_bytes(&bytes).expect("a v4 atlas loads");
    assert_eq!(back.layers, atlas.layers);
    assert_eq!(back.glyphs, atlas.glyphs);
    assert_eq!(back.pixel_data, atlas.pixel_data);
    assert_eq!(back.layer_count(), 2);
    assert_eq!(back.layer_index(AtlasLayer::new(4, GlyphStyle::Bold)), Ok(1));
    assert_eq!(back.to_bytes(), bytes, "the round trip is not a fixed point");
}

/// **A v4 file is refused by a build that reads only v3**, which is the whole
/// reason layers cost a version.
///
/// The pixel data is the reason and it is the thing this states: a v3 reader
/// computes `w * h * channels` and would find MORE than that, pass its length
/// check, take the first layer, and load an atlas that draws every layer-0
/// glyph correctly and every other glyph from whatever coordinate landed on.
/// The version stops that; the length alone would not have.
#[test]
fn a_layered_file_could_not_have_been_read_as_a_single_layer_one() {
    let bytes = two_layer_atlas().to_bytes();
    let single_layer_pixels = 4 * 4 * 3;
    assert!(
        bytes.len() > libmsdf::ATLAS_HEADER_SIZE + single_layer_pixels,
        "vacuity: this file is not longer than a single-layer one"
    );
    assert_ne!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        ATLAS_VERSION,
        "a v3 reader would have accepted this file's version and misread its pixels"
    );
}

/// An entry naming a layer the file does not declare is refused, not clamped.
/// A GPU array index out of range is a clamp on some drivers, which draws the
/// wrong glyph rather than failing - so this must never reach a frame.
#[test]
fn an_entry_naming_an_undeclared_layer_is_refused() {
    let mut atlas = two_layer_atlas();
    atlas.layers.pop();
    // The pixels still describe two layers; only the table says one.
    let bytes = atlas.to_bytes();
    let err = FontAtlas::from_bytes(&bytes).expect_err("this must not load");
    assert!(err.contains("layer"), "{err}");
}

/// A file declaring more layers than the guarantee is refused with the number
/// named, rather than allocating whatever it asks for.
#[test]
fn a_file_declaring_more_layers_than_the_guarantee_is_refused() {
    let mut bytes = two_layer_atlas().to_bytes();
    let n = (MAX_ATLAS_LAYERS as u32) + 1;
    bytes[libmsdf::ATLAS_HEADER_SIZE..libmsdf::ATLAS_HEADER_SIZE + 4]
        .copy_from_slice(&n.to_le_bytes());
    let err = FontAtlas::from_bytes(&bytes).expect_err("this must not load");
    assert!(err.contains("256"), "{err}");
}

// ── the seam: one formula for where a cell is ───────────────────────────

/// **A cell's texels come from the cell's OWN layer.**
///
/// The layer is a second term in an address that used to have one, and a
/// reader that forgets it gets plausible bytes from the wrong layer instead of
/// an error - which is why `cell_texels` is published rather than written out
/// at each call site. Both cells here sit at (0, 0) of their layer, so a
/// layer-blind read returns the SAME bytes for both and this fails.
#[test]
fn a_cells_texels_come_from_its_own_layer() {
    let atlas = two_layer_atlas();
    let bold = GlyphStyle::Bold.styled_glyph_id(66).unwrap();
    let regular_texels = atlas.cell_texels(66).expect("a cell for 66");
    let bold_texels = atlas.cell_texels(bold).expect("a cell for bold 66");

    assert_eq!(atlas.get_glyph(66).unwrap().atlas_x, atlas.get_glyph(bold).unwrap().atlas_x);
    assert_eq!(atlas.get_glyph(66).unwrap().atlas_y, atlas.get_glyph(bold).unwrap().atlas_y);
    assert!(regular_texels.iter().all(|&b| b == 0x11), "layer 0 was not read");
    assert!(bold_texels.iter().all(|&b| b == 0x99), "layer 1 was not read");
    assert_eq!(regular_texels.len(), 4 * 4 * 3);
}

/// `layer_offset` is the one place the layer stride is written down, and the
/// pixel buffer is exactly as long as it says.
#[test]
fn the_layer_stride_has_one_definition() {
    let atlas = two_layer_atlas();
    assert_eq!(atlas.layer_offset(0), 0);
    assert_eq!(atlas.layer_offset(1), 4 * 4 * 3);
    assert_eq!(atlas.layer_offset(atlas.layer_count() as u16), atlas.pixel_data.len());
}

// ── the bake, for real (native + msdfgen) ───────────────────────────────

/// ```text
/// CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm cargo test --features cpu-bake
/// ```
#[cfg(all(feature = "cpu-bake", not(target_arch = "wasm32")))]
mod baked {
    use super::*;
    use libmsdf::font::{
        FontAtlasBuilder, GlyphSet, ROBOTO_ASCII_MSYMBOLS, ROBOTO_BOLD_ASCII, ROBOTO_ITALIC_ASCII,
        atlas_capacity,
    };

    /// Small cells and a small range: this is a test about LAYERS and cell
    /// placement, not about field quality, and 634 cells of msdfgen at 48px
    /// would be a minute of it.
    const GS: u32 = 16;
    const PX_RANGE: f64 = 2.0;

    fn shipped_only() -> FontAtlas {
        let mut b = FontAtlasBuilder::new(ROBOTO_ASCII_MSYMBOLS.to_vec(), GS, PX_RANGE);
        b.add_shipped_coverage();
        b.build().expect("the shipped coverage bakes")
    }

    fn shipped_with_both_styles() -> FontAtlas {
        let mut b = FontAtlasBuilder::new(ROBOTO_ASCII_MSYMBOLS.to_vec(), GS, PX_RANGE);
        b.add_shipped_coverage();
        b.add_styled_coverage(GlyphStyle::Bold, ROBOTO_BOLD_ASCII.to_vec())
            .expect("the bold face parses");
        b.add_styled_coverage(GlyphStyle::Italic, ROBOTO_ITALIC_ASCII.to_vec())
            .expect("the italic face parses");
        b.build().expect("a styled bake fits now - that is the point of the wave")
    }

    /// **THE UNBLOCKING TEST.** The full shipped coverage plus both bundled
    /// style faces is 634 cells. No single grid holds it - 320 is the pin, and
    /// `ATLAS_ROWS` cannot grow because 41 rows is 2050px against a 2048 floor.
    /// Three layers hold it with room in each, and the texture is the same
    /// rectangle it was.
    #[test]
    fn the_full_shipped_coverage_and_both_styles_bake_into_three_layers() {
        let atlas = shipped_with_both_styles();
        assert_eq!(
            atlas.layers,
            vec![
                AtlasLayer::new(GS as u16, GlyphStyle::Regular),
                AtlasLayer::new(GS as u16, GlyphStyle::Bold),
                AtlasLayer::new(GS as u16, GlyphStyle::Italic),
            ],
            "the layer table is not (size, style) in style order"
        );
        assert!(
            atlas.glyphs.len() > atlas_capacity(),
            "vacuity: {} cells would have fitted one layer, so this proves nothing",
            atlas.glyphs.len()
        );

        // Every layer inside the pin, and every cell in the layer its STYLE
        // names - the style is in the address, so this checks the two agree.
        for (i, layer) in atlas.layers.iter().enumerate() {
            let n = atlas.glyphs.iter().filter(|e| e.layer as usize == i).count();
            assert!(n <= atlas_capacity(), "layer {i} ({layer}) holds {n} cells");
            assert!(n > 0, "layer {i} ({layer}) is empty");
        }
        for e in &atlas.glyphs {
            // Glyph 0 is the one style-invariant cell and lives in layer 0.
            let style = GlyphStyle::split_glyph_id(e.glyph_id).0;
            assert_eq!(
                atlas.layers[e.layer as usize],
                AtlasLayer::new(GS as u16, style),
                "glyph {} is a {style:?} cell in layer {}",
                e.glyph_id,
                e.layer
            );
        }
        assert_eq!(atlas.layer_index(AtlasLayer::new(GS as u16, GlyphStyle::Bold)), Ok(1));
        // ...and the cut nothing was baked for is still refused, on both axes.
        assert!(atlas.layer_index(AtlasLayer::new(GS as u16, GlyphStyle::BoldItalic)).is_err());
        assert!(atlas.layer_index(AtlasLayer::new(48, GlyphStyle::Regular)).is_err());
    }

    /// **Adding two styles moved nothing in layer 0** - not an entry, not a
    /// cell coordinate, not a texel, not the texture's dimensions.
    ///
    /// Compared where it actually matters: the pixels the shader samples, over
    /// the whole layer rather than cell by cell, so a texel outside every cell
    /// is covered too.
    #[test]
    fn a_styled_bake_moves_no_texel_of_layer_zero() {
        let (plain, styled) = (shipped_only(), shipped_with_both_styles());
        assert_eq!(plain.width, styled.width, "the texture width moved");
        assert_eq!(plain.height, styled.height, "the texture height moved");
        assert_eq!(plain.layer_count(), 1);
        assert_eq!(styled.layer_count(), 3);

        for entry in &plain.glyphs {
            let after = styled
                .get_glyph(entry.glyph_id)
                .unwrap_or_else(|| panic!("glyph {} lost its cell", entry.glyph_id));
            assert_eq!(entry, after, "glyph {} changed", entry.glyph_id);
        }
        // The whole of layer 0, texel for texel.
        assert_eq!(
            plain.pixel_data,
            styled.pixel_data[..styled.layer_offset(1)],
            "layer 0's texels changed when a style was added"
        );
        // And the unstyled set headers are untouched: the styled sets are rows
        // added after them, not a re-measurement of them.
        for set in [GlyphSet::Placeholder, GlyphSet::Text, GlyphSet::BorrowedIcons] {
            assert_eq!(plain.set_metrics(set), styled.set_metrics(set), "{set:?}");
        }
    }

    /// A styled bake survives the file: three layers out, three layers back,
    /// and every cell still in the layer its style names.
    #[test]
    fn a_styled_bake_round_trips_through_the_file() {
        let atlas = shipped_with_both_styles();
        let bytes = atlas.to_bytes();
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), ATLAS_VERSION_LAYERED);
        let back = FontAtlas::from_bytes(&bytes).expect("a styled atlas loads");
        assert_eq!(back.layers, atlas.layers);
        assert_eq!(back.glyphs, atlas.glyphs);
        assert_eq!(back.pixel_data, atlas.pixel_data);
        assert_eq!(back.sets, atlas.sets);
    }
}

// ── the layer reaches the GPU ───────────────────────────────────────────

/// **On a device, the shader samples the layer the TABLE named.**
///
/// Everything above is CPU-side bookkeeping; this is the half that a rendered
/// frame depends on. Two glyphs share one cell rectangle and differ ONLY in
/// their layer, and the two layers hold opposite fields - so a shader that
/// ignored `g1.w`, or clamped it, would draw both the same and this fails.
///
/// ```text
/// CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm cargo test --features gpu-tests
/// ```
#[cfg(feature = "gpu-tests")]
mod on_the_gpu {
    use super::*;
    use libmsdf::drawlist::DrawList;
    use libmsdf::font::{ShapedGlyph, ShapedRun};
    use libmsdf::gpu::GpuSdfRenderer;

    const CELL: u32 = 32;
    const W: u32 = 128;
    const H: u32 = 64;
    const PX_RANGE: f32 = 6.0;

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

    /// Two layers, one cell each at the same coordinates: layer 0 entirely
    /// OUTSIDE the field (draws nothing), layer 1 entirely INSIDE (draws).
    fn opposed_layers() -> FontAtlas {
        let layers = [
            AtlasLayer::new(CELL as u16, GlyphStyle::Regular),
            AtlasLayer::new(CELL as u16, GlyphStyle::Bold),
        ];
        let mut atlas = FontAtlas::empty_layered(64, 64, 3, &layers);
        let per_layer = 64 * 64 * 3;
        atlas.pixel_data[..per_layer].fill(0x00);
        atlas.pixel_data[per_layer..].fill(0xFF);
        for (glyph_id, layer) in [(1u16, 0u16), (2, 1)] {
            atlas.insert_entry(GlyphEntry {
                glyph_id,
                atlas_x: 0,
                atlas_y: 0,
                atlas_w: CELL as u16,
                atlas_h: CELL as u16,
                layer,
                advance_x: 1.0,
                baseline_row: CELL as f32 * 0.7,
                px_per_em: CELL as f32 / 1.3,
                x_margin: 0.0,
            });
        }
        atlas
    }

    fn one_glyph(glyph_id: u16) -> ShapedRun {
        ShapedRun {
            glyphs: vec![ShapedGlyph { glyph_id, x_advance: 1000, x_offset: 0, y_offset: 0, cluster: 0 }],
            total_advance: 1000,
            units_per_em: 1000,
        }
    }

    /// Renders the two glyphs side by side and returns the frame.
    fn render() -> Option<Vec<u8>> {
        let (device, queue) = gpu()?;
        let atlas = opposed_layers();
        let renderer = GpuSdfRenderer::new_with_msdf_layers(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            atlas.width,
            atlas.height,
            atlas.layer_count(),
        );
        assert_eq!(renderer.msdf_atlas_layers(), 2, "the texture was not created with two layers");
        renderer.upload_msdf_atlas(&queue, atlas.width, atlas.height, &atlas.to_rgba_bytes());
        renderer.upload_glyph_table(&queue, &atlas.glyph_table_u32s());

        let mut list = DrawList::new();
        list.push_shaped_text(&one_glyph(1), &atlas, [0.0, 0.0], CELL as f32, PX_RANGE, [1.0; 4]);
        list.push_shaped_text(&one_glyph(2), &atlas, [64.0, 0.0], CELL as f32, PX_RANGE, [1.0; 4]);

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        renderer.render_draw_list(&device, &queue, &view, W, H, 1.0, &list, 0.0);

        let bpp = 4u32;
        let padded_row = (W * bpp).next_multiple_of(256);
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
            wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
        );
        queue.submit(std::iter::once(enc.finish()));

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let data = slice.get_mapped_range();
        let mut img = Vec::with_capacity((W * H * bpp) as usize);
        for row in 0..H {
            let start = (row * padded_row) as usize;
            img.extend_from_slice(&data[start..start + (W * bpp) as usize]);
        }
        drop(data);
        readback.unmap();
        Some(img)
    }

    #[test]
    fn the_shader_samples_the_layer_the_table_named() {
        let Some(img) = render() else {
            eprintln!("SKIP: no GPU adapter");
            return;
        };
        let at = |x: u32, y: u32| img[((y * W + x) * 4) as usize];
        // Middle of each glyph's cell.
        let layer0 = at(16, 16);
        let layer1 = at(80, 16);
        assert_eq!(layer0, 0, "the layer-0 glyph drew ink from an all-outside field");
        assert!(
            layer1 > 200,
            "the layer-1 glyph drew {layer1}, not the all-inside field of its own layer - the \
             shader read layer 0, or clamped the array index"
        );
    }
}
