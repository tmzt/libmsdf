//! Glyph outline extraction + edge coloring — the CPU-side front half of the
//! runtime compute-shader MSDF generator (`gpu::MsdfCompute`).
//!
//! Uses only ttf-parser (wasm-clean). Edges are kept in font units, y-up;
//! the projection into the atlas cell happens on the GPU using the same
//! `GlyphProjection` numbers as the CPU msdfgen bake.
//!
//! Edge coloring follows msdfgen's `edgeColoringSimple` in spirit: contours
//! without corners are white (all channels); contours with corners split
//! into spans at the corners, adjacent spans (including around the contour
//! seam) getting different two-channel colors. Exact color assignment
//! differs from msdfgen — only the *median* of the three channels is
//! contract, which is what the comparison test checks.

/// Channel masks (bit0 = R, bit1 = G, bit2 = B).
pub const COLOR_YELLOW: u32 = 0b011;
pub const COLOR_MAGENTA: u32 = 0b101;
pub const COLOR_CYAN: u32 = 0b110;
pub const COLOR_WHITE: u32 = 0b111;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    /// p0 → p1
    Line,
    /// p0 → p2, control p1
    Quad,
    /// p0 → p3, controls p1, p2
    Cubic,
}

/// One outline edge in font units (y-up). Unused point slots are zero.
#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub kind: EdgeKind,
    /// Channel mask (COLOR_*).
    pub color: u32,
    pub pts: [[f32; 2]; 4],
}

impl Edge {
    /// End point of the edge.
    pub fn end(&self) -> [f32; 2] {
        match self.kind {
            EdgeKind::Line => self.pts[1],
            EdgeKind::Quad => self.pts[2],
            EdgeKind::Cubic => self.pts[3],
        }
    }

    /// Direction at the start of the edge (unnormalized, with degenerate
    /// fallbacks like msdfgen).
    pub fn dir_start(&self) -> [f32; 2] {
        let p0 = self.pts[0];
        let candidates: &[[f32; 2]] = match self.kind {
            EdgeKind::Line => &[self.pts[1]],
            EdgeKind::Quad => &[self.pts[1], self.pts[2]],
            EdgeKind::Cubic => &[self.pts[1], self.pts[2], self.pts[3]],
        };
        for c in candidates {
            let d = [c[0] - p0[0], c[1] - p0[1]];
            if d[0] != 0.0 || d[1] != 0.0 {
                return d;
            }
        }
        [1.0, 0.0]
    }

    /// Direction at the end of the edge.
    pub fn dir_end(&self) -> [f32; 2] {
        let e = self.end();
        let candidates: &[[f32; 2]] = match self.kind {
            EdgeKind::Line => &[self.pts[0]],
            EdgeKind::Quad => &[self.pts[1], self.pts[0]],
            EdgeKind::Cubic => &[self.pts[2], self.pts[1], self.pts[0]],
        };
        for c in candidates {
            let d = [e[0] - c[0], e[1] - c[1]];
            if d[0] != 0.0 || d[1] != 0.0 {
                return d;
            }
        }
        [1.0, 0.0]
    }
}

/// A glyph outline as a colored edge list, grouped into contours.
#[derive(Debug, Clone, Default)]
pub struct GlyphOutline {
    pub edges: Vec<Edge>,
    /// Half-open ranges into `edges`, one per contour.
    pub contours: Vec<std::ops::Range<usize>>,
}

struct OutlineSink {
    outline: GlyphOutline,
    start: [f32; 2],
    cur: [f32; 2],
    contour_start_idx: usize,
}

impl OutlineSink {
    fn new() -> Self {
        Self {
            outline: GlyphOutline::default(),
            start: [0.0, 0.0],
            cur: [0.0, 0.0],
            contour_start_idx: 0,
        }
    }

    fn push(&mut self, kind: EdgeKind, pts: [[f32; 2]; 4]) {
        self.outline.edges.push(Edge {
            kind,
            color: COLOR_WHITE,
            pts,
        });
    }

    fn end_contour(&mut self) {
        // Implicit closing line back to the contour start.
        if self.cur != self.start {
            let (s, c) = (self.start, self.cur);
            self.push(EdgeKind::Line, [c, s, [0.0; 2], [0.0; 2]]);
            self.cur = s;
        }
        let end = self.outline.edges.len();
        if end > self.contour_start_idx {
            self.outline.contours.push(self.contour_start_idx..end);
        }
        self.contour_start_idx = end;
    }
}

impl ttf_parser::OutlineBuilder for OutlineSink {
    fn move_to(&mut self, x: f32, y: f32) {
        self.start = [x, y];
        self.cur = [x, y];
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let c = self.cur;
        self.push(EdgeKind::Line, [c, [x, y], [0.0; 2], [0.0; 2]]);
        self.cur = [x, y];
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let c = self.cur;
        self.push(EdgeKind::Quad, [c, [x1, y1], [x, y], [0.0; 2]]);
        self.cur = [x, y];
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let c = self.cur;
        self.push(EdgeKind::Cubic, [c, [x1, y1], [x2, y2], [x, y]]);
        self.cur = [x, y];
    }

    fn close(&mut self) {
        self.end_contour();
    }
}

/// Extract a glyph's outline as a colored edge list. Returns None for
/// glyphs without outlines (whitespace).
pub fn extract_outline(face: &ttf_parser::Face, glyph_id: u16) -> Option<GlyphOutline> {
    let mut sink = OutlineSink::new();
    face.outline_glyph(ttf_parser::GlyphId(glyph_id), &mut sink)?;
    // ttf-parser calls close() per contour, but guard a trailing open one.
    sink.end_contour();
    let mut outline = sink.outline;
    if outline.edges.is_empty() {
        return None;
    }
    color_edges(&mut outline, 3.0);
    Some(outline)
}

fn normalize(v: [f32; 2]) -> [f32; 2] {
    let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if len <= 0.0 {
        [0.0, 0.0]
    } else {
        [v[0] / len, v[1] / len]
    }
}

fn cross(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}

fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

/// msdfgen-style corner test: directions form a corner when they oppose
/// (dot ≤ 0) or bend more than the angle threshold (|cross| > sin θ).
fn is_corner(a: [f32; 2], b: [f32; 2], sin_threshold: f32) -> bool {
    dot(a, b) <= 0.0 || cross(a, b).abs() > sin_threshold
}

/// Assign channel colors to edges (msdfgen `edgeColoringSimple` scheme).
/// `angle_threshold` is in radians (3.0 matches the CPU bake).
pub fn color_edges(outline: &mut GlyphOutline, angle_threshold: f32) {
    let sin_threshold = angle_threshold.sin().abs();
    let contours = outline.contours.clone();

    for contour in contours {
        let n = contour.len();
        if n == 0 {
            continue;
        }

        // Find corner edge-boundaries: index i is a corner when the turn
        // from edge (i-1) into edge i is sharp.
        let mut corners = Vec::new();
        for i in 0..n {
            let prev = &outline.edges[contour.start + (i + n - 1) % n];
            let next = &outline.edges[contour.start + i];
            let a = normalize(prev.dir_end());
            let b = normalize(next.dir_start());
            if is_corner(a, b, sin_threshold) {
                corners.push(i);
            }
        }

        match corners.len() {
            0 => {
                // Smooth contour: all channels.
                for i in 0..n {
                    outline.edges[contour.start + i].color = COLOR_WHITE;
                }
            }
            1 => {
                // "Teardrop": one corner — split the loop into three arcs so
                // the edges meeting at the corner get different colors.
                let colors = [COLOR_MAGENTA, COLOR_YELLOW, COLOR_CYAN];
                let c0 = corners[0];
                for k in 0..n {
                    let i = (c0 + k) % n;
                    let group = if n >= 3 { (3 * k) / n } else { k.min(2) };
                    outline.edges[contour.start + i].color = colors[group];
                }
            }
            _ => {
                // Spans between consecutive corners; adjacent spans (and the
                // first/last pair around the seam) must differ.
                let m = corners.len();
                for s in 0..m {
                    let from = corners[s];
                    let to = corners[(s + 1) % m];
                    let span_len = if s + 1 == m { n - from + to } else { to - from };
                    let color = if s + 1 == m && m % 2 == 1 {
                        COLOR_CYAN
                    } else if s % 2 == 0 {
                        COLOR_MAGENTA
                    } else {
                        COLOR_YELLOW
                    };
                    for k in 0..span_len {
                        let i = (from + k) % n;
                        outline.edges[contour.start + i].color = color;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roboto_face() -> ttf_parser::Face<'static> {
        ttf_parser::Face::parse(crate::font::ROBOTO_REGULAR_ASCII, 0).unwrap()
    }

    #[test]
    fn extracts_letter_outlines() {
        let face = roboto_face();
        let gid_a = face.glyph_index('A').unwrap().0;
        let outline = extract_outline(&face, gid_a).expect("A has an outline");
        // 'A' has an outer contour + counter (hole).
        assert!(outline.contours.len() >= 2, "A should have ≥2 contours");
        assert!(outline.edges.len() >= 6);
        // Every edge got a color with exactly 2 or 3 channels.
        for e in &outline.edges {
            let bits = e.color.count_ones();
            assert!(bits == 2 || bits == 3, "bad color mask {:#b}", e.color);
        }
    }

    #[test]
    fn corners_get_distinct_adjacent_colors() {
        let face = roboto_face();
        // 'H' is all straight lines and right angles — every edge boundary
        // is a corner, so adjacent edges must differ in color.
        let gid = face.glyph_index('H').unwrap().0;
        let outline = extract_outline(&face, gid).expect("H has an outline");
        for contour in &outline.contours {
            let n = contour.len();
            if n < 2 {
                continue;
            }
            for i in 0..n {
                let a = outline.edges[contour.start + i].color;
                let b = outline.edges[contour.start + (i + 1) % n].color;
                // Sharing at most one channel keeps the median crisp; equal
                // masks on adjacent corner edges would round the corner.
                assert_ne!(a, b, "adjacent corner edges share a color mask");
            }
        }
    }

    #[test]
    fn smooth_contour_is_white() {
        let face = roboto_face();
        // 'o' outer/inner contours are smooth curves → all white.
        let gid = face.glyph_index('o').unwrap().0;
        let outline = extract_outline(&face, gid).expect("o has an outline");
        let white = outline
            .edges
            .iter()
            .filter(|e| e.color == COLOR_WHITE)
            .count();
        assert!(
            white >= outline.edges.len() / 2,
            "smooth 'o' should be mostly white-colored edges ({white}/{})",
            outline.edges.len()
        );
    }

    #[test]
    fn whitespace_has_no_outline() {
        let face = roboto_face();
        let gid_space = face.glyph_index(' ').unwrap().0;
        assert!(extract_outline(&face, gid_space).is_none());
    }
}
