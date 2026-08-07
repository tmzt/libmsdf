//! The edge-marker primitive, checked against the SHIPPED FACES and the REAL
//! BAKED ATLAS — and, where a GPU is available, against the rendered pixels.
//!
//! `push_marker` reads three numbers off the baked cell (`advance_x`,
//! `baseline_row`, `atlas_h`) and writes none of them down, so the geometry
//! lives entirely in `fonts/marker.py`. That is only safe if something asserts
//! the font still says what Rust assumes, against the bytes that ship rather
//! than against a rebuild. This file is that something.

use libmsdf::drawlist::{LINE_BOX_RATIO, X_MARGIN_FRAC};
use libmsdf::font::{
    FontAtlas, MARKERS, MARKER_ARROW, MSYMBOLS_ICONS, PRIVATE_USE, ROBOTO_ASCII_MSYMBOLS,
    ROBOTO_REGULAR_ASCII, TextShaper,
};
use libmsdf::DrawList;

const ATLAS_FIXTURE: &[u8] = include_bytes!("fixtures/roboto-ascii-48.atlas");
const PX_RANGE: f32 = 6.0;

fn atlas() -> FontAtlas {
    FontAtlas::from_bytes(ATLAS_FIXTURE).expect("fixture atlas parses")
}

fn shaper() -> TextShaper {
    TextShaper::new(ROBOTO_REGULAR_ASCII.to_vec()).expect("the shipped face parses")
}

fn marker_run(shaper: &TextShaper, ch: char) -> libmsdf::font::ShapedRun {
    shaper.shape(ch.encode_utf8(&mut [0u8; 4]))
}

/// The MSDF median at one texel — above 0.5 is inside the glyph.
fn median_at(a: &FontAtlas, x: u32, y: u32) -> f32 {
    let o = ((y * a.width + x) * a.channels) as usize;
    let (r, g, b) = (
        a.pixel_data[o] as f32 / 255.0,
        a.pixel_data[o + 1] as f32 / 255.0,
        a.pixel_data[o + 2] as f32 / 255.0,
    );
    r.min(g).max(r.max(g).min(b))
}

// ── the carveout ────────────────────────────────────────────────────────

/// **A borrowed icon and a drawn marker can never land on the same
/// codepoint.** Material Symbols' codepoints are the vendor's, so the only
/// defence is where OUR block sits: above all of them, at the top of the
/// Private Use Area. That also keeps the bake queue append-only — the PUA is
/// scanned in codepoint order, so a marker added above every icon leaves every
/// icon's cell exactly where it was.
#[test]
fn icons_sort_below_the_marker_block() {
    let (pua_lo, pua_hi) = PRIVATE_USE;
    let (m_lo, m_hi) = MARKERS;
    assert!(m_lo >= pua_lo && m_hi <= pua_hi, "the marker block escapes the carveout");
    assert!(m_hi == pua_hi, "markers are allocated from the TOP of the carveout down to m_lo");
    for &(name, cp) in MSYMBOLS_ICONS {
        assert!(cp < m_lo, "icon {name:?} at U+{:04X} is inside the marker block", cp as u32);
    }
    assert!(MARKER_ARROW >= m_lo && MARKER_ARROW <= m_hi);
}

// ── the design contract the font declares and Rust relies on ────────────

/// **The marker glyphs still have the shape `push_marker` assumes**, read off
/// the shipped face bytes.
///
/// `fonts/marker.py` is where the geometry is decided; this is the assertion
/// that a redesign there which forgot Rust fails a test rather than drawing
/// every arrowhead a few pixels off its curve. Both faces, because a marker is
/// geometry the repo draws with rather than an icon set, so the two bundled
/// faces carry the identical block.
#[test]
fn marker_contract_holds() {
    for (label, bytes) in [
        ("plain", ROBOTO_REGULAR_ASCII),
        ("merged", ROBOTO_ASCII_MSYMBOLS),
    ] {
        let face = ttf_parser::Face::parse(bytes, 0).expect("the shipped face parses");
        let upem = face.units_per_em();
        for cp in (MARKERS.0 as u32)..=(MARKERS.1 as u32) {
            let ch = char::from_u32(cp).unwrap();
            let Some(gid) = face.glyph_index(ch) else { continue };
            let bbox = face.glyph_bounding_box(gid).expect("a marker has an outline");
            let advance = face.glyph_hor_advance(gid).expect("a marker has an advance") as i16;
            assert_eq!(
                (bbox.x_min, bbox.x_max),
                (0, advance),
                "{label} U+{cp:04X}: ink must span x in [0, advance] — the anchor is the \
                 advance-width point, so ink outside that puts the point off the curve",
            );
            assert_eq!(
                bbox.y_min, -bbox.y_max,
                "{label} U+{cp:04X}: ink must be symmetric about the baseline — the anchor \
                 sits ON the baseline, and rotating about it swings an asymmetric marker \
                 off its edge",
            );
            // The cell has to hold the ink AND `px_range/2` of field around it,
            // or the lower edge stops antialiasing (see fonts/marker.py).
            let em_scale = 48.0 / (upem as f32 * LINE_BOX_RATIO);
            let baseline_row = 48.0 * 0.15 + em_scale * 1456.0; // cap height of 'A'
            let bottom_margin = 48.0 - (baseline_row + bbox.y_max as f32 * em_scale);
            assert!(
                bottom_margin >= PX_RANGE / 2.0,
                "{label} U+{cp:04X}: only {bottom_margin:.2}px of cell below the ink, \
                 under the {:.1}px the distance field needs",
                PX_RANGE / 2.0,
            );
        }
    }
}

/// The arrow shapes, is baked, and its cell has INK — the three separate ways a
/// glyph goes missing, asked together the way `coverage.rs` asks them of text.
#[test]
fn the_arrow_shapes_and_is_baked_with_ink() {
    let (shaper, atlas) = (shaper(), atlas());
    assert!(shaper.covers(MARKER_ARROW), "the shipped face has no arrow glyph");
    let run = marker_run(&shaper, MARKER_ARROW);
    assert_eq!(run.glyphs.len(), 1, "a marker is one glyph");
    assert_eq!(run.notdef_count(), 0);
    let gid = run.glyphs[0].glyph_id;
    let e = *atlas.get_glyph(gid).expect("the bake has no cell for the arrow");
    assert!(
        (0..e.atlas_h as u32)
            .flat_map(|dy| (0..e.atlas_w as u32).map(move |dx| (dx, dy)))
            .any(|(dx, dy)| median_at(&atlas, e.atlas_x as u32 + dx, e.atlas_y as u32 + dy) > 0.5),
        "the arrow baked to a blank cell",
    );
    // The two bundled faces agree about it, as they do about text.
    let merged = TextShaper::new(ROBOTO_ASCII_MSYMBOLS.to_vec()).expect("the merged face parses");
    assert!(merged.covers(MARKER_ARROW));
    assert_eq!(
        marker_run(&merged, MARKER_ARROW).total_advance,
        run.total_advance,
        "the faces disagree about how long the arrow is",
    );
}

/// **The cell is a triangle pointing +x**: ink at the right edge of the ink
/// bounds is a single row tall, ink at the left edge spans the full height.
///
/// Read off the baked field rather than the font, because "the font has a
/// triangle" and "the atlas drew one" are different claims and only the second
/// reaches a screen. It is also the assertion a flipped contour winding would
/// fail — an inverted field inks the cell's CORNERS instead of its middle.
#[test]
fn the_baked_arrow_cell_is_a_triangle_pointing_forward() {
    let (shaper, atlas) = (shaper(), atlas());
    let gid = marker_run(&shaper, MARKER_ARROW).glyphs[0].glyph_id;
    let e = *atlas.get_glyph(gid).unwrap();
    let inked: Vec<(u32, u32)> = (0..e.atlas_h as u32)
        .flat_map(|y| (0..e.atlas_w as u32).map(move |x| (x, y)))
        .filter(|&(x, y)| median_at(&atlas, e.atlas_x as u32 + x, e.atlas_y as u32 + y) > 0.5)
        .collect();
    assert!(!inked.is_empty());
    let column_height = |col: u32| inked.iter().filter(|p| p.0 == col).count();
    let (x0, x1) = (
        inked.iter().map(|p| p.0).min().unwrap(),
        inked.iter().map(|p| p.0).max().unwrap(),
    );
    assert!(
        column_height(x0) > 4 * column_height(x1).max(1),
        "the tail ({} px tall) is not much taller than the point ({} px) — this is not a \
         triangle pointing +x",
        column_height(x0),
        column_height(x1),
    );
    // Monotonically narrowing, which a diamond or a flipped field would not be.
    let (mid, quarter) = (column_height((x0 + x1) / 2), column_height((3 * x0 + x1) / 4));
    assert!(column_height(x0) >= quarter && quarter >= mid && mid >= column_height(x1));
}

// ── placement, through the real baked numbers ───────────────────────────

/// The end-to-end version of the unit test's claim, with the real cell's
/// metrics: at any size, the point of the drawn arrow is the anchor.
#[test]
fn the_shipped_arrow_points_at_its_anchor() {
    let (shaper, atlas) = (shaper(), atlas());
    let run = marker_run(&shaper, MARKER_ARROW);
    let e = *atlas.get_glyph(run.glyphs[0].glyph_id).unwrap();
    for &size in &[5.0f32, 9.0, 18.0, 64.0] {
        let anchor = [412.5f32, 96.25];
        let mut list = DrawList::new();
        let rect = list
            .push_marker(&run, &atlas, anchor, 0.0, size, PX_RANGE, [1.0; 4])
            .expect("the shipped bake carries the arrow");
        let inst = list.instances[0];
        let line_h = inst.size[1];
        let pen_x = inst.position[0] + X_MARGIN_FRAC * line_h;
        let baseline_y = inst.position[1] + e.baseline_row * line_h / e.atlas_h as f32;
        assert!((pen_x + size - anchor[0]).abs() < 1e-3, "size {size}");
        assert!((baseline_y - anchor[1]).abs() < 1e-3, "size {size}");
        // Unrotated, the bound is the instance itself. Its ORIGIN is exact —
        // `Radians(0.0)` builds an identity matrix from `sin_cos`, whose entries
        // really are 0.0/1.0 at zero. Its EXTENT is not, and that is a property
        // of `rotate_rect` rather than of this call: it bounds the four
        // transformed corners and returns `max - min`, and `(pos + size) - pos`
        // re-rounds. Sub-ulp, and nothing downstream is exact-comparing an AABB.
        assert_eq!(rect.0, inst.position, "size {size}");
        assert!((rect.1[0] - inst.size[0]).abs() < 1e-4, "size {size}");
        assert!((rect.1[1] - inst.size[1]).abs() < 1e-4, "size {size}");
    }
}

// ── the rendered sweep ──────────────────────────────────────────────────

#[cfg(feature = "gpu-tests")]
mod rendered {
    use super::*;

    const WIDTH: u32 = 1460;
    const HEIGHT: u32 = 560;
    /// Angles the sweep draws — twelve around the circle, so both
    /// axis-aligned and fully oblique tangents are covered.
    const ANGLE_STEPS: u32 = 12;
    const SIZES: [f32; 4] = [8.0, 14.0, 24.0, 44.0];
    /// Grid spacing, chosen so a neighbour's ink can never enter a cell's
    /// search radius: the radius is ~1.15x the marker's length, a neighbour's
    /// tail reaches at most its own length toward us, and the largest marker is
    /// 44px. Getting this wrong does not fail loudly — it silently measures the
    /// wrong arrow, which is exactly what it did on the first run.
    const DX: f32 = 120.0;
    const DY: f32 = 130.0;

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

    /// One anchor per (size, angle) cell of the sweep grid.
    fn anchor(row: usize, step: u32) -> [f32; 2] {
        [80.0 + step as f32 * DX, 100.0 + row as f32 * DY]
    }

    /// `crosshairs` draws a one-pixel cross AT each anchor, so a reader can see
    /// where the point was asked to go. The MEASURED render leaves them off:
    /// the cross sits exactly under the point it is marking, so anything that
    /// thresholded ink with it present would be thresholding a blend.
    fn scene(atlas: &FontAtlas, crosshairs: bool) -> DrawList {
        let shaper = shaper();
        let run = marker_run(&shaper, MARKER_ARROW);
        let mut list = DrawList::new();
        // **An OPAQUE background, and it is load-bearing rather than
        // decorative.** The pipeline blends `REPLACE` and the shader discards
        // uncovered fragments, so over the cleared target a white run comes
        // back as flat white RGB with the coverage in the ALPHA channel — an
        // image that looks like a hard-edged binary mask, and measures like one
        // too. Compositing inside the draw list instead puts the antialiasing
        // where both a reader and a threshold can see it.
        list.push(libmsdf::SdfInstance {
            kind: libmsdf::SdfKind::Box,
            position: [0.0, 0.0],
            size: [WIDTH as f32, HEIGHT as f32],
            color: [0.06, 0.06, 0.08, 1.0],
            anim: 0,
        });
        for (row, &size) in SIZES.iter().enumerate() {
            for step in 0..ANGLE_STEPS {
                let a = anchor(row, step);
                let angle = step as f32 * std::f32::consts::TAU / ANGLE_STEPS as f32;
                if crosshairs {
                    for (pos, sz) in [
                        ([a[0] - 14.0, a[1]], [29.0, 1.0]),
                        ([a[0], a[1] - 14.0], [1.0, 29.0]),
                    ] {
                        list.push(libmsdf::SdfInstance {
                            kind: libmsdf::SdfKind::Box,
                            position: pos,
                            size: sz,
                            color: [1.0, 0.25, 0.25, 1.0],
                            anim: 0,
                        });
                    }
                }
                list.push_marker(&run, atlas, a, angle, size, PX_RANGE, [1.0, 1.0, 1.0, 1.0])
                    .expect("the shipped bake carries the arrow");
            }
        }
        list
    }

    fn render(list: &DrawList, atlas: &FontAtlas) -> Option<Vec<u8>> {
        let (device, queue) = gpu()?;
        let renderer = libmsdf::GpuSdfRenderer::new_with_msdf(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            atlas.width,
            atlas.height,
        );
        renderer.upload_msdf_atlas(&queue, atlas.width, atlas.height, &atlas.to_rgba_bytes());
        renderer.upload_glyph_table(&queue, &atlas.glyph_table_u32s());

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("marker-sweep"),
            size: wgpu::Extent3d { width: WIDTH, height: HEIGHT, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        renderer.render_draw_list(&device, &queue, &view, WIDTH, HEIGHT, 1.0, list, 0.0);

        let bpp = 4u32;
        let padded_row = (WIDTH * bpp).next_multiple_of(256);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rb"),
            size: (padded_row * HEIGHT) as u64,
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
            wgpu::Extent3d { width: WIDTH, height: HEIGHT, depth_or_array_layers: 1 },
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
        let mut img = Vec::with_capacity((WIDTH * HEIGHT * bpp) as usize);
        for row in 0..HEIGHT {
            let start = (row * padded_row) as usize;
            img.extend_from_slice(&data[start..start + (WIDTH * bpp) as usize]);
        }
        drop(data);
        readback.unmap();
        Some(img)
    }

    /// **The arrow's point lands on its anchor, at every angle and size, in the
    /// PIXELS** — the claim the CPU tests make about numbers, made about ink.
    ///
    /// For each cell of the sweep the coverage is projected onto the arrow's
    /// own axis and three things are measured: where the ink's MASS sits (a
    /// triangle's centroid is two thirds back from its apex), how much ink
    /// there is (a triangle `size` long and `0.8*size` wide), and how far
    /// forward of the anchor any ink reaches. Together those pin the point
    /// sub-pixel and pin it as the LEADING feature, which is what distinguishes
    /// an arrow anchored by its point from one anchored by its centre.
    ///
    /// `LIBMSDF_DUMP=<dir>` writes the frame out to be READ, which is the other
    /// half of checking this and is not something a threshold can do.
    #[test]
    fn the_rendered_arrow_points_at_its_anchor_at_every_angle() {
        let atlas = atlas();
        let Some(img) = render(&scene(&atlas, false), &atlas) else {
            eprintln!("SKIP: no GPU adapter");
            return;
        };

        if let Ok(dir) = std::env::var("LIBMSDF_DUMP") {
            std::fs::create_dir_all(&dir).unwrap();
            for (name, list) in [
                ("marker-sweep", scene(&atlas, true)),
                ("marker-sweep-bare", scene(&atlas, false)),
            ] {
                let frame = render(&list, &atlas).expect("the adapter was there a moment ago");
                let path = format!("{dir}/{name}.png");
                let file = std::fs::File::create(&path).unwrap();
                let mut enc = png::Encoder::new(file, WIDTH, HEIGHT);
                enc.set_color(png::ColorType::Rgba);
                enc.set_depth(png::BitDepth::Eight);
                enc.write_header().unwrap().write_image_data(&frame).unwrap();
                eprintln!("DUMPED {path}");
            }
        }

        // Green against the dark background is the marker's coverage. The
        // background contributes a floor, so it is subtracted out rather than
        // thresholded away: what follows measures with weights, not with a
        // yes/no test, which is what makes it sub-pixel.
        const BG: f32 = 0.06 * 255.0;
        let cover = |x: u32, y: u32| {
            let g = img[((y * WIDTH + x) * 4 + 1) as usize] as f32;
            ((g - BG) / (255.0 - BG)).clamp(0.0, 1.0)
        };
        let shaper = shaper();
        let e = *atlas
            .get_glyph(marker_run(&shaper, MARKER_ARROW).glyphs[0].glyph_id)
            .unwrap();
        let mut worst_centroid = 0.0f32;
        let mut worst_lead = f32::MIN;
        for (row, &size) in SIZES.iter().enumerate() {
            for step in 0..ANGLE_STEPS {
                let a = anchor(row, step);
                let angle = step as f32 * std::f32::consts::TAU / ANGLE_STEPS as f32;
                let (s, c) = angle.sin_cos();
                // The marker reaches `size` back along its axis and 0.4*size
                // across, so 1.15*size bounds it whatever the rotation; the
                // grid spacing keeps a neighbour's ink outside this.
                let r = (size * 1.15).ceil() as i32 + 4;
                let (mut area, mut m_along, mut m_across) = (0.0f32, 0.0f32, 0.0f32);
                let mut lead = f32::MIN;
                for dy in -r..=r {
                    for dx in -r..=r {
                        let (x, y) = (a[0] as i32 + dx, a[1] as i32 + dy);
                        if x < 0 || y < 0 || x >= WIDTH as i32 || y >= HEIGHT as i32 {
                            continue;
                        }
                        let w = cover(x as u32, y as u32);
                        if w <= 0.02 {
                            continue;
                        }
                        // Pixel centres, so the comparison is against the same
                        // continuous coordinates the draw list was given.
                        let p = [x as f32 + 0.5 - a[0], y as f32 + 0.5 - a[1]];
                        let along = p[0] * c + p[1] * s;
                        area += w;
                        m_along += w * along;
                        m_across += w * (-p[0] * s + p[1] * c);
                        lead = lead.max(along);
                    }
                }
                assert!(area > 1.0, "size {size} step {step}: no marker ink near the anchor");

                // **Where the ink's mass is.** A triangle's centroid sits two
                // thirds of its height back from the apex, on its axis — so
                // this pins the point sub-pixel without ever having to decide
                // which pixel the apex "is", which at a 44-degree corner is a
                // question the pixel grid answers to within a pixel and a half
                // no matter how exact the drawing.
                //
                // Minus HALF AN ATLAS TEXEL on each of the glyph's own axes,
                // which is not this primitive's doing and is not corrected
                // here. `sdf_render.wgsl` case 8u samples at
                // `(atlas_g + ac + 0.5) / atlas_dim`, so a screen point at
                // `ac = k` reads texel k's CENTRE — the value that belongs at
                // `ac = k + 0.5`. Every MSDF glyph in the engine is therefore
                // drawn half a texel up and to the left of where its metrics
                // put it, which is `0.5 * line_h / cell_px` screen pixels and
                // scales with size: 0.17px on 8px type, invisible, and 0.95px
                // on the 44px marker below, which is why it shows up here
                // first. Compensating for it in ONE primitive would put the
                // marker half a texel off from every letter beside it; the fix
                // belongs in the shader, where it moves every frame in the
                // repo at once. Written into the expectation rather than
                // hidden in a tolerance, so a shader that stops doing it fails
                // HERE, with this comment attached.
                let half_texel = 0.5 * (size / e.advance_x) * LINE_BOX_RATIO / e.atlas_h as f32;
                let (ca, cx) = (m_along / area, m_across / area);
                let off = (ca + size * 2.0 / 3.0 + half_texel).hypot(cx + half_texel);
                worst_centroid = worst_centroid.max(off);
                assert!(
                    off <= 0.35,
                    "size {size} step {step}: ink centroid is {off:.2}px from where a \
                     triangle anchored here would put it (along {ca:.2}, across {cx:.2}, \
                     half-texel {half_texel:.2})",
                );
                // ...and it is the POINT that leads, not the tail: no ink may
                // sit forward of the anchor beyond the antialiasing that
                // straddles it. This is the half that would fail if the marker
                // were anchored by its centre or its tail instead.
                worst_lead = worst_lead.max(lead);
                assert!(lead <= 1.0, "size {size} step {step}: ink runs {lead:.2}px past the anchor");
                // Scale: a triangle `size` long and 0.8*size wide.
                let expect_area = 0.4 * size * size;
                assert!(
                    (area - expect_area).abs() <= 0.12 * expect_area,
                    "size {size} step {step}: {area:.1}px of ink, expected about {expect_area:.1}",
                );
            }
        }
        eprintln!(
            "sweep: worst centroid error {worst_centroid:.2}px, \
             furthest ink past the anchor {worst_lead:.2}px"
        );
    }
}
