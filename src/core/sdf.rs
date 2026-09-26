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
// 6 and 7 are RETIRED. They were `RIBBON_BEGIN`/`RIBBON_END`, control
// commands the shader interpreted with a single "current scissor" register.
// A clip is now resolved on the CPU and carried per instance in
// [`SdfDrawCmd::clip`], so there is no control command and no register — and
// therefore no "do not nest" rule either. The numbers stay unused rather than
// being recycled: an old lowered frame must not decode as a new shape.
pub const DRAW_TYPE_MSDF_TEXT: f32 = 8.0;
/// Outline (rounded rect stroke, no fill). params: [9, radius, thickness, 0]
pub const DRAW_TYPE_OUTLINE: f32 = 9.0;
/// Cubic Bézier stroke (nav-graph arcs). pos = P0, size = P3 (absolute),
/// params: [10, thickness, anim_idx, param_bank index → (C1.xy, C2.xy)].
/// Whether the stroke casts a drop shadow is [`XFORM_RAISED`], the same slot
/// every other type reads it from.
pub const DRAW_TYPE_BEZIER: f32 = 10.0;
/// Rounded box with per-corner radii (e.g. the M3 modal nav drawer: square
/// against the screen edge, rounded on the trailing side). Like SLAB but the
/// four corner radii live in the aux param bank as [tl, tr, br, bl] (screen
/// space, +x right / +y down). params: [11, 0, anim_idx, param_bank index].
pub const DRAW_TYPE_SLAB_PC: f32 = 11.0;

/// `xform[1]`, the per-instance **raised** flag: `0.0` (the default) is flat
/// and casts nothing; this value makes the instance cast the renderer's drop
/// shadow.
///
/// **A shadow is declared, never inherited.** It used to be the other way
/// round — every filled shape cast one and `XFORM_FLAT` opted out — which was
/// a default nothing had chosen: all fifteen `elevation` declarations in
/// authored TSX were `elevation={0}`, i.e. authors only ever fighting it. It
/// also could not be right, because the shader derives the shadow from the
/// SHAPE and not from the instance's alpha, so a fully transparent filled box
/// still dimmed what was behind it (the drawer scrim declared 0.4 and landed
/// at an effective 0.49).
///
/// It lives in `xform` rather than in `params` because `params` is already full
/// on two of the four shadow-casting types (`params[1]` is the radius for SLAB
/// and CIRCLE, `params[3]` the aux-bank index for SLAB_PC), while `xform[1..4]`
/// were reserved on *every* type — so one decode in the shader covers all four
/// rather than four per-type ones. Mirrored in `sdf_render.wgsl`'s shadow block
/// — keep the two in sync.
///
/// Bézier strokes read this same slot. They used to carry their own opt-in
/// (a high bit on the param_bank index) because their default differed from
/// the filled shapes'; once both default to no shadow the two flags said the
/// identical thing, so there is one.
pub const XFORM_RAISED: f32 = 1.0;

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
    pub layer: u32, // index into texture_2d_array
    pub flags: u32, // format, filtering mode
}

impl GpuTexture {
    pub const NONE: Self = Self {
        width: 0,
        height: 0,
        layer: 0,
        flags: 0,
    };
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
    pub const NONE: Self = Self {
        freq: 0.0,
        duty: 1.0,
        enable_ref: 0,
        _pad: 0,
    };
}

/// The axis-aligned region an instance is allowed to paint into, in the same
/// logical pixels as [`SdfDrawCmd::pos`].
///
/// **A clip is per-instance data, not a control command.** The scissor used to
/// be a pair of instructions in the stream (`RIBBON_BEGIN`/`RIBBON_END`) that
/// the shader interpreted with one "currently active region" register, which
/// forced two things: clips could not nest, and *leaving* an inner clip had to
/// restate the enclosing rect or the enclosing bound was silently handed away
/// to everything drawn afterwards. Resolving the stack on the CPU and
/// snapshotting the answer per instance — the same shape
/// `DrawList::instance_effects` already had for blur/ombré — removes the
/// register, the ordering dependency and both failure modes; nesting is just
/// intersection, and an instance carries its own bound wherever it lands in
/// the stream. The cost is these four floats on every command.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ClipRect {
    /// Inclusive top-left corner.
    pub min: [f32; 2],
    /// Exclusive bottom-right corner.
    pub max: [f32; 2],
}

/// Half-extent of [`ClipRect::UNBOUNDED`]. Deliberately a large FINITE number
/// rather than `f32::INFINITY`: this value is uploaded into a storage buffer
/// and compared in WGSL, and infinities are the kind of thing a downlevel
/// GLES/WebGL backend is allowed to be creative about. `1e30` is ~2.5e25
/// screens wide at any sane DPI, so it cannot be reached by real geometry, and
/// `max - min` still finishes finite.
pub const CLIP_UNBOUNDED_EXTENT: f32 = 1.0e30;

impl ClipRect {
    /// The clip an instance pushed outside any [`crate::DrawList::push_clip`]
    /// scope gets: so wide nothing can fall outside it, so the shader needs no
    /// "is there a clip" branch at all.
    pub const UNBOUNDED: Self = Self {
        min: [-CLIP_UNBOUNDED_EXTENT, -CLIP_UNBOUNDED_EXTENT],
        max: [CLIP_UNBOUNDED_EXTENT, CLIP_UNBOUNDED_EXTENT],
    };

    /// The rect at `pos` with `size`, as callers of `push_clip` spell it.
    /// A negative extent is clamped to zero rather than inverting the rect.
    pub fn from_pos_size(pos: [f32; 2], size: [f32; 2]) -> Self {
        Self {
            min: pos,
            max: [pos[0] + size[0].max(0.0), pos[1] + size[1].max(0.0)],
        }
    }

    /// The overlap of two clips — what nesting one inside another means.
    ///
    /// A miss yields an EMPTY rect (zero extent) rather than an inverted one,
    /// and empty is a real answer: a row scrolled fully out of its viewport
    /// has no pixels, and this says exactly that.
    pub fn intersect(self, other: Self) -> Self {
        let min = [self.min[0].max(other.min[0]), self.min[1].max(other.min[1])];
        let max = [
            self.max[0].min(other.max[0]).max(min[0]),
            self.max[1].min(other.max[1]).max(min[1]),
        ];
        Self { min, max }
    }

    /// Top-left corner, the `pos` half of the `pos`/`size` spelling.
    pub fn pos(self) -> [f32; 2] {
        self.min
    }

    /// Extent, the `size` half of the `pos`/`size` spelling.
    pub fn size(self) -> [f32; 2] {
        [self.max[0] - self.min[0], self.max[1] - self.min[1]]
    }

    /// Whether this is [`ClipRect::UNBOUNDED`] — i.e. no clip was in force.
    pub fn is_unbounded(self) -> bool {
        self == Self::UNBOUNDED
    }

    /// Whether the clip admits no pixels at all.
    pub fn is_empty(self) -> bool {
        self.max[0] <= self.min[0] || self.max[1] <= self.min[1]
    }

    /// Whether `point` is inside — the CPU statement of the shader's reject,
    /// min-inclusive / max-exclusive.
    pub fn contains(self, point: [f32; 2]) -> bool {
        point[0] >= self.min[0]
            && point[0] < self.max[0]
            && point[1] >= self.min[1]
            && point[1] < self.max[1]
    }

    /// The wire form: `[min.x, min.y, max.x, max.y]`.
    pub fn to_wire(self) -> [f32; 4] {
        [self.min[0], self.min[1], self.max[0], self.max[1]]
    }
}

/// A single SDF draw command. repr(C) for GPU buffer upload.
///
/// Wire format: 80 bytes (20 × f32).
/// ```text
/// pos:    [f32; 2]   x, y
/// size:   [f32; 2]   w, h
/// color:  [f32; 4]   r, g, b, a (0.0-1.0)
/// params: [f32; 4]   [type, radius, anim_idx, slot]
///   params[0] = draw type (DRAW_TYPE_*)
///   params[1] = radius (Slab, Circle) / thickness (Outline, Bézier)
///   params[2] = anim_bank index (0 = no animation, 1+ = AnimBank[idx-1])
///   params[3] = slot (Text: string ref; Bézier: param_bank index, bitcast)
/// xform:  [f32; 4]   [xform_idx, raised, blur_radius, alpha_ombre]
///   xform[1] = [`XFORM_RAISED`] to cast this instance's drop shadow, 0.0
///     (the default for every instance) to stay flat.
///   xform[0] = SdfRotate transform bank index (0 = none/identity, 1+ =
///     `param_bank[(idx-1)*2]` = [a,b,c,d], `param_bank[(idx-1)*2+1]` =
///     [tx,ty,_,_] — the forward affine `x'=a*x+b*y+tx`, `y'=c*x+d*y+ty`.
///     Applied to `effective_pixel` BEFORE any per-type SDF evaluation (see
///     `sdf_render.wgsl`'s xform block and `sdf_eval_with_params` below) so
///     every draw type — shapes and text alike — sees the pre-rotation
///     frame uniformly, with no per-type special case.
///   xform[2] = non-negative node-scoped edge-softening radius in logical
///     pixels; xform[3] = node-scoped bottom-fade strength, 0..1.
/// clip:   [f32; 4]   [min.x, min.y, max.x, max.y] — see [`ClipRect`].
///   The region this instance may paint into, already intersected with every
///   enclosing clip. [`SdfDrawCmd::NO_CLIP`] when none was in force.
/// ```
/// A trailing full vec4 (rather than a lone scalar) is deliberate: WGSL
/// pads a storage-buffer array's stride up to its element's own alignment
/// (16, forced by the vec4 members), so a scalar 5th field would leave 12
/// bytes of stride padding the Rust-side struct wouldn't otherwise have,
/// silently misaligning every command after the first when uploaded as raw
/// bytes. Full vec4s keep both sides at an already-16-byte-aligned 80.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SdfDrawCmd {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub params: [f32; 4],
    pub xform: [f32; 4],
    pub clip: [f32; 4],
}

impl SdfDrawCmd {
    /// The `clip` field of an instance under no clip scope — the wire form of
    /// [`ClipRect::UNBOUNDED`]. NOT `[0.0; 4]`: an all-zero clip is an EMPTY
    /// rect, which would make the instance invisible.
    pub const NO_CLIP: [f32; 4] = [
        -CLIP_UNBOUNDED_EXTENT,
        -CLIP_UNBOUNDED_EXTENT,
        CLIP_UNBOUNDED_EXTENT,
        CLIP_UNBOUNDED_EXTENT,
    ];

    pub const ZERO: Self = Self {
        pos: [0.0; 2],
        size: [0.0; 2],
        color: [0.0; 4],
        params: [0.0; 4],
        xform: [0.0; 4],
        clip: Self::NO_CLIP,
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

/// Signed distance to a rounded box with independent per-corner radii.
/// `radii` = [top-left, top-right, bottom-right, bottom-left] in screen space
/// (+x right, +y down). Selects the radius for the quadrant `(px, py)` lands in
/// then evaluates the standard rounded-box distance — the exact per-corner
/// isometry with [`sd_rounded_box`] (matches the `sd_rounded_box_pc` WGSL fn).
pub fn sd_rounded_box_per_corner(
    px: f32,
    py: f32,
    half_w: f32,
    half_h: f32,
    radii: [f32; 4],
) -> f32 {
    let r = if px > 0.0 {
        if py > 0.0 { radii[2] } else { radii[1] } // right side: br / tr
    } else if py > 0.0 {
        radii[3] // bl
    } else {
        radii[0] // tl
    };
    sd_rounded_box(px, py, half_w, half_h, r)
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

/// Undo a command's `SdfRotate` (see `SdfDrawCmd::xform` doc) so the
/// remainder of evaluation runs in the shape's pre-rotation frame, exactly
/// mirroring `sdf_render.wgsl`'s xform block — keep the two in sync.
/// `xform[0] == 0` (no rotation, overwhelmingly the common case) is a no-op.
fn undo_xform(cmd: &SdfDrawCmd, px: f32, py: f32, param_bank: &[[f32; 4]]) -> (f32, f32) {
    let idx = cmd.xform[0] as u32;
    if idx == 0 {
        return (px, py);
    }
    let base = ((idx - 1) * 2) as usize;
    let (Some(row0), Some(row1)) = (param_bank.get(base), param_bank.get(base + 1)) else {
        return (px, py);
    };
    let (rel_x, rel_y) = (px - row1[0], py - row1[1]);
    // Inverse of a rotation matrix is its transpose — exact for the
    // quarter-turn case, whose entries are only ever 0/1/-1.
    (
        row0[0] * rel_x + row0[2] * rel_y,
        row0[1] * rel_x + row0[3] * rel_y,
    )
}

/// Evaluate the SDF for a single draw command, with access to the aux
/// param bank (`param_bank[bitcast(params[3])]` = Bézier C1.xy, C2.xy; also
/// where `SdfRotate` transforms live — see [`SdfDrawCmd::xform`]).
pub fn sdf_eval_with_params(
    cmd: &SdfDrawCmd,
    px: f32,
    py: f32,
    param_bank: &[[f32; 4]],
) -> (f32, [f32; 4]) {
    let (px, py) = undo_xform(cmd, px, py, param_bank);
    let cx = cmd.pos[0] + cmd.size[0] * 0.5;
    let cy = cmd.pos[1] + cmd.size[1] * 0.5;
    let local_x = px - cx;
    let local_y = py - cy;
    let hw = cmd.size[0] * 0.5;
    let hh = cmd.size[1] * 0.5;

    let d = match cmd.draw_type() as u32 {
        0 => sd_box(local_x, local_y, hw, hh), // Box
        1 => sd_rounded_box(local_x, local_y, hw, hh, cmd.radius()), // Slab
        2 => sd_circle(local_x, local_y, hw.min(hh)), // Circle
        3 => {
            // Line: pos = (x1,y1), size = (x2,y2)
            sd_segment(
                px,
                py,
                cmd.pos[0],
                cmd.pos[1],
                cmd.size[0],
                cmd.size[1],
                1.0,
            )
        }
        4 => sd_box(local_x, local_y, hw, hh), // Text (placeholder box)
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
        11 => {
            // Per-corner slab: param bank holds [tl, tr, br, bl].
            let idx = cmd.params[3].to_bits() as usize;
            let radii = param_bank.get(idx).copied().unwrap_or([cmd.radius(); 4]);
            sd_rounded_box_per_corner(local_x, local_y, hw, hh, radii)
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
                if slot < int_bank.len() {
                    int_bank[slot] as f32
                } else {
                    0.0
                }
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
    fn sd_rounded_box_per_corner_squares_and_rounds() {
        // Square on the leading (left) corners, rounded on the trailing (right)
        // corners — the M3 modal drawer shape. radii = [tl, tr, br, bl].
        let radii = [0.0, 8.0, 8.0, 0.0];
        // Top-left corner (square) contains its near-corner point…
        assert!(sd_rounded_box_per_corner(-9.0, -9.0, 10.0, 10.0, radii) < 0.0);
        assert!(sd_rounded_box_per_corner(-9.0, 9.0, 10.0, 10.0, radii) < 0.0);
        // …while the top-right/bottom-right corners are cut away by the radius.
        assert!(sd_rounded_box_per_corner(9.0, -9.0, 10.0, 10.0, radii) > 0.0);
        assert!(sd_rounded_box_per_corner(9.0, 9.0, 10.0, 10.0, radii) > 0.0);
    }

    #[test]
    fn sdf_eval_per_corner_slab_reads_param_bank() {
        // 20×20 box centered at (10,10); square left, rounded right.
        let cmd = SdfDrawCmd {
            pos: [0.0, 0.0],
            size: [20.0, 20.0],
            color: [1.0; 4],
            params: [DRAW_TYPE_SLAB_PC, 0.0, 0.0, f32::from_bits(0)],
            xform: [0.0; 4],
            clip: SdfDrawCmd::NO_CLIP,
        };
        let bank = [[0.0f32, 8.0, 8.0, 0.0]]; // [tl, tr, br, bl]
        // Near the top-left corner (square) → inside.
        assert!(sdf_eval_with_params(&cmd, 1.0, 1.0, &bank).0 < 0.0);
        // Near the top-right corner (rounded) → outside.
        assert!(sdf_eval_with_params(&cmd, 19.0, 1.0, &bank).0 > 0.0);
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
            xform: [0.0; 4],
            clip: SdfDrawCmd::NO_CLIP,
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
            xform: [0.0; 4],
            clip: SdfDrawCmd::NO_CLIP,
        };
        let params = [[30.0f32, 0.0, 70.0, 0.0]]; // C1, C2 pull the curve to y≈25..75
        let mid = cubic_point([0.0, 100.0], [30.0, 0.0], [70.0, 0.0], [100.0, 100.0], 0.5);
        let (d_on, _) = sdf_eval_with_params(&cmd, mid[0], mid[1], &params);
        assert!(d_on < 0.0, "curve midpoint should be inside stroke: {d_on}");
        // Chord midpoint is far from the bowed curve.
        let (d_chord, _) = sdf_eval_with_params(&cmd, 50.0, 100.0, &params);
        assert!(
            d_chord > 10.0,
            "chord midpoint should be outside: {d_chord}"
        );
    }

    // ── SdfRotate (xform) — mirrors the `sdf_render.wgsl` xform block ──────

    #[test]
    fn sdf_eval_undoes_a_quarter_turn_so_a_rotated_box_still_hits_at_its_drawn_spot() {
        // A 40x10 box at (0,0), rotated 90 degrees about its own center
        // (20,5) — on screen this reads as a 10x40 box. Evaluating at a
        // point that lies on the ROTATED shape (but well outside the
        // unrotated pos/size box) must land inside once xform is undone.
        let cmd = SdfDrawCmd {
            pos: [0.0, 0.0],
            size: [40.0, 10.0],
            color: [1.0; 4],
            params: [DRAW_TYPE_BOX, 0.0, 0.0, 0.0],
            xform: [1.0, 0.0, 0.0, 0.0], // transform bank id 1 -> param_bank[0..2]
            clip: SdfDrawCmd::NO_CLIP,
        };
        // Quarter(1) about pivot (20,5): a=0,b=-1,c=1,d=0, tx=25,ty=-15
        // (see drawlist::tests for the derivation of this exact matrix).
        let bank = [[0.0f32, -1.0, 1.0, 0.0], [25.0, -15.0, 0.0, 0.0]];
        // (20, -10) is inside the drawn (rotated) 10x40 box (it spans
        // x:[15,25], y:[-15,25]) but well outside the original 40x10
        // pos/size box (y:[0,10]) entirely.
        let (d_on, _) = sdf_eval_with_params(&cmd, 20.0, -10.0, &bank);
        assert!(
            d_on < 0.0,
            "point on the rotated box should be inside: {d_on}"
        );
        // (35, 5) is inside the UN-rotated footprint (x:[0,40], y:[0,10])
        // but outside the rotated one (x:[15,25]) — must now read outside.
        let (d_off, _) = sdf_eval_with_params(&cmd, 35.0, 5.0, &bank);
        assert!(
            d_off > 0.0,
            "point outside the rotated box should be outside: {d_off}"
        );
    }

    #[test]
    fn xform_zero_is_a_no_op_even_with_a_nonempty_bank() {
        // The overwhelmingly common case (no rotation): xform[0] == 0 must
        // never consult param_bank at all, so a caller can share the same
        // bank across rotated and unrotated commands safely.
        let cmd = SdfDrawCmd {
            pos: [10.0, 10.0],
            size: [20.0, 20.0],
            color: [1.0; 4],
            params: [DRAW_TYPE_BOX, 0.0, 0.0, 0.0],
            xform: [0.0; 4],
            clip: SdfDrawCmd::NO_CLIP,
        };
        let bank = [[9.0f32, 9.0, 9.0, 9.0], [9.0, 9.0, 9.0, 9.0]];
        let (d, _) = sdf_eval_with_params(&cmd, 20.0, 20.0, &bank);
        assert!(d < 0.0);
    }
}
