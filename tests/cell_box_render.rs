//! **The cell box on the GPU** (`docs/design/20261023_02_PER_ATLAS_CELL_BOX_PLAN.md`,
//! section 5 and step 4): the tall glyphs of Texturina and Grenze Gotisch,
//! drawn from their committed cell-box atlases, keep their whole outline at
//! phone and display sizes, and both of Claude Code's spinners keep the text
//! after them in place on the terminal atlases.
//!
//! ```text
//! CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm cargo test \
//!     --features gpu-tests,fonts-ofl-ledger,fonts-ofl-terminal --test cell_box_render
//! ```
//!
//! Each case's frame is written to `out/cell_box/` as a PNG.

#![cfg(all(
    feature = "gpu-tests",
    feature = "font-ofl-texturina",
    feature = "font-ofl-grenze-gotisch",
    feature = "font-ofl-noto-sans-mono",
    feature = "font-ofl-noto-terminal-symbols"
))]

use libmsdf::gpu::GpuSdfRenderer;
use libmsdf::{AtlasSet, DrawList, FontAtlas, GlyphStyle, TerminalShaper};

const SIZES: [f32; 4] = [12.0, 16.0, 24.0, 56.0];
const SCALES: [f32; 2] = [1.0, 2.0];
const WHITE: [f32; 4] = [1.0; 4];
const PX_RANGE: f32 = 6.0;
/// A row counts as inked at a quarter coverage: the tip of an acute accent
/// is narrower than a pixel at 12 px and never reaches half.
const INK: u8 = 64;

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
/// channel where a glyph drew, 0 where nothing did).
struct Frame {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Frame {
    fn at(&self, x: u32, y: u32) -> u8 {
        self.px[(y * self.w + x) as usize]
    }

    /// The first and last rows with any pixel at or above `threshold`.
    fn ink_rows(&self, threshold: u8) -> Option<(u32, u32)> {
        let rows: Vec<u32> = (0..self.h)
            .filter(|&y| (0..self.w).any(|x| self.at(x, y) >= threshold))
            .collect();
        Some((*rows.first()?, *rows.last()?))
    }

    fn save(&self, name: &str) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("out/cell_box");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{name}.png")),
            png(self.w, self.h, &self.px),
        )
        .unwrap();
    }
}

/// Draw `list` from `set` into a `w` x `h` logical target at `scale`.
fn render(
    gpu: &(wgpu::Device, wgpu::Queue),
    set: &AtlasSet,
    list: &DrawList,
    scale: f32,
    (w, h): (f32, f32),
) -> Frame {
    let (device, queue) = gpu;
    let (pw, ph) = ((w * scale).ceil() as u32, (h * scale).ceil() as u32);
    let r = GpuSdfRenderer::new_with_atlas_set(device, wgpu::TextureFormat::Rgba8Unorm, set);
    r.upload_atlas_set(queue, set);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cell box"),
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
    r.render_draw_list_in(device, queue, &view, pw, ph, scale, list, set, 0.0);
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

/// The tall glyphs of each face, with their outline extents in font units
/// (ttf-parser's bounding boxes of the committed faces).
fn faces() -> Vec<(&'static str, &'static [u8], &'static [u8], &'static str)> {
    vec![
        (
            "texturina",
            libmsdf::TEXTURINA_ATLAS,
            libmsdf::TEXTURINA_TTF,
            "\u{C2}\u{139}\u{125}\u{13A}\u{C5}\u{16E}\u{BF}\u{2030}",
        ),
        (
            "grenze-gotisch",
            libmsdf::GRENZE_GOTISCH_ATLAS,
            libmsdf::GRENZE_GOTISCH_TTF,
            "\u{C5}\u{16E}\u{C1}\u{BF}",
        ),
    ]
}

/// **Every tall glyph is whole**: drawn alone, its topmost and bottommost
/// ink rows fall where its outline's top and bottom do, within a pixel, at
/// every size and scale. The renderer puts an edge a fixed fraction of a
/// pixel from the outline (its half-texel sampling and the MSDF threshold),
/// the same for every glyph; `H`, which fits every cell, is drawn the same
/// way and measures that offset, and each tall glyph is compared after
/// removing it. The line-box atlases of libmsdf 0.5 fail this at every size
/// here, ĥ and ĺ by up to 8.8 px at 56 px (docs/FONT_TEXTURINA.md).
#[test]
fn tall_glyphs_keep_their_whole_outline() {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let mut failures = Vec::new();
    for (name, atlas_bytes, ttf, glyphs) in faces() {
        let atlas = FontAtlas::from_bytes(atlas_bytes).unwrap();
        assert!(!atlas.cell_box().is_line_box(), "{name} has its cell box");
        let face = ttf_parser::Face::parse(ttf, 0).unwrap();
        let upem = face.units_per_em() as f32;
        let mut set = AtlasSet::new();
        let id = set.register(atlas).unwrap();
        let a = set.atlas(id).unwrap();
        let shaper = a.shaper(GlyphStyle::Regular).unwrap();
        // (measured - outline) of the top and bottom rows of `ch`, in
        // physical pixels.
        let measure = |ch: char, size: f32, scale: f32, save: bool| -> Option<(f32, f32)> {
            let gid = face.glyph_index(ch).unwrap();
            let bb = face.glyph_bounding_box(gid).unwrap();
            let run = shaper.shape(&ch.to_string());
            let mut list = DrawList::new();
            list.push_shaped_text_with(&run, a, [size, size], size, PX_RANGE, WHITE);
            let inst = list.instances[0];
            let entry = *a.get_glyph(run.glyphs[0].glyph_id).unwrap();
            let baseline =
                inst.position[1] + entry.baseline_row * inst.size[1] / entry.atlas_h as f32;
            let f = render(&gpu, &set, &list, scale, (size * 4.0, size * 3.5));
            if save {
                f.save(&format!("{name}_U+{:04X}_{size}px_x{scale}", ch as u32));
            }
            let want_top = (baseline - bb.y_max as f32 * size / upem) * scale;
            let want_bottom = (baseline - bb.y_min as f32 * size / upem) * scale;
            let (t, b) = f.ink_rows(INK)?;
            Some((t as f32 + 0.5 - want_top, b as f32 + 0.5 - want_bottom))
        };
        for size in SIZES {
            for scale in SCALES {
                let (ht, hb) = measure('H', size, scale, false).expect("H draws");
                for ch in glyphs.chars() {
                    let Some((t, b)) = measure(ch, size, scale, true) else {
                        failures.push(format!("{name} {ch} {size}px x{scale}: no ink"));
                        continue;
                    };
                    if (t - ht).abs() > 1.0 || (b - hb).abs() > 1.0 {
                        failures.push(format!(
                            "{name} {ch} {size}px x{scale}: top {:+.2} px, bottom {:+.2} px \
                             from the outline, against H's {ht:+.2} and {hb:+.2}",
                            t, b
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// **Both spinners keep the text after them in place**: every frame of the
/// starburst and Braille spinners draws ink in its own cell, and the pixels
/// of "Working" after it are the same in every frame.
#[test]
fn spinner_frames_keep_the_text_in_place() {
    let Some(gpu) = gpu() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let text = FontAtlas::from_bytes(libmsdf::NOTO_SANS_MONO_ATLAS).unwrap();
    let page = FontAtlas::from_bytes(libmsdf::TERMINAL_SYMBOLS_ATLAS).unwrap();
    let shaper = TerminalShaper::new(&text, &page).unwrap();
    let mut set = AtlasSet::new();
    let (tid, pid) = (set.register(text).unwrap(), set.register(page).unwrap());
    let spinners: [&[char]; 2] = [
        &[
            '\u{B7}', '\u{2722}', '\u{2733}', '\u{2736}', '\u{273B}', '\u{273D}', '*',
        ],
        &[
            '\u{280B}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283C}', '\u{2834}', '\u{2826}',
            '\u{2827}', '\u{2807}', '\u{280F}',
        ],
    ];
    for size in [14.0f32, 24.0] {
        for scale in SCALES {
            let cell = size * shaper.cell_em();
            for (s, frames) in spinners.iter().enumerate() {
                let mut text_px: Option<Vec<u8>> = None;
                for (i, &frame) in frames.iter().enumerate() {
                    let line = shaper.line(&format!("{frame} Working"), GlyphStyle::Regular);
                    let mut list = DrawList::new();
                    list.push_terminal_line(
                        &line,
                        set.atlas(tid).unwrap(),
                        set.atlas(pid).unwrap(),
                        [4.0, 4.0],
                        size,
                        PX_RANGE,
                        WHITE,
                    );
                    let f = render(&gpu, &set, &list, scale, (4.0 + cell * 10.0, size * 2.0));
                    f.save(&format!("spinner{s}_{i}_{size}px_x{scale}"));
                    let split = ((4.0 + cell * 1.5) * scale) as u32;
                    let frame_ink = (0..f.h).any(|y| (0..split).any(|x| f.at(x, y) > 0));
                    assert!(frame_ink, "spinner {s} frame {i} ({frame}) draws nothing");
                    let after: Vec<u8> = (0..f.h)
                        .flat_map(|y| (split..f.w).map(move |x| (x, y)))
                        .map(|(x, y)| f.at(x, y))
                        .collect();
                    match &text_px {
                        None => text_px = Some(after),
                        Some(first) => assert!(
                            first == &after,
                            "spinner {s} frame {i} ({frame}) moves the text at {size}px x{scale}"
                        ),
                    }
                }
            }
        }
    }
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
