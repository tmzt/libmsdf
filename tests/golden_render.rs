//! Headless golden-image test (carried over from matter-stream's
//! wgpu_msdf_render.rs pattern): DrawList (primitives + Bézier arcs + MSDF
//! text from the baked Roboto fixture) → GpuSdfRenderer → readback →
//! tolerance-based comparison against a checked-in golden PNG.
//!
//! Regenerate the golden after intentional visual changes:
//! `LIBMSDF_BLESS=1 CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm \
//!    cargo test -p libmsdf --features gpu-tests --test golden_render`
//! (the comparison itself is tolerance-based).

#![cfg(feature = "gpu-tests")]

use libmsdf::drawlist::{DrawList, SdfInstance, SdfKind};
use libmsdf::font::{FontAtlas, TextShaper};
use libmsdf::gpu::GpuSdfRenderer;

const WIDTH: u32 = 800;
const HEIGHT: u32 = 400;
const ATLAS_FIXTURE: &[u8] = include_bytes!("fixtures/roboto-ascii-48.atlas");
/// Distance range the fixture atlas was baked with, in atlas texels.
const PX_RANGE: f32 = 6.0;
const GOLDEN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/golden_scene.png"
);

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

fn scene(atlas: &FontAtlas) -> DrawList {
    let shaper = TextShaper::new(libmsdf::ROBOTO_REGULAR_ASCII.to_vec()).unwrap();
    let mut list = DrawList::new();

    // Background card.
    list.push(SdfInstance {
        kind: SdfKind::RoundedBox { radius: 16.0 },
        position: [40.0, 40.0],
        size: [720.0, 320.0],
        color: [0.13, 0.14, 0.20, 1.0],
        anim: 0,
    });
    // Accent circle.
    list.push(SdfInstance {
        kind: SdfKind::Circle,
        position: [600.0, 80.0],
        size: [120.0, 120.0],
        color: [0.95, 0.55, 0.20, 1.0],
        anim: 0,
    });
    // Outline.
    list.push(SdfInstance {
        kind: SdfKind::Outline { radius: 10.0, thickness: 3.0 },
        position: [70.0, 200.0],
        size: [200.0, 120.0],
        color: [0.55, 0.75, 0.95, 1.0],
        anim: 0,
    });
    // Line.
    list.push(SdfInstance {
        kind: SdfKind::Line,
        position: [70.0, 170.0],
        size: [660.0, 2.0],
        color: [0.8, 0.8, 0.85, 1.0],
        anim: 0,
    });
    // Containment arc (primary weight) and flow arc (secondary weight) —
    // the ZUI nav-graph pair.
    list.push_bezier(
        [90.0, 330.0],
        [250.0, 180.0],
        [450.0, 380.0],
        [700.0, 240.0],
        6.0,
        [0.90, 0.85, 0.60, 1.0],
    );
    list.push_bezier(
        [90.0, 350.0],
        [300.0, 300.0],
        [500.0, 400.0],
        [700.0, 300.0],
        2.0,
        [0.60, 0.85, 0.70, 1.0],
    );
    // MSDF text from the baked fixture atlas. PX_RANGE must match the bake or
    // the shader's screen-space alpha ramp is scaled wrong.
    let run = shaper.shape("Highbay MSDF");
    list.push_shaped_text(&run, atlas, [80.0, 80.0], 42.0, PX_RANGE, [0.93, 0.93, 0.95, 1.0]);
    // Small text with hairlines (t/f crossbars, i dots) — the size band where a
    // wrong distance range eats sub-pixel strokes.
    let small = shaper.shape("tftlt: fifty little SDF outlines, effortlessly");
    list.push_shaped_text(&small, atlas, [80.0, 132.0], 12.0, PX_RANGE, [0.75, 0.76, 0.80, 1.0]);

    list
}

fn render_scene() -> Option<Vec<u8>> {
    let (device, queue) = gpu()?;
    let atlas = FontAtlas::from_bytes(ATLAS_FIXTURE).expect("fixture atlas parses");

    let renderer = GpuSdfRenderer::new_with_msdf(
        &device,
        wgpu::TextureFormat::Rgba8Unorm,
        atlas.width,
        atlas.height,
    );
    renderer.upload_msdf_atlas(&queue, atlas.width, atlas.height, &atlas.to_rgba_bytes());
    renderer.upload_glyph_table(&queue, &atlas.glyph_table_u32s());

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen"),
        size: wgpu::Extent3d { width: WIDTH, height: HEIGHT, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());

    let list = scene(&atlas);
    renderer.render_draw_list(&device, &queue, &view, WIDTH, HEIGHT, 1.0, &list, 0.0);

    // Readback.
    let bpp = 4u32;
    let padded_row = (WIDTH * bpp).next_multiple_of(256);
    let buf_size = (padded_row * HEIGHT) as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rb"),
        size: buf_size,
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

fn write_png(path: &str, img: &[u8]) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(file, WIDTH, HEIGHT);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(img).unwrap();
}

fn read_png(path: &str) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    assert_eq!((info.width, info.height), (WIDTH, HEIGHT), "golden size mismatch");
    buf.truncate(info.buffer_size());
    Some(buf)
}

#[test]
fn golden_scene_renders() {
    let Some(img) = render_scene() else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };

    // Structural sanity, independent of the golden file.
    let px = |x: u32, y: u32| {
        let i = ((y * WIDTH + x) * 4) as usize;
        (img[i], img[i + 1], img[i + 2])
    };
    // Card interior is the dark card color, not clear-black.
    let (r, g, b) = px(400, 220);
    assert!(b > r && b > 20, "card interior should be blue-ish dark: {:?}", (r, g, b));
    // Circle center is orange.
    let (r, _g, b) = px(660, 140);
    assert!(r > 180 && b < 120, "circle center should be orange");
    // Text row has bright pixels.
    let text_row = (0..WIDTH).filter(|&x| px(x, 105).0 > 150).count();
    assert!(text_row > 20, "headline row should have bright text pixels: {text_row}");
    // Bézier arcs put ink between the card rows.
    let arc_ink: usize = (0..WIDTH).filter(|&x| {
        let (r, g, b) = px(x, 300);
        r > 60 || g > 60 || b > 60
    }).count();
    assert!(arc_ink > 10, "arc band should contain stroke pixels: {arc_ink}");

    // Golden comparison (tolerance-based; BLESS regenerates).
    if std::env::var("LIBMSDF_BLESS").is_ok() {
        write_png(GOLDEN_PATH, &img);
        eprintln!("BLESSED: wrote {GOLDEN_PATH}");
        return;
    }
    let Some(golden) = read_png(GOLDEN_PATH) else {
        panic!("golden missing — run with LIBMSDF_BLESS=1 to create {GOLDEN_PATH}");
    };
    assert_eq!(golden.len(), img.len());

    let tolerance = 12u8;
    let mismatched = img
        .chunks_exact(4)
        .zip(golden.chunks_exact(4))
        .filter(|(a, b)| {
            a.iter().zip(b.iter()).take(3).any(|(&x, &y)| x.abs_diff(y) > tolerance)
        })
        .count();
    let total = (WIDTH * HEIGHT) as usize;
    let frac = mismatched as f64 / total as f64;
    assert!(
        frac <= 0.005,
        "golden mismatch: {mismatched}/{total} px ({:.3}%) differ beyond ±{tolerance}",
        frac * 100.0
    );
}
