//! `DrawList` — the public render contract consumed by highbay_ui.
//!
//! A `DrawList` is a retained list of SDF/MSDF instances with per-instance
//! style. It lowers to an [`SdfFrame`] (packed `SdfDrawCmd`s + char buffer +
//! aux param bank) which the renderer draws in ONE full-screen-triangle
//! pass — the "single instanced draw path". The caller owns the wgpu
//! device/queue throughout (`GpuSdfRenderer` never creates them).
//!
//! Generalized from matter-stream's mtd1 lowering (`mtd1_to_sdf.rs`, same
//! author) away from the mtd1 container toward the Phase-7 node-graph:
//! highbay_ui's layout pass emits a `DrawList` per frame.

pub mod stream;

use crate::core::sdf::{
    BEZIER_SHADOW_BIT, ClipRect, DRAW_TYPE_BEZIER, DRAW_TYPE_BOX, DRAW_TYPE_CIRCLE,
    DRAW_TYPE_LINE, DRAW_TYPE_MSDF_TEXT, DRAW_TYPE_OUTLINE, DRAW_TYPE_SLAB, DRAW_TYPE_SLAB_PC,
    SdfDrawCmd, XFORM_FLAT,
};
use crate::font::atlas::FontAtlas;
use crate::font::shaper::ShapedRun;

/// Horizontal ink margin of an atlas cell as a fraction of the line box
/// (matches the 15% margin baked by `FontAtlasBuilder`).
pub const X_MARGIN_FRAC: f32 = 0.15;

/// Line box height as a multiple of font size (matches the 1.3× em-square
/// safety margin baked into atlas cells).
pub const LINE_BOX_RATIO: f32 = 1.3;

/// The narrowest alpha ramp the MSDF shader will use, in screen pixels.
/// Below one pixel the ramp stops being antialiasing and starts being a
/// threshold — see [`crate::FontAtlas::min_antialiased_font_size`].
pub const MIN_SCREEN_PX_RANGE: f32 = 1.0;

/// The screen-space distance range of an MSDF text run — Chlumsky's
/// `screenPxRange`, and the single source of truth for the number
/// `sdf_render.wgsl` (case `8u`) recomputes per fragment.
///
/// The atlas field spans `atlas_px_range` **texels** of a `cell_px` glyph
/// cell; that cell is drawn scaled to the run's line box (`font_size ×`
/// [`LINE_BOX_RATIO`]), so on screen the same field spans
/// `atlas_px_range × line_box_h / cell_px` **pixels**. Feeding the atlas-space
/// range into the alpha ramp instead over-sharpens it by the minification
/// factor and drops sub-pixel-thin glyph features; clamping at
/// [`MIN_SCREEN_PX_RANGE`] keeps the ramp usable at extreme minification.
pub fn screen_px_range(atlas_px_range: f32, cell_px: f32, font_size: f32) -> f32 {
    if cell_px <= 0.0 {
        return MIN_SCREEN_PX_RANGE;
    }
    (atlas_px_range * font_size * LINE_BOX_RATIO / cell_px).max(MIN_SCREEN_PX_RANGE)
}

/// The SDF shape an instance renders. Geometry parameters that aren't the
/// bounding box live here; color/animation are per-instance fields.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SdfKind {
    /// Filled box; `position`/`size` are the rect.
    Box,
    /// Filled rounded box with corner `radius`.
    RoundedBox { radius: f32 },
    /// Filled rounded box with independent per-corner radii, `[top-left,
    /// top-right, bottom-right, bottom-left]` in screen space (+x right, +y
    /// down). The M3 modal navigation drawer uses this: square against the
    /// screen edge, rounded on the exposed trailing side. Lowers through the
    /// aux param bank like [`SdfKind::BezierStroke`].
    RoundedBoxPerCorner { radii: [f32; 4] },
    /// Filled circle inscribed in `size` (radius = min(w,h)/2).
    Circle,
    /// Horizontal line across the rect: length = size.x, thickness = size.y.
    Line,
    /// Rounded-rect stroke (no fill). NOTE: `thickness` occupies the anim
    /// param slot in the wire format — outlines don't animate.
    Outline { radius: f32, thickness: f32 },
    /// Cubic Bézier stroke from `position` to `end` (absolute coords) with
    /// control points `c1`, `c2` — nav-graph containment/flow arcs.
    /// `shadow` opts this instance into the same drop shadow Box/RoundedBox/
    /// Circle/RoundedBoxPerCorner cast (see [`DrawList::push_bezier_shadowed`]);
    /// plain [`DrawList::push_bezier`] leaves it off, matching prior behavior.
    BezierStroke {
        c1: [f32; 2],
        c2: [f32; 2],
        end: [f32; 2],
        thickness: f32,
        shadow: bool,
    },
    /// A shaped MSDF text run referencing `char_count` packed entries at
    /// `char_start` in the list's char buffer. Produced by
    /// [`DrawList::push_shaped_text`].
    MsdfText {
        char_start: u32,
        char_count: u32,
        px_range: f32,
    },
}

/// **Whether an instance casts the renderer's drop shadow** — the paint
/// layer's whole vocabulary for elevation, and the answer to what used to be
/// an unconditional rule.
///
/// Every filled shape (Box / RoundedBox / RoundedBoxPerCorner / Circle) casts
/// a soft offset shadow, and until this existed nothing could opt out. Two
/// things followed, both load-bearing: paint order between *non-overlapping*
/// neighbours a few pixels apart was visible, and **a surface spanning two
/// abutting bands could not be split into one node per band**, because each
/// band would shadow the one below and put a dark seam between them.
///
/// **What this type is NOT.** It is not an M3 elevation scale. This layer owns
/// one shadow and knows whether to cast it; *which* elevation level a surface
/// is at, and therefore whether it should, is a design-system question that
/// belongs above the renderer — see `libteststand`'s `elevation` node prop,
/// which maps a declared M3 level onto this. Growing a per-level shadow spec
/// (offset/reach/alpha as a function of dp) is the honest next step and is
/// deliberately not taken here: it would change every existing frame, and it
/// is not what unblocked the split.
///
/// [`Elevation::Default`] is what an instance pushed through
/// [`DrawList::push`] gets, so nothing that does not ask changes.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Elevation {
    /// Cast the shadow — the behaviour every filled shape has always had, and
    /// what [`DrawList::push`] tags an instance with.
    #[default]
    Default,
    /// **Flat**: the shape sits directly on whatever is behind it and casts
    /// nothing. M3 elevation level 0 — a `surface` app bar at rest, a status
    /// strip, any band abutting another band of the same surface.
    Flat,
}

/// Visual effects inherited by every instance emitted in a node scope.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct DrawEffects {
    /// Gaussian edge-softening radius in logical pixels.
    pub blur_radius: f32,
    /// Bottom-fade strength for an alpha ombré, from 0 (none) to 1 (full).
    pub alpha_ombre: f32,
}

impl Elevation {
    /// This elevation as the wire format's `xform[1]` (see
    /// [`crate::core::sdf::XFORM_FLAT`]).
    fn to_wire(self) -> f32 {
        match self {
            Elevation::Default => 0.0,
            Elevation::Flat => XFORM_FLAT,
        }
    }
}

/// One instance in the draw list.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SdfInstance {
    pub kind: SdfKind,
    /// Top-left position in logical pixels (Bézier: start point; text: the
    /// line box origin as computed by `push_shaped_text`).
    pub position: [f32; 2],
    pub size: [f32; 2],
    /// Straight RGBA, 0.0–1.0.
    pub color: [f32; 4],
    /// Anim bank index (0 = none, 1+ = AnimBank[idx-1]).
    pub anim: u32,
}

/// A rotation to apply to every instance pushed while [`DrawList`]'s
/// transform stack is non-empty (see [`DrawList::push_rotate`]).
///
/// Tim, 2026-07-28 (relayed from the coordinator): **do not run trig for
/// right angles.** `cos(PI/2)` in `f32` is not exactly `0.0` — it is about
/// `-4.37e-8` — so building a rotation matrix from `sin`/`cos` at a
/// multiple of 90 degrees lands the geometry a fraction of a pixel off the
/// pixel grid: text resamples soft instead of crisp, edges that should be
/// exactly vertical/horizontal pick up a sub-pixel slope, and the AABB
/// [`rotate_rect`] derives for hit-testing no longer matches the drawn rect
/// exactly. [`SdfRotate::Quarter`] sidesteps this entirely: its matrix
/// entries are chosen by an integer `match`, never by `sin`/`cos`, so they
/// are exactly `0.0`/`1.0`/`-1.0` — a rotated axis-aligned rect is still an
/// EXACT axis-aligned rect. Taking the turn count as its own variant
/// (rather than snapping a radians value with an epsilon check) makes an
/// exact right angle the only thing `Quarter` can express — there is no
/// "nearly 90 degrees" value to accidentally construct.
///
/// [`SdfRotate::Radians`] is the escape hatch for genuinely arbitrary
/// angles; it is never exact and callers needing a right angle should use
/// `Quarter` instead. Today's only caller — the Add-form tabs — are exactly
/// 90 degrees, so they use `Quarter`; `Radians` exists to be correct, not to
/// be fast, and has no caller yet.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SdfRotate {
    /// An exact multiple of 90 degrees, `(dx, dy) -> (-dy, dx)` per turn
    /// (screen space, +y down — this reads as clockwise on screen).
    /// Normalized mod 4, so `Quarter(-1) == Quarter(3)`.
    Quarter(i32),
    /// An arbitrary angle in radians. General trig matrix — not exact even
    /// when the value happens to be a right angle; use `Quarter` for those.
    Radians(f32),
}

impl SdfRotate {
    /// The forward 2x2 rotation matrix as `[m00, m01, m10, m11]`
    /// (row-major: `x' = m00*x + m01*y`, `y' = m10*x + m11*y`). Exact
    /// (0.0/1.0/-1.0 only, no float error) for [`SdfRotate::Quarter`].
    fn matrix2(self) -> [f32; 4] {
        match self {
            SdfRotate::Quarter(q) => match q.rem_euclid(4) {
                0 => [1.0, 0.0, 0.0, 1.0],
                1 => [0.0, -1.0, 1.0, 0.0],
                2 => [-1.0, 0.0, 0.0, -1.0],
                3 => [0.0, 1.0, -1.0, 0.0],
                _ => unreachable!("rem_euclid(4) is always 0..4"),
            },
            SdfRotate::Radians(theta) => {
                let (s, c) = theta.sin_cos();
                [c, -s, s, c]
            }
        }
    }
}

/// A composed forward affine transform: `x' = a*x + b*y + tx`, `y' = c*x +
/// d*y + ty`. Always a rotation (the 2x2 [a,b,c,d] part is orthogonal) about
/// some absolute pivot baked into `tx`/`ty` — never a general affine (no
/// scale/shear), which is what lets both the shader and [`rotate_rect`]
/// invert it by transposing the 2x2 part instead of a general matrix
/// inverse.
#[derive(Copy, Clone, Debug, PartialEq)]
struct RotationTransform {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    tx: f32,
    ty: f32,
}

impl RotationTransform {
    const IDENTITY: Self = Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 };

    /// The transform for `rotation` about the absolute pivot `pivot`:
    /// `forward(v) = M * (v - pivot) + pivot`, expanded to affine form.
    /// Exact when `rotation` is [`SdfRotate::Quarter`] (`m00..m11` are
    /// exactly 0/1/-1, so every product/sum here is an exact float op).
    fn about(rotation: SdfRotate, pivot: [f32; 2]) -> Self {
        let [m00, m01, m10, m11] = rotation.matrix2();
        let (px, py) = (pivot[0], pivot[1]);
        Self {
            a: m00,
            b: m01,
            c: m10,
            d: m11,
            tx: px - m00 * px - m01 * py,
            ty: py - m10 * px - m11 * py,
        }
    }

    /// Compose so the result applies `inner` FIRST, then `outer` —
    /// `glPushMatrix`/`glRotate` semantics (`M' = M * R`; a vertex
    /// transforms via `M * (R * v)`). Nesting `push_rotate` while one is
    /// already active passes the previous top as `outer` and the
    /// newly-requested rotation as `inner`. Exact when both inputs are
    /// exact (composing two `Quarter` transforms stays exact: products and
    /// sums of `{-1,0,1}` introduce no rounding).
    fn compose(outer: &Self, inner: &Self) -> Self {
        Self {
            a: outer.a * inner.a + outer.b * inner.c,
            b: outer.a * inner.b + outer.b * inner.d,
            c: outer.c * inner.a + outer.d * inner.c,
            d: outer.c * inner.b + outer.d * inner.d,
            tx: outer.a * inner.tx + outer.b * inner.ty + outer.tx,
            ty: outer.c * inner.tx + outer.d * inner.ty + outer.ty,
        }
    }

    /// Apply the forward transform to a point.
    fn apply(&self, v: [f32; 2]) -> [f32; 2] {
        [self.a * v[0] + self.b * v[1] + self.tx, self.c * v[0] + self.d * v[1] + self.ty]
    }
}

/// Rotate the axis-aligned rect `(pos, size)` by `rotation` about the
/// absolute `pivot`, returning the axis-aligned bounding box of the result
/// as `(pos, size)`. **Hit-testing must call this** rather than
/// re-deriving the matrix independently, so a click is tested against
/// exactly what got drawn ([`DrawList::push_rotate`]'s doc has the full
/// rationale).
///
/// For [`SdfRotate::Quarter`] this AABB is not an approximation: rotating
/// an axis-aligned rect by an exact multiple of 90 degrees yields another
/// axis-aligned rect (only w/h can swap), so the bounding box of its four
/// rotated corners **is** the rotated rect, bit-for-bit. For
/// [`SdfRotate::Radians`] this is the usual looser AABB bound.
pub fn rotate_rect(pos: [f32; 2], size: [f32; 2], rotation: SdfRotate, pivot: [f32; 2]) -> ([f32; 2], [f32; 2]) {
    let t = RotationTransform::about(rotation, pivot);
    let corners = [
        [pos[0], pos[1]],
        [pos[0] + size[0], pos[1]],
        [pos[0], pos[1] + size[1]],
        [pos[0] + size[0], pos[1] + size[1]],
    ];
    let mut min = [f32::MAX, f32::MAX];
    let mut max = [f32::MIN, f32::MIN];
    for c in corners {
        let p = t.apply(c);
        min[0] = min[0].min(p[0]);
        min[1] = min[1].min(p[1]);
        max[0] = max[0].max(p[0]);
        max[1] = max[1].max(p[1]);
    }
    (min, [max[0] - min[0], max[1] - min[1]])
}

/// GPU-ready lowered frame: what [`DrawList::lower`] produces and
/// `GpuSdfRenderer` consumes.
#[derive(Clone, Debug, Default)]
pub struct SdfFrame {
    pub draws: Vec<SdfDrawCmd>,
    pub char_buffer: Vec<u32>,
    pub param_bank: Vec<[f32; 4]>,
}

/// Retained draw list: instances + packed text chars + aux params.
#[derive(Clone, Debug, Default)]
pub struct DrawList {
    pub instances: Vec<SdfInstance>,
    chars: Vec<u32>,
    /// The `SdfRotate` transform bank, in push order: an entry's 1-based
    /// index is what `instance_transforms` records for the instances pushed
    /// under it. Baked into the lowered [`SdfFrame`]'s `param_bank` (two
    /// consecutive vec4 slots per entry — see [`DrawList::lower`]).
    transforms: Vec<RotationTransform>,
    /// The active (possibly nested) rotation stack, holding 1-based
    /// `transforms` ids: [`DrawList::push_rotate`] composes a new entry onto
    /// `active_transform.last()` (or identity) and pushes its id here;
    /// [`DrawList::push_rotate_end`] pops it. `push_clip`/`push_clip_end`,
    /// `begin_effects`/`end_effects` and this are now the same shape: a CPU
    /// stack resolved at push time, snapshotted onto the instance. Nothing
    /// inherited defers to the consumer any more.
    active_transform: Vec<u32>,
    /// Parallel to `instances` (same length, same index): the 1-based
    /// `transforms` id each instance was tagged with at push time (`0` =
    /// none). Kept OUT of [`SdfInstance`] itself — deliberately — rather
    /// than as a field on it: dozens of call sites across `highbay_ui`
    /// construct `SdfInstance` literals directly (then hand them to
    /// [`DrawList::push`], the one and only place instances enter the
    /// list), and a struct-literal field every one of those has to name
    /// (even as an inert `0`) is exactly the kind of churn a side channel
    /// avoids: `push` is already the sole intercept point, so it can tag
    /// here without the instance's own shape ever needing to change.
    instance_transforms: Vec<u32>,
    /// Parallel to `instances` in exactly the way `instance_transforms` is,
    /// and a side channel for exactly the same reason (see its doc): the
    /// [`Elevation`] each instance was pushed at. [`DrawList::push`] tags
    /// [`Elevation::Default`]; [`DrawList::push_fill`] is the door for
    /// anything else.
    instance_elevation: Vec<Elevation>,
    /// Stack of node-scoped visual effects. Every pushed instance snapshots
    /// the current top, so descendants inherit without special push methods.
    active_effects: Vec<DrawEffects>,
    /// Effects snapshot parallel to `instances`.
    instance_effects: Vec<DrawEffects>,
    /// Stack of scissor regions, each entry ALREADY intersected with the one
    /// below it (see [`DrawList::push_clip`]) — so the top is always the
    /// answer, with no walk. Resolving here rather than in the shader is what
    /// lets clips nest at all: the GPU used to hold one "current region"
    /// register, which made an inner clip's *end* clear the enclosing bound
    /// instead of returning to it.
    active_clips: Vec<ClipRect>,
    /// Clip snapshot parallel to `instances`, exactly as `instance_effects` is
    /// (and a side channel for the same reason — see `instance_transforms`).
    /// [`ClipRect::UNBOUNDED`] when nothing was clipping.
    instance_clips: Vec<ClipRect>,
}

impl DrawList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.instances.clear();
        self.chars.clear();
        self.transforms.clear();
        self.active_transform.clear();
        self.instance_transforms.clear();
        self.instance_elevation.clear();
        self.active_effects.clear();
        self.instance_effects.clear();
        self.active_clips.clear();
        self.instance_clips.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// The packed MSDF char entries backing `MsdfText` instances.
    pub fn chars(&self) -> &[u32] {
        &self.chars
    }

    /// The 1-based id of the currently active [`DrawList::push_rotate`]
    /// scope (`0` = none) — what every instance pushed right now will be
    /// tagged with.
    fn active_transform(&self) -> u32 {
        self.active_transform.last().copied().unwrap_or(0)
    }

    /// Push a shape instance, tagging it (in the side-channel
    /// `instance_transforms`, not on `instance` itself) with whatever
    /// [`DrawList::push_rotate`] scope is currently active (`0` if none) —
    /// the single point every other push method routes through, so no
    /// caller has to know about the rotate stack.
    pub fn push(&mut self, instance: SdfInstance) {
        self.instance_transforms.push(self.active_transform());
        self.instance_elevation.push(Elevation::Default);
        self.instance_effects.push(self.active_effects.last().copied().unwrap_or_default());
        self.instance_clips.push(self.active_clip());
        self.instances.push(instance);
    }

    /// The scissor in force right now — what an instance pushed at this moment
    /// will be bounded by. [`ClipRect::UNBOUNDED`] when no scope is open.
    pub fn active_clip(&self) -> ClipRect {
        self.active_clips.last().copied().unwrap_or(ClipRect::UNBOUNDED)
    }

    /// The scissor instance `index` was pushed under, or `None` if it was
    /// unbounded (or the index is past the end).
    ///
    /// This is the successor to reading `SdfKind::ClipBegin`/`ClipEnd` markers
    /// out of the stream and replaying them: an instance's bound is a property
    /// of the instance, so asking it is a lookup rather than a scan, and it
    /// cannot disagree with what the GPU will do.
    pub fn instance_clip(&self, index: usize) -> Option<ClipRect> {
        self.instance_clips
            .get(index)
            .copied()
            .filter(|clip| !clip.is_unbounded())
    }

    /// Every instance's resolved clip, parallel to [`DrawList::instances`].
    pub fn instance_clips(&self) -> &[ClipRect] {
        &self.instance_clips
    }

    /// The DISTINCT scissor regions this list's ink was drawn under, in order
    /// of first appearance; unbounded ink contributes nothing.
    ///
    /// The answer to "how many regions does this component scissor, and where",
    /// which used to be read by counting `ClipBegin` markers in the stream. It
    /// is not the same number: a region re-entered after an inner clip closes
    /// counted twice as markers and counts once here, which is what was being
    /// asked all along.
    pub fn clip_regions(&self) -> Vec<ClipRect> {
        let mut out: Vec<ClipRect> = Vec::new();
        for &clip in &self.instance_clips {
            if !clip.is_unbounded() && !out.contains(&clip) {
                out.push(clip);
            }
        }
        out
    }

    /// Begin a node-scoped effects scope. Descendant instances inherit it.
    pub fn begin_effects(&mut self, effects: DrawEffects) {
        self.active_effects.push(effects);
    }

    /// End the innermost node-scoped effects scope.
    pub fn end_effects(&mut self) {
        self.active_effects.pop();
    }

    /// [`DrawList::push`], at an explicit [`Elevation`] — the one way to say
    /// that a filled shape casts **no** drop shadow.
    ///
    /// Named for the *fill* because that is the whole of what it affects: the
    /// four filled shape kinds are the only unconditional casters, an
    /// `Outline` and an MSDF run never cast one, and a Bézier stroke has its
    /// own opt-IN ([`DrawList::push_bezier_shadowed`]). Passing a non-filled
    /// kind here is harmless and inert rather than an error — the shader
    /// simply has no shadow to suppress.
    ///
    /// It routes through [`DrawList::push`] rather than beside it, so that
    /// method stays the single point at which an instance enters the list —
    /// which is what `libteststand`'s drawing-allowlist scan is written
    /// against. The name is likewise deliberate: that scan's `fill(` needle
    /// already matches `push_fill(`, so a module that reached for this
    /// instead of `push` to slip past the check would trip it anyway.
    pub fn push_fill(&mut self, instance: SdfInstance, elevation: Elevation) {
        self.push(instance);
        if let Some(last) = self.instance_elevation.last_mut() {
            *last = elevation;
        }
    }

    /// Begin a rotation scope: every instance pushed after this — until the
    /// matching [`DrawList::push_rotate_end`] — is rotated by `rotation`
    /// about the absolute `pivot`. Nestable, exactly like
    /// `glPushMatrix`/`glRotate`/`glPopMatrix`: a rotation pushed while
    /// another is already active composes ONTO it (applied to the instance
    /// first, with the enclosing rotation applied after), never replaces
    /// it. Mirrors [`DrawList::push_clip`]'s API shape.
    pub fn push_rotate(&mut self, rotation: SdfRotate, pivot: [f32; 2]) {
        let outer = self
            .active_transform
            .last()
            .map(|&id| self.transforms[(id - 1) as usize])
            .unwrap_or(RotationTransform::IDENTITY);
        let inner = RotationTransform::about(rotation, pivot);
        self.transforms.push(RotationTransform::compose(&outer, &inner));
        self.active_transform.push(self.transforms.len() as u32);
    }

    /// End the current [`DrawList::push_rotate`] scope, reverting to
    /// whatever scope (if any) enclosed it.
    pub fn push_rotate_end(&mut self) {
        self.active_transform.pop();
    }

    /// Convenience: push a cubic Bézier stroke from `p0` to `p3`.
    pub fn push_bezier(
        &mut self,
        p0: [f32; 2],
        c1: [f32; 2],
        c2: [f32; 2],
        p3: [f32; 2],
        thickness: f32,
        color: [f32; 4],
    ) {
        self.push(SdfInstance {
            kind: SdfKind::BezierStroke { c1, c2, end: p3, thickness, shadow: false },
            position: p0,
            size: [0.0, 0.0],
            color,
            anim: 0,
        });
    }

    /// Same as [`DrawList::push_bezier`], but the stroke casts the same
    /// offset drop shadow as filled shapes (Box/RoundedBox/Circle). For
    /// strokes that stand in for a shadowed shape's outline — e.g. the
    /// rounded-corner arcs of a dashed border whose straight runs are drawn
    /// as shadowed boxes — so the shadow reads continuously across both.
    pub fn push_bezier_shadowed(
        &mut self,
        p0: [f32; 2],
        c1: [f32; 2],
        c2: [f32; 2],
        p3: [f32; 2],
        thickness: f32,
        color: [f32; 4],
    ) {
        self.push(SdfInstance {
            kind: SdfKind::BezierStroke { c1, c2, end: p3, thickness, shadow: true },
            position: p0,
            size: [0.0, 0.0],
            color,
            anim: 0,
        });
    }

    /// Begin a rectangular scissor clip at `pos`/`size`. Instances pushed
    /// after this (until the matching [`DrawList::push_clip_end`]) are
    /// clipped to the rect — text included — so a caller can confine a
    /// sub-scene, e.g. a device "screen" or a scrolling viewport.
    ///
    /// **Nestable.** A clip pushed while another is open is the INTERSECTION
    /// with it, and ending it returns to the enclosing region rather than
    /// clearing anything. That used not to be true: the scissor was a pair of
    /// control commands driving one register in the shader, so an inner clip
    /// replaced the outer one and its end handed the outer bound away to
    /// everything drawn afterwards (fixed once at the caller in 2fcb8d0, which
    /// is the bug this encoding makes unrepresentable). Callers therefore do
    /// NOT need to intersect by hand, and do not need to restate an enclosing
    /// rect on the way out.
    ///
    /// Emits no instance: the resolved rect is snapshotted onto each instance
    /// pushed under it ([`DrawList::instance_clip`]).
    pub fn push_clip(&mut self, pos: [f32; 2], size: [f32; 2]) {
        let rect = ClipRect::from_pos_size(pos, size);
        let resolved = match self.active_clips.last() {
            Some(&outer) => outer.intersect(rect),
            None => rect,
        };
        self.active_clips.push(resolved);
    }

    /// End the innermost [`DrawList::push_clip`] region, reverting to whatever
    /// region (if any) encloses it.
    pub fn push_clip_end(&mut self) {
        self.active_clips.pop();
    }

    /// Push a shaped text run as an MSDF text instance.
    ///
    /// `pos` is the pen origin: x = first glyph origin, y = top of the line
    /// box (`font_size * LINE_BOX_RATIO` tall). Glyph advances come from
    /// the shaper (kerning included), encoded as deltas against the atlas's
    /// standard advances exactly like the extracted mtd1 lowering. Glyphs
    /// missing from the atlas fall back to table index 0.
    ///
    /// Keep `font_size` at or above the atlas's
    /// [`FontAtlas::min_antialiased_font_size`] for `px_range`: that is where
    /// the baked field still covers a full pixel of alpha ramp. Below it
    /// antialiasing degrades smoothly (a continuously zooming view is expected
    /// to pass through it — the ZUI's nav graph and hi-fi phone both scale
    /// their type), so this is guidance, not a hard error. What IS asserted, in
    /// debug builds, is the point where the field has less than HALF a pixel of
    /// range left and antialiasing is simply gone: a fixed style down there is
    /// a bug, not a zoom level.
    ///
    /// Returns the width of the run in pixels.
    pub fn push_shaped_text(
        &mut self,
        run: &ShapedRun,
        atlas: &FontAtlas,
        pos: [f32; 2],
        font_size: f32,
        px_range: f32,
        color: [f32; 4],
    ) -> f32 {
        debug_assert!(
            font_size + 1e-3 >= 0.5 * atlas.min_antialiased_font_size(px_range),
            "font size {font_size} leaves this atlas under half a pixel of \
             distance range (antialiasing floor {:.2}px for {}px cells at \
             px_range {px_range}) — glyph hairlines are gone at that size; \
             bake a finer atlas (smaller cells or a wider px_range) instead",
            atlas.min_antialiased_font_size(px_range),
            atlas.cell_px().unwrap_or(0.0),
        );
        let char_start = self.chars.len() as u32;
        let scale = font_size / run.units_per_em as f32;
        let mut total_advance = 0.0f32;
        let mut count = 0u32;

        for g in &run.glyphs {
            let advance_px = g.x_advance as f32 * scale;
            let (table_idx, std_advance_norm) = match atlas.glyph_table_index(g.glyph_id) {
                Some(idx) => (idx as u32, atlas.glyphs[idx].advance_x),
                None => (0, 0.5),
            };
            let std_advance_px = std_advance_norm * font_size;
            let delta_px = advance_px - std_advance_px;
            // Fixed-point 1/16 px, biased by 2048 (±128 px), like upstream.
            let delta_fixed = ((delta_px * 16.0) as i32 + 2048).clamp(0, 0xFFFF) as u32;
            self.chars.push((table_idx << 16) | delta_fixed);
            total_advance += advance_px;
            count += 1;
        }

        let line_box_h = font_size * LINE_BOX_RATIO;
        let left_margin = X_MARGIN_FRAC * line_box_h;
        let box_w = total_advance + 2.0 * left_margin;

        self.push(SdfInstance {
            kind: SdfKind::MsdfText { char_start, char_count: count, px_range },
            // The shader recovers the pen origin via pos.x + margin.
            position: [pos[0] - left_margin, pos[1]],
            size: [box_w, line_box_h],
            color,
            anim: 0,
        });

        total_advance
    }

    /// Lower to the GPU wire format: one `SdfDrawCmd` per instance, plus
    /// the packed char buffer and the aux param bank (Bézier controls, and
    /// the `SdfRotate` transform bank — see below).
    pub fn lower(&self) -> SdfFrame {
        debug_assert!(
            self.active_transform.is_empty(),
            "DrawList::lower called with {} unclosed push_rotate scope(s) — every \
             push_rotate needs a matching push_rotate_end, or the leaked transform \
             silently keeps applying to every instance drawn afterward this frame",
            self.active_transform.len(),
        );
        debug_assert!(
            self.active_clips.is_empty(),
            "DrawList::lower called with {} unclosed push_clip scope(s) — every \
             push_clip needs a matching push_clip_end, or the leaked scissor \
             silently bounds every instance drawn afterward this frame",
            self.active_clips.len(),
        );

        let mut draws = Vec::with_capacity(self.instances.len());
        let mut param_bank: Vec<[f32; 4]> = Vec::new();

        // Bake the SdfRotate transform bank FIRST, at fixed slots [0..2N),
        // so an instance's 1-based `transform` id maps directly to
        // `param_bank[(id-1)*2]`/`[(id-1)*2+1]` with no extra bookkeeping —
        // every per-instance param_bank push below (Bézier controls,
        // per-corner radii) is appended after and never disturbs this
        // range.
        for t in &self.transforms {
            param_bank.push([t.a, t.b, t.c, t.d]);
            param_bank.push([t.tx, t.ty, 0.0, 0.0]);
        }

        debug_assert_eq!(
            self.instances.len(),
            self.instance_transforms.len(),
            "instances and instance_transforms are always pushed together in DrawList::push"
        );
        debug_assert_eq!(
            self.instances.len(),
            self.instance_elevation.len(),
            "instances and instance_elevation are always pushed together in DrawList::push"
        );
        debug_assert_eq!(
            self.instances.len(),
            self.instance_effects.len(),
            "instances and instance_effects are always pushed together in DrawList::push"
        );
        debug_assert_eq!(
            self.instances.len(),
            self.instance_clips.len(),
            "instances and instance_clips are always pushed together in DrawList::push"
        );

        for ((((inst, &transform), &elevation), &effects), &clip) in self
            .instances
            .iter()
            .zip(&self.instance_transforms)
            .zip(&self.instance_elevation)
            .zip(&self.instance_effects)
            .zip(&self.instance_clips)
        {
            let anim = inst.anim as f32;
            let (pos, size, params) = match inst.kind {
                SdfKind::Box => (inst.position, inst.size, [DRAW_TYPE_BOX, 0.0, anim, 0.0]),
                SdfKind::RoundedBox { radius } => {
                    (inst.position, inst.size, [DRAW_TYPE_SLAB, radius, anim, 0.0])
                }
                SdfKind::RoundedBoxPerCorner { radii } => {
                    let idx = param_bank.len() as u32;
                    param_bank.push(radii);
                    (
                        inst.position,
                        inst.size,
                        [DRAW_TYPE_SLAB_PC, 0.0, anim, f32::from_bits(idx)],
                    )
                }
                SdfKind::Circle => {
                    let radius = inst.size[0].min(inst.size[1]) * 0.5;
                    (inst.position, inst.size, [DRAW_TYPE_CIRCLE, radius, anim, 0.0])
                }
                SdfKind::Line => (inst.position, inst.size, [DRAW_TYPE_LINE, 0.0, anim, 0.0]),
                SdfKind::Outline { radius, thickness } => (
                    inst.position,
                    inst.size,
                    // params.z carries thickness in the wire format (shader
                    // quirk inherited from upstream) — anim unsupported.
                    [DRAW_TYPE_OUTLINE, radius, thickness, 0.0],
                ),
                SdfKind::BezierStroke { c1, c2, end, thickness, shadow } => {
                    let idx = param_bank.len() as u32;
                    debug_assert!(idx & BEZIER_SHADOW_BIT == 0, "param_bank overflowed the Bézier shadow flag bit");
                    param_bank.push([c1[0], c1[1], c2[0], c2[1]]);
                    let slot = if shadow { idx | BEZIER_SHADOW_BIT } else { idx };
                    (
                        inst.position,
                        end,
                        [DRAW_TYPE_BEZIER, thickness, anim, f32::from_bits(slot)],
                    )
                }
                SdfKind::MsdfText { char_start, char_count, px_range } => {
                    let packed = (char_start << 16) | (char_count & 0xFFFF);
                    (
                        inst.position,
                        inst.size,
                        [DRAW_TYPE_MSDF_TEXT, px_range, X_MARGIN_FRAC, f32::from_bits(packed)],
                    )
                }
            };
            let xform = [transform as f32, elevation.to_wire(), effects.blur_radius.max(0.0), effects.alpha_ombre.clamp(0.0, 1.0)];
            draws.push(SdfDrawCmd {
                pos,
                size,
                color: inst.color,
                params,
                xform,
                clip: clip.to_wire(),
            });
        }

        SdfFrame {
            draws,
            char_buffer: self.chars.clone(),
            param_bank,
        }
    }
}

impl SdfFrame {
    /// Wrap into a full [`crate::core::RenderFrame`] at the given size.
    pub fn into_render_frame(self, width: u32, height: u32) -> crate::core::RenderFrame {
        let mut frame = crate::core::RenderFrame::new(width, height);
        frame.draws = self.draws;
        frame.char_buffer = self.char_buffer;
        frame.param_bank = self.param_bank;
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_maps_kinds_to_draw_types() {
        let mut list = DrawList::new();
        list.push(SdfInstance {
            kind: SdfKind::RoundedBox { radius: 8.0 },
            position: [10.0, 20.0],
            size: [100.0, 50.0],
            color: [1.0, 0.0, 0.0, 1.0],
            anim: 0,
        });
        list.push(SdfInstance {
            kind: SdfKind::Circle,
            position: [0.0, 0.0],
            size: [40.0, 60.0],
            color: [0.0, 1.0, 0.0, 1.0],
            anim: 2,
        });
        list.push_bezier([0.0, 0.0], [10.0, 0.0], [20.0, 10.0], [30.0, 10.0], 3.0, [1.0; 4]);

        let frame = list.lower();
        assert_eq!(frame.draws.len(), 3);
        assert_eq!(frame.draws[0].params[0], DRAW_TYPE_SLAB);
        assert_eq!(frame.draws[0].params[1], 8.0);
        assert_eq!(frame.draws[1].params[0], DRAW_TYPE_CIRCLE);
        assert_eq!(frame.draws[1].params[1], 20.0, "circle radius = min(w,h)/2");
        assert_eq!(frame.draws[1].params[2], 2.0, "anim index carried");
        assert_eq!(frame.draws[2].params[0], DRAW_TYPE_BEZIER);
        assert_eq!(frame.param_bank.len(), 1);
        assert_eq!(frame.param_bank[0], [10.0, 0.0, 20.0, 10.0]);
        assert_eq!(frame.draws[2].size, [30.0, 10.0], "size slot = P3");
    }

    #[test]
    fn per_corner_slab_lowers_through_param_bank() {
        let mut list = DrawList::new();
        list.push(SdfInstance {
            kind: SdfKind::RoundedBoxPerCorner { radii: [0.0, 16.0, 16.0, 0.0] },
            position: [4.0, 8.0],
            size: [200.0, 400.0],
            color: [0.5; 4],
            anim: 3,
        });
        let frame = list.lower();
        assert_eq!(frame.draws[0].params[0], DRAW_TYPE_SLAB_PC);
        assert_eq!(frame.draws[0].params[2], 3.0, "anim index carried");
        // params.w bitcasts to the aux-bank index holding the four radii.
        let idx = frame.draws[0].params[3].to_bits() as usize;
        assert_eq!(frame.param_bank[idx], [0.0, 16.0, 16.0, 0.0]);
    }

    /// **An instance can say it is flat, and one that says nothing is not.**
    ///
    /// The default is asserted first and asserted on `push` itself, because
    /// the whole safety of adding this knob is that every existing caller —
    /// which is every caller — keeps lowering to exactly the bytes it did
    /// before. The flag rides `xform[1]`, so it is also asserted not to
    /// disturb `xform[0]`, which the rotate stack owns.
    #[test]
    fn an_instance_can_be_pushed_flat_and_the_default_is_unchanged() {
        let boxy = |y: f32| SdfInstance {
            kind: SdfKind::Box,
            position: [0.0, y],
            size: [10.0, 10.0],
            color: [1.0; 4],
            anim: 0,
        };

        let mut list = DrawList::new();
        list.push(boxy(0.0));
        list.push_fill(boxy(10.0), Elevation::Flat);
        list.push_fill(boxy(20.0), Elevation::Default);
        let frame = list.lower();

        assert_eq!(frame.draws[0].xform, [0.0, 0.0, 0.0, 0.0], "push says nothing");
        assert_eq!(frame.draws[1].xform, [0.0, XFORM_FLAT, 0.0, 0.0], "flat is on the wire");
        assert_eq!(frame.draws[2].xform, [0.0, 0.0, 0.0, 0.0], "an explicit default is the default");
        // Nothing but xform[1] moved: the three commands are otherwise the
        // same shape, and the rotate slot is untouched.
        for d in &frame.draws {
            assert_eq!(d.params[0], DRAW_TYPE_BOX);
            assert_eq!(d.xform[0], 0.0);
        }

        // ...and it travels with the instance through a rotate scope, whose
        // own slot it must not collide with.
        let mut list = DrawList::new();
        list.push_rotate(SdfRotate::Quarter(1), [5.0, 5.0]);
        list.push_fill(boxy(0.0), Elevation::Flat);
        list.push_rotate_end();
        let frame = list.lower();
        assert_eq!(frame.draws[0].xform[0], 1.0, "the rotate id still lands in xform[0]");
        assert_eq!(frame.draws[0].xform[1], XFORM_FLAT);
    }

    /// `clear` resets the elevation channel with everything else — a reused
    /// list must not inherit the previous frame's flags, which is the failure
    /// mode a parallel `Vec` has and a struct field does not.
    #[test]
    fn clear_resets_the_elevation_channel() {
        let mut list = DrawList::new();
        list.push_fill(
            SdfInstance {
                kind: SdfKind::Box,
                position: [0.0, 0.0],
                size: [1.0, 1.0],
                color: [1.0; 4],
                anim: 0,
            },
            Elevation::Flat,
        );
        list.clear();
        list.push(SdfInstance {
            kind: SdfKind::Box,
            position: [0.0, 0.0],
            size: [1.0, 1.0],
            color: [1.0; 4],
            anim: 0,
        });
        assert_eq!(list.lower().draws[0].xform[1], 0.0);
    }

    #[test]
    fn bezier_param_indices_are_dense() {
        let mut list = DrawList::new();
        for i in 0..3 {
            let y = i as f32 * 10.0;
            list.push_bezier([0.0, y], [5.0, y], [10.0, y], [15.0, y], 2.0, [1.0; 4]);
        }
        let frame = list.lower();
        for (i, d) in frame.draws.iter().enumerate() {
            assert_eq!(d.params[3].to_bits(), i as u32);
        }
        assert_eq!(frame.param_bank.len(), 3);
    }

    fn box_at(position: [f32; 2]) -> SdfInstance {
        SdfInstance {
            kind: SdfKind::Box,
            position,
            size: [5.0, 5.0],
            color: [1.0; 4],
            anim: 0,
        }
    }

    /// A clip costs no instruction: it rides on the instances it bounds.
    #[test]
    fn a_clip_scope_emits_no_command_of_its_own() {
        let mut list = DrawList::new();
        list.push_clip([10.0, 20.0], [100.0, 50.0]);
        list.push(box_at([12.0, 22.0]));
        list.push_clip_end();

        let frame = list.lower();
        assert_eq!(frame.draws.len(), 1, "one shape drawn, one command emitted");
        assert_eq!(frame.draws[0].params[0], DRAW_TYPE_BOX);
        assert_eq!(
            frame.draws[0].clip,
            [10.0, 20.0, 110.0, 70.0],
            "the shape carries the region as [min.x, min.y, max.x, max.y]",
        );
    }

    /// An instance drawn under no clip must be bounded by nothing — and
    /// "nothing" has to be a rect that admits every pixel, NOT the all-zero
    /// rect a defaulted field would give, which would admit none.
    #[test]
    fn an_unclipped_instance_carries_a_bound_that_admits_everything() {
        let mut list = DrawList::new();
        list.push(box_at([12.0, 22.0]));
        let frame = list.lower();
        assert_eq!(frame.draws[0].clip, SdfDrawCmd::NO_CLIP);
        assert!(ClipRect::UNBOUNDED.contains([0.0, 0.0]));
        assert!(ClipRect::UNBOUNDED.contains([-9.9e5, 9.9e5]));
        assert_eq!(list.instance_clip(0), None, "and it reads back as no clip");
    }

    /// **Clips nest, and leaving one returns to the one around it.**
    ///
    /// This is the invariant the old encoding could not hold: with a single
    /// scissor register in the shader, the inner clip REPLACED the outer, and
    /// the inner's end CLEARED the register, so `after` below would have
    /// painted with no bound at all.
    #[test]
    fn an_inner_clip_intersects_the_outer_one_and_ending_it_returns_there() {
        let mut list = DrawList::new();
        list.push_clip([0.0, 0.0], [100.0, 100.0]);
        list.push(box_at([1.0, 1.0])); // 0: outer only
        list.push_clip([50.0, 50.0], [100.0, 100.0]); // overhangs the outer
        list.push(box_at([60.0, 60.0])); // 1: intersection
        list.push_clip_end();
        list.push(box_at([2.0, 2.0])); // 2: back to the outer
        list.push_clip_end();
        list.push(box_at([3.0, 3.0])); // 3: unbounded again

        let outer = ClipRect { min: [0.0, 0.0], max: [100.0, 100.0] };
        let inner = ClipRect { min: [50.0, 50.0], max: [100.0, 100.0] };
        assert_eq!(list.instance_clip(0), Some(outer));
        assert_eq!(
            list.instance_clip(1),
            Some(inner),
            "the inner clip is its own rect INTERSECTED with the outer one, so \
             the overhang past x/y=100 is cut off",
        );
        assert_eq!(
            list.instance_clip(2),
            Some(outer),
            "leaving the inner clip returns to the outer one, it does not clear it",
        );
        assert_eq!(list.instance_clip(3), None);

        let frame = list.lower();
        assert_eq!(frame.draws.len(), 4, "still no control commands");
        assert_eq!(frame.draws[1].clip, [50.0, 50.0, 100.0, 100.0]);
        assert_eq!(frame.draws[2].clip, [0.0, 0.0, 100.0, 100.0]);
    }

    /// Two clips that miss each other leave a real, empty region — the answer
    /// for a row scrolled fully out of its viewport.
    #[test]
    fn disjoint_clips_intersect_to_an_empty_region_not_an_inverted_one() {
        let mut list = DrawList::new();
        list.push_clip([0.0, 0.0], [10.0, 10.0]);
        list.push_clip([50.0, 50.0], [10.0, 10.0]);
        list.push(box_at([51.0, 51.0]));
        list.push_clip_end();
        list.push_clip_end();

        let clip = list.instance_clip(0).expect("clipped");
        assert!(clip.is_empty(), "no pixel is inside: {clip:?}");
        assert!(clip.size()[0] >= 0.0 && clip.size()[1] >= 0.0, "and it is not inverted");
        assert!(!clip.contains([51.0, 51.0]));
    }

    #[test]
    fn shaped_text_packs_chars() {
        let shaper = crate::font::TextShaper::new(crate::font::ROBOTO_REGULAR_ASCII.to_vec())
            .expect("fixture parses");
        // Atlas without real pixels: entries only, enough for packing.
        let mut atlas = FontAtlas::empty(64, 64, 3);
        for ch in ['H', 'i'] {
            let gid = shaper.glyph_id_for_char(ch).unwrap();
            atlas.insert_entry(crate::font::GlyphEntry {
                glyph_id: gid,
                atlas_x: 1, atlas_y: 1, atlas_w: 48, atlas_h: 48,
                advance_x: 0.5, baseline_row: 36.0, px_per_em: 36.9, x_margin: 7.2,
            });
        }

        let mut list = DrawList::new();
        let run = shaper.shape("Hi");
        let width = list.push_shaped_text(&run, &atlas, [100.0, 50.0], 16.0, 4.0, [1.0; 4]);
        assert!(width > 0.0);
        assert_eq!(list.chars().len(), 2);

        let frame = list.lower();
        assert_eq!(frame.draws.len(), 1);
        let d = &frame.draws[0];
        assert_eq!(d.params[0], DRAW_TYPE_MSDF_TEXT);
        // Packed slot: offset 0, count 2.
        assert_eq!(d.params[3].to_bits(), 2);
        // Line box height honors the ratio; x shifted left by the margin.
        assert_eq!(d.size[1], 16.0 * LINE_BOX_RATIO);
        assert!(d.pos[0] < 100.0);
        // Char entries reference valid table indices.
        for c in frame.char_buffer {
            assert!(((c >> 16) as usize) < atlas.glyphs.len());
        }
    }

    // ── SdfRotate ────────────────────────────────────────────────────────

    #[test]
    fn quarter_turn_matrices_are_bit_exact_integers_never_trig() {
        // The whole point: NO sin/cos call anywhere on this path, so these
        // must be the LITERAL 0.0/1.0/-1.0 values, not "very close to".
        assert_eq!(SdfRotate::Quarter(0).matrix2(), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(SdfRotate::Quarter(1).matrix2(), [0.0, -1.0, 1.0, 0.0]);
        assert_eq!(SdfRotate::Quarter(2).matrix2(), [-1.0, 0.0, 0.0, -1.0]);
        assert_eq!(SdfRotate::Quarter(3).matrix2(), [0.0, 1.0, -1.0, 0.0]);
        // Normalized mod 4, both directions.
        assert_eq!(SdfRotate::Quarter(4).matrix2(), SdfRotate::Quarter(0).matrix2());
        assert_eq!(SdfRotate::Quarter(-1).matrix2(), SdfRotate::Quarter(3).matrix2());
        assert_eq!(SdfRotate::Quarter(-4).matrix2(), SdfRotate::Quarter(0).matrix2());
    }

    #[test]
    fn rotate_rect_maps_a_known_rect_to_the_exact_expected_aabb() {
        // A 40x10 rect at (100,50), rotated 90 degrees about its own center
        // (120,55), becomes a 10x40 rect centered on the same point:
        // top-left (115,35). `assert_eq!` on the floats, not an epsilon —
        // this is exactly the assertion a regression back to trig would
        // fail (cos(PI/2) in f32 is off by ~4.37e-8, which would leak into
        // every one of these coordinates).
        let (pos, size) = rotate_rect([100.0, 50.0], [40.0, 10.0], SdfRotate::Quarter(1), [120.0, 55.0]);
        assert_eq!(pos, [115.0, 35.0]);
        assert_eq!(size, [10.0, 40.0]);

        // A full 180 about an off-center pivot (the origin): every corner
        // reflects through it (w/h unchanged — 180 degrees never swaps
        // width/height).
        let (pos, size) = rotate_rect([100.0, 50.0], [40.0, 10.0], SdfRotate::Quarter(2), [0.0, 0.0]);
        assert_eq!(pos, [-140.0, -60.0]);
        assert_eq!(size, [40.0, 10.0]);

        // Quarter(0) is a true no-op, bit for bit.
        let (pos, size) = rotate_rect([100.0, 50.0], [40.0, 10.0], SdfRotate::Quarter(0), [120.0, 55.0]);
        assert_eq!(pos, [100.0, 50.0]);
        assert_eq!(size, [40.0, 10.0]);
    }

    #[test]
    fn nested_push_rotate_composes_like_gl_push_matrix() {
        // Two nested 90-degree turns about the SAME pivot compose to
        // exactly Quarter(2)'s matrix — still bit-exact, since composing
        // two integer-valued matrices introduces no rounding.
        let mut list = DrawList::new();
        list.push_rotate(SdfRotate::Quarter(1), [50.0, 50.0]);
        list.push_rotate(SdfRotate::Quarter(1), [50.0, 50.0]);
        list.push(SdfInstance {
            kind: SdfKind::Box,
            position: [40.0, 40.0],
            size: [20.0, 20.0],
            color: [1.0; 4],
            anim: 0,
        });
        list.push_rotate_end();
        // Back to the single outer 90-degree scope.
        list.push(SdfInstance {
            kind: SdfKind::Box,
            position: [0.0, 0.0],
            size: [1.0, 1.0],
            color: [1.0; 4],
            anim: 0,
        });
        list.push_rotate_end();
        // Stack empty: unrotated.
        list.push(SdfInstance {
            kind: SdfKind::Box,
            position: [0.0, 0.0],
            size: [1.0, 1.0],
            color: [1.0; 4],
            anim: 0,
        });

        let frame = list.lower();
        assert_eq!(frame.draws.len(), 3);

        // Innermost (180 total): xform id 2, param_bank[2..4]. NOTE: xform.x
        // is a plain small integer VALUE (like anim_idx), not a bitcast
        // payload like the Bézier/SlabPC `params[3]` slot — `as u32`, never
        // `.to_bits()`.
        let nested_id = frame.draws[0].xform[0] as u32;
        assert_ne!(nested_id, 0);
        let base = ((nested_id - 1) * 2) as usize;
        let expect180 = SdfRotate::Quarter(2).matrix2();
        assert_eq!(frame.param_bank[base], expect180, "two nested 90s == one 180, bit-exact");

        // Middle instance: back to the outer 90-degree scope, a DIFFERENT
        // (smaller) transform id than the nested one, and NOT zero.
        let outer_id = frame.draws[1].xform[0] as u32;
        assert_ne!(outer_id, 0, "still inside the outer push_rotate after the inner pop");
        assert_ne!(outer_id, nested_id, "popped back to a different scope than the nested one");
        let outer_base = ((outer_id - 1) * 2) as usize;
        assert_eq!(frame.param_bank[outer_base], SdfRotate::Quarter(1).matrix2());

        // Last instance: both scopes popped, stack empty, no leak.
        assert_eq!(frame.draws[2].xform[0], 0.0, "transform stack must not leak past its pop");
    }

    #[test]
    #[should_panic(expected = "unclosed push_rotate")]
    fn lower_asserts_the_rotate_stack_is_empty() {
        // An unmatched push_rotate (missing its push_rotate_end) is exactly
        // the "transform stack leaks" failure mode call out as the classic
        // bug here — catch it at the source instead of silently rotating
        // every subsequent instance in the frame.
        let mut list = DrawList::new();
        list.push_rotate(SdfRotate::Quarter(1), [0.0, 0.0]);
        list.push(SdfInstance {
            kind: SdfKind::Box,
            position: [0.0, 0.0],
            size: [1.0, 1.0],
            color: [1.0; 4],
            anim: 0,
        });
        let _ = list.lower();
    }

    #[test]
    fn instances_outside_any_push_rotate_scope_carry_no_transform() {
        let mut list = DrawList::new();
        list.push(SdfInstance {
            kind: SdfKind::Box,
            position: [0.0, 0.0],
            size: [1.0, 1.0],
            color: [1.0; 4],
            anim: 0,
        });
        let frame = list.lower();
        assert_eq!(frame.draws[0].xform, [0.0, 0.0, 0.0, 0.0]);
        assert!(frame.param_bank.is_empty(), "no push_rotate ever happened -> nothing baked");
    }

    #[test]
    fn node_effect_scope_is_snapshotted_by_children_and_restored_after_pop() {
        let mut list = DrawList::new();
        list.begin_effects(DrawEffects { blur_radius: 4.0, alpha_ombre: 0.5 });
        list.push(SdfInstance {
            kind: SdfKind::Box,
            position: [0.0, 0.0],
            size: [1.0, 1.0],
            color: [1.0; 4],
            anim: 0,
        });
        list.end_effects();
        list.push(SdfInstance {
            kind: SdfKind::Box,
            position: [1.0, 0.0],
            size: [1.0, 1.0],
            color: [1.0; 4],
            anim: 0,
        });
        let frame = list.lower();
        assert_eq!(frame.draws[0].xform, [0.0, 0.0, 4.0, 0.5]);
        assert_eq!(frame.draws[1].xform, [0.0, 0.0, 0.0, 0.0]);
    }
}
