// Runtime MSDF generation: glyph outline → edge-list SSBO → compute pass
// writing packed RGBA8 texels for one atlas cell.
//
// The CPU msdfgen bake (font::atlas, native-only) is the reference
// implementation; this compute path is the runtime/wasm generator. Per
// channel it reports the pseudo-distance MAGNITUDE of the nearest
// same-colored edge (msdfgen's scheme: true-distance chooses the edge,
// pseudo-distance — the perpendicular distance to the endpoint tangent line
// beyond an edge's own span — supplies the value, which is what sharpens
// corners). Curves are flattened for the distance queries.
//
// The SIGN is authoritative from a nonzero-winding fill test rather than the
// per-edge local orientation: a single edge's local sign is unreliable in
// the shape interior, so all three channels take the winding sign and keep
// their own magnitude — the equivalent of msdfgen's correct_sign, and what
// makes the compute median track the CPU msdfgen median.
//
// Coordinates: edges are in font units, y-up (TrueType convention: filled
// area to the right of travel). cell_px = (shape + translate) * scale,
// matching font::atlas::GlyphProjection; output rows are top-down.

struct Params {
    scale: f32,       // font units → cell px
    range_px: f32,    // distance field range in output px
    cell: u32,        // cell size (gs); output is cell × cell
    edge_count: u32,
    tx: f32,          // translate (font units)
    ty: f32,
    row_stride: u32,  // texels per output row (256B-aligned for texture copy)
    _pad: u32,
}

struct Edge {
    kind: u32,        // 0 = line (p0→p1), 1 = quad (p0,c p1,→p2), 2 = cubic
    color: u32,       // channel mask: bit0 = R, bit1 = G, bit2 = B
    p0: vec2<f32>,
    p1: vec2<f32>,
    p2: vec2<f32>,
    p3: vec2<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> edges: array<Edge>;
@group(0) @binding(2) var<storage, read_write> out_pixels: array<u32>;

const QUAD_SEGS: u32 = 8u;
const CUBIC_SEGS: u32 = 16u;
const BIG: f32 = 1e30;

fn edge_point(e: Edge, t: f32) -> vec2<f32> {
    if e.kind == 0u {
        return mix(e.p0, e.p1, t);
    }
    if e.kind == 1u {
        let u = 1.0 - t;
        return u * u * e.p0 + 2.0 * u * t * e.p1 + t * t * e.p2;
    }
    let u = 1.0 - t;
    return u * u * u * e.p0 + 3.0 * u * u * t * e.p1 + 3.0 * u * t * t * e.p2 + t * t * t * e.p3;
}

fn seg_count(e: Edge) -> u32 {
    if e.kind == 0u { return 1u; }
    if e.kind == 1u { return QUAD_SEGS; }
    return CUBIC_SEGS;
}

fn cross2(a: vec2<f32>, b: vec2<f32>) -> f32 {
    return a.x * b.y - a.y * b.x;
}

// Result of one edge's distance query (both magnitudes unsigned; the sign
// is applied globally from the winding fill in main()).
struct EdgeDist {
    true_d: f32,    // unsigned true distance to the edge
    pseudo_d: f32,  // unsigned pseudo-distance magnitude (corner overshoot)
}

// Distance from p to edge ei, flattened. Pseudo-distance extends along the
// tangent line beyond the edge's own endpoints (only at the edge ends, not
// at interior flattening joints); elsewhere it equals the true distance.
fn edge_distance(ei: u32, p: vec2<f32>) -> EdgeDist {
    let e = edges[ei];
    let n = seg_count(e);

    var best_d2 = BIG;
    var clamped_start = false;
    var clamped_end = false;

    var prev = e.p0;
    for (var i = 1u; i <= n; i = i + 1u) {
        let t = f32(i) / f32(n);
        let q = edge_point(e, t);
        let ab = q - prev;
        let ap = p - prev;
        let denom = max(dot(ab, ab), 1e-12);
        let h = clamp(dot(ap, ab) / denom, 0.0, 1.0);
        let closest = prev + ab * h;
        let dv = p - closest;
        let d2 = dot(dv, dv);
        if d2 < best_d2 {
            best_d2 = d2;
            clamped_start = (i == 1u && h <= 0.0);
            clamped_end = (i == n && h >= 1.0);
        }
        prev = q;
    }

    let true_d = sqrt(best_d2);
    var pseudo = true_d;

    // Pseudo-distance: beyond the edge's endpoints, use the perpendicular
    // distance to the endpoint tangent line (always ≤ true_d — the overshoot
    // that lets a neighbouring channel keep the corner sharp).
    if clamped_start {
        let a = e.p0;
        let dir = normalize(edge_point(e, 1.0 / f32(n)) - a);
        pseudo = abs(cross2(dir, p - a));
    } else if clamped_end {
        let b = edge_point(e, 1.0);
        let dir = normalize(b - edge_point(e, 1.0 - 1.0 / f32(n)));
        pseudo = abs(cross2(dir, p - b));
    }

    return EdgeDist(true_d, pseudo);
}

// Winding contribution of edge ei for a +x ray from p (nonzero rule).
fn edge_winding(ei: u32, p: vec2<f32>) -> i32 {
    let e = edges[ei];
    let n = seg_count(e);
    var w = 0;
    var prev = e.p0;
    for (var i = 1u; i <= n; i = i + 1u) {
        let t = f32(i) / f32(n);
        let q = edge_point(e, t);
        let a_below = prev.y <= p.y;
        let b_below = q.y <= p.y;
        if a_below != b_below {
            let ft = (p.y - prev.y) / (q.y - prev.y);
            let x_int = prev.x + ft * (q.x - prev.x);
            if x_int > p.x {
                w = w + select(-1, 1, q.y > prev.y);
            }
        }
        prev = q;
    }
    return w;
}

fn to_u8(sd_shape: f32) -> u32 {
    // Mirror the CPU msdfgen bake exactly. msdfgen measures distance in SHAPE
    // (font) units and stores distance/range — Framing.range is a bare f64, so
    // msdfgen treats it as Range::Unit (shape units), and the projection scale
    // never enters the stored value. The CPU bake then maps that stored value
    // through msdf_to_u8: value * 0.5/px_range + 0.5. range == px_range ==
    // params.range_px, so the two divisions collapse to 0.5/range_px².
    let stored = sd_shape / params.range_px;
    let normalized = clamp(stored * 0.5 / params.range_px + 0.5, 0.0, 1.0);
    return u32(normalized * 255.0);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let cell = params.cell;
    if gid.x >= cell || gid.y >= cell {
        return;
    }

    // Output rows are top-down; shape space is y-up.
    let y_up = cell - 1u - gid.y;
    let px_center = vec2<f32>(f32(gid.x) + 0.5, f32(y_up) + 0.5);
    let p = px_center / params.scale - vec2<f32>(params.tx, params.ty);

    // Per channel: pseudo-distance magnitude of the edge nearest in TRUE
    // distance (msdfgen's edge selection). Magnitudes only — the sign comes
    // from winding below.
    var best_true = vec3<f32>(BIG, BIG, BIG);
    var best_pd = vec3<f32>(BIG, BIG, BIG);
    var global_true = BIG;
    var winding = 0;

    for (var ei = 0u; ei < params.edge_count; ei = ei + 1u) {
        let color = edges[ei].color;
        let ed = edge_distance(ei, p);
        global_true = min(global_true, ed.true_d);
        winding = winding + edge_winding(ei, p);

        if (color & 1u) != 0u && ed.true_d < best_true.x {
            best_true.x = ed.true_d;
            best_pd.x = ed.pseudo_d;
        }
        if (color & 2u) != 0u && ed.true_d < best_true.y {
            best_true.y = ed.true_d;
            best_pd.y = ed.pseudo_d;
        }
        if (color & 4u) != 0u && ed.true_d < best_true.z {
            best_true.z = ed.true_d;
            best_pd.z = ed.pseudo_d;
        }
    }

    // Authoritative inside/outside from the nonzero-winding fill rule, applied
    // as the sign of every channel (msdfgen's correct_sign equivalent). Each
    // channel keeps its own pseudo-distance magnitude, so the three cross the
    // 0.5 boundary at slightly different places and the median stays sharp at
    // corners while its sign matches the CPU baseline everywhere. Channels with
    // no same-colored edge fall back to the global true distance.
    let sign = select(-1.0, 1.0, winding != 0);
    let mx = select(global_true, best_pd.x, best_true.x < BIG);
    let my = select(global_true, best_pd.y, best_true.y < BIG);
    let mz = select(global_true, best_pd.z, best_true.z < BIG);
    let sd = vec3<f32>(sign * mx, sign * my, sign * mz);

    let r = to_u8(sd.x);
    let g = to_u8(sd.y);
    let b = to_u8(sd.z);
    let idx = gid.y * params.row_stride + gid.x;
    out_pixels[idx] = r | (g << 8u) | (b << 16u) | (255u << 24u);
}
