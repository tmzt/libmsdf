//! SDF draw command type and evaluation functions.
//!
//! `SdfDrawCmd` is the unified GPU-uploadable draw command format; both the
//! GPU (wgpu) renderer and CPU reference evaluation consume it.

/// Draw type constants for SdfDrawCmd.params[0].
pub const DRAW_TYPE_BOX: f32 = 0.0;
pub const DRAW_TYPE_SLAB: f32 = 1.0;
pub const DRAW_TYPE_CIRCLE: f32 = 2.0;
pub const DRAW_TYPE_LINE: f32 = 3.0;
pub const DRAW_TYPE_TEXT: f32 = 4.0;
pub const DRAW_TYPE_TEXTURE: f32 = 5.0;
pub const DRAW_TYPE_RIBBON_BEGIN: f32 = 6.0;
pub const DRAW_TYPE_RIBBON_END: f32 = 7.0;
pub const DRAW_TYPE_MSDF_TEXT: f32 = 8.0;
/// Outline (rounded rect stroke, no fill). params: [9, radius, thickness, 0]
pub const DRAW_TYPE_OUTLINE: f32 = 9.0;
/// Cubic Bézier stroke (nav-graph arcs). pos = P0, size = P3 (absolute),
/// params: [10, thickness, anim_idx, param_bank index → (C1.xy, C2.xy)].
pub const DRAW_TYPE_BEZIER: f32 = 10.0;

/// Maximum draw commands per frame.
pub const MAX_DRAW_CMDS: usize = 4096;

/// Maximum animation bank entries.
pub const MAX_ANIMS: usize = 32;

/// Maximum texture bank entries.
pub const MAX_TEXTURES: usize = 8;

/// Segments used to flatten a cubic Bézier for CPU distance evaluation
/// (matches BEZIER_FLATTEN_STEPS in sdf_render.wgsl).
pub const BEZIER_FLATTEN_STEPS: u32 = 24;

/// GPU texture descriptor — stored in texture_bank, 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpuTexture {
    pub width: u32,
    pub height: u32,
    pub layer: u32,      // index into texture_2d_array
    pub flags: u32,      // format, filtering mode
}

impl GpuTexture {
    pub const NONE: Self = Self { width: 0, height: 0, layer: 0, flags: 0 };
}

/// Animation descriptor — stored in AnimBank, referenced by SdfDrawCmd.params[2].
/// 16 bytes, GPU-uploadable.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anim {
    /// Oscillation frequency in Hz. 0 = no oscillation (effectively off).
    pub freq: f32,
    /// Duty cycle 0.0-1.0. 1.0 = always on, 0.5 = blink.
    pub duty: f32,
    /// Packed bank ref for enable flag (bank_type << 16 | slot). 0 = always enabled.
    pub enable_ref: u32,
    pub _pad: u32,
}

impl Anim {
    pub const NONE: Self = Self { freq: 0.0, duty: 1.0, enable_ref: 0, _pad: 0 };
}

/// A single SDF draw command. repr(C) for GPU buffer upload.
///
/// Wire format: 48 bytes (12 × f32).
/// ```text
/// pos:    [f32; 2]   x, y
/// size:   [f32; 2]   w, h
/// color:  [f32; 4]   r, g, b, a (0.0-1.0)
/// params: [f32; 4]   [type, radius, anim_idx, slot]
///   params[0] = draw type (DRAW_TYPE_*)
///   params[1] = radius (Slab, Circle) / thickness (Outline, Bézier)
///   params[2] = anim_bank index (0 = no animation, 1+ = AnimBank[idx-1])
///   params[3] = slot (Text: string ref; Bézier: param_bank index, bitcast)
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SdfDrawCmd {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub params: [f32; 4],
}

impl SdfDrawCmd {
    pub const ZERO: Self = Self {
        pos: [0.0; 2],
        size: [0.0; 2],
        color: [0.0; 4],
        params: [0.0; 4],
    };

    /// Draw type from params[0].
    pub fn draw_type(&self) -> f32 {
        self.params[0]
    }

    /// Radius from params[1] (Slab, Circle) / thickness (Outline, Bézier).
    pub fn radius(&self) -> f32 {
        self.params[1]
    }

    /// Slot/string index from params[3] (Text).
    pub fn slot(&self) -> f32 {
        self.params[3]
    }
}

// ── SDF evaluation functions (pure math, shared between CPU and shader) ──

/// Signed distance to a rounded box centered at origin.
/// `half_size` = (w/2, h/2), `radius` = corner radius.
/// Returns negative inside, positive outside.
pub fn sd_rounded_box(px: f32, py: f32, half_w: f32, half_h: f32, radius: f32) -> f32 {
    let qx = px.abs() - half_w + radius;
    let qy = py.abs() - half_h + radius;
    let outside = (qx.max(0.0) * qx.max(0.0) + qy.max(0.0) * qy.max(0.0)).sqrt();
    let inside = qx.max(qy).min(0.0);
    outside + inside - radius
}

/// Signed distance to a box (axis-aligned, no rounding).
pub fn sd_box(px: f32, py: f32, half_w: f32, half_h: f32) -> f32 {
    sd_rounded_box(px, py, half_w, half_h, 0.0)
}

/// Signed distance to a circle centered at origin.
pub fn sd_circle(px: f32, py: f32, radius: f32) -> f32 {
    (px * px + py * py).sqrt() - radius
}

/// Signed distance to a line segment from (ax,ay) to (bx,by), with thickness.
pub fn sd_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32, thickness: f32) -> f32 {
    sd_segment_raw(px, py, ax, ay, bx, by) - thickness * 0.5
}

/// Unsigned distance to a line segment (no thickness applied).
fn sd_segment_raw(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let pax = px - ax;
    let pay = py - ay;
    let bax = bx - ax;
    let bay = by - ay;
    let denom = (bax * bax + bay * bay).max(1e-6);
    let h = ((pax * bax + pay * bay) / denom).clamp(0.0, 1.0);
    let dx = pax - bax * h;
    let dy = pay - bay * h;
    (dx * dx + dy * dy).sqrt()
}

/// Point on a cubic Bézier at parameter `t`.
pub fn cubic_point(p0: [f32; 2], c1: [f32; 2], c2: [f32; 2], p3: [f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    let w0 = u * u * u;
    let w1 = 3.0 * u * u * t;
    let w2 = 3.0 * u * t * t;
    let w3 = t * t * t;
    [
        w0 * p0[0] + w1 * c1[0] + w2 * c2[0] + w3 * p3[0],
        w0 * p0[1] + w1 * c1[1] + w2 * c2[1] + w3 * p3[1],
    ]
}

/// Signed distance to a stroked cubic Bézier (flattened to
/// `BEZIER_FLATTEN_STEPS` segments; matches the WGSL implementation).
/// Negative inside the stroke.
pub fn sd_cubic_stroke(
    px: f32,
    py: f32,
    p0: [f32; 2],
    c1: [f32; 2],
    c2: [f32; 2],
    p3: [f32; 2],
    thickness: f32,
) -> f32 {
    let mut prev = p0;
    let mut dmin = f32::MAX;
    for i in 1..=BEZIER_FLATTEN_STEPS {
        let t = i as f32 / BEZIER_FLATTEN_STEPS as f32;
        let pt = cubic_point(p0, c1, c2, p3, t);
        dmin = dmin.min(sd_segment_raw(px, py, prev[0], prev[1], pt[0], pt[1]));
        prev = pt;
    }
    dmin - thickness * 0.5
}

/// Evaluate the SDF for a single draw command at pixel position (px, py).
/// Returns (distance, color) — negative distance means inside.
/// Commands that need the aux param bank (Bézier control points) fall back
/// to degenerate values; use [`sdf_eval_with_params`] for those.
pub fn sdf_eval(cmd: &SdfDrawCmd, px: f32, py: f32) -> (f32, [f32; 4]) {
    sdf_eval_with_params(cmd, px, py, &[])
}

/// Evaluate the SDF for a single draw command, with access to the aux
/// param bank (`param_bank[bitcast(params[3])]` = Bézier C1.xy, C2.xy).
pub fn sdf_eval_with_params(
    cmd: &SdfDrawCmd,
    px: f32,
    py: f32,
    param_bank: &[[f32; 4]],
) -> (f32, [f32; 4]) {
    let cx = cmd.pos[0] + cmd.size[0] * 0.5;
    let cy = cmd.pos[1] + cmd.size[1] * 0.5;
    let local_x = px - cx;
    let local_y = py - cy;
    let hw = cmd.size[0] * 0.5;
    let hh = cmd.size[1] * 0.5;

    let d = match cmd.draw_type() as u32 {
        0 => sd_box(local_x, local_y, hw, hh),                         // Box
        1 => sd_rounded_box(local_x, local_y, hw, hh, cmd.radius()),   // Slab
        2 => sd_circle(local_x, local_y, hw.min(hh)),                  // Circle
        3 => {
            // Line: pos = (x1,y1), size = (x2,y2)
            sd_segment(px, py, cmd.pos[0], cmd.pos[1], cmd.size[0], cmd.size[1], 1.0)
        }
        4 => sd_box(local_x, local_y, hw, hh),                         // Text (placeholder box)
        9 => {
            // Outline: rounded-rect stroke
            let box_d = sd_rounded_box(local_x, local_y, hw, hh, cmd.radius());
            box_d.abs() - cmd.params[2] * 0.5
        }
        10 => {
            // Bézier stroke: pos = P0, size = P3, param bank holds C1/C2.
            let p0 = cmd.pos;
            let p3 = cmd.size;
            let idx = cmd.params[3].to_bits() as usize;
            let (c1, c2) = match param_bank.get(idx) {
                Some(ctrl) => ([ctrl[0], ctrl[1]], [ctrl[2], ctrl[3]]),
                None => (p0, p3),
            };
            sd_cubic_stroke(px, py, p0, c1, c2, p3, cmd.radius().max(1.0))
        }
        _ => f32::MAX,
    };

    (d, cmd.color)
}

/// Evaluate SDF with time-based animation from AnimBank.
/// `time_ms` = elapsed milliseconds from GlobalUniforms.
/// `anim_bank` = current animation descriptors.
/// `int_bank` = for reading enable flags.
/// Returns (distance, animated_color) with alpha modulated by animation.
pub fn sdf_eval_animated(
    cmd: &SdfDrawCmd,
    px: f32,
    py: f32,
    time_ms: f32,
    anim_bank: &[Anim],
    int_bank: &[i32],
) -> (f32, [f32; 4]) {
    let (d, mut color) = sdf_eval(cmd, px, py);

    // params[2] = anim_bank index. 0 = no animation, 1+ = AnimBank[idx-1]
    let anim_idx = cmd.params[2] as u32;
    if anim_idx > 0 {
        let idx = (anim_idx - 1) as usize;
        if idx < anim_bank.len() {
            let anim = &anim_bank[idx];

            // Read enable from packed int_bank ref
            let enabled = if anim.enable_ref != 0 {
                let slot = (anim.enable_ref & 0xFFFF) as usize;
                if slot < int_bank.len() { int_bank[slot] as f32 } else { 0.0 }
            } else {
                1.0 // no enable ref = always enabled
            };

            if anim.freq > 0.0 {
                let time_s = time_ms / 1000.0;
                let phase = (time_s * anim.freq * std::f32::consts::TAU).sin();
                let threshold = 1.0 - anim.duty * 2.0;
                let pulse = smoothstep(0.0, 0.1, phase - threshold);
                color[3] *= enabled * pulse;
            } else {
                // freq=0: static, just apply enable
                color[3] *= enabled;
            }
        }
    }

    (d, color)
}

/// Smoothstep interpolation (matches WGSL smoothstep).
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Check if any animation in the bank is active (freq > 0 or enable != 0).
/// Used to determine refresh rate — if no animations, can drop to low FPS.
pub fn any_animation_active(anim_bank: &[Anim], int_bank: &[i32]) -> bool {
    anim_bank.iter().any(|a| {
        if a.freq > 0.0 {
            if a.enable_ref != 0 {
                let slot = (a.enable_ref & 0xFFFF) as usize;
                slot < int_bank.len() && int_bank[slot] != 0
            } else {
                true
            }
        } else {
            false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sd_box_inside() {
        assert!(sd_box(0.0, 0.0, 10.0, 10.0) < 0.0);
    }

    #[test]
    fn sd_box_outside() {
        assert!(sd_box(15.0, 0.0, 10.0, 10.0) > 0.0);
    }

    #[test]
    fn sd_rounded_box_corner() {
        // Point at corner, just outside the rounding
        let d = sd_rounded_box(9.0, 9.0, 10.0, 10.0, 2.0);
        assert!(d < 0.0); // inside the rounded box
    }

    #[test]
    fn sd_circle_center() {
        assert!(sd_circle(0.0, 0.0, 5.0) < 0.0);
    }

    #[test]
    fn sd_circle_outside() {
        assert!(sd_circle(10.0, 0.0, 5.0) > 0.0);
    }

    #[test]
    fn sdf_eval_box() {
        let cmd = SdfDrawCmd {
            pos: [10.0, 10.0],
            size: [20.0, 20.0],
            color: [1.0, 0.0, 0.0, 1.0],
            params: [DRAW_TYPE_BOX, 0.0, 0.0, 0.0],
        };
        let (d, _) = sdf_eval(&cmd, 20.0, 20.0); // center
        assert!(d < 0.0);
        let (d, _) = sdf_eval(&cmd, 0.0, 0.0); // outside
        assert!(d > 0.0);
    }

    #[test]
    fn cubic_endpoints() {
        let p0 = [10.0, 10.0];
        let p3 = [90.0, 50.0];
        let c1 = [30.0, -20.0];
        let c2 = [70.0, 80.0];
        let a = cubic_point(p0, c1, c2, p3, 0.0);
        let b = cubic_point(p0, c1, c2, p3, 1.0);
        assert!((a[0] - p0[0]).abs() < 1e-5 && (a[1] - p0[1]).abs() < 1e-5);
        assert!((b[0] - p3[0]).abs() < 1e-5 && (b[1] - p3[1]).abs() < 1e-5);
    }

    #[test]
    fn bezier_stroke_on_and_off_curve() {
        // A straight "curve" (collinear controls) behaves like a segment.
        let p0 = [0.0, 0.0];
        let c1 = [25.0, 0.0];
        let c2 = [75.0, 0.0];
        let p3 = [100.0, 0.0];
        let on = sd_cubic_stroke(50.0, 0.0, p0, c1, c2, p3, 4.0);
        let off = sd_cubic_stroke(50.0, 20.0, p0, c1, c2, p3, 4.0);
        assert!(on < 0.0, "point on curve should be inside stroke: {on}");
        assert!(off > 0.0, "point 20px off curve should be outside: {off}");
    }

    #[test]
    fn sdf_eval_bezier_uses_param_bank() {
        // Arc bowing upward: midpoint of the curve is far from the chord.
        let cmd = SdfDrawCmd {
            pos: [0.0, 100.0],
            size: [100.0, 100.0], // P3
            color: [1.0; 4],
            params: [DRAW_TYPE_BEZIER, 6.0, 0.0, f32::from_bits(0)],
        };
        let params = [[30.0f32, 0.0, 70.0, 0.0]]; // C1, C2 pull the curve to y≈25..75
        let mid = cubic_point([0.0, 100.0], [30.0, 0.0], [70.0, 0.0], [100.0, 100.0], 0.5);
        let (d_on, _) = sdf_eval_with_params(&cmd, mid[0], mid[1], &params);
        assert!(d_on < 0.0, "curve midpoint should be inside stroke: {d_on}");
        // Chord midpoint is far from the bowed curve.
        let (d_chord, _) = sdf_eval_with_params(&cmd, 50.0, 100.0, &params);
        assert!(d_chord > 10.0, "chord midpoint should be outside: {d_chord}");
    }
}
