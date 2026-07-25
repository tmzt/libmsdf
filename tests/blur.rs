//! Headless GPU test for the backdrop-blur post-pass ([`libmsdf::BlurPass`]).
//!
//! Renders a sharp vertical black/white step into a `backdrop` texture, runs
//! the blur pass, reads the result back, and asserts the step became a gradient
//! (a real blur — intermediate values appear at the edge) while the far field
//! is untouched. A second case composites an opaque overlay and asserts it
//! survives the blur crisp.
//!
//! Gated like the other GPU integration tests (Cranelift aborts wgpu init on
//! aarch64). Run with:
//! `CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm cargo test -p libmsdf --features gpu-tests --test blur`

#![cfg(feature = "gpu-tests")]

use libmsdf::BlurPass;

const W: u32 = 64;
const H: u32 = 32;
const FMT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

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

/// A sampleable texture initialised from RGBA8 pixels.
fn tex_from_rgba(device: &wgpu::Device, queue: &wgpu::Queue, rgba: &[u8]) -> wgpu::Texture {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blur-src"),
        size: wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FMT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(W * 4), rows_per_image: Some(H) },
        wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
    );
    tex
}

fn target(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blur-target"),
        size: wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FMT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn readback(device: &wgpu::Device, queue: &wgpu::Queue, tex: &wgpu::Texture) -> Vec<u8> {
    let padded = (W * 4).next_multiple_of(256);
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("blur-readback"),
        size: (padded * H) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: None },
        },
        wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
    );
    queue.submit(std::iter::once(enc.finish()));

    let slice = buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let data = slice.get_mapped_range();
    let mut img = Vec::with_capacity((W * H * 4) as usize);
    for row in 0..H {
        let start = (row * padded) as usize;
        img.extend_from_slice(&data[start..start + (W * 4) as usize]);
    }
    drop(data);
    buf.unmap();
    img
}

fn px(img: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [img[i], img[i + 1], img[i + 2], img[i + 3]]
}

#[test]
fn blur_turns_a_hard_edge_into_a_gradient() {
    let Some((device, queue)) = gpu() else {
        eprintln!("blur test: SKIP — no GPU adapter");
        return;
    };

    // Backdrop: a hard vertical step — left half white, right half black.
    let mut src = vec![0u8; (W * H * 4) as usize];
    for y in 0..H {
        for x in 0..W {
            let i = ((y * W + x) * 4) as usize;
            let v = if x < W / 2 { 255 } else { 0 };
            src[i] = v;
            src[i + 1] = v;
            src[i + 2] = v;
            src[i + 3] = 255;
        }
    }
    let backdrop = tex_from_rgba(&device, &queue, &src);
    // Fully transparent overlay → the output is the pure blur.
    let overlay = tex_from_rgba(&device, &queue, &vec![0u8; (W * H * 4) as usize]);
    let out_tex = target(&device);

    let pass = BlurPass::new(&device, FMT);
    pass.render(
        &device,
        &queue,
        &out_tex.create_view(&Default::default()),
        &backdrop.create_view(&Default::default()),
        &overlay.create_view(&Default::default()),
        W,
        H,
        6.0,
    );
    let img = readback(&device, &queue, &out_tex);

    let mid = H / 2;
    // The far field is essentially untouched (local blur).
    assert!(px(&img, 2, mid)[0] > 220, "far-left stays near-white: {:?}", px(&img, 2, mid));
    assert!(px(&img, W - 3, mid)[0] < 35, "far-right stays near-black: {:?}", px(&img, W - 3, mid));
    // The boundary column band now holds intermediate greys — proof the hard
    // step got smeared into a gradient by a real sampling blur.
    let boundary_intermediate = (W / 2 - 4..W / 2 + 4)
        .any(|x| { let r = px(&img, x, mid)[0]; r > 40 && r < 215 });
    assert!(boundary_intermediate, "the hard edge became a gradient (intermediate greys at the boundary)");
}

#[test]
fn overlay_composites_crisp_over_the_blur() {
    let Some((device, queue)) = gpu() else {
        eprintln!("blur test: SKIP — no GPU adapter");
        return;
    };

    // Uniform grey backdrop (blur leaves it grey).
    let backdrop = tex_from_rgba(&device, &queue, &vec![128u8; (W * H * 4) as usize]);
    // Overlay: opaque red on the left half, transparent on the right
    // (premultiplied — opaque red = (255,0,0,255)).
    let mut ov = vec![0u8; (W * H * 4) as usize];
    for y in 0..H {
        for x in 0..W / 2 {
            let i = ((y * W + x) * 4) as usize;
            ov[i] = 255;
            ov[i + 3] = 255;
        }
    }
    let overlay = tex_from_rgba(&device, &queue, &ov);
    let out_tex = target(&device);

    let pass = BlurPass::new(&device, FMT);
    pass.render(
        &device,
        &queue,
        &out_tex.create_view(&Default::default()),
        &backdrop.create_view(&Default::default()),
        &overlay.create_view(&Default::default()),
        W,
        H,
        6.0,
    );
    let img = readback(&device, &queue, &out_tex);
    let mid = H / 2;
    // Left: the opaque overlay wins (crisp red, no grey bleed).
    let left = px(&img, 4, mid);
    assert!(left[0] > 220 && left[1] < 40 && left[2] < 40, "overlay is crisp red over the blur: {left:?}");
    // Right: no overlay → the (blurred) grey backdrop shows through.
    let right = px(&img, W - 4, mid);
    assert!((right[0] as i32 - 128).abs() < 24, "uncovered region keeps the backdrop: {right:?}");
}
