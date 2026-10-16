//! Headless GPU tests for filled paths (`SdfKind::PathFill`, draw type 12):
//! the shader's `case 12u` against the CPU mirror and the other kinds.
//! Plan: `docs/design/20261015_01_FILLED_BEZIER_PATH_PLAN.md`, steps 6 and 7.

#![cfg(feature = "gpu-tests")]

use libmsdf::drawlist::{DrawList, Elevation, Path, PathBuilder};
use libmsdf::gpu::GpuSdfRenderer;

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

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

/// An RGBA8 image of `list` drawn over a transparent target.
struct Img {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Img {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.w + x) * 4) as usize;
        [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]]
    }

    fn alpha(&self, x: u32, y: u32) -> u8 {
        self.at(x, y)[3]
    }
}

fn render(list: &DrawList, w: u32, h: u32) -> Option<Img> {
    let (device, queue) = gpu()?;
    let renderer = GpuSdfRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("path_fill"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
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
    // A new texture is zero (transparent); Load keeps it so, where nothing
    // is drawn, alpha is 0.
    let frame = list.lower();
    renderer.upload_chars(&queue, &frame.char_buffer);
    renderer.upload_params(&queue, &frame.param_bank);
    renderer.render_full_scaled_load(
        &device,
        &queue,
        &view,
        w,
        h,
        1.0,
        &frame.draws,
        0.0,
        &[0; 16],
        &[],
        None,
    );

    let padded = (w * 4).next_multiple_of(256);
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rb"),
        size: (padded * h) as u64,
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
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(enc.finish()));
    let slice = buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let data = slice.get_mapped_range().unwrap();
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        let start = (row * padded) as usize;
        px.extend_from_slice(&data[start..start + (w * 4) as usize]);
    }
    drop(data);
    buf.unmap();
    Some(Img { w, h, px })
}

fn square(x: f32, y: f32, s: f32) -> Path {
    PathBuilder::new()
        .move_to([x, y])
        .line_to([x + s, y])
        .line_to([x + s, y + s])
        .line_to([x, y + s])
        .build()
        .unwrap()
}

#[test]
fn a_filled_square_has_ink_inside_and_none_outside() {
    let mut list = DrawList::new();
    list.push_path_fill(&square(10.0, 10.0, 20.0), WHITE, Elevation::Flat)
        .unwrap();
    let Some(img) = render(&list, 64, 64) else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    assert_eq!(img.alpha(20, 20), 255, "centre");
    assert_eq!(img.alpha(5, 5), 0, "outside");
    assert_eq!(img.h, 64);
}

// ── Step 7: GPU golden tests ─────────────────────────────────────────────

use libmsdf::core::sdf_eval_with_params;
use libmsdf::drawlist::{DrawEffects, SdfInstance, SdfKind, SdfRotate};

const K: f32 = 0.552_284_8;
const OUT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/out/path_fill");
const GOLDEN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/golden_path_fill.png"
);

/// Evidence for out/path_fill/LOG.md (Rule 3.5); never compared.
fn save(name: &str, img: &Img) {
    let _ = std::fs::create_dir_all(OUT_DIR);
    write_png(&format!("{OUT_DIR}/{name}.png"), img);
}

fn write_png(path: &str, img: &Img) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(file, img.w, img.h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .unwrap()
        .write_image_data(&img.px)
        .unwrap();
}

fn read_png(path: &str) -> Option<Img> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = png::Decoder::new(file).read_info().ok()?;
    let mut px = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut px).ok()?;
    px.truncate(info.buffer_size());
    Some(Img {
        w: info.width,
        h: info.height,
        px,
    })
}

fn add_circle(b: &mut PathBuilder, cx: f32, cy: f32, r: f32, reverse: bool) {
    let k = K * r;
    let s = if reverse { -1.0 } else { 1.0 };
    b.move_to([cx + r, cy]);
    b.cubic_to([cx + r, cy + s * k], [cx + k, cy + s * r], [cx, cy + s * r]);
    b.cubic_to([cx - k, cy + s * r], [cx - r, cy + s * k], [cx - r, cy]);
    b.cubic_to([cx - r, cy - s * k], [cx - k, cy - s * r], [cx, cy - s * r]);
    b.cubic_to([cx + k, cy - s * r], [cx + r, cy - s * k], [cx + r, cy]);
    b.close();
}

fn circle(cx: f32, cy: f32, r: f32) -> Path {
    let mut b = PathBuilder::new();
    add_circle(&mut b, cx, cy, r, false);
    b.build().unwrap()
}

/// A rounded square (corner radius 16) with a circular hole and an island
/// inside the hole.
fn holed(x: f32, y: f32, reverse_hole: bool) -> Path {
    let (s, r) = (120.0, 16.0);
    let k = K * r;
    let mut b = PathBuilder::new();
    b.move_to([x + r, y]);
    b.line_to([x + s - r, y]);
    b.cubic_to([x + s - r + k, y], [x + s, y + r - k], [x + s, y + r]);
    b.line_to([x + s, y + s - r]);
    b.cubic_to(
        [x + s, y + s - r + k],
        [x + s - r + k, y + s],
        [x + s - r, y + s],
    );
    b.line_to([x + r, y + s]);
    b.cubic_to([x + r - k, y + s], [x, y + s - r + k], [x, y + s - r]);
    b.line_to([x, y + r]);
    b.cubic_to([x, y + r - k], [x + r - k, y], [x + r, y]);
    b.close();
    add_circle(&mut b, x + 60.0, y + 60.0, 40.0, reverse_hole);
    add_circle(&mut b, x + 60.0, y + 60.0, 15.0, false);
    b.build().unwrap()
}

/// The trowel blade of the tower defense game's tool icon (24 px design
/// grid), placed into the square at (x, y) of side `s`.
fn blade(x: f32, y: f32, s: f32) -> Path {
    PathBuilder::new()
        .move_to([4.0, 14.0])
        .cubic_to([7.0, 6.0], [13.0, 3.0], [20.0, 2.0])
        .cubic_to([18.0, 9.0], [14.0, 13.0], [8.0, 16.0])
        .close()
        .build()
        .unwrap()
        .mapped(|[px, py]| [x + px * s / 24.0, y + py * s / 24.0])
}

/// An asymmetric shape: an L with one quadratic and one cubic edge.
fn flag(map: impl Fn([f32; 2]) -> [f32; 2]) -> Path {
    PathBuilder::new()
        .move_to(map([20.0, 20.0]))
        .line_to(map([90.0, 20.0]))
        .quad_to(map([110.0, 40.0]), map([70.0, 50.0]))
        .line_to(map([50.0, 50.0]))
        .cubic_to(map([50.0, 80.0]), map([70.0, 90.0]), map([40.0, 100.0]))
        .line_to(map([20.0, 100.0]))
        .build()
        .unwrap()
}

/// The CPU mirror's alpha for every pixel of a single-instance list, by the
/// shader's own fill formula (white, unanimated, no ombré).
fn cpu_alpha(list: &DrawList, w: u32, h: u32) -> Vec<f32> {
    let frame = list.lower();
    let cmd = frame.draws[0];
    let blur = cmd.xform[2].max(0.0);
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let (d, _) =
                sdf_eval_with_params(&cmd, x as f32 + 0.5, y as f32 + 0.5, &frame.param_bank);
            let t = ((d + 0.5 + blur) / (1.0 + 2.0 * blur)).clamp(0.0, 1.0);
            out.push(cmd.color[3] * (1.0 - t * t * (3.0 - 2.0 * t)));
        }
    }
    out
}

/// Largest |GPU alpha - CPU alpha| over the image, in 1/255 steps.
fn worst_vs_cpu(img: &Img, cpu: &[f32]) -> u8 {
    let mut worst = 0u8;
    for y in 0..img.h {
        for x in 0..img.w {
            let want = (cpu[(y * img.w + x) as usize] * 255.0).round() as u8;
            worst = worst.max(img.alpha(x, y).abs_diff(want));
        }
    }
    worst
}

#[test]
fn a_four_cubic_circle_matches_the_circle_kind() {
    let mut path = DrawList::new();
    path.push_path_fill(&circle(80.0, 80.0, 60.0), WHITE, Elevation::Flat)
        .unwrap();
    let mut kind = DrawList::new();
    kind.push(SdfInstance {
        kind: SdfKind::Circle,
        position: [20.0, 20.0],
        size: [120.0, 120.0],
        color: WHITE,
        anim: 0,
    });
    let Some(a) = render(&path, 160, 160) else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let b = render(&kind, 160, 160).unwrap();
    save("circle_path", &a);
    save("circle_kind", &b);
    let diffs: Vec<u8> =
        a.px.iter()
            .zip(&b.px)
            .map(|(x, y)| x.abs_diff(*y))
            .collect();
    let max = *diffs.iter().max().unwrap();
    let mean = diffs.iter().map(|&d| d as f64).sum::<f64>() / diffs.len() as f64;
    eprintln!("circle: max {max}/255, mean {mean:.4}/255");
    assert!(max <= 12, "max channel difference {max}/255");
    assert!(mean < 1.0, "mean channel difference {mean}/255");
}

#[test]
fn a_hole_is_background_whatever_its_direction() {
    let render_holed = |reverse| {
        let mut list = DrawList::new();
        list.push_path_fill(&holed(20.0, 20.0, reverse), WHITE, Elevation::Flat)
            .unwrap();
        render(&list, 160, 160)
    };
    let Some(a) = render_holed(false) else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let b = render_holed(true).unwrap();
    save("hole", &a);
    // Centre (80, 80) is the island; the hole is the annulus 15 < r < 40.
    assert_eq!(a.alpha(80, 80), 255, "island");
    assert_eq!(a.alpha(80, 52), 0, "hole (r = 28)");
    assert_eq!(a.alpha(80, 30), 255, "ring (r = 50)");
    assert_eq!(a.alpha(10, 10), 0, "outside");
    assert_eq!(a.alpha(22, 22), 0, "outside the rounded corner");
    assert!(a.px == b.px, "reversing the hole changes the image");
}

/// An asymmetric polygon: straight edges only.
fn notch(map: impl Fn([f32; 2]) -> [f32; 2]) -> Path {
    PathBuilder::new()
        .move_to(map([20.0, 20.0]))
        .line_to(map([100.0, 24.0]))
        .line_to(map([60.0, 50.0]))
        .line_to(map([90.0, 100.0]))
        .line_to(map([24.0, 90.0]))
        .build()
        .unwrap()
}

/// A shape built through a point map.
type ShapeFn = dyn Fn(&dyn Fn([f32; 2]) -> [f32; 2]) -> Path;

#[test]
fn a_quarter_turn_matches_pre_rotated_coordinates() {
    let pivot = [64.0, 64.0];
    let turn = |[x, y]: [f32; 2]| [pivot[0] - (y - pivot[1]), pivot[1] + (x - pivot[0])];
    let pair = |shape: &ShapeFn| {
        let mut rotated = DrawList::new();
        rotated.push_rotate(SdfRotate::Quarter(1), pivot);
        rotated
            .push_path_fill(&shape(&|p| p), WHITE, Elevation::Flat)
            .unwrap();
        rotated.push_rotate_end();
        let mut pre = DrawList::new();
        pre.push_path_fill(&shape(&turn), WHITE, Elevation::Flat)
            .unwrap();
        Some((render(&rotated, 128, 128)?, render(&pre, 128, 128)?))
    };
    let diff = |a: &Img, b: &Img| {
        let max = a.px.iter().zip(&b.px).map(|(x, y)| x.abs_diff(*y)).max();
        let n = a.px.iter().zip(&b.px).filter(|(x, y)| x != y).count();
        (max.unwrap(), n)
    };
    // Straight edges: the same segments either way, so the same pixels.
    let Some((a, b)) = pair(&|m| notch(m)) else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    save("quarter_rotated_lines", &a);
    assert!(a.px == b.px, "lines: {:?}", diff(&a, &b));
    // Curved edges: pre-rotating moves the y-extrema the lowering splits
    // at, so the two lowered paths are different (equal) curves and f32
    // rounding may differ by one step on an edge pixel (CHANGES.md,
    // 2026-10-16, step 7).
    let (a, b) = pair(&|m| flag(m)).unwrap();
    save("quarter_rotated", &a);
    save("quarter_pre_rotated", &b);
    let (max, n) = diff(&a, &b);
    eprintln!("quarter turn, curved: max {max}/255 over {n} channels");
    assert!(
        max <= 1 && n <= 16,
        "curved: max {max}/255 over {n} channels"
    );
}

#[test]
fn an_arbitrary_rotation_matches_the_cpu_mirror() {
    let mut list = DrawList::new();
    list.push_rotate(SdfRotate::Radians(0.3), [64.0, 64.0]);
    list.push_path_fill(&flag(|p| p), WHITE, Elevation::Flat)
        .unwrap();
    list.push_rotate_end();
    let Some(img) = render(&list, 128, 128) else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    save("radians_0_3", &img);
    let worst = worst_vs_cpu(&img, &cpu_alpha(&list, 128, 128));
    eprintln!("radians 0.3: worst {worst}/255 vs CPU");
    assert!(worst <= 12, "{worst}/255");
}

/// Lengths of the runs of partially covered pixels on row `y` that lie
/// between an empty pixel and a fully covered one: the edge ramps.
fn edge_ramps(img: &Img, y: u32) -> Vec<u32> {
    let mut ramps = Vec::new();
    let mut x = 0;
    while x < img.w {
        let a = img.alpha(x, y);
        if a > 0 && a < 255 {
            let start = x;
            while x < img.w && img.alpha(x, y) > 0 && img.alpha(x, y) < 255 {
                x += 1;
            }
            let before = if start == 0 {
                0
            } else {
                img.alpha(start - 1, y)
            };
            let after = if x == img.w { 0 } else { img.alpha(x, y) };
            if (before == 0) != (after == 0) {
                ramps.push(x - start);
            }
        } else {
            x += 1;
        }
    }
    ramps
}

#[test]
fn the_blade_is_crisp_at_8_px_and_512_px() {
    for (size, row) in [(8u32, 4u32), (512, 200)] {
        let mut list = DrawList::new();
        list.push_path_fill(&blade(0.0, 0.0, size as f32), WHITE, Elevation::Flat)
            .unwrap();
        let Some(img) = render(&list, size, size) else {
            eprintln!("SKIP: no GPU adapter");
            return;
        };
        save(&format!("blade_{size}"), &img);
        let ramps = edge_ramps(&img, row);
        eprintln!(
            "blade {size}: row {row} alphas {:?} ramps {ramps:?}",
            (0..size.min(16))
                .map(|x| img.alpha(x, row))
                .collect::<Vec<_>>()
        );
        if size == 512 {
            assert_eq!(ramps.len(), 2, "two edges cross row {row}");
        }
        assert!(
            ramps.iter().all(|&n| (1..=2).contains(&n)),
            "blade {size}: edge ramps {ramps:?}"
        );
        let worst = worst_vs_cpu(&img, &cpu_alpha(&list, size, size));
        eprintln!("blade {size}: worst {worst}/255 vs CPU");
        assert!(worst <= 12, "blade {size}: {worst}/255 vs CPU");
    }
}

#[test]
fn shadow_clip_and_blur_behave_as_for_other_kinds() {
    let sq = square(20.0, 20.0, 40.0);
    let draw = |elevation: Elevation, clip: bool, blur: f32| {
        let mut list = DrawList::new();
        if clip {
            list.push_clip([0.0, 0.0], [40.0, 100.0]);
        }
        list.begin_effects(DrawEffects {
            blur_radius: blur,
            alpha_ombre: 0.0,
        });
        list.push_path_fill(&sq, WHITE, elevation).unwrap();
        list.end_effects();
        if clip {
            list.push_clip_end();
        }
        render(&list, 100, 100)
    };
    let Some(flat) = draw(Elevation::Flat, false, 0.0) else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    let raised = draw(Elevation::Raised, false, 0.0).unwrap();
    let clipped = draw(Elevation::Flat, true, 0.0).unwrap();
    let blurred = draw(Elevation::Flat, false, 2.0).unwrap();
    save("shadow_raised", &raised);
    save("clip", &clipped);
    save("blur_2", &blurred);
    // Just past the right and bottom edges, within the (2, 3) offset.
    for (x, y) in [(61, 40), (40, 62)] {
        assert_eq!(flat.alpha(x, y), 0, "flat casts nothing at ({x}, {y})");
        assert!(raised.alpha(x, y) > 0, "raised darkens ({x}, {y})");
    }
    // The clip at x = 40 halves the square.
    assert_eq!(clipped.alpha(30, 40), 255);
    assert_eq!(clipped.alpha(39, 40), 255);
    assert_eq!(clipped.alpha(40, 40), 0);
    assert_eq!(clipped.alpha(50, 40), 0);
    // Blur 2 widens the one-pixel ramp to about five pixels.
    let ramp = |img: &Img| {
        (0..img.w)
            .filter(|&x| x > 40 && img.alpha(x, 40) > 0 && img.alpha(x, 40) < 255)
            .count()
    };
    let (sharp, soft) = (ramp(&flat), ramp(&blurred));
    eprintln!("ramp: sharp {sharp} px, blur 2 {soft} px");
    assert!(sharp <= 2, "sharp ramp {sharp}");
    assert!((4..=6).contains(&soft), "blurred ramp {soft}");
}

/// Everything above in one scene, against tests/fixtures/golden_path_fill.png
/// (regenerate with `LIBMSDF_BLESS=1 cargo test --features gpu-tests --test
/// path_fill`), compared like golden_render.rs.
#[test]
fn golden_path_fill_scene() {
    let mut list = DrawList::new();
    list.push_path_fill(
        &circle(80.0, 80.0, 60.0),
        [0.95, 0.55, 0.2, 1.0],
        Elevation::Raised,
    )
    .unwrap();
    list.push_path_fill(
        &holed(170.0, 20.0, true),
        [0.55, 0.75, 0.95, 1.0],
        Elevation::Flat,
    )
    .unwrap();
    list.push_rotate(SdfRotate::Radians(0.3), [380.0, 80.0]);
    list.push_path_fill(
        &flag(|[x, y]| [x + 316.0, y + 16.0]),
        [0.6, 0.85, 0.7, 1.0],
        Elevation::Flat,
    )
    .unwrap();
    list.push_rotate_end();
    list.push_path_fill(
        &blade(20.0, 160.0, 96.0),
        [0.8, 0.8, 0.85, 1.0],
        Elevation::Raised,
    )
    .unwrap();
    list.push_path_fill(
        &blade(130.0, 200.0, 24.0),
        [0.8, 0.8, 0.85, 1.0],
        Elevation::Flat,
    )
    .unwrap();
    list.push_path_fill(
        &blade(170.0, 216.0, 8.0),
        [0.8, 0.8, 0.85, 1.0],
        Elevation::Flat,
    )
    .unwrap();
    list.begin_effects(DrawEffects {
        blur_radius: 2.0,
        alpha_ombre: 0.5,
    });
    list.push_path_fill(
        &square(220.0, 170.0, 60.0),
        [0.9, 0.85, 0.6, 1.0],
        Elevation::Flat,
    )
    .unwrap();
    list.end_effects();
    list.push_clip([300.0, 160.0], [50.0, 100.0]);
    list.push_path_fill(
        &circle(350.0, 210.0, 40.0),
        [0.93, 0.93, 0.95, 1.0],
        Elevation::Flat,
    )
    .unwrap();
    list.push_clip_end();
    let (w, h) = (480, 280);
    let Some(img) = render(&list, w, h) else {
        eprintln!("SKIP: no GPU adapter");
        return;
    };
    save("golden_scene", &img);
    if std::env::var("LIBMSDF_BLESS").is_ok() {
        write_png(GOLDEN_PATH, &img);
        eprintln!("BLESSED: wrote {GOLDEN_PATH}");
        return;
    }
    let golden =
        read_png(GOLDEN_PATH).expect("golden missing: run with LIBMSDF_BLESS=1 to create it");
    assert_eq!((golden.w, golden.h), (w, h), "golden size mismatch");
    let tolerance = 12u8;
    let mismatched = img
        .px
        .chunks_exact(4)
        .zip(golden.px.chunks_exact(4))
        .filter(|(a, b)| {
            a.iter()
                .zip(b.iter())
                .any(|(&x, &y)| x.abs_diff(y) > tolerance)
        })
        .count();
    let frac = mismatched as f64 / (w * h) as f64;
    assert!(
        frac <= 0.005,
        "golden mismatch: {mismatched} px ({:.3}%) differ beyond ±{tolerance}",
        frac * 100.0
    );
}
