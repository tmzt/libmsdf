//! Runtime compute-shader MSDF generation tests (native headless).
//!
//! - Smoke: the compute path produces a plausible field for a glyph
//!   (inside bright / outside dark), and `generate_into_texture` +
//!   `AtlasManager` populate an atlas region that renders.
//! - Comparison (requires `--features cpu-bake`, run with the LLVM backend
//!   per Cargo.toml notes): compute output vs the CPU msdfgen baseline,
//!   median-of-channels within tolerance.

#![cfg(feature = "gpu-tests")]

use libmsdf::font::atlas::glyph_projection;
use libmsdf::font::outline::extract_outline;
use libmsdf::gpu::MsdfCompute;
use libmsdf::gpu::compute::read_cell_rgba;

const CELL: u32 = 48;
const PX_RANGE: f32 = 4.0;

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

fn roboto_face() -> ttf_parser::Face<'static> {
    ttf_parser::Face::parse(libmsdf::ROBOTO_REGULAR_ASCII, 0).unwrap()
}

/// Generate one glyph cell on the GPU, returning tightly packed RGBA rows.
fn compute_cell(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    ch: char,
) -> Option<Vec<u8>> {
    let face = roboto_face();
    let gid = face.glyph_index(ch)?.0;
    let outline = extract_outline(&face, gid)?;
    let proj = glyph_projection(&face, gid, CELL)?;
    let generator = MsdfCompute::new(device);
    let cell = generator.generate_cell(device, queue, &outline, &proj, CELL, PX_RANGE);
    Some(read_cell_rgba(device, queue, &cell))
}

fn median3(r: u8, g: u8, b: u8) -> f32 {
    let (r, g, b) = (r as f32, g as f32, b as f32);
    (r.min(g).max(r.max(g).min(b))) / 255.0
}

#[test]
fn compute_msdf_smoke() {
    let Some((device, queue)) = gpu() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let rgba = compute_cell(&device, &queue, 'A').expect("A generates");
    assert_eq!(rgba.len(), (CELL * CELL * 4) as usize);

    // Median field statistics: an 'A' covers a meaningful part of the cell.
    let mut inside = 0usize;
    let mut outside = 0usize;
    for texel in rgba.chunks_exact(4) {
        if median3(texel[0], texel[1], texel[2]) > 0.5 {
            inside += 1;
        } else {
            outside += 1;
        }
    }
    let total = (CELL * CELL) as usize;
    assert!(
        inside > total / 20 && outside > total / 4,
        "field looks degenerate: inside={inside} outside={outside}"
    );

    // The cap-height alignment puts ink around the glyph vertical middle;
    // corners of the cell are outside.
    let texel = |x: u32, y: u32| {
        let i = ((y * CELL + x) * 4) as usize;
        median3(rgba[i], rgba[i + 1], rgba[i + 2])
    };
    assert!(texel(1, 1) < 0.5, "top-left corner should be outside");
    assert!(texel(CELL - 2, 1) < 0.5, "top-right corner should be outside");
    // Stem of the 'A' near the baseline center-left/right is inside.
    let mid_row = (0..CELL).filter(|&x| texel(x, CELL * 2 / 3) > 0.5).count();
    assert!(mid_row >= 2, "expected stem crossings on the lower third: {mid_row}");
}

#[test]
fn compute_into_texture_and_manager() {
    let Some((device, queue)) = gpu() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let face = roboto_face();
    let generator = MsdfCompute::new(&device);

    // Dynamic atlas: allocate cells via the manager, generate glyphs into
    // the renderer's atlas texture.
    let mut manager = libmsdf::AtlasManager::new(256, 256);
    let renderer = libmsdf::GpuSdfRenderer::new_with_msdf(
        &device,
        wgpu::TextureFormat::Rgba8Unorm,
        manager.width(),
        manager.height(),
    );

    for ch in ['Z', 'g'] {
        let gid = face.glyph_index(ch).unwrap().0;
        let outline = extract_outline(&face, gid).unwrap();
        let proj = glyph_projection(&face, gid, CELL).unwrap();
        let region = manager.alloc(gid, CELL, CELL).expect("atlas has room");
        generator.generate_into_texture(
            &device,
            &queue,
            renderer.msdf_atlas_texture(),
            region.x,
            region.y,
            &outline,
            &proj,
            CELL,
            PX_RANGE,
        );
    }
    assert_eq!(manager.len(), 2);

    // Evict one and confirm the region recycles.
    let gid_z = face.glyph_index('Z').unwrap().0;
    let freed = manager.evict(gid_z).unwrap();
    let gid_q = face.glyph_index('Q').unwrap().0;
    let reused = manager.alloc(gid_q, CELL, CELL).unwrap();
    assert_eq!((freed.x, freed.y), (reused.x, reused.y));

    // Read the texture back and confirm the 'g' region has field content.
    let region_g = manager.region_of(face.glyph_index('g').unwrap().0).unwrap();
    let padded_row = (manager.width() * 4).next_multiple_of(256);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("tex_rb"),
        size: (padded_row * manager.height()) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: renderer.msdf_atlas_texture(),
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
            width: manager.width(),
            height: manager.height(),
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
    let data = slice.get_mapped_range();

    let mut bright = 0usize;
    for y in region_g.y..region_g.y + region_g.h {
        for x in region_g.x..region_g.x + region_g.w {
            let i = (y * padded_row + x * 4) as usize;
            if median3(data[i], data[i + 1], data[i + 2]) > 0.5 {
                bright += 1;
            }
        }
    }
    assert!(bright > 20, "'g' region should contain field ink: {bright}");
}

/// Compute output vs the CPU msdfgen reference, median-of-channels.
/// Edge coloring differs between the two implementations, so individual
/// channels aren't comparable — the median (what the shader renders) is.
#[cfg(feature = "cpu-bake")]
#[test]
fn compute_matches_cpu_baseline() {
    let Some((device, queue)) = gpu() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };

    let builder = libmsdf::FontAtlasBuilder::new(
        libmsdf::ROBOTO_REGULAR_ASCII.to_vec(),
        CELL,
        PX_RANGE as f64,
    );
    let face = roboto_face();

    for ch in ['A', 'o', 'g', '5', '#'] {
        let gid = face.glyph_index(ch).unwrap().0;
        let cpu = builder.bake_cell(gid).expect("cpu bake");
        let gpu_rgba = compute_cell(&device, &queue, ch).expect("gpu generate");

        let total = (CELL * CELL) as usize;
        let mut sum_abs = 0.0f64;
        let mut beyond_tol = 0usize;
        let mut inter = 0usize;
        let mut union = 0usize;

        for i in 0..total {
            let c = &cpu.rgb[i * 3..i * 3 + 3];
            let g = &gpu_rgba[i * 4..i * 4 + 3];
            let m_cpu = median3(c[0], c[1], c[2]);
            let m_gpu = median3(g[0], g[1], g[2]);
            let d = (m_cpu - m_gpu).abs();
            sum_abs += d as f64;
            if d > 0.125 {
                beyond_tol += 1;
            }
            let in_cpu = m_cpu > 0.5;
            let in_gpu = m_gpu > 0.5;
            if in_cpu && in_gpu {
                inter += 1;
            }
            if in_cpu || in_gpu {
                union += 1;
            }
        }

        // TEMP DEBUG: locate deviations
        if std::env::var("LIBMSDF_DEBUG").is_ok() {
            let mut worst: Vec<(f32, u32, u32, f32, f32)> = Vec::new();
            let mut band_near = (0usize, 0.0f64, 0usize); // |cpu-0.5|<=0.25
            let mut band_far = (0usize, 0.0f64, 0usize);
            for i in 0..total {
                let c = &cpu.rgb[i * 3..i * 3 + 3];
                let g = &gpu_rgba[i * 4..i * 4 + 3];
                let m_cpu = median3(c[0], c[1], c[2]);
                let m_gpu = median3(g[0], g[1], g[2]);
                let d = (m_cpu - m_gpu).abs();
                let (x, y) = (i as u32 % CELL, i as u32 / CELL);
                if (m_cpu - 0.5).abs() <= 0.25 {
                    band_near.0 += 1; band_near.1 += d as f64; if d > 0.125 { band_near.2 += 1; }
                } else {
                    band_far.0 += 1; band_far.1 += d as f64; if d > 0.125 { band_far.2 += 1; }
                }
                worst.push((d, x, y, m_cpu, m_gpu));
            }
            if ch == 'A' {
                for (name, is_cpu) in [("CPU", true), ("GPU", false)] {
                    eprintln!("  {name} median map:");
                    for y in 0..CELL {
                        let mut row = String::new();
                        for x in 0..CELL {
                            let i = (y*CELL+x) as usize;
                            let m = if is_cpu { median3(cpu.rgb[i*3],cpu.rgb[i*3+1],cpu.rgb[i*3+2]) }
                                    else { median3(gpu_rgba[i*4],gpu_rgba[i*4+1],gpu_rgba[i*4+2]) };
                            row.push(char::from_digit((m*9.99) as u32, 10).unwrap_or('?'));
                        }
                        eprintln!("   {row}");
                    }
                }
            }
            for &(px, py) in &[(21u32,10u32),(23,7),(21,17),(10,38)] {
                let i = (py*CELL+px) as usize;
                let c=&cpu.rgb[i*3..i*3+3]; let g=&gpu_rgba[i*4..i*4+3];
                eprintln!("  texel({px},{py}) cpu=({},{},{}) gpu=({},{},{})", c[0],c[1],c[2],g[0],g[1],g[2]);
            }
            worst.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            eprintln!("  near band: n={} meanD={:.4} beyond={}", band_near.0, band_near.1 / band_near.0.max(1) as f64, band_near.2);
            eprintln!("  far band:  n={} meanD={:.4} beyond={}", band_far.0, band_far.1 / band_far.0.max(1) as f64, band_far.2);
            for w in worst.iter().take(12) {
                eprintln!("  worst d={:.3} at ({},{}) cpu={:.3} gpu={:.3}", w.0, w.1, w.2, w.3, w.4);
            }
        }
        let mean = sum_abs / total as f64;
        let frac_beyond = beyond_tol as f64 / total as f64;
        let iou = if union == 0 { 1.0 } else { inter as f64 / union as f64 };
        eprintln!(
            "'{ch}': mean|Δmedian|={mean:.4}, >0.125: {:.2}%, IoU={iou:.3}",
            frac_beyond * 100.0
        );

        assert!(mean <= 0.04, "'{ch}': mean median deviation too high: {mean:.4}");
        assert!(
            frac_beyond <= 0.08,
            "'{ch}': too many texels beyond tolerance: {:.2}%",
            frac_beyond * 100.0
        );
        assert!(iou >= 0.85, "'{ch}': coverage IoU too low: {iou:.3}");
    }
}
