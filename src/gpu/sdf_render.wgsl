// Fragment shader: SDF-based 2D rendering pipeline.
// Extracted from matter-stream's matterstream-ui-gpu/shader_render.wgsl;
// Highbay additions: cubic-Bézier strokes (type 10), aux param bank
// (binding 12), and the lo-fi noise-distortion hook.
//
// Vertex stage: full-screen triangle (3 vertices, no vertex buffer).
// Fragment stage: iterate over DrawCmd nodes, evaluate SDF primitives,
// composite with alpha blending front-to-back.
//
// Each DrawCmd specifies type via params.x:
//   0 = Box, 1 = Slab (rounded rect), 2 = Circle, 3 = Line, 4 = Text,
//   5 = Texture, 6/7 = Ribbon clip begin/end, 8 = MSDF Text, 9 = Outline,
//   10 = Cubic Bézier stroke, 11 = Slab per-corner
//
// SdfRotate (xform.x, a separate per-instance field, not a params.x type):
// an optional rotation-about-a-pivot applied to `effective_pixel` BEFORE
// the type switch below, so every type — shapes and text alike — is
// rotated uniformly with no per-type special case.

// ── DrawCmd ──

struct DrawCmd {
    pos: vec2<f32>,
    size: vec2<f32>,
    color: vec4<f32>,
    params: vec4<f32>,   // [ty, radius, anim_idx, slot]
    // SdfRotate transform bank index (see param_bank, binding 12): 0 = none
    // (identity, the overwhelmingly common case), 1+ = 1-based index; the
    // forward affine lives at param_bank[(xform.x-1)*2] = [a,b,c,d] and
    // param_bank[(xform.x-1)*2+1] = [tx,ty,_,_]. .y/.z/.w reserved.
    xform: vec4<f32>,
};

struct GpuUniforms {
    time_delta: vec4<f32>,
    resolution: vec4<f32>,
    mouse: vec4<f32>,
    theme: vec4<f32>,
    vec4_bank: array<vec4<f32>, 16>,
    vec3_bank: array<vec4<f32>, 16>,
    scalar_bank: array<vec4<f32>, 4>,
    int_bank: array<vec4<i32>, 4>,
    zero_page: array<vec4<u32>, 16>,
    // Font descriptor: [glyph_w, glyph_h, first_cp, last_cp]
    font: vec4<u32>,
    // Lo-fi hook: [distortion_amount_px, noise_scale, reserved, reserved]
    style_params: vec4<f32>,
    // Inlined from former separate storage buffers (GLES compat: max 4 storage)
    header: vec4<u32>,    // .x = cmd_count
    anim_bank: array<Anim, 32>,
    texture_bank: array<GpuTexture, 8>,
};

struct Anim {
    freq: f32,
    duty: f32,
    enable_ref: u32,
    _pad: u32,
};

struct GpuTexture {
    width: u32,
    height: u32,
    layer: u32,
    flags: u32,
};

// ── Bindings ──

@group(0) @binding(0) var<uniform> uniforms: GpuUniforms;
@group(0) @binding(1) var<storage, read> draw_cmds: array<DrawCmd>;
// bindings 2,3 merged into uniforms (header, anim_bank)
@group(0) @binding(4) var<storage, read> glyph_bitmap: array<u32>;
@group(0) @binding(5) var<storage, read> char_buffer: array<u32>;
@group(0) @binding(6) var tex_array: texture_2d_array<f32>;
@group(0) @binding(7) var tex_sampler: sampler;
// binding 8 merged into uniforms (texture_bank)

// MSDF atlas for high-quality text rendering
@group(0) @binding(9)  var msdf_atlas: texture_2d<f32>;
@group(0) @binding(10) var msdf_sampler: sampler;

// Per-glyph atlas lookup: 2 × vec4<u32> per entry
// g0 = [glyph_id, atlas_xy_packed, atlas_wh_packed, advance_x_bits]
// g1 = [baseline_row_bits, px_per_em_bits, x_margin_bits, 0]
@group(0) @binding(11) var<storage, read> glyph_table: array<vec4<u32>>;

// Aux per-instance data: Bézier control points (C1.xy, C2.xy), future
// lo-fi per-instance parameters. Indexed by bitcast<u32>(cmd.params.w).
@group(0) @binding(12) var<storage, read> param_bank: array<vec4<f32>>;

// ── Vertex shader: full-screen triangle ──

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(vi & 1u)) * 4.0 - 1.0;
    let y = f32(i32(vi >> 1u)) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    let scale = max(uniforms.resolution.z, 1.0);
    out.uv = vec2<f32>(
        (x + 1.0) * 0.5 * uniforms.resolution.x / scale,
        (1.0 - (y + 1.0) * 0.5) * uniforms.resolution.y / scale
    );
    return out;
}

// ── SDF primitives ──

fn sd_box(p: vec2<f32>, half_size: vec2<f32>) -> f32 {
    let d = abs(p) - half_size;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

fn sd_rounded_box(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let r = min(radius, min(half_size.x, half_size.y));
    return sd_box(p, half_size - vec2<f32>(r)) - r;
}

// Rounded box with independent per-corner radii. `radii` = (tl, tr, br, bl) in
// screen space (+x right, +y down): pick the quadrant's radius, then evaluate
// the standard rounded-box distance (mirrors core::sdf::sd_rounded_box_per_corner).
fn sd_rounded_box_pc(p: vec2<f32>, half_size: vec2<f32>, radii: vec4<f32>) -> f32 {
    var r: f32;
    if p.x > 0.0 {
        r = select(radii.y, radii.z, p.y > 0.0); // right: tr / br
    } else {
        r = select(radii.x, radii.w, p.y > 0.0); // left: tl / bl
    }
    let rr = min(r, min(half_size.x, half_size.y));
    return sd_box(p, half_size - vec2<f32>(rr)) - rr;
}

fn sd_circle(p: vec2<f32>, radius: f32) -> f32 {
    return length(p) - radius;
}

fn sd_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, thickness: f32) -> f32 {
    return sd_segment_raw(p, a, b) - thickness;
}

fn sd_segment_raw(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-6), 0.0, 1.0);
    return length(pa - ba * h);
}

// ── Cubic Bézier stroke (nav-graph arcs) ──

const BEZIER_FLATTEN_STEPS: u32 = 24u;

fn cubic_point(p0: vec2<f32>, c1: vec2<f32>, c2: vec2<f32>, p3: vec2<f32>, t: f32) -> vec2<f32> {
    let u = 1.0 - t;
    return u * u * u * p0
        + 3.0 * u * u * t * c1
        + 3.0 * u * t * t * c2
        + t * t * t * p3;
}

/// Distance to a stroked cubic Bézier, flattened to BEZIER_FLATTEN_STEPS
/// segments (matches sd_cubic_stroke in core::sdf). Negative inside.
fn sd_cubic_stroke(p: vec2<f32>, p0: vec2<f32>, c1: vec2<f32>, c2: vec2<f32>, p3: vec2<f32>, thickness: f32) -> f32 {
    var prev = p0;
    var dmin = 1e6;
    for (var i: u32 = 1u; i <= BEZIER_FLATTEN_STEPS; i = i + 1u) {
        let t = f32(i) / f32(BEZIER_FLATTEN_STEPS);
        let pt = cubic_point(p0, c1, c2, p3, t);
        dmin = min(dmin, sd_segment_raw(p, prev, pt));
        prev = pt;
    }
    return dmin - thickness * 0.5;
}

// ── Lo-fi pencil-sketch hook (Phase 8) ──
//
// The Phase-5 contract is the plumbing: a per-frame distortion amount in
// uniforms.style_params.x (px) and noise scale in .y, applied to the sample
// position before SDF evaluation. Phase 8 replaces this placeholder
// sin-jitter with the real pencil noise field (and may add per-instance
// parameters via param_bank).
fn lofi_distort(p: vec2<f32>) -> vec2<f32> {
    let amount = uniforms.style_params.x;
    if amount <= 0.0 {
        return p;
    }
    let scale = max(uniforms.style_params.y, 0.05);
    let jitter = vec2<f32>(
        sin(p.y * scale) * 0.7 + sin(p.y * scale * 2.7 + 1.3) * 0.3,
        sin(p.x * scale * 1.1 + 0.7) * 0.7 + sin(p.x * scale * 3.1 + 2.1) * 0.3,
    );
    return p + jitter * amount;
}

/// MSDF median: the middle value of RGB channels gives the signed distance.
fn msdf_median(r: f32, g: f32, b: f32) -> f32 {
    return max(min(r, g), min(max(r, g), b));
}

/// Alpha-blend src over dst (premultiplied alpha).
fn blend_over(dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {
    let out_a = src.a + dst.a * (1.0 - src.a);
    if out_a < 0.001 {
        return vec4<f32>(0.0);
    }
    let out_rgb = (src.rgb * src.a + dst.rgb * dst.a * (1.0 - src.a)) / out_a;
    return vec4<f32>(out_rgb, out_a);
}

// ── Fragment shader ──

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let pixel = lofi_distort(in.uv);

    // Start transparent so we can be composited on top of prior passes
    // without obliterating them. Pixels that no SDF cmd covers get
    // `discard`ed at the tail of the shader and the destination content
    // passes through unchanged.
    var result = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    let count = uniforms.header.x;

    var in_ribbon: bool = false;
    var ribbon_clip_min: vec2<f32> = vec2<f32>(0.0);
    var ribbon_clip_max: vec2<f32> = vec2<f32>(0.0);
    var ribbon_scroll: vec2<f32> = vec2<f32>(0.0);

    for (var i: u32 = 0u; i < count; i = i + 1u) {
        let cmd = draw_cmds[i];
        let ty = u32(cmd.params.x);

        if ty == 6u {
            in_ribbon = true;
            ribbon_clip_min = cmd.pos;
            ribbon_clip_max = cmd.pos + cmd.size;
            let slot = u32(cmd.params.y);
            let pack = slot / 4u;
            let comp = slot % 4u;
            let scroll_val = uniforms.scalar_bank[min(pack, 3u)][min(comp, 3u)];
            let dir = cmd.params.z;
            if dir > 0.5 {
                ribbon_scroll = vec2<f32>(0.0, scroll_val);
            } else {
                ribbon_scroll = vec2<f32>(scroll_val, 0.0);
            }
            continue;
        }
        if ty == 7u {
            in_ribbon = false;
            ribbon_scroll = vec2<f32>(0.0);
            continue;
        }

        if in_ribbon {
            if pixel.x < ribbon_clip_min.x || pixel.x >= ribbon_clip_max.x ||
               pixel.y < ribbon_clip_min.y || pixel.y >= ribbon_clip_max.y {
                continue;
            }
        }

        var effective_pixel = pixel;
        if in_ribbon {
            effective_pixel = pixel - ribbon_scroll;
        }

        // SdfRotate: undo the instance's rotation-about-a-pivot BEFORE
        // anything below reads effective_pixel, so every draw type inherits
        // it uniformly (MSDF/bitmap text read effective_pixel directly;
        // shapes read it via `p = effective_pixel - center` just below) —
        // no per-type special case. xform.x == 0 (no rotation) is the
        // overwhelmingly common path and costs one branch.
        //
        // The forward transform is `x' = a*x + b*y + tx`, `y' = c*x + d*y +
        // ty`; it is always a pure rotation (the [a,b;c,d] part is
        // orthogonal — never a general affine), so its inverse is its own
        // transpose. That is exact for a quarter-turn (a/b/c/d are only
        // ever 0/1/-1, chosen by the CPU side without any sin/cos call —
        // see `SdfRotate::Quarter` in libmsdf's drawlist), which is what
        // keeps a 90-degree-rotated rect landing back on the exact pixel
        // grid instead of a fraction of a pixel off it.
        let xform_idx = u32(cmd.xform.x);
        if xform_idx != 0u {
            let base = (xform_idx - 1u) * 2u;
            if base + 1u < arrayLength(&param_bank) {
                let row_ab_cd = param_bank[base];       // [a, b, c, d]
                let row_txty = param_bank[base + 1u];   // [tx, ty, 0, 0]
                let rel = effective_pixel - row_txty.xy;
                effective_pixel = vec2<f32>(
                    row_ab_cd.x * rel.x + row_ab_cd.z * rel.y,
                    row_ab_cd.y * rel.x + row_ab_cd.w * rel.y,
                );
            }
        }

        // Cheap vertical-band reject for the TEXT commands (4u bitmap,
        // 8u MSDF) — the only per-fragment loops in this shader. A text
        // draw occupies the vertical span [pos.y, pos.y + size.y]; a
        // fragment outside it can't cover any glyph, so skip before
        // entering the char loop. Shapes are single-eval and cheap, and
        // their radius can exceed size.y, so they are NOT reject-gated.
        if ty == 4u || ty == 8u {
            if effective_pixel.y < cmd.pos.y - 4.0 ||
               effective_pixel.y > cmd.pos.y + cmd.size.y + 4.0 {
                continue;
            }
        }

        let center = cmd.pos + cmd.size * 0.5;
        let p = effective_pixel - center;

        var d: f32 = 1e6;
        var shadow_d: f32 = 1e6;
        // Whether shadow_d (above) is meaningful for this command — box/
        // slab/circle shapes always cast one; a Bézier stroke (case 10u)
        // only does when the caller opts in via the high bit of its
        // param_bank slot (see BEZIER_SHADOW_BIT), since most strokes
        // (nav-graph arcs, icon glyphs) are not meant to be shadowed.
        var shadow_on: bool = false;

        switch ty {
            case 0u: {
                let half = cmd.size * 0.5;
                d = sd_box(p, half);
                shadow_d = sd_box(p - vec2<f32>(2.0, 3.0), half);
                shadow_on = true;
            }
            case 1u: {
                let half = cmd.size * 0.5;
                let radius = cmd.params.y;
                d = sd_rounded_box(p, half, radius);
                shadow_d = sd_rounded_box(p - vec2<f32>(2.0, 3.0), half, radius);
                shadow_on = true;
            }
            case 11u: { // Per-corner rounded box — radii in the aux param bank
                let half = cmd.size * 0.5;
                let pidx = bitcast<u32>(cmd.params.w);
                var radii = vec4<f32>(0.0);
                if pidx < arrayLength(&param_bank) {
                    radii = param_bank[pidx];
                }
                d = sd_rounded_box_pc(p, half, radii);
                shadow_d = sd_rounded_box_pc(p - vec2<f32>(2.0, 3.0), half, radii);
                shadow_on = true;
            }
            case 2u: {
                let radius = cmd.params.y;
                d = sd_circle(p, radius);
                shadow_d = sd_circle(p - vec2<f32>(1.5, 2.0), radius);
                shadow_on = true;
            }
            case 3u: {
                let half_len = cmd.size.x * 0.5;
                let a = vec2<f32>(-half_len, 0.0);
                let b = vec2<f32>(half_len, 0.0);
                d = sd_segment(p, a, b, cmd.size.y * 0.5);
            }
            case 4u: { // Text — bitmap font atlas
                let glyph_w = uniforms.font.x;
                let glyph_h = uniforms.font.y;
                let first_cp = uniforms.font.z;

                if glyph_w > 0u && glyph_h > 0u {
                    let packed = bitcast<u32>(cmd.params.w);
                    let char_offset = packed >> 16u;
                    let char_count = packed & 0xFFFFu;

                    let text_x = cmd.pos.x;
                    let text_y = cmd.pos.y;
                    let text_size = cmd.size.y;
                    let scale_f = max(text_size / f32(glyph_h), 1.0);
                    let advance = f32(glyph_w + 1u) * scale_f;

                    let cb_len = arrayLength(&char_buffer);
                    let gb_len = arrayLength(&glyph_bitmap);
                    for (var ci: u32 = 0u; ci < char_count; ci = ci + 1u) {
                        let cb_idx = char_offset + ci;
                        if cb_idx >= cb_len { break; }
                        let cp = char_buffer[cb_idx];
                        let glyph_idx = clamp(cp, first_cp, uniforms.font.w) - first_cp;

                        let char_x = text_x + f32(ci) * advance;
                        let local_x = effective_pixel.x - char_x;
                        let local_y = effective_pixel.y - text_y;

                        if local_x >= 0.0 && local_x < f32(glyph_w) * scale_f &&
                           local_y >= 0.0 && local_y < f32(glyph_h) * scale_f {
                            let gx = u32(local_x / scale_f);
                            let gy = u32(local_y / scale_f);
                            let gbm_idx = glyph_idx * glyph_h + gy;
                            if gbm_idx >= gb_len { continue; }
                            let row_byte = glyph_bitmap[gbm_idx];
                            let bit = glyph_w - 1u - gx;
                            if (row_byte & (1u << bit)) != 0u {
                                d = -1.0;
                                break;
                            }
                        }
                    }
                }
            }
            case 5u: { // Texture
                let tex_idx = u32(cmd.params.y);
                let half = cmd.size * 0.5;
                if abs(p.x) < half.x && abs(p.y) < half.y {
                    let uv = (p + half) / cmd.size;
                    let tex = uniforms.texture_bank[tex_idx];
                    // textureSampleLevel: this sample is inside a
                    // non-uniform `if` (per-fragment rect test) —
                    // implicit derivatives there are UB and hang some
                    // mobile GPUs. Textures are non-mipmapped, so LOD 0
                    // is exact.
                    let color_sample = textureSampleLevel(tex_array, tex_sampler, uv, i32(tex.layer), 0.0);
                    let blend_alpha = color_sample.a * cmd.color.a;
                    if blend_alpha > 0.001 {
                        let tinted = vec4<f32>(color_sample.rgb * cmd.color.rgb, blend_alpha);
                        result = blend_over(result, tinted);
                    }
                }
            }
            case 8u: { // MSDF Text — uniform em-square projection
                // The distance range the ATLAS was baked with, in atlas
                // texels. It is NOT the screen-space range: the glyph cell is
                // scaled to the line box, so the field is minified with it
                // (see `screen_px_range` below).
                let atlas_px_range = max(cmd.params.y, 1.0);
                let x_margin_frac = cmd.params.z;
                let packed = bitcast<u32>(cmd.params.w);
                let char_offset = packed >> 16u;
                let char_count = packed & 0xFFFFu;

                let line_h = cmd.size.y;
                // line_x is the actual origin of the first glyph.
                // cmd.pos.x was shifted left by x_margin_frac * line_h in
                // the drawlist lowering.
                let line_x = cmd.pos.x + x_margin_frac * line_h;
                let line_y = cmd.pos.y;

                let atlas_dim = vec2<f32>(textureDimensions(msdf_atlas));

                var cursor_x: f32 = 0.0;

                let char_buf_len = arrayLength(&char_buffer);
                let glyph_tbl_len = arrayLength(&glyph_table);
                for (var ci: u32 = 0u; ci < char_count; ci = ci + 1u) {
                    // Bounds-guard every storage read: an out-of-range
                    // char_offset / gt_idx here is a GPU pagefault on
                    // some drivers (device lost), not a benign garbage
                    // read.
                    let ce_idx = char_offset + ci;
                    if ce_idx >= char_buf_len { break; }
                    let entry = char_buffer[ce_idx];
                    let gt_idx = entry >> 16u;
                    let delta_biased = f32(entry & 0xFFFFu);
                    let delta_px = (delta_biased - 2048.0) / 16.0;

                    if gt_idx * 2u + 1u >= glyph_tbl_len { continue; }
                    let g0 = glyph_table[gt_idx * 2u];
                    let g1 = glyph_table[gt_idx * 2u + 1u];

                    let atlas_gx = f32(g0.y & 0xFFFFu);
                    let atlas_gy = f32(g0.y >> 16u);
                    let atlas_gw = f32(g0.z & 0xFFFFu);
                    let atlas_gh = f32(g0.z >> 16u);

                    // Fetch projection metrics from g1
                    let baseline_row = bitcast<f32>(g1.x);
                    let px_per_em_atlas = bitcast<f32>(g1.y);
                    let x_margin = bitcast<f32>(g1.z);

                    // Scale: atlas pixels per screen pixel.
                    // To avoid clipping, we map the entire line_h box to the entire atlas_gh cell.
                    let scale = atlas_gh / line_h;
                    // Derived EM size on screen:
                    let font_size = px_per_em_atlas / scale;

                    // Screen-space distance range (Chlumsky's `screenPxRange`).
                    // The baked field spans `atlas_px_range` TEXELS; the cell is
                    // minified by `scale` texels per screen pixel, so on screen
                    // the field spans `atlas_px_range / scale` PIXELS. Feeding
                    // the atlas-space range straight into the alpha ramp (as
                    // this did before) makes the ramp `scale`× too steep — at
                    // 12px text on 48px cells that is 3.08×, a near-binary
                    // threshold that drops any feature thinner than one pixel
                    // whenever it falls between two pixel centres (Roboto's 't'
                    // crossbar is 0.069em ≈ 0.83px at 12px). Clamping at 1.0
                    // keeps the ramp at least a pixel wide under extreme
                    // minification, which is also where the baked range runs
                    // out — see FontAtlas::min_antialiased_font_size.
                    let screen_px_range = max(atlas_px_range / scale, 1.0);

                    let advance_x_norm = bitcast<f32>(g0.w);
                    let advance_px = advance_x_norm * font_size + delta_px;

                    let gx = line_x + cursor_x;
                    let gy = line_y;

                    // Screen pixel → atlas cell pixel
                    // Shift by x_margin so the font origin (fx=0) maps to the cursor (gx)
                    let acx = (effective_pixel.x - gx) * scale + x_margin;
                    // Vertical mapping: line_y corresponds to the atlas cell top (0.0).
                    // The scale ensures the line box maps perfectly to the gs x gs cell.
                    let acy = (effective_pixel.y - gy) * scale;

                    // Sample within cell, MSDF masks via distance field
                    if acx >= 0.0 && acx < atlas_gw &&
                       acy >= 0.0 && acy < atlas_gh {
                        // Offset by 0.5 to sample from pixel centers and avoid edge bleed
                        let u = (atlas_gx + acx + 0.5) / atlas_dim.x;
                        let v = (atlas_gy + acy + 0.5) / atlas_dim.y;

                        // textureSampleLevel (explicit LOD 0), NOT
                        // textureSample: this call sits inside a
                        // per-fragment char loop AND a non-uniform if —
                        // implicit-derivative sampling there is UB in
                        // WGSL. MSDF atlases are single-level, so LOD 0
                        // is exact and derivative-free.
                        let sample = textureSampleLevel(msdf_atlas, msdf_sampler, vec2<f32>(u, v), 0.0);
                        let sd = msdf_median(sample.r, sample.g, sample.b);

                        // Standard: sd > 0.5 is inside. (sd - 0.5) *
                        // screen_px_range is the signed distance to the
                        // outline in SCREEN pixels, so + 0.5 is the pixel's
                        // coverage — proper analytic antialiasing.
                        let alpha = clamp(screen_px_range * (sd - 0.5) + 0.5, 0.0, 1.0);

                        if alpha > 0.01 {
                            let glyph_color = vec4<f32>(cmd.color.rgb, cmd.color.a * alpha);
                            result = blend_over(result, glyph_color);
                        }
                    }

                    cursor_x += advance_px;
                }
            }
            case 9u: { // Outline — rounded rect stroke, no fill
                let half = cmd.size * 0.5;
                let radius = cmd.params.y;
                let thickness = cmd.params.z;
                let box_d = sd_rounded_box(p, half, radius);
                d = abs(box_d) - thickness * 0.5;
            }
            case 10u: { // Cubic Bézier stroke (containment / flow arcs)
                // pos = P0, size = P3 (absolute coords); params.y =
                // thickness; params.w = bitcast param_bank index holding
                // (C1.xy, C2.xy) — high bit (BEZIER_SHADOW_BIT) is a
                // per-instance opt-in flag for the drop shadow below, set
                // by dashed-border corner arcs so they read uniformly with
                // the straight dash boxes; unset (the common case: nav-graph
                // arcs, icon glyphs) leaves those strokes unshadowed as
                // before. Degenerate fallback: straight segment.
                let p0 = cmd.pos;
                let p3 = cmd.size;
                var c1 = p0;
                var c2 = p3;
                let raw_idx = bitcast<u32>(cmd.params.w);
                let wants_shadow = (raw_idx & 0x80000000u) != 0u;
                let pidx = raw_idx & 0x7FFFFFFFu;
                if pidx < arrayLength(&param_bank) {
                    let ctrl = param_bank[pidx];
                    c1 = ctrl.xy;
                    c2 = ctrl.zw;
                }
                let thickness = max(cmd.params.y, 1.0);
                // Control-hull bbox reject: the curve lies inside the
                // convex hull of its control points. Padded further when
                // shadowed so the offset+blurred shadow (reaches ~7px past
                // the stroke, see the shadow block below) isn't clipped.
                let margin = vec2<f32>(select(thickness, thickness + 8.0, wants_shadow));
                let bb_min = min(min(p0, p3), min(c1, c2)) - margin;
                let bb_max = max(max(p0, p3), max(c1, c2)) + margin;
                if effective_pixel.x >= bb_min.x && effective_pixel.x <= bb_max.x &&
                   effective_pixel.y >= bb_min.y && effective_pixel.y <= bb_max.y {
                    d = sd_cubic_stroke(effective_pixel, p0, c1, c2, p3, thickness);
                    if wants_shadow {
                        shadow_d = sd_cubic_stroke(effective_pixel - vec2<f32>(2.0, 3.0), p0, c1, c2, p3, thickness);
                        shadow_on = true;
                    }
                }
            }
            default: {
            }
        }

        // Shadow (skip for outline/text, and for bezier strokes that don't
        // opt in — see shadow_on above)
        if shadow_on {
            let shadow_alpha = 0.15 * (1.0 - smoothstep(-1.0, 4.0, shadow_d));
            let shadow_color = vec4<f32>(0.0, 0.0, 0.0, shadow_alpha);
            result = blend_over(result, shadow_color);
        }

        // Animation
        var anim_alpha: f32 = 1.0;
        let anim_idx = u32(cmd.params.z);
        let has_anim = step(0.5, f32(anim_idx));
        if has_anim > 0.0 {
            let a = uniforms.anim_bank[max(anim_idx, 1u) - 1u];
            let has_enable = step(0.5, f32(a.enable_ref));
            let slot = a.enable_ref & 0xFFFFu;
            let pack = slot / 4u;
            let comp = slot % 4u;
            let bank_val = f32(uniforms.int_bank[min(pack, 3u)][min(comp, 3u)]);
            let enabled = mix(1.0, bank_val, has_enable);
            let time_s = uniforms.time_delta.x / 1000.0;
            let phase = sin(time_s * a.freq * 6.283185);
            let threshold = 1.0 - a.duty * 2.0;
            let pulse = smoothstep(0.0, 0.1, phase - threshold);
            let has_freq = step(0.001, a.freq);
            anim_alpha = mix(enabled, enabled * pulse, has_freq);
        }

        // Shape fill (skip for texture and MSDF text — they blend internally)
        if ty != 5u && ty != 8u {
            let fill_alpha = cmd.color.a * anim_alpha * (1.0 - smoothstep(-0.5, 0.5, d));
            if fill_alpha > 0.001 {
                let shape_color = vec4<f32>(cmd.color.rgb, fill_alpha);
                result = blend_over(result, shape_color);
            }
        }
    }

    // If no draw covered this pixel, discard so the destination content
    // (from a prior pass on the same target) shows through.
    if result.a < 0.001 {
        discard;
    }

    return result;
}
