//! Filled paths: closed outlines of line, quadratic and cubic Bézier
//! segments, with any number of contours, filled by the even-odd rule as a
//! true signed distance ([`crate::DrawList::push_path_fill`]).
//!
//! Plan: `docs/design/20261015_01_FILLED_BEZIER_PATH_PLAN.md`.

/// One source segment of a contour, in absolute logical pixels (the same
/// space `SdfInstance::position` is in, before any push_rotate/push_translate).
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PathSegment {
    Line {
        to: [f32; 2],
    },
    Quad {
        ctrl: [f32; 2],
        to: [f32; 2],
    },
    Cubic {
        c1: [f32; 2],
        c2: [f32; 2],
        to: [f32; 2],
    },
}

impl PathSegment {
    /// The point this segment ends at.
    pub fn end(&self) -> [f32; 2] {
        match *self {
            PathSegment::Line { to }
            | PathSegment::Quad { to, .. }
            | PathSegment::Cubic { to, .. } => to,
        }
    }

    /// Every point the segment carries, controls included, in order.
    fn points_mut(&mut self) -> impl Iterator<Item = &mut [f32; 2]> {
        let v: Vec<&mut [f32; 2]> = match self {
            PathSegment::Line { to } => vec![to],
            PathSegment::Quad { ctrl, to } => vec![ctrl, to],
            PathSegment::Cubic { c1, c2, to } => vec![c1, c2, to],
        };
        v.into_iter()
    }

    fn points(&self) -> Vec<[f32; 2]> {
        match *self {
            PathSegment::Line { to } => vec![to],
            PathSegment::Quad { ctrl, to } => vec![ctrl, to],
            PathSegment::Cubic { c1, c2, to } => vec![c1, c2, to],
        }
    }

    /// Whether every point equals `from`: the segment has zero length.
    fn is_degenerate_from(&self, from: [f32; 2]) -> bool {
        self.points().iter().all(|&p| p == from)
    }
}

/// A closed contour: a start point and the segments that follow it. Always
/// closed: if the last segment does not end at `start`, a straight closing
/// segment is implied.
#[derive(Clone, Debug, PartialEq)]
pub struct Contour {
    pub start: [f32; 2],
    pub segments: Vec<PathSegment>,
}

impl Contour {
    /// Whether at least one segment has non-zero length.
    fn has_extent(&self) -> bool {
        let mut from = self.start;
        for seg in &self.segments {
            if !seg.is_degenerate_from(from) {
                return true;
            }
            from = seg.end();
        }
        false
    }
}

/// A validated, closed, multi-contour path, filled by the **even-odd** rule:
/// two overlapping contours in ONE path cancel where they overlap (draw
/// overlapping parts as separate instances to union them).
///
/// Any contour inside another is a hole, whichever direction either was
/// drawn in. Built with [`PathBuilder`]; filled with
/// [`crate::DrawList::push_path_fill`].
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    contours: Vec<Contour>,
}

impl Path {
    /// The contours, in the order they were built.
    pub fn contours(&self) -> &[Contour] {
        &self.contours
    }

    /// Axis-aligned bound of every point, control points included (a bound
    /// on the ink, not its tight box), as `(pos, size)`.
    pub fn bounds(&self) -> ([f32; 2], [f32; 2]) {
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        for contour in &self.contours {
            let points = std::iter::once(contour.start)
                .chain(contour.segments.iter().flat_map(|s| s.points()));
            for p in points {
                for axis in 0..2 {
                    min[axis] = min[axis].min(p[axis]);
                    max[axis] = max[axis].max(p[axis]);
                }
            }
        }
        (min, [max[0] - min[0], max[1] - min[1]])
    }

    /// Apply a point map to every point, controls included. Exact for any
    /// affine map (Béziers are affine-invariant), which is what an icon
    /// drawn on a design grid (e.g. 24x24) and placed into a rect needs:
    /// `path.mapped(|[x, y]| [rx + x * s, ry + y * s])`.
    pub fn mapped(&self, f: impl Fn([f32; 2]) -> [f32; 2]) -> Path {
        let mut out = self.clone();
        for contour in &mut out.contours {
            contour.start = f(contour.start);
            for seg in &mut contour.segments {
                for p in seg.points_mut() {
                    *p = f(*p);
                }
            }
        }
        out
    }
}

/// Why a path could not be built or filled.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PathError {
    /// No contour with at least one non-degenerate segment.
    Empty,
    /// A segment was added with no move_to before it (including after close()).
    NoCurrentPoint,
    /// A point was NaN or infinite.
    NonFinite,
    /// After lowering the path needs more GPU segments than
    /// [`PATH_MAX_SEGMENTS`]. Nothing was pushed.
    TooManySegments { needed: u32, max: u32 },
}

impl std::fmt::Display for PathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathError::Empty => write!(f, "path has no contour with extent"),
            PathError::NoCurrentPoint => write!(f, "path segment added before move_to"),
            PathError::NonFinite => write!(f, "path point is NaN or infinite"),
            PathError::TooManySegments { needed, max } => {
                write!(f, "path lowers to {needed} GPU segments, more than {max}")
            }
        }
    }
}

impl std::error::Error for PathError {}

/// Builds a [`Path`]. Methods take `&mut self`, so contours can be built in
/// loops (gear teeth) as well as in chains. Errors are collected and
/// reported by [`PathBuilder::build`], never by panicking.
#[derive(Clone, Debug, Default)]
pub struct PathBuilder {
    contours: Vec<Contour>,
    open: Option<Contour>,
    error: Option<PathError>,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    fn fail(&mut self, error: PathError) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }

    fn check(&mut self, points: &[[f32; 2]]) -> bool {
        if points.iter().flatten().all(|v| v.is_finite()) {
            true
        } else {
            self.fail(PathError::NonFinite);
            false
        }
    }

    fn segment(&mut self, seg: PathSegment) -> &mut Self {
        if !self.check(&seg.points()) {
            return self;
        }
        match &mut self.open {
            Some(contour) => contour.segments.push(seg),
            None => self.fail(PathError::NoCurrentPoint),
        }
        self
    }

    /// Start a new contour at `p`, implicitly closing the open one.
    pub fn move_to(&mut self, p: [f32; 2]) -> &mut Self {
        if let Some(open) = self.open.take() {
            self.contours.push(open);
        }
        if self.check(&[p]) {
            self.open = Some(Contour {
                start: p,
                segments: Vec::new(),
            });
        }
        self
    }

    pub fn line_to(&mut self, p: [f32; 2]) -> &mut Self {
        self.segment(PathSegment::Line { to: p })
    }

    pub fn quad_to(&mut self, ctrl: [f32; 2], p: [f32; 2]) -> &mut Self {
        self.segment(PathSegment::Quad { ctrl, to: p })
    }

    pub fn cubic_to(&mut self, c1: [f32; 2], c2: [f32; 2], p: [f32; 2]) -> &mut Self {
        self.segment(PathSegment::Cubic { c1, c2, to: p })
    }

    /// Close the open contour (adds the closing line if it is not already
    /// at the start). The next segment needs a move_to first.
    pub fn close(&mut self) -> &mut Self {
        if let Some(mut open) = self.open.take() {
            let last = open.segments.last().map_or(open.start, |s| s.end());
            if last != open.start {
                open.segments.push(PathSegment::Line { to: open.start });
            }
            self.contours.push(open);
        }
        self
    }

    /// The path built so far (the open contour included, implicitly
    /// closed). Contours whose segments are all zero length are dropped.
    pub fn build(&self) -> Result<Path, PathError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let contours: Vec<Contour> = self
            .contours
            .iter()
            .chain(self.open.iter())
            .filter(|c| c.has_extent())
            .cloned()
            .collect();
        if contours.is_empty() {
            return Err(PathError::Empty);
        }
        Ok(Path { contours })
    }
}

/// Most GPU segments one filled path may lower to. A path that needs more is
/// refused ([`PathError::TooManySegments`]) rather than drawn in part:
/// dropping segments from an even-odd path inverts parts of it.
pub const PATH_MAX_SEGMENTS: u32 = 256;

/// Largest distance, in logical pixels, between a cubic and the quadratics
/// that replace it (for cubics within the [`PATH_MAX_QUADS_PER_CUBIC`] cap).
pub const PATH_CUBIC_TOLERANCE: f32 = 1.0 / 64.0;

/// Most quadratics one cubic is split into.
pub const PATH_MAX_QUADS_PER_CUBIC: u32 = 16;

/// A quadratic that deviates from its chord by less than this many logical
/// pixels is lowered as a line.
pub const PATH_LINE_DEMOTE: f32 = 1.0 / 256.0;

/// One GPU segment: a line (`kind == 0`, from p0 to p2; `ctrl` is set to the
/// midpoint so the three-point hull is still the segment's bound) or a
/// y-monotone quadratic (`kind == 1`, p0 -> ctrl -> p2). In the aux param
/// bank it is the two vec4s `[p0.x, p0.y, ctrl.x, ctrl.y]`,
/// `[p2.x, p2.y, kind, 0]`.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuPathSegment {
    pub p0: [f32; 2],
    pub ctrl: [f32; 2],
    pub p2: [f32; 2],
    pub kind: f32,
    pub _pad: f32,
}

impl GpuPathSegment {
    /// `kind` of a line.
    pub const LINE: f32 = 0.0;
    /// `kind` of a y-monotone quadratic.
    pub const QUAD: f32 = 1.0;

    fn line(p0: [f32; 2], p2: [f32; 2]) -> Self {
        Self {
            p0,
            ctrl: [(p0[0] + p2[0]) * 0.5, (p0[1] + p2[1]) * 0.5],
            p2,
            kind: Self::LINE,
            _pad: 0.0,
        }
    }

    fn quad(p0: [f32; 2], ctrl: [f32; 2], p2: [f32; 2]) -> Self {
        Self {
            p0,
            ctrl,
            p2,
            kind: Self::QUAD,
            _pad: 0.0,
        }
    }

    /// The segment as its two param-bank rows.
    pub fn to_rows(self) -> [[f32; 4]; 2] {
        [
            [self.p0[0], self.p0[1], self.ctrl[0], self.ctrl[1]],
            [self.p2[0], self.p2[1], self.kind, 0.0],
        ]
    }

    /// Move all three points by `(dx, dy)`.
    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        for p in [&mut self.p0, &mut self.ctrl, &mut self.p2] {
            p[0] += dx;
            p[1] += dy;
        }
    }
}

fn lerp(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// The cubic's blossom `B(a, b, c)`: `B(t, t, t)` is the point at `t`, and
/// `B(t0,t0,t0), B(t0,t0,t1), B(t0,t1,t1), B(t1,t1,t1)` are the control
/// points of the piece from `t0` to `t1`.
fn blossom(p: &[[f32; 2]; 4], a: f32, b: f32, c: f32) -> [f32; 2] {
    let l1 = [
        lerp(p[0], p[1], a),
        lerp(p[1], p[2], a),
        lerp(p[2], p[3], a),
    ];
    let l2 = [lerp(l1[0], l1[1], b), lerp(l1[1], l1[2], b)];
    lerp(l2[0], l2[1], c)
}

/// How many uniform pieces a cubic is split into so each piece's
/// mid-point quadratic is within [`PATH_CUBIC_TOLERANCE`] (plan 3.3 step 2).
pub(crate) fn cubic_quad_count(p0: [f32; 2], c1: [f32; 2], c2: [f32; 2], p3: [f32; 2]) -> u32 {
    let dx = p3[0] - 3.0 * c2[0] + 3.0 * c1[0] - p0[0];
    let dy = p3[1] - 3.0 * c2[1] + 3.0 * c1[1] - p0[1];
    let err = 3.0f32.sqrt() / 36.0 * (dx * dx + dy * dy).sqrt();
    let n = (err / PATH_CUBIC_TOLERANCE).cbrt().ceil();
    (n as u32).clamp(1, PATH_MAX_QUADS_PER_CUBIC)
}

struct Lowering {
    out: Vec<GpuPathSegment>,
}

impl Lowering {
    fn line(&mut self, p0: [f32; 2], p2: [f32; 2]) {
        if p0 != p2 {
            self.out.push(GpuPathSegment::line(p0, p2));
        }
    }

    /// A quadratic already known to be y-monotone: demoted to a line when
    /// near-straight (plan 3.3 step 3).
    fn monotone_quad(&mut self, p0: [f32; 2], c: [f32; 2], p2: [f32; 2]) {
        let sx = p0[0] - 2.0 * c[0] + p2[0];
        let sy = p0[1] - 2.0 * c[1] + p2[1];
        if (sx * sx + sy * sy).sqrt() * 0.25 < PATH_LINE_DEMOTE {
            self.line(p0, p2);
        } else {
            self.out.push(GpuPathSegment::quad(p0, c, p2));
        }
    }

    /// Any quadratic: split at its y-extremum when that is strictly inside
    /// (0, 1), so every emitted quadratic is y-monotone (plan 3.3 step 4).
    fn quad(&mut self, p0: [f32; 2], c: [f32; 2], p2: [f32; 2]) {
        // Demote first (step 3), so a near-straight quadratic with a shallow
        // y-extremum becomes one line rather than two.
        let sx = p0[0] - 2.0 * c[0] + p2[0];
        let sy = p0[1] - 2.0 * c[1] + p2[1];
        if (sx * sx + sy * sy).sqrt() * 0.25 < PATH_LINE_DEMOTE {
            self.line(p0, p2);
            return;
        }
        let denom = p0[1] - 2.0 * c[1] + p2[1];
        if denom != 0.0 {
            let t = (p0[1] - c[1]) / denom;
            if t > 0.0 && t < 1.0 {
                let q0 = lerp(p0, c, t);
                let q1 = lerp(c, p2, t);
                let m = lerp(q0, q1, t);
                // At the extremum both new controls are level with the split
                // point; say so exactly, so rounding cannot leave either half
                // a hair non-monotone.
                self.monotone_quad(p0, [q0[0], m[1]], m);
                self.monotone_quad(m, [q1[0], m[1]], p2);
                return;
            }
        }
        self.monotone_quad(p0, c, p2);
    }

    fn cubic(&mut self, p0: [f32; 2], c1: [f32; 2], c2: [f32; 2], p3: [f32; 2]) {
        let n = cubic_quad_count(p0, c1, c2, p3);
        let pts = [p0, c1, c2, p3];
        let mut start = p0;
        for i in 0..n {
            let t0 = i as f32 / n as f32;
            let t1 = (i + 1) as f32 / n as f32;
            let a1 = blossom(&pts, t0, t0, t1);
            let a2 = blossom(&pts, t0, t1, t1);
            let end = if i + 1 == n {
                p3
            } else {
                blossom(&pts, t1, t1, t1)
            };
            let ctrl = [
                (3.0 * (a1[0] + a2[0]) - (start[0] + end[0])) * 0.25,
                (3.0 * (a1[1] + a2[1]) - (start[1] + end[1])) * 0.25,
            ];
            self.quad(start, ctrl, end);
            start = end;
        }
    }
}

/// A lowered path: its GPU segments and their bound `(pos, size)`.
pub(crate) type LoweredPath = (Vec<GpuPathSegment>, [f32; 2], [f32; 2]);

/// Lower `path` to GPU segments (plan 3.3): cubics to quadratics within
/// [`PATH_CUBIC_TOLERANCE`], near-straight quadratics to lines, quadratics
/// split to be y-monotone, degenerate segments dropped, each contour closed.
/// Returns the segments and their control-hull bound as `(pos, size)`.
pub(crate) fn lower_path(path: &Path) -> Result<LoweredPath, PathError> {
    let mut l = Lowering { out: Vec::new() };
    for contour in path.contours() {
        let mut cur = contour.start;
        for seg in &contour.segments {
            match *seg {
                PathSegment::Line { to } => l.line(cur, to),
                PathSegment::Quad { ctrl, to } => l.quad(cur, ctrl, to),
                PathSegment::Cubic { c1, c2, to } => l.cubic(cur, c1, c2, to),
            }
            cur = seg.end();
        }
        l.line(cur, contour.start);
    }
    let segments = l.out;
    if segments.is_empty() {
        return Err(PathError::Empty);
    }
    if segments.len() > PATH_MAX_SEGMENTS as usize {
        return Err(PathError::TooManySegments {
            needed: segments.len() as u32,
            max: PATH_MAX_SEGMENTS,
        });
    }
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for s in &segments {
        for p in [s.p0, s.ctrl, s.p2] {
            for axis in 0..2 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
    }
    Ok((segments, min, [max[0] - min[0], max[1] - min[1]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chained_path_builds() {
        let path = PathBuilder::new()
            .move_to([4.0, 14.0])
            .cubic_to([7.0, 6.0], [13.0, 3.0], [20.0, 2.0])
            .cubic_to([18.0, 9.0], [14.0, 13.0], [8.0, 16.0])
            .close()
            .build()
            .unwrap();
        assert_eq!(path.contours().len(), 1);
        // Two cubics and the closing heel.
        assert_eq!(path.contours()[0].segments.len(), 3);
        assert_eq!(
            path.contours()[0].segments[2],
            PathSegment::Line { to: [4.0, 14.0] }
        );
    }

    #[test]
    fn a_loop_built_path_builds() {
        let mut b = PathBuilder::new();
        for i in 0..3 {
            let x = i as f32 * 10.0;
            b.move_to([x, 0.0]);
            b.line_to([x + 5.0, 0.0]);
            b.quad_to([x + 5.0, 5.0], [x, 5.0]);
            b.close();
        }
        let path = b.build().unwrap();
        assert_eq!(path.contours().len(), 3);
        assert!(path.contours().iter().all(|c| c.segments.len() == 3));
    }

    #[test]
    fn close_adds_no_line_when_already_at_the_start() {
        let path = PathBuilder::new()
            .move_to([0.0, 0.0])
            .line_to([10.0, 0.0])
            .line_to([10.0, 10.0])
            .line_to([0.0, 0.0])
            .close()
            .build()
            .unwrap();
        assert_eq!(path.contours()[0].segments.len(), 3);
    }

    #[test]
    fn line_to_before_move_to_is_no_current_point() {
        let err = PathBuilder::new().line_to([1.0, 1.0]).build();
        assert_eq!(err, Err(PathError::NoCurrentPoint));
    }

    #[test]
    fn a_segment_after_close_needs_a_move_to() {
        let err = PathBuilder::new()
            .move_to([0.0, 0.0])
            .line_to([10.0, 0.0])
            .line_to([10.0, 10.0])
            .close()
            .line_to([5.0, 5.0])
            .build();
        assert_eq!(err, Err(PathError::NoCurrentPoint));
    }

    #[test]
    fn nan_is_non_finite() {
        let err = PathBuilder::new()
            .move_to([0.0, 0.0])
            .line_to([f32::NAN, 1.0])
            .line_to([1.0, 1.0])
            .build();
        assert_eq!(err, Err(PathError::NonFinite));
        let err = PathBuilder::new()
            .move_to([f32::INFINITY, 0.0])
            .line_to([1.0, 1.0])
            .build();
        assert_eq!(err, Err(PathError::NonFinite));
    }

    #[test]
    fn an_empty_builder_is_empty() {
        assert_eq!(PathBuilder::new().build(), Err(PathError::Empty));
        assert_eq!(
            PathBuilder::new().move_to([1.0, 1.0]).build(),
            Err(PathError::Empty)
        );
    }

    #[test]
    fn an_all_zero_length_contour_is_dropped() {
        assert_eq!(
            PathBuilder::new()
                .move_to([1.0, 1.0])
                .line_to([1.0, 1.0])
                .cubic_to([1.0, 1.0], [1.0, 1.0], [1.0, 1.0])
                .close()
                .build(),
            Err(PathError::Empty)
        );
        let path = PathBuilder::new()
            .move_to([1.0, 1.0])
            .line_to([1.0, 1.0])
            .move_to([0.0, 0.0])
            .line_to([4.0, 0.0])
            .line_to([0.0, 4.0])
            .build()
            .unwrap();
        assert_eq!(path.contours().len(), 1);
        assert_eq!(path.contours()[0].start, [0.0, 0.0]);
    }

    #[test]
    fn move_to_closes_the_previous_contour() {
        let path = PathBuilder::new()
            .move_to([0.0, 0.0])
            .line_to([10.0, 0.0])
            .line_to([10.0, 10.0])
            .move_to([20.0, 0.0])
            .line_to([30.0, 0.0])
            .line_to([30.0, 10.0])
            .build()
            .unwrap();
        assert_eq!(path.contours().len(), 2);
        assert_eq!(path.contours()[0].segments.len(), 2);
        assert_eq!(path.contours()[1].start, [20.0, 0.0]);
    }

    #[test]
    fn mapped_moves_every_point_including_controls() {
        let path = PathBuilder::new()
            .move_to([0.0, 0.0])
            .quad_to([12.0, -4.0], [24.0, 0.0])
            .cubic_to([24.0, 8.0], [12.0, 24.0], [0.0, 24.0])
            .close()
            .build()
            .unwrap();
        let m = path.mapped(|[x, y]| [100.0 + x * 2.0, 50.0 + y * 2.0]);
        let c = &m.contours()[0];
        assert_eq!(c.start, [100.0, 50.0]);
        assert_eq!(
            c.segments[0],
            PathSegment::Quad {
                ctrl: [124.0, 42.0],
                to: [148.0, 50.0]
            }
        );
        assert_eq!(
            c.segments[1],
            PathSegment::Cubic {
                c1: [148.0, 66.0],
                c2: [124.0, 98.0],
                to: [100.0, 98.0]
            }
        );
        assert_eq!(c.segments[2], PathSegment::Line { to: [100.0, 50.0] });
        assert_eq!(m.bounds(), ([100.0, 42.0], [48.0, 56.0]));
    }

    #[test]
    fn bounds_include_control_points() {
        let path = PathBuilder::new()
            .move_to([0.0, 0.0])
            .quad_to([5.0, -10.0], [10.0, 0.0])
            .build()
            .unwrap();
        assert_eq!(path.bounds(), ([0.0, -10.0], [10.0, 10.0]));
    }

    // ── Lowering (plan 3.3, step 3) ──────────────────────────────────────

    fn cubic_at(p: [[f32; 2]; 4], t: f32) -> [f32; 2] {
        crate::core::sdf::cubic_point(p[0], p[1], p[2], p[3], t)
    }

    fn quad_at(s: &GpuPathSegment, t: f32) -> [f32; 2] {
        let u = 1.0 - t;
        [
            u * u * s.p0[0] + 2.0 * u * t * s.ctrl[0] + t * t * s.p2[0],
            u * u * s.p0[1] + 2.0 * u * t * s.ctrl[1] + t * t * s.p2[1],
        ]
    }

    fn seg_dist(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
        let (bx, by) = (b[0] - a[0], b[1] - a[1]);
        let (px, py) = (p[0] - a[0], p[1] - a[1]);
        let h = ((px * bx + py * by) / (bx * bx + by * by).max(1e-12)).clamp(0.0, 1.0);
        ((px - bx * h).powi(2) + (py - by * h).powi(2)).sqrt()
    }

    /// Distance from `p` to the lowered segments, each densely sampled.
    fn dist_to_segments(p: [f32; 2], segs: &[GpuPathSegment]) -> f32 {
        let mut d = f32::INFINITY;
        for s in segs {
            const N: usize = 400;
            let mut prev = s.p0;
            for i in 1..=N {
                let q = quad_at(s, i as f32 / N as f32);
                d = d.min(seg_dist(p, prev, q));
                prev = q;
            }
        }
        d
    }

    fn one_cubic(p: [[f32; 2]; 4]) -> Vec<GpuPathSegment> {
        let path = PathBuilder::new()
            .move_to(p[0])
            .cubic_to(p[1], p[2], p[3])
            .build()
            .unwrap();
        let (segs, _, _) = lower_path(&path).unwrap();
        // Drop the implicit closing line (the last segment).
        segs[..segs.len() - 1].to_vec()
    }

    const K: f32 = 0.552_284_8;

    fn quarter_circle(r: f32) -> [[f32; 2]; 4] {
        [[r, 0.0], [r, r * K], [r * K, r], [0.0, r]]
    }

    #[test]
    fn quads_stay_within_the_cubic_tolerance() {
        let cubics: [(&str, [[f32; 2]; 4]); 6] = [
            ("quarter r10", quarter_circle(10.0)),
            ("quarter r100", quarter_circle(100.0)),
            (
                "s-curve",
                [[0.0, 0.0], [80.0, -60.0], [20.0, 160.0], [100.0, 100.0]],
            ),
            (
                "cusp",
                [[0.0, 0.0], [100.0, 100.0], [0.0, 100.0], [100.0, 0.0]],
            ),
            (
                "loop",
                [[0.0, 0.0], [150.0, 100.0], [-50.0, 100.0], [100.0, 0.0]],
            ),
            (
                "1000 px swoop",
                [[0.0, 0.0], [400.0, -300.0], [600.0, -300.0], [1000.0, 0.0]],
            ),
        ];
        for (name, c) in cubics {
            let segs = one_cubic(c);
            let mut worst = 0.0f32;
            for i in 0..=1000 {
                let p = cubic_at(c, i as f32 / 1000.0);
                worst = worst.max(dist_to_segments(p, &segs));
            }
            assert!(
                worst <= PATH_CUBIC_TOLERANCE,
                "{name}: {worst} px from the cubic ({} segments)",
                segs.len()
            );
        }
    }

    #[test]
    fn quarter_circles_need_three_and_six_quads() {
        let [a, b, c, d] = quarter_circle(10.0);
        assert_eq!(cubic_quad_count(a, b, c, d), 3);
        let [a, b, c, d] = quarter_circle(100.0);
        assert_eq!(cubic_quad_count(a, b, c, d), 6);
    }

    #[test]
    fn every_emitted_quadratic_is_y_monotone() {
        let path = PathBuilder::new()
            .move_to([0.0, 0.0])
            .cubic_to([80.0, -60.0], [20.0, 160.0], [100.0, 100.0])
            .quad_to([50.0, 200.0], [0.0, 100.0])
            .cubic_to([150.0, 100.0], [-50.0, 0.0], [0.0, 0.0])
            .build()
            .unwrap();
        let (segs, _, _) = lower_path(&path).unwrap();
        let quads: Vec<_> = segs
            .iter()
            .filter(|s| s.kind == GpuPathSegment::QUAD)
            .collect();
        assert!(quads.len() > 4);
        for s in quads {
            let a = s.ctrl[1] - s.p0[1];
            let b = s.p2[1] - s.ctrl[1];
            assert!(a * b >= 0.0, "non-monotone quad {s:?}");
        }
        // The bump quad (y 100 -> 200 -> 100) is split at its extremum.
        assert!(segs.iter().any(|s| s.p2 == [50.0, 150.0]));
    }

    #[test]
    fn near_straight_quadratics_come_out_as_lines() {
        let path = PathBuilder::new()
            .move_to([0.0, 0.0])
            .quad_to([50.0, 0.001], [100.0, 0.0])
            .line_to([100.0, 50.0])
            .build()
            .unwrap();
        let (segs, _, _) = lower_path(&path).unwrap();
        assert_eq!(segs[0], GpuPathSegment::line([0.0, 0.0], [100.0, 0.0]));
        assert!(segs.iter().all(|s| s.kind == GpuPathSegment::LINE));
    }

    #[test]
    fn implicit_closing_adds_one_line_only_when_needed() {
        let open = PathBuilder::new()
            .move_to([0.0, 0.0])
            .line_to([10.0, 0.0])
            .line_to([10.0, 10.0])
            .build()
            .unwrap();
        let (segs, pos, size) = lower_path(&open).unwrap();
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[2], GpuPathSegment::line([10.0, 10.0], [0.0, 0.0]));
        assert_eq!((pos, size), ([0.0, 0.0], [10.0, 10.0]));

        let closed = PathBuilder::new()
            .move_to([0.0, 0.0])
            .line_to([10.0, 0.0])
            .line_to([10.0, 10.0])
            .line_to([0.0, 0.0])
            .build()
            .unwrap();
        assert_eq!(lower_path(&closed).unwrap().0.len(), 3);
    }

    fn polygon(n: usize) -> Path {
        let mut b = PathBuilder::new();
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            let p = [500.0 + 400.0 * a.cos(), 500.0 + 400.0 * a.sin()];
            if i == 0 {
                b.move_to(p);
            } else {
                b.line_to(p);
            }
        }
        b.build().unwrap()
    }

    #[test]
    fn the_segment_limit_is_256() {
        assert_eq!(lower_path(&polygon(256)).unwrap().0.len(), 256);
        assert_eq!(
            lower_path(&polygon(257)),
            Err(PathError::TooManySegments {
                needed: 257,
                max: PATH_MAX_SEGMENTS
            })
        );
    }

    #[test]
    fn a_line_carries_its_midpoint_as_control() {
        let s = GpuPathSegment::line([0.0, 0.0], [10.0, 4.0]);
        assert_eq!(s.ctrl, [5.0, 2.0]);
        assert_eq!(
            s.to_rows(),
            [[0.0, 0.0, 5.0, 2.0], [10.0, 4.0, GpuPathSegment::LINE, 0.0]]
        );
    }
}
