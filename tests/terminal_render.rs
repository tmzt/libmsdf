//! **The terminal faces on the GPU** (`docs/design/20261023_01_TERMINAL_FONTS_PLAN.md`,
//! section 5 and step 6): box drawing joins with no seam between cells or
//! lines, Braille spinner frames keep their dots and the text after them in
//! place, and Bold `Å` keeps its ring, at phone sizes.
//!
//! ```text
//! CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm cargo test \
//!     --features gpu-tests,fonts-ofl-terminal --test terminal_render
//! ```
//!
//! Each case's frame is written to `out/terminal/` as a PNG.

#![cfg(all(
    feature = "gpu-tests",
    feature = "font-ofl-noto-sans-mono",
    feature = "font-ofl-noto-terminal-symbols"
))]

use libmsdf::gpu::GpuSdfRenderer;
use libmsdf::{AtlasId, AtlasSet, DrawList, FontAtlas, GlyphStyle, TerminalShaper};

const SIZES: [f32; 5] = [12.0, 14.0, 16.0, 20.0, 24.0];
const SCALES: [f32; 3] = [1.0, 2.0, 3.0];
const WHITE: [f32; 4] = [1.0; 4];
const PX_RANGE: f32 = 6.0;

struct Terminal {
    set: AtlasSet,
    text: AtlasId,
    page: AtlasId,
    shaper: TerminalShaper,
}

fn terminal() -> Terminal {
    let text = FontAtlas::from_bytes(libmsdf::NOTO_SANS_MONO_ATLAS).unwrap();
    let page = FontAtlas::from_bytes(libmsdf::TERMINAL_SYMBOLS_ATLAS).unwrap();
    let shaper = TerminalShaper::new(&text, &page).unwrap();
    let mut set = AtlasSet::new();
    let text = set.register(text).unwrap();
    let page = set.register(page).unwrap();
    Terminal {
        set,
        text,
        page,
        shaper,
    }
}

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

/// A rendered frame: physical pixels, one coverage byte each (the alpha
/// channel where a glyph drew: the renderer replaces a drawn pixel with the
/// glyph colour and its coverage as alpha, and leaves an undrawn one the
/// opaque black clear).
struct Frame {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Frame {
    fn at(&self, x: u32, y: u32) -> u8 {
        self.px[(y * self.w + x) as usize]
    }

    /// Write the frame to `out/terminal/<name>.png` (8-bit grey, stored).
    fn save(&self, name: &str) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("out/terminal");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{name}.png")),
            png(self.w, self.h, &self.px),
        )
        .unwrap();
    }
}

/// Draw `lines` (each a string at a line-box top-left, logical pixels) at
/// `font_size`, into a `w` x `h` logical target at `scale`.
fn render(
    gpu: &(wgpu::Device, wgpu::Queue),
    t: &Terminal,
    lines: &[(&str, GlyphStyle, [f32; 2])],
    font_size: f32,
    scale: f32,
    (w, h): (f32, f32),
) -> Frame {
    let (device, queue) = gpu;
    let mut list = DrawList::new();
    for &(s, style, pos) in lines {
        let line = t.shaper.line(s, style);
        list.push_terminal_line(
            &line,
            t.set.atlas(t.text).unwrap(),
            t.set.atlas(t.page).unwrap(),
            pos,
            font_size,
            PX_RANGE,
            WHITE,
        );
    }
    let (pw, ph) = ((w * scale).ceil() as u32, (h * scale).ceil() as u32);
    let r = GpuSdfRenderer::new_with_atlas_set(device, wgpu::TextureFormat::Rgba8Unorm, &t.set);
    r.upload_atlas_set(queue, &t.set);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("terminal"),
        size: wgpu::Extent3d {
            width: pw,
            height: ph,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    r.render_draw_list_in(device, queue, &view, pw, ph, scale, &list, &t.set, 0.0);
    let padded_row = (pw * 4).next_multiple_of(256);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rb"),
        size: (padded_row * ph) as u64,
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
            width: pw,
            height: ph,
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
    let mut px = Vec::with_capacity((pw * ph) as usize);
    for row in 0..ph {
        let start = (row * padded_row) as usize;
        px.extend(
            data[start..start + (pw * 4) as usize]
                .chunks(4)
                .map(|p| if p[0] == 0 { 0 } else { p[3] }),
        );
    }
    drop(data);
    readback.unmap();
    Frame { w: pw, h: ph, px }
}

/// Every case at every size and scale, with the GPU, or a skip.
fn each_case(mut case: impl FnMut(&(wgpu::Device, wgpu::Queue), &Terminal, f32, f32)) {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let t = terminal();
    for size in SIZES {
        for scale in SCALES {
            case(&gpu, &t, size, scale);
        }
    }
}

/// A 3 x 3 block of `█` is solid inside its outer edge: no seam between
/// cells or between lines.
#[test]
fn blocks_have_no_seam() {
    each_case(|gpu, t, size, scale| {
        let (cell, pitch) = (0.6 * size, 1.3 * size);
        let origin = [4.0, 4.0];
        let lines: Vec<_> = (0..3)
            .map(|i| {
                (
                    "\u{2588}\u{2588}\u{2588}",
                    GlyphStyle::Regular,
                    [origin[0], origin[1] + i as f32 * pitch],
                )
            })
            .collect();
        let f = render(
            gpu,
            t,
            &lines,
            size,
            scale,
            (8.0 + 3.0 * cell, 8.0 + 3.0 * pitch),
        );
        f.save(&format!("blocks-{size}px-x{scale}"));
        let (x0, y0) = (
            (origin[0] * scale).ceil() as u32 + 1,
            (origin[1] * scale).ceil() as u32 + 1,
        );
        let (x1, y1) = (
            ((origin[0] + 3.0 * cell) * scale).floor() as u32 - 1,
            ((origin[1] + 3.0 * pitch) * scale).floor() as u32 - 1,
        );
        let mut worst = (255u8, 0, 0);
        for y in y0..y1 {
            for x in x0..x1 {
                if f.at(x, y) < worst.0 {
                    worst = (f.at(x, y), x, y);
                }
            }
        }
        assert!(
            worst.0 >= 252,
            "{size}px x{scale}: coverage {} at ({}, {}) inside the block",
            worst.0,
            worst.1,
            worst.2
        );
    });
}

/// Along the centre line of every stroke of a `┼─┼─┼` grid and a box, the
/// coverage across each cell and line boundary is at least 0.9 of its value
/// mid-cell.
#[test]
fn strokes_join_across_cells_and_lines() {
    each_case(|gpu, t, size, scale| {
        let (cell, pitch) = (0.6 * size, 1.3 * size);
        let o = [4.0, 4.0];
        let grid = "\u{253C}\u{2500}\u{253C}\u{2500}\u{253C}";
        let lines = vec![
            (grid, GlyphStyle::Regular, [o[0], o[1]]),
            (grid, GlyphStyle::Regular, [o[0], o[1] + pitch]),
            (grid, GlyphStyle::Regular, [o[0], o[1] + 2.0 * pitch]),
            (
                "\u{256D}\u{2500}\u{2500}\u{256E}",
                GlyphStyle::Regular,
                [o[0] + 6.0 * cell, o[1]],
            ),
            (
                "\u{2502}  \u{2502}",
                GlyphStyle::Regular,
                [o[0] + 6.0 * cell, o[1] + pitch],
            ),
            (
                "\u{2570}\u{2500}\u{2500}\u{256F}",
                GlyphStyle::Regular,
                [o[0] + 6.0 * cell, o[1] + 2.0 * pitch],
            ),
        ];
        let f = render(
            gpu,
            t,
            &lines,
            size,
            scale,
            (8.0 + 10.0 * cell, 8.0 + 3.0 * pitch),
        );
        f.save(&format!("strokes-{size}px-x{scale}"));
        // The strokes' centres, in font units of the 1.3 em line box: ─ at
        // y 379 of [-271, 1029], │ at x 299.5 of the 600 cell.
        let row_frac = (1029.0 - 379.0) / 1300.0;
        let col_frac = 299.5 / 600.0;
        let px = |v: f32| (v * scale).round() as u32;
        // A stroke's ink across its width: the coverage summed over 2 px
        // either side of its centre line, so a thin stroke that straddles two
        // pixel rows counts whole.
        let across_row = |x: u32, y: u32| (y - 2..=y + 2).map(|y| f.at(x, y) as u32).sum::<u32>();
        let across_col = |x: u32, y: u32| (x - 2..=x + 2).map(|x| f.at(x, y) as u32).sum::<u32>();
        // No dip below 0.9 of mid-cell along a stretch of stroke; and no bead
        // above 1.15 of it where two strokes overlap at a join (a run's
        // glyphs are composited as one shape; measured at most 1.09).
        let mut beads: Vec<f32> = Vec::new();
        let mut check = |name: &str, samples: Vec<u32>, mid: u32| {
            let (at, &min) = samples.iter().enumerate().min_by_key(|s| s.1).unwrap();
            let max = *samples.iter().max().unwrap();
            assert!(
                mid > 128 && min as f32 >= 0.9 * mid as f32,
                "{size}px x{scale}: {name} dips to {min} against {mid} mid-cell, {at} px \
                 along: {samples:?}"
            );
            assert!(
                (max as f32) <= 1.15 * mid as f32,
                "{size}px x{scale}: {name} beads to {max} against {mid}: {samples:?}"
            );
            beads.push(max as f32 / mid as f32);
        };
        // Horizontal stretches, in cells: the grid rows between their crosses
        // (each stretch spans two cell boundaries), and the box's top and
        // bottom from where the rounded corners' arcs have ended.
        for (line, x_from, x_to) in [
            (0.0, 0.8, 2.2),
            (0.0, 2.8, 4.2),
            (1.0, 0.8, 2.2),
            (1.0, 2.8, 4.2),
            (2.0, 0.8, 2.2),
            (2.0, 2.8, 4.2),
            (0.0, 6.95, 9.05),
            (2.0, 6.95, 9.05),
        ] {
            let y = px(o[1] + (line + row_frac) * pitch);
            let (a, b) = (px(o[0] + x_from * cell), px(o[0] + x_to * cell));
            let samples: Vec<u32> = (a..=b).map(|x| across_row(x, y)).collect();
            let mid = across_row(px(o[0] + (x_from.floor() + 1.5) * cell), y);
            check(&format!("row {line} from {x_from}"), samples, mid);
        }
        // Vertical stretches, in lines: the grid's crosses between their
        // horizontal strokes (each spans a line boundary), and the box's sides
        // from where the corners' arcs end.
        for (col, y_from, y_to) in [
            (0.0, 0.65, 1.35),
            (0.0, 1.65, 2.35),
            (2.0, 0.65, 1.35),
            (2.0, 1.65, 2.35),
            (4.0, 0.65, 1.35),
            (4.0, 1.65, 2.35),
            (6.0, 0.95, 2.05),
            (9.0, 0.95, 2.05),
        ] {
            let x = px(o[0] + (col + col_frac) * cell);
            let (a, b) = (px(o[1] + y_from * pitch), px(o[1] + y_to * pitch));
            let samples: Vec<u32> = (a..=b).map(|y| across_col(x, y)).collect();
            // Mid-line, away from where a cross's horizontal stroke meets it.
            let mid = across_col(x, px(o[1] + 1.25 * pitch));
            check(&format!("column {col} from {y_from}"), samples, mid);
        }
        let worst = beads.iter().cloned().fold(0.0, f32::max);
        eprintln!("strokes {size}px x{scale}: widest bead at a join {worst:.2} of mid-cell");
    });
}

/// The ink components of `f` above `threshold`, as centroids.
fn dots(f: &Frame, x_range: (u32, u32), threshold: u8) -> Vec<[f32; 2]> {
    let mut seen = vec![false; f.px.len()];
    let mut out = Vec::new();
    for y in 0..f.h {
        for x in x_range.0..x_range.1.min(f.w) {
            let i = (y * f.w + x) as usize;
            if seen[i] || f.px[i] < threshold {
                continue;
            }
            let (mut sx, mut sy, mut sw) = (0.0f32, 0.0f32, 0.0f32);
            let mut stack = vec![(x, y)];
            seen[i] = true;
            while let Some((cx, cy)) = stack.pop() {
                let v = f.at(cx, cy) as f32;
                sx += v * cx as f32;
                sy += v * cy as f32;
                sw += v;
                for (nx, ny) in [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ] {
                    if nx >= x_range.0 && nx < x_range.1.min(f.w) && ny < f.h {
                        let j = (ny * f.w + nx) as usize;
                        if !seen[j] && f.px[j] >= threshold {
                            seen[j] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
            }
            out.push([sx / sw, sy / sw]);
        }
    }
    out
}

/// Each spinner frame's dots are where `⠿` puts them, and the text after
/// the spinner is the same pixels in every frame.
#[test]
fn braille_spinners_stay_on_the_grid() {
    let frames = "\u{280B}\u{2819}\u{2839}\u{2838}\u{283C}\u{2834}\u{2826}\u{2827}\u{2807}\u{280F}";
    each_case(|gpu, t, size, scale| {
        if size * scale < 24.0 {
            // Below 24 physical px a dot is under 3 px and the dots of a
            // column touch: no stable centroids. Phone sizes at 2x and 3x are
            // all at or above it.
            return;
        }
        let cell = 0.6 * size;
        let dims = (8.0 + 10.0 * cell, 8.0 + 1.3 * size);
        let draw = |s: &str| {
            render(
                gpu,
                t,
                &[(s, GlyphStyle::Regular, [4.0, 4.0])],
                size,
                scale,
                dims,
            )
        };
        let split = ((4.0 + cell) * scale).round() as u32;
        let full = draw("\u{283F}");
        let places = dots(&full, (0, split), 96);
        assert_eq!(
            places.len(),
            6,
            "{size}px x{scale}: ⠿ has six dots: {places:?}"
        );
        let mut text: Option<Vec<u8>> = None;
        for (i, frame) in frames.chars().enumerate() {
            let f = draw(&format!("{frame} Working"));
            if i == 0 {
                f.save(&format!("spinner-{size}px-x{scale}"));
            }
            for d in dots(&f, (0, split), 96) {
                let near = places
                    .iter()
                    .map(|p| ((p[0] - d[0]).powi(2) + (p[1] - d[1]).powi(2)).sqrt())
                    .fold(f32::MAX, f32::min);
                assert!(
                    near <= 0.5,
                    "{size}px x{scale} {frame}: a dot at {d:?} is {near} px off"
                );
            }
            let right: Vec<u8> = (0..f.h)
                .flat_map(|y| (split..f.w).map(move |x| (x, y)))
                .map(|(x, y)| f.at(x, y))
                .collect();
            match &text {
                None => text = Some(right),
                Some(first) => assert!(
                    first == &right,
                    "{size}px x{scale}: the text moved at {frame}"
                ),
            }
        }
    });
}

/// Bold `Å` keeps its ring: its ink starts where the ring's top is (1004
/// units, 0.025 em below the line box's top at the terminal placement),
/// within a pixel, rather than cut off lower down.
#[test]
fn bold_ring_is_whole() {
    each_case(|gpu, t, size, scale| {
        let f = render(
            gpu,
            t,
            &[("\u{C5}", GlyphStyle::Bold, [4.0, 4.0])],
            size,
            scale,
            (8.0 + 0.6 * size, 8.0 + 1.3 * size),
        );
        f.save(&format!("ring-{size}px-x{scale}"));
        let want = (4.0 + 0.025 * size) * scale;
        let first = (0..f.h)
            .find(|&y| (0..f.w).any(|x| f.at(x, y) >= 128))
            .unwrap() as f32;
        assert!(
            (first + 0.5 - want).abs() <= 1.0,
            "{size}px x{scale}: the ring's ink starts at row {first}, its top is at {want}"
        );
    });
}

/// An 8-bit greyscale PNG with stored (uncompressed) deflate blocks.
fn png(w: u32, h: u32, grey: &[u8]) -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], body: &[u8]) {
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        let mut t = tag.to_vec();
        t.extend_from_slice(body);
        out.extend_from_slice(&t);
        out.extend_from_slice(&crc(&t).to_be_bytes());
    }
    let mut raw = Vec::with_capacity(((w + 1) * h) as usize);
    for row in grey.chunks(w as usize) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push((i + 1 == blocks.len()) as u8);
        z.extend_from_slice(&(b.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(b.len() as u16)).to_le_bytes());
        z.extend_from_slice(b);
    }
    let (mut a, mut bsum) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        bsum = (bsum + a) % 65521;
    }
    z.extend_from_slice(&((bsum << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 0, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}
