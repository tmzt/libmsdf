// Backdrop blur + overlay composite — the modal-backdrop post-pass.
//
// A cheap single-pass separable-style gaussian blur over a `backdrop` texture
// (the already-rendered scene), with a sharp premultiplied `overlay` texture
// (the modal + its scrim) composited over the blurred result. One full-screen
// triangle, two texture reads. Only run while a modal is open, so the tap
// count is a fixed 5×5 kernel regardless of radius — plenty for the
// "slightly blurred" backdrop and trivially cheap at these resolutions.
//
// The backdrop is treated as opaque (alpha 1); the overlay is premultiplied
// (the SDF renderer emits premultiplied `result`), so the composite is a
// straight `over`: out = overlay.rgb + backdrop * (1 - overlay.a).

struct BlurUniforms {
    // x = 1/width, y = 1/height, z = radius (px), w = unused
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: BlurUniforms;
@group(0) @binding(1) var backdrop_tex: texture_2d<f32>;
@group(0) @binding(2) var overlay_tex: texture_2d<f32>;
@group(0) @binding(3) var samp: sampler;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var pts = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let p = pts[vi];
    var out: VsOut;
    out.position = vec4<f32>(p, 0.0, 1.0);
    // uv (0,0) = top-left: framebuffer top (NDC y = +1) maps to uv.y = 0.
    out.uv = vec2<f32>((p.x + 1.0) * 0.5, 1.0 - (p.y + 1.0) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let texel = u.params.xy;
    let radius = max(u.params.z, 0.0);
    // Ring spacing in texels: half the radius per kernel step (a 5-tap span
    // reaches ±2 steps = ±radius px).
    let stride = radius * 0.5;

    var sum = vec3<f32>(0.0, 0.0, 0.0);
    var wsum = 0.0;
    for (var j: i32 = -2; j <= 2; j = j + 1) {
        for (var i: i32 = -2; i <= 2; i = i + 1) {
            let off = vec2<f32>(f32(i), f32(j)) * stride * texel;
            let d2 = f32(i * i + j * j);
            let w = exp(-d2 / 4.0);
            sum = sum + textureSampleLevel(backdrop_tex, samp, in.uv + off, 0.0).rgb * w;
            wsum = wsum + w;
        }
    }
    let backdrop = sum / max(wsum, 1e-4);

    // Sharp overlay (premultiplied) composited over the blurred backdrop.
    let overlay = textureSampleLevel(overlay_tex, samp, in.uv, 0.0);
    let rgb = overlay.rgb + backdrop * (1.0 - overlay.a);
    return vec4<f32>(rgb, 1.0);
}
