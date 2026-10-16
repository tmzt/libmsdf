---
title: Filled Bézier paths as an SDF primitive
date: 2026-10-15
status: In implementation
covers: >
  A new filled-path primitive for libmsdf: closed shapes built from line,
  quadratic and cubic Bézier segments, with any number of contours and holes,
  filled by a true signed distance (inside/outside from the even-odd crossing
  count), antialiased like every other kind. Covers the public API
  (`PathBuilder`, `Path`, `DrawList::push_path_fill`, `SdfKind::PathFill`),
  the CPU lowering (cubic to quadratic, monotone split), the wire format in
  the aux param bank, the WGSL evaluation, the CPU reference mirror, limits,
  tests and the downstream consumers.
sources:
  - >
    Owner (Tim), 2026-10-15, relayed for Noble's Keep: "if filled beziers are
    needed, add them to libmsdf the same way we've done everything else ...
    don't draw strokes to fill".
  - ~/src/NoblesKeep/docs/design/20261015_01_INITIAL_UI_PLAN.md, section 4.1
    (tool icons as libmsdf components; trowel blade and Heavy Axe head are
    filled Bézier outlines) and its amendment of 2026-10-15 23:44 EDT
    ("filled paths come from libmsdf, never from strokes").
  - src/drawlist/mod.rs (SdfKind, SdfInstance, Elevation, DrawEffects,
    rotate/translate/clip stacks, translate_from, append_from, lower)
  - src/core/sdf.rs (DRAW_TYPE_* constants, SdfDrawCmd wire doc,
    sd_cubic_stroke, sdf_eval_with_params)
  - src/gpu/sdf_render.wgsl (fs_main per-kind switch, sd_cubic_stroke,
    BEZIER_FLATTEN_STEPS, shadow block, lofi_distort hook, shared fill block)
  - src/gpu/mod.rs (MAX_PARAM_BANK_ENTRIES, upload_params truncation)
  - src/drawlist/stream.rs, build.rs, tests/golden_render.rs
related:
  - ~/src/NoblesKeep/docs/design/20261015_01_INITIAL_UI_PLAN.md (consumer)
  - tinytrowel (deps/libmsdf pinned at 69e6c5c), sigilweaver (libmsdf branch
    main-sigilweaver), libcontrols (git rev 69e6c5c), highbay (libmsdf branch
    highbay): consumers that must keep compiling
---

# Filled Bézier paths as an SDF primitive

## 1. Purpose

Noble's Keep draws its tool icons as libmsdf components: plain functions that
push primitives into a `DrawList`. Several of those icons have closed curved
outlines that must be filled: the trowel blade (two Bézier edges and a straight
heel), the bearded Heavy Axe head, a gear with a hub hole. libmsdf today has
boxes, rounded boxes, circles, outlines, straight lines, cubic Bézier
**strokes** and MSDF text. It has no filled path.

The owner's direction is that a fill is added to libmsdf as a real primitive,
the same way every other kind was added, and that strokes are never stacked to
imitate a fill. This plan adds one new kind, `SdfKind::PathFill`, whose
fragment value is the signed distance to a closed path: negative inside,
positive outside, exact (to f32) for lines and quadratics and within a stated
bound for cubics. It goes through the same shared fill block as the other
kinds, so its antialiasing, blur, ombré, animation, clip, rotation, translation
and drop shadow behave exactly like theirs.

Nothing about the existing kinds changes: not their behaviour, not their wire
layout, not their draw-type numbers.

## 2. Current state (what this builds on)

- **One full-screen pass.** `fs_main` loops over every `DrawCmd` for every
  fragment. Each command has a per-instance scissor test, an optional rotation
  undo (`xform.x`, transform bank at the front of `param_bank`), a per-type
  `switch` that produces `d` (and `shadow_d` for casters), then one shared
  shadow block and one shared fill block:
  `fill_alpha = color.a * anim * ombre * (1 - smoothstep(-0.5 - blur, 0.5 + blur, d))`.
  `d` is in logical pixels (`in.uv` is divided by `resolution.z`), so every
  kind has the same one-logical-pixel AA ramp widened by `xform.z` blur.
- **Aux param bank.** `param_bank: array<vec4<f32>>` (binding 12, capacity
  `MAX_PARAM_BANK_ENTRIES = 4096` vec4s, 64 KiB). `DrawList::lower` writes the
  rotation/translation transform bank first (two vec4 per transform), then
  appends per-instance entries in instance order: a line's far endpoint, a
  Bézier's two controls, per-corner radii. An instance points at its entry
  with `params.w = f32::from_bits(index)`. `upload_params` truncates at
  capacity with a `log::warn`; the shader bounds-checks against
  `arrayLength(&param_bank)`, which is the buffer size, not the uploaded length.
- **Bézier stroke (type 10).** Flattens the cubic to
  `BEZIER_FLATTEN_STEPS = 24` chords and takes the minimum chord distance per
  fragment, after a control-hull bounding-box reject padded by the thickness
  (plus 8 px when raised). It is a stroke only; it has no inside.
- **Elevation.** `xform.y = XFORM_RAISED` turns on the shadow; the shadow is
  the same SDF evaluated at `p - (2, 3)` and blended as
  `0.15 * (1 - smoothstep(-1, 4, shadow_d))`.
- **Pencil-noise hook.** `lofi_distort` jitters the sample point once, before
  the command loop, so every kind inherits it with no per-kind code.
- **Kinds are `Copy`.** `SdfKind` derives `Copy` and `PartialEq`, so a kind
  cannot own a `Vec`. Variable-length data (MSDF text) lives in a bank owned
  by the `DrawList` (`chars`), and the kind holds `start`/`count` into it;
  `append_from` rebases `char_start`. Paths follow the same pattern.
- **Absolute geometry.** `Line` and `BezierStroke` keep absolute points inside
  the kind and are moved by `SdfKind::translate`, whose match is deliberately
  exhaustive. Paths keep absolute points in their bank and must be moved too.

## 3. Design

### 3.1 Public API

New module `src/drawlist/path.rs`, re-exported from `drawlist` and from the
crate root next to `DrawList`.

```rust
/// One source segment of a contour, in absolute logical pixels (the same
/// space `SdfInstance::position` is in, before any push_rotate/push_translate).
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PathSegment {
    Line { to: [f32; 2] },
    Quad { ctrl: [f32; 2], to: [f32; 2] },
    Cubic { c1: [f32; 2], c2: [f32; 2], to: [f32; 2] },
}

/// A closed contour: a start point and the segments that follow it. Always
/// closed: if the last segment does not end at `start`, a straight closing
/// segment is implied.
#[derive(Clone, Debug, PartialEq)]
pub struct Contour { pub start: [f32; 2], pub segments: Vec<PathSegment> }

/// A validated, closed, multi-contour path. Filled by the even-odd rule.
#[derive(Clone, Debug, PartialEq)]
pub struct Path { contours: Vec<Contour> }

impl Path {
    pub fn contours(&self) -> &[Contour];
    /// Axis-aligned bound of every point, control points included (a bound
    /// on the ink, not its tight box).
    pub fn bounds(&self) -> ([f32; 2], [f32; 2]); // (pos, size)
    /// Apply a point map to every point, controls included. Exact for any
    /// affine map (Béziers are affine-invariant), which is what an icon
    /// drawn on a design grid (e.g. 24x24) and placed into a rect needs:
    /// `path.mapped(|[x, y]| [rx + x * s, ry + y * s])`.
    pub fn mapped(&self, f: impl Fn([f32; 2]) -> [f32; 2]) -> Path;
}

#[derive(Clone, Debug, Default)]
pub struct PathBuilder { /* contours so far + the open one */ }

impl PathBuilder {
    pub fn new() -> Self;
    /// Start a new contour at `p`, implicitly closing the open one.
    pub fn move_to(&mut self, p: [f32; 2]) -> &mut Self;
    pub fn line_to(&mut self, p: [f32; 2]) -> &mut Self;
    pub fn quad_to(&mut self, ctrl: [f32; 2], p: [f32; 2]) -> &mut Self;
    pub fn cubic_to(&mut self, c1: [f32; 2], c2: [f32; 2], p: [f32; 2]) -> &mut Self;
    /// Close the open contour (adds the closing line if it is not already
    /// at the start). The next segment needs a move_to first.
    pub fn close(&mut self) -> &mut Self;
    pub fn build(&self) -> Result<Path, PathError>;
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PathError {
    /// No contour with at least one non-degenerate segment.
    Empty,
    /// A segment was added with no move_to before it (including after close()).
    NoCurrentPoint,
    /// A point was NaN or infinite.
    NonFinite,
    /// After lowering (section 3.3) the path needs more GPU segments than
    /// PATH_MAX_SEGMENTS. Nothing was pushed.
    TooManySegments { needed: u32, max: u32 },
}
```

`PathBuilder` methods take `&mut self` so icon code can build contours in
loops (gear teeth) as well as in chains. Errors are collected and reported
by `build`, never by panicking. Contours whose segments are all zero length
are dropped by `build`.

`SdfKind` gains one variant, holding a range into a bank the `DrawList` owns,
exactly as `MsdfText` does:

```rust
/// A filled closed path (even-odd), lowered to line and quadratic segments
/// held in the list's own path-segment bank at `seg_start..seg_start+seg_count`.
/// `SdfInstance::position`/`size` are the segments' control-hull bound.
/// Produced by [`DrawList::push_path_fill`]; not meant to be built by hand.
PathFill { seg_start: u32, seg_count: u32 },
```

`DrawList` gains one door:

```rust
/// Fill `path` with `color` at `elevation`. Lowers the path (cubics to
/// quadratics within PATH_CUBIC_TOLERANCE, monotone split) at push time,
/// stores the segments in the list's path bank and pushes ONE instance
/// through push_fill, so it is tagged with the active rotate/translate,
/// clip and effects scopes like any other instance.
/// On error nothing is pushed and the list is unchanged.
pub fn push_path_fill(
    &mut self,
    path: &Path,
    color: [f32; 4],
    elevation: Elevation,
) -> Result<(), PathError>;
```

It routes through `push_fill` (and so through `push`), keeping `push` the one
point where instances enter the list, which the highbay drawing-allowlist
scans rely on; the name contains `fill(` for the same reason `push_fill`'s
does.

Example (the trowel blade in Noble's Keep, design grid 0..24 mapped to a rect):

```rust
let blade = PathBuilder::new()
    .move_to([4.0, 14.0])
    .cubic_to([7.0, 6.0], [13.0, 3.0], [20.0, 2.0])   // upper edge to the point
    .cubic_to([18.0, 9.0], [14.0, 13.0], [8.0, 16.0]) // lower edge back
    .close()                                           // straight heel
    .build()?
    .mapped(|[x, y]| [r.x + x * r.w / 24.0, r.y + y * r.h / 24.0]);
list.push_path_fill(&blade, style.steel, Elevation::Flat)?;
```

### 3.2 Fill rule: even-odd

**Even-odd is chosen.** A point is inside when a ray from it crosses the
path's segments an odd number of times. The reasons:

1. **The distance is exact under even-odd and not under nonzero.** The
   shader's magnitude is the minimum distance to any segment. Under even-odd
   every segment is a real boundary of the filled region (crossing any
   segment flips inside/outside), so that minimum is the true distance to the
   region's boundary. Under nonzero, two overlapping contours wound the same
   way leave their edges inside the filled region; the minimum distance then
   finds those interior edges and the AA ramp draws a faint seam through the
   fill (at 0.2 px from an interior edge the fill drops to about 70%). The two
   rules differ only where winding magnitude is 2 or more, which is exactly
   where nonzero's min-distance breaks. Making nonzero correct needs
   per-contour signed distances combined by union/difference, ordered nesting
   rules and per-contour accumulators in the shader: more state, more cost,
   and a rule that is no longer plain nonzero.
2. **Holes do not depend on direction.** Any contour inside another is a
   hole, whichever way it was drawn. Hand-authored icon outlines and SVG
   exports are often inconsistent in direction; even-odd does not care.
3. **Overlap has a libmsdf answer already.** Two overlapping parts of an icon
   (an axe head over a haft, three overlapping heads) are drawn as separate
   instances, which is how every other kind composes. Separate instances in
   the same colour union cleanly; in different colours they layer.

Documented consequence: two overlapping contours in ONE path cancel where they
overlap. `Path` docs and the `push_path_fill` doc say so in the first line.

### 3.3 CPU lowering (at push time)

`push_path_fill` converts the `Path` into **GPU segments**, each a line or a
y-monotone quadratic, in absolute logical pixels:

1. **Implicit close.** Each contour gets a closing line from its last point
   to `start` unless they are equal (bitwise after the builder's own close).
2. **Cubic to quadratics.** A cubic `P0, C1, C2, P3` is split uniformly into
   `n` pieces, each approximated by the quadratic with the same endpoints and
   control `(3 (C1 + C2) - (P0 + P3)) / 4` of that piece. The distance between
   a cubic and this quadratic is at most `(√3 / 36) |P3 - 3 C2 + 3 C1 - P0|`,
   and splitting into `n` equal parameter pieces divides that term by `n³`,
   so
   `n = clamp(ceil(cbrt((√3/36) |P3 - 3C2 + 3C1 - P0| / PATH_CUBIC_TOLERANCE)), 1, PATH_MAX_QUADS_PER_CUBIC)`.
   With `PATH_CUBIC_TOLERANCE = 1/64` logical px a quarter-circle cubic of
   radius 10 px needs 3 quads, radius 100 px needs 6; the typical icon cubic
   needs 2 to 4. At the cap of 16 the bound still holds for any cubic whose
   `|P3 - 3C2 + 3C1 - P0|` is under about 1,330 px, far beyond an icon.
   This is the one approximation in the primitive, and it is a stated bound:
   1/64 logical px is at most 1/16 of a device pixel at device scales up to 4,
   well inside the one-pixel AA ramp.
3. **Near-straight quadratics become lines.** A quadratic deviates from its
   chord by at most `|P0 - 2 C + P2| / 4`; if that is below
   `PATH_LINE_DEMOTE = 1/256` px it is emitted as a line. This also removes
   the near-degenerate case where the exact quadratic distance (3.5) loses
   precision (`|P0 - 2C + P2|` close to 0).
4. **Monotone split.** Each quadratic is split at its y-extremum
   `t = (P0.y - C.y) / (P0.y - 2 C.y + P2.y)` when that `t` is strictly inside
   (0, 1), so every GPU quadratic is monotone in y. Then each segment crosses
   any horizontal line at most once, which makes the crossing count in the
   shader one root and one comparison per segment, with no tangent cases.
5. **Degenerate segments** (zero length after the above) are dropped.
6. **Limit.** If the result has more than `PATH_MAX_SEGMENTS = 256` GPU
   segments, `push_path_fill` returns `TooManySegments` and pushes nothing.
   A partial path is never drawn: dropping segments from an even-odd path
   inverts parts of it.
7. **Bound.** The instance's `position`/`size` are the axis-aligned bound of
   every GPU segment's three points (control hull), so the curve lies inside
   it. It is used for the shader's bounding-box reject and for the ombré
   gradient's vertical extent, like `pos`/`size` on every filled kind.

Typical sizes: an icon outline of 4 to 16 source segments lowers to about 8 to
48 GPU segments (two to four quads per cubic, about half of them split once
at a y-extremum).

The lowered segment type, stored in the list's new private
`path_segments: Vec<GpuPathSegment>` bank:

```rust
/// One GPU segment: a line (`kind == 0`, from p0 to p2; `ctrl` is set to the
/// midpoint so the three-point hull is still the segment's bound) or a
/// y-monotone quadratic (`kind == 1`, p0 -> ctrl -> p2).
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuPathSegment { pub p0: [f32; 2], pub ctrl: [f32; 2], pub p2: [f32; 2], pub kind: f32, pub _pad: f32 }
```

### 3.4 Wire format

- **New draw type `DRAW_TYPE_PATH_FILL = 12.0`** in `core::sdf`, next to the
  others. 6 and 7 stay retired. No other type's number or layout changes.
- **`SdfDrawCmd` is unchanged (80 bytes).** For a path:
  - `pos`, `size`: the control-hull bound (3.3 step 7), pre-transform space.
  - `params = [12, seg_count as f32, anim_idx, f32::from_bits(bank_index)]`.
    `seg_count` is at most 256, exact in f32.
  - `xform`, `clip`, `color`: as for every kind.
- **Segments go in the existing aux param bank, not a new buffer.** Each GPU
  segment is two consecutive vec4s:
  `[p0.x, p0.y, ctrl.x, ctrl.y]`, `[p2.x, p2.y, kind, 0]`.
  `lower()` appends an instance's `2 * seg_count` vec4s at `bank_index` when
  it reaches that instance, after the transform bank, in instance order, the
  same as every other per-instance aux entry. A new storage binding was
  rejected: it changes the bind group layout every consumer's renderer is
  built from, and it costs one of the storage-buffer slots the downlevel
  targets count.
- **Capacity.** `MAX_PARAM_BANK_ENTRIES` (4096 vec4s, `src/gpu/mod.rs`,
  private) becomes `pub const PARAM_BANK_CAPACITY: usize = 4096` in
  `core::sdf`, used by both `gpu::mod` (same value, so no behaviour change)
  and `lower()`. One full 256-segment path uses 512 vec4s; a frame of twenty
  48-segment icons uses 1,920.
- **Past the frame capacity.** If appending a path's segments would take
  `param_bank.len()` past `PARAM_BANK_CAPACITY`, `lower()` writes nothing for
  it, emits the command with `seg_count = 0` (the shader draws nothing for it)
  and `log::warn!`s once per frame with the number of paths dropped. It must
  not leave the path to `upload_params`' truncation, because the shader's
  bound check is against the buffer size and would read stale segments from
  the previous frame. Later non-path entries keep today's behaviour unchanged.
- **The 32-bit command stream** (`drawlist::stream`) does not encode paths;
  it does not encode Bézier strokes either. Out of scope.

### 3.5 Shader: `case 12u`

Evaluated in `effective_pixel` (absolute, after the scissor test, the
rotation/translation undo and `lofi_distort`), like `Line` and `BezierStroke`,
because the segments are absolute points.

```wgsl
case 12u: { // Filled path, even-odd; segments in param_bank
    let n = u32(cmd.params.y);
    let base = bitcast<u32>(cmd.params.w);
    let wants_shadow = cmd.xform.y >= 0.5;
    let band = 1.0 + max(cmd.xform.z, 0.0);           // AA half-width + blur, logical px
    let margin = select(band, band + 8.0, wants_shadow);
    let q = effective_pixel;
    if n > 0u && base + 2u * n <= arrayLength(&param_bank) &&
       q.x >= cmd.pos.x - margin && q.x <= cmd.pos.x + cmd.size.x + margin &&
       q.y >= cmd.pos.y - margin && q.y <= cmd.pos.y + cmd.size.y + margin {
        d = sd_path_fill(q, base, n, band);
        if wants_shadow {
            shadow_d = sd_path_fill(q - vec2<f32>(2.0, 3.0), base, n, 4.0);
            shadow_on = true;
        }
    }
}
```

`sd_path_fill(q, base, n, band)` walks the `n` segments once and keeps two
things:

1. **Crossing parity** (the sign). The ray goes from `q` toward +x. For each
   segment with `min(p0.y, p2.y) <= q.y < max(p0.y, p2.y)` (half-open, so a
   shared vertex is counted exactly once and horizontal segments never
   count) and whose hull max x is greater than `q.x`, find the x where the
   segment meets `y = q.y` and flip `inside` if that x is greater than `q.x`.
   A line interpolates. A monotone quadratic solves
   `a t² + b t + c = 0` with `a = y0 - 2 y1 + y2`, `b = 2 (y1 - y0)`,
   `c = y0 - q.y` (linear when `|a|` is tiny, otherwise the numerically stable
   pair `t1 = s / a`, `t2 = c / s` with `s = -0.5 (b + sign(b) sqrt(b² - 4ac))`),
   taking the root in [0, 1]; monotonicity guarantees there is exactly one.
2. **Minimum unsigned distance, within the band.** A segment is measured
   only if `q` lies inside its three-point hull bound expanded by `band`;
   otherwise its distance is certainly greater than `band` and it cannot
   change the pixel. Lines use `sd_segment_raw` (exact). Quadratics use the
   closed-form distance (Inigo Quilez's `sdBezier`: reduce
   `(B(t) - q) · B'(t) = 0` to a depressed cubic, Cardano when it has one real
   root, the trigonometric form when it has three, `t` clamped to [0, 1],
   `acos` argument clamped to [-1, 1]), which is exact up to f32 rounding for
   the non-degenerate quadratics 3.3 step 3 leaves.

The result is `select(1.0, -1.0, inside) * min(dmin, band)`. Outside the band
the magnitude is clamped to `band`, where `smoothstep(-0.5 - blur, 0.5 + blur,
d)` is already saturated, so the clamp changes no pixel. It is what makes the
interior cheap: deep inside or outside the shape no segment is measured, only
counted. The shadow call uses band 4, the far end of the shadow's
`smoothstep(-1, 4, shadow_d)`.

Everything after the switch is shared and unchanged: the elevation gate, the
shadow blend, animation, blur/ombré and
`fill_alpha = ... * (1 - smoothstep(-0.5 - blur, 0.5 + blur, d))`. So:

- **AA width** is the same one logical pixel as Box, Circle and the strokes,
  widened by blur exactly as theirs is.
- **push_rotate / push_translate**: handled by the existing undo of
  `effective_pixel`; segments are stored in pre-transform space, like a
  Bézier's controls. Rotations are rigid, so the cubic tolerance and the
  distances are the same after rotation.
- **Clip**: the existing per-instance scissor test runs first. Nothing new.
- **Raised shadow**: as above; same offset (2, 3) and blend as Box/Slab and
  the Bézier stroke; bbox padded by 8 px when raised, as the stroke's is.
- **Pencil noise**: `lofi_distort` jitters `pixel` before the loop, so the
  path's edge wobbles exactly as other kinds' do. When Phase 8 replaces the
  placeholder jitter, paths need no change. The bbox reject compares the
  jittered point with the geometry, which is the frame the distance is
  evaluated in, so it needs no extra padding.
- **Crispness at any scale**: segments are in final logical pixels (there is
  no `push_scale`; a zoomed icon is re-pushed with new coordinates and
  re-lowered), so the field is exact at every size and the edge is always a
  one-pixel ramp, never a resampled texture.

**Per-fragment cost.** Outside the path's bound (most fragments) it is one
bbox test, as for the Bézier stroke. Inside it, every segment costs a y-range
test and at most one root and compare (about 15 to 25 flops); only the one or
two segments within `band` of the fragment get the exact distance (about
60 to 80 flops including one `sqrt` and either one `pow` pair or one
`acos`/`cos`/`sin`). A 32-segment icon is then about 600 to 900 flops per
covered fragment, comparable to one Bézier stroke (24 `cubic_point` plus 24
`sd_segment_raw`), and double that where it is raised. A typical 48 x 48 px
tile icon of three or four instances adds a few thousand covered fragments.

The CPU mirror goes in `core::sdf`: `pub fn sd_path_fill(px, py, segments:
&[[f32; 4]], band: f32) -> f32` (pass `f32::INFINITY` for the unclamped
distance) and an arm `12 =>` in `sdf_eval_with_params` reading the segments
from `param_bank`, with "keep in sync with `sdf_render.wgsl` case 12u", as
the other mirrors say.

### 3.6 DrawList bookkeeping

- `path_segments: Vec<GpuPathSegment>`: new private field; cleared by
  `clear()`.
- `SdfKind::translate`: a `PathFill { .. }` arm that changes nothing in the
  kind (it holds indices, not points), with a comment pointing at
  `translate_from`.
- `translate_from(start, dx, dy)`: for every `PathFill` instance in the range,
  also adds `(dx, dy)` to `p0`, `ctrl` and `p2` of its segments. Each instance
  owns its own range, so nothing moves twice.
- `append_from`: rebases `seg_start` by the destination's
  `path_segments.len()` and appends the source's segments, the same way
  `char_start` is rebased.
- `lower()`: the new arm described in 3.4.

### 3.7 Compatibility and version

- Wire layout and behaviour of types 0 to 11 are unchanged; `SdfDrawCmd`,
  the bind group layout and the uniform block are unchanged. A list with no
  paths lowers to byte-identical `SdfFrame`s (asserted by a test, step 5).
- `SdfKind` is a public enum without `#[non_exhaustive]`, so adding a variant
  breaks any downstream exhaustive `match` on it. On 2026-10-15 the consumers
  were searched: libcontrols, tinytrowel, sigilweaver and highbay only use
  `matches!` or `match` with a `_` arm on `SdfKind`. Under Cargo's SemVer
  rules a new variant on an exhaustive public enum is still a breaking change,
  so the crate goes from `0.2.0` to **`0.3.0`**. Every consumer uses a path or
  git dependency, so the version number gates nothing; it records the change.

## 4. Constants

| Name | Value | Where |
|---|---|---|
| `DRAW_TYPE_PATH_FILL` | `12.0` | `core::sdf` |
| `PARAM_BANK_CAPACITY` | `4096` (vec4s; was private `MAX_PARAM_BANK_ENTRIES`) | `core::sdf` |
| `PATH_MAX_SEGMENTS` | `256` GPU segments per path | `drawlist::path` |
| `PATH_CUBIC_TOLERANCE` | `1.0 / 64.0` logical px | `drawlist::path` |
| `PATH_MAX_QUADS_PER_CUBIC` | `16` | `drawlist::path` |
| `PATH_LINE_DEMOTE` | `1.0 / 256.0` logical px | `drawlist::path` |

## 5. Steps

Each step is one commit (or a short series) on `libmsdf` main, passing
`cargo fmt -p libmsdf -- --check`, `cargo clippy -p libmsdf --all-targets`
with no new warnings and `cargo test -p libmsdf` (Rule 3.4). GPU tests run
with `CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm cargo test -p libmsdf --features gpu-tests`.

1. **Constants and wire documentation.** Add `DRAW_TYPE_PATH_FILL` and
   `PARAM_BANK_CAPACITY` to `core::sdf`; make `gpu::mod` use
   `PARAM_BANK_CAPACITY` instead of its private constant; extend the
   `SdfDrawCmd` wire doc and the shader's header comment with type 12. This
   first commit also sets this plan to `In implementation` with its amendment
   and README row (Rules 1.5, 3.2, 4.3).
   **Done when:** the crate builds and all existing tests pass with no output
   change; `param_bank` buffer size is still 64 KiB.

2. **Path model.** Add `src/drawlist/path.rs` with `PathSegment`, `Contour`,
   `Path`, `PathBuilder`, `PathError`, `Path::bounds` and `Path::mapped`;
   re-export them. Unit tests: chained and loop-built paths; `line_to` before
   `move_to` gives `NoCurrentPoint`; NaN gives `NonFinite`; an empty builder
   and an all-zero-length contour give `Empty`; `move_to` closes the previous
   contour; `mapped` with a scale+offset moves every point including controls.
   **Done when:** those tests pass and the API compiles on native and wasm32.

3. **Lowering to GPU segments.** Add `GpuPathSegment` and
   `fn lower_path(path: &Path) -> Result<(Vec<GpuPathSegment>, [f32; 2], [f32; 2]), PathError>`
   (segments, bound pos, bound size) implementing 3.3. Unit tests: for a set
   of cubics (quarter circle r = 10 and r = 100, S-curve, cusp, loop, a
   1,000 px swoop) the quads stay within `PATH_CUBIC_TOLERANCE` of the cubic
   at 1,000 sampled parameters each; quad counts for the quarter circles are
   3 and 6; every emitted quadratic is y-monotone; near-straight quadratics
   come out as lines; implicit closing adds exactly one line when needed and
   none when the contour already ends at its start; a 257-segment result
   returns `TooManySegments` and a 256-segment one succeeds.
   **Done when:** those tests pass.

4. **CPU reference SDF.** Add `core::sdf::sd_path_fill` and the `12 =>` arm
   of `sdf_eval_with_params` (3.5). Unit tests: sign by even-odd for points
   inside a square, inside its square hole (drawn clockwise and again
   counter-clockwise, same result), in the ring, outside; a point level with a
   vertex and a point level with a horizontal edge get the right parity;
   unsigned distance agrees with brute-force dense sampling of the same
   segments within 0.01 px at 500 random points near the edges; clamped
   magnitude equals `band` beyond it; a 4-cubic circle of radius 60 agrees
   with `sd_circle` within 0.05 px along its boundary band.
   **Done when:** those tests pass.

5. **DrawList integration.** Add `SdfKind::PathFill`, the `path_segments`
   bank, `push_path_fill`, the `translate`, `translate_from`, `append_from`,
   `clear` and `lower` changes (3.4, 3.6). Unit tests: lowering puts
   `[12, n, anim, idx]` in `params`, the bound in `pos`/`size` and
   `2n` rows at `idx` after the transform bank; a path pushed inside
   `push_rotate`/`push_translate`/`push_clip`/`begin_effects` carries the
   same `xform`/`clip` as a Box pushed there; `Elevation::Raised` sets
   `xform.y`; `translate_from` moves segments and bound together (a range
   containing two paths moves each once); `append_from` rebases `seg_start`
   so the appended path lowers to the same segments; an error push leaves
   the list's lengths unchanged; enough paths to pass `PARAM_BANK_CAPACITY`
   lower the overflowing ones with `seg_count = 0` and earlier ones intact;
   and a fixed list of every existing kind (no paths) lowers to the same
   `SdfFrame` bytes as recorded before this step.
   **Done when:** those tests and all existing drawlist tests pass.

6. **Shader.** Add `ud_quad`, `path_crossing`, `sd_path_fill` and `case 12u`
   to `sdf_render.wgsl` (3.5), mirroring the CPU code line for line.
   **Done when:** `build.rs`'s naga parse + validation passes, and a headless
   render of one filled square at (10, 10) 20 x 20 has alpha 255 at its centre
   and 0 at (5, 5).

7. **GPU golden tests.** New `tests/path_fill.rs` (gated on `gpu-tests`,
   same harness as `golden_render.rs`, its own fixture
   `tests/fixtures/golden_path_fill.png`, blessed with `LIBMSDF_BLESS=1`):
   - **Circle agreement:** a 4-cubic circle (k = 0.5523) of radius 60 beside
     `SdfKind::Circle` of the same radius, each rendered alone into its own
     image; per-pixel channel difference at most 12/255 everywhere and mean
     under 1/255.
   - **Hole:** a rounded square with a circular hole and a nested island;
     the hole centre is background, the ring and island are ink; reversing
     the hole's direction produces an identical image.
   - **Rotation:** an asymmetric shape under `SdfRotate::Quarter(1)` matches
     the same shape with pre-rotated coordinates exactly (both quarter-turn
     exact); under `SdfRotate::Radians(0.3)` it matches the CPU mirror's
     coverage within 12/255.
   - **Scale:** the trowel-blade path at 8 px and at 512 px: along a scanline
     crossing an edge, the number of partially covered pixels is 1 or 2 at
     both sizes, and both match the CPU mirror's coverage within 12/255.
   - **Shadow, clip, blur:** `Elevation::Raised` darkens pixels at
     `(+2, +3)` off the edge and not with `Flat`; a `push_clip` halves the
     shape on its line; `blur_radius = 2` widens the ramp to about 5 px.
   - The existing `golden_scene.png` comparison passes unchanged (no
     re-bless).
   **Done when:** all of these pass headless on Metal.

8. **Release and consumers.** Bump `Cargo.toml` to `0.3.0`; add "filled
   Bézier path" to the README's primitive list. Then, in each consumer's
   checkout, point its libmsdf at the new commit locally and run its own
   build: tinytrowel (`deps/libmsdf` submodule, now at 69e6c5c),
   libcontrols (git rev 69e6c5c), sigilweaver (`deps/libmsdf` on branch
   `main-sigilweaver`; merge `main` into `main-sigilweaver`) and highbay
   (`deps/libmsdf`, branch `highbay`), plus Noble's Keep once its workspace
   depends on libmsdf. Re-run the search for exhaustive `match` on
   `SdfKind` in each. Run
   `cargo check -p libmsdf --target wasm32-unknown-unknown`. Bumping each
   consumer's pin is that consumer's own commit, in its own repo.
   **Done when:** libmsdf passes the wasm32 check and every listed consumer
   builds and passes its tests against the new libmsdf with no source change
   other than the pin.

## 6. Out of scope

- Stroking a path (a path outline at a thickness). `BezierStroke` and `Line`
  remain the strokes; a path stroke would be its own plan.
- Nonzero fill (see 3.2 and open question 1).
- Gradients or textures inside a path (the fill is the instance colour,
  like every other kind).
- Encoding paths in the 32-bit `drawlist::stream` format.
- Arcs as a segment type (`arc_to`); callers convert arcs to cubics.

## 7. Open questions

1. **Fill rule.** Even-odd is chosen (3.2). Is that acceptable for the
   icon work, given overlapping parts are drawn as separate instances? If a
   nonzero path is ever required, it would be a follow-up plan with
   per-contour union/difference.
2. **Cubic tolerance.** 1/64 logical px with a cap of 16 quads per cubic.
   Tighter costs segments; looser is visible only at extreme device scales.
3. **Segment encoding.** Two vec4s per segment (32 bytes) wastes the shared
   endpoint and the line's control. A denser shared-endpoint stream (about
   1.5 vec4 per segment) is possible at the cost of a sequential decode.
   Keep the simple form unless the bank fills up in practice?
4. **Param bank capacity.** 4096 vec4s shared by transforms, strokes, lines,
   per-corner boxes and now paths. Raise it (for example to 16,384, 256 KiB)
   now, or wait for a frame that overflows?
5. **Version.** 0.3.0 because a variant is added to an exhaustive public
   enum, though no consumer matches exhaustively. Or 0.2.1 as purely
   additive in practice? Or mark `SdfKind` `#[non_exhaustive]` in the same
   release so later kinds are minor bumps?
6. **Branches.** Should the feature also be merged into libmsdf's
   `main-sigilweaver` and `highbay` branches at step 8, or only once those
   consumers need it?

## Amendment — 2026-10-16 02:12 EDT: implementation started; document now immutable

Implementation has started on the owner's instruction ("implement libmsdf
changes"), and this document is now immutable (AGENTS.md Rule 1.5). It
replaces no earlier text. The open questions of section 7 were not answered
one by one; the plan's own recommended answers are in force:

1. **Fill rule:** even-odd (3.2). Overlapping parts are separate instances.
2. **Cubic tolerance:** `PATH_CUBIC_TOLERANCE = 1/64` logical px, at most
   `PATH_MAX_QUADS_PER_CUBIC = 16` quadratics per cubic (3.3).
3. **Segment encoding:** two vec4s per GPU segment,
   `[p0.x, p0.y, ctrl.x, ctrl.y]`, `[p2.x, p2.y, kind, 0]` (3.4).
4. **Param bank capacity:** unchanged at 4096 vec4s (64 KiB), as drafted,
   now public as `PARAM_BANK_CAPACITY`.
5. **Version:** 0.3.0, by AGENTS.md Rule 5.4: a variant added to an
   exhaustive public enum is not strictly additive. In the same release
   `SdfKind` is marked `#[non_exhaustive]` (step 5), following libcontrols,
   which marked `Key` `#[non_exhaustive]` in its own 0.3.0, so that later
   kinds are additive. No dependency of libmsdf moves to a
   semver-incompatible version, so Rule 5.5 adds nothing here.
6. **Branches:** the feature is merged into `main` only, not into
   `main-sigilweaver` or `highbay`. Consumers are checked against the new
   libmsdf through local, uncommitted overrides (step 8).

How the steps land (one commit per step on the branch `impl/0.3.0`,
squash-merged into `main`) and any departures are recorded in `CHANGES.md`.
