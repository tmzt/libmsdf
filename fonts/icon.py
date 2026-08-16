#!/usr/bin/env -S uv run --quiet --with fonttools python3
"""Add the repo's own UI ICON glyphs to a baked Highbay face.

**This, `marker.py`, `msymbols.py` and `widen.py` are how `fonts/*.ttf` are
made, and all four are additive by construction.** Existing glyphs are never
re-derived: this appends new outlines after every glyph already there, so no
glyph id moves and `GDEF`/`GPOS`/`GSUB` (kerning, the fi/fl/ffi/ffl ligatures)
are carried through untouched.

    ./icon.py <base.ttf> <out.ttf>

Run for BOTH faces (see `../src/font/mod.rs` for what each one is). These are
not Material Symbols and must never pretend to be: they carry no Material
name, they sit nowhere near Material's codepoints, and `msymbols_codepoint`
answers `None` for every one of them. They are OURS, in `HIGHBAY_ICONS`.

**Re-runnable, because the set grows.** A name already in the face is skipped
(its outline is the one that shipped, and re-deriving it is exactly what the
additive premise forbids); only names the face does not carry are appended. So
adding an icon is an edit to `ICONS` below plus one run per face, not a
rebuild of everything.

# Why a glyph rather than an `SdfKind`

The alternative on the table was a procedural shape - a new `SdfKind` arm
drawing the grid and the rows from Rust. That buys a new shader arm and a new
wire-format entry to redo work the text path already does, and it makes a UI
icon a thing the RENDERER knows the name of rather than a thing an app ASKS
for. As a glyph, `<Icon name="table">` resolves exactly as `name="settings"`
does - one lookup, one shaper, one atlas - and the only difference between the
two is which manifest owns the name.

# Where the outlines come from

Table and Props were ported from `draw_table_glyph` / `draw_props_glyph` in
`crates/highbay_ui/src/zui/toolbar.rs` (Tim, 2026-07-28) - the authored designs,
whose proportions were measured off those functions and rescaled onto the icon
grid. **Those functions are gone** (2026-08-16: the toolbar draws these as
glyphs now, so keeping a second hand-drawn copy was the drift the port existed
to end), which makes THIS FILE the authored design. The per-glyph docstrings
below record what each preserved and the one place a proportion was corrected;
read them as the source rather than as a summary of something else.

Graph is new, drawn to complete the trio. Screen is newer still, drawn when the
Screens pane needed a mark that could not be confused with a rename pencil.

# The design contract, and it is declared HERE

An icon is placed by the same code that places a letter, from the baked cell's
own metrics. Every icon glyph therefore obeys:

* `advance == UPEM` - one em, which is what the borrowed Material set uses, so
  an icon occupies the same advance box whichever vocabulary it came from;
* ink is centred on `(UPEM/2, UPEM/2)`, again as the Material set is, so a
  Highbay icon and a Material icon sit at the same optical height in a row;
* ink stays inside the CELL CEILING derived below, so the distance field is
  complete on every side.

`highbay_icon_contract_holds` in `../src/font/mod.rs`'s test suite asserts
exactly this against the shipped bytes.

# Sizing, and why the numbers are what they are

Cells are baked at 48px with `px_range` 6.0, from a 2048upem face, at
`em_scale = 48 / (2048 * 1.3)`; the cell's baseline row is `48*0.15 +
em_scale*cap_height`, about 33.4. Ink centred half an em ABOVE the baseline
therefore sits HIGH in the cell, and the binding constraint is the TOP edge -
the mirror image of `marker.py`, whose ink is centred ON the baseline and runs
out of room at the bottom. The field needs `px_range/2` = 3px of clear cell
above the ink or the top edge stops antialiasing, which puts the ceiling at
y = 1688 (`CEILING` below).

The live box is therefore 15 Material grid units tall rather than the 18-20 a
Material icon uses. The borrowed set is ALREADY over this line and it is not
close: `person` has 2.7px of headroom against the 3 it needs, `home` 1.1px,
and `settings`/`chat`/`library_books` are OVER by 0.4px - their ink reaches
the cell's top row, so the field above them does not exist and their top edge
is a hard cut rather than an antialiased one. Rather than copy that, these are
6% smaller than `person` and complete. Beside a Material icon the difference
in size is visible and the difference in the top edge would not be; the
judgement here is that a mark drawn 6% small is a smaller error than one drawn
with a shorn edge, and that the borrowed set's clipping is a defect to report
rather than a standard to match.
"""

import math
import sys

from fontTools.ttLib import TTFont
from fontTools.ttLib.tables._g_l_y_f import Glyph, GlyphCoordinates, flagOnCurve
from fontTools.ttLib.tables.ttProgram import Program

UPEM = 2048

# The repo-owned ICON block, mirrored by `HIGHBAY_ICONS_BLOCK` in
# `../src/font/mod.rs`: U+F800..U+F8EF, immediately below `MARKERS`
# (U+F8F0..U+F8FF). Together the two make the top 256 codepoints of the
# Private Use Area exactly the repo's own, so "is this glyph ours?" is one
# comparison. Allocating UPWARD from the base keeps the bake queue's ordering
# append-only for this block, as it does for the markers.
ICON_BASE = 0xF800
ICON_LIMIT = 0xF8EF

# Ink centre. Half an em above the baseline, matching the Material set.
CX = CY = UPEM // 2

# **The cell ceiling.** `em_scale` and `g_ty` reproduce
# `glyph_projection`/`default_cell_metrics` in `../src/font/atlas.rs`; the
# field needs `px_range/2` of clear cell above the ink.
GLYPH_PX, PX_RANGE, CAP_HEIGHT = 48.0, 6.0, 1456.0
_EM_SCALE = GLYPH_PX / (UPEM * 1.3)
_G_TY = (GLYPH_PX - GLYPH_PX * 0.15) / _EM_SCALE - CAP_HEIGHT
CEILING = int((GLYPH_PX - PX_RANGE / 2.0) / _EM_SCALE - _G_TY)

# The live box every icon in the trio is drawn inside: 16 x 15 grid units.
# Wider than tall because the Table glyph it is measured from is (its pixel
# original is 15x14), and the other two follow it so the set shares one
# optical footprint.
HALF_W, HALF_H = 683, 640

# Stroke weights, carried over as RATIOS from the pixel originals: the Table's
# frame is 1.4 of a 15-wide box, its header rule 2.0 and its dividers 1.2, so
# the hierarchy header > frame > divider is the authored one rather than a new
# opinion. 15 pixels map to 1366 units, i.e. ~91 units per authored pixel.
PX = (2 * HALF_W) / 15.0


def px(n):
    """`n` authored pixels of the toolbar original, in font units."""
    return round(n * PX)


def signed_area2(points):
    """Twice the shoelace area: negative is clockwise in a y-up frame."""
    total = 0
    for i, (x1, y1) in enumerate(points):
        x2, y2 = points[(i + 1) % len(points)]
        total += x1 * y2 - x2 * y1
    return total


# ── contour constructors ────────────────────────────────────────────────
#
# Each returns (points, flags) with `flagOnCurve` set per point, so a contour
# can mix on- and off-curve points (TrueType quadratics). Outer contours wind
# CW in the y-up frame and counters CCW, which is Roboto's own convention and
# what `build` re-checks.


def _wind(pts, flags, cw):
    """Round to integer units and orient the contour.

    Reversing a point list reverses the curve, and each off-curve point keeps
    the same two neighbours, so the flags reverse with the points.
    """
    pts = [(round(x), round(y)) for x, y in pts]
    if (signed_area2(pts) < 0) != cw:
        pts, flags = pts[::-1], flags[::-1]
    return pts, flags


def rect(x0, y0, x1, y1, cw=True):
    """An axis-aligned rectangle."""
    return _wind([(x0, y0), (x0, y1), (x1, y1), (x1, y0)], [flagOnCurve] * 4, cw)


def rounded_rect(x0, y0, x1, y1, r, cw=True):
    """A rectangle with `r`-radius corners, one quadratic arc per corner.

    A single quadratic with its control point AT the corner deviates from a
    true quarter-circle by 0.076*r at worst - 10 units at the 137-unit radius
    the Table uses, which is 0.08px at the 16px these are drawn down to.
    Splitting each corner in two would buy nothing a reader could see.
    """
    pts, flags = [], []
    # Corners in CCW order for a y-up frame - SW, SE, NE, NW - each as the
    # arc's (start, control, end); the straight edges are implied between one
    # corner's end and the next one's start.
    for start, ctrl, end in [
        ((x0, y0 + r), (x0, y0), (x0 + r, y0)),
        ((x1 - r, y0), (x1, y0), (x1, y0 + r)),
        ((x1, y1 - r), (x1, y1), (x1 - r, y1)),
        ((x0 + r, y1), (x0, y1), (x0, y1 - r)),
    ]:
        pts += [start, ctrl, end]
        flags += [flagOnCurve, 0, flagOnCurve]
    return _wind(pts, flags, cw)


def pill(x0, y0, x1, y1, cw=True):
    """A horizontal bar with semicircular ends - a round-capped stroke.

    The pixel originals draw their rows with `push_bezier`, whose line
    primitive is round-capped, so a butt-capped rectangle here would be a
    different mark from the one that was designed.
    """
    return rounded_rect(x0, y0, x1, y1, (y1 - y0) / 2.0, cw)


def circle(cx, cy, r, cw=True):
    """A circle as eight quadratic arcs.

    Off-curve points sit at `r/cos(22.5)` on the 22.5-degree bisectors, whose
    axis projections come back to exactly `r` - so the CONTROL-POINT bounding
    box, which is the one `recalcBounds` writes into `glyf`, is the true one.
    """
    k = r / math.cos(math.pi / 8)
    pts, flags = [], []
    for i in range(8):
        a, b = i * math.pi / 4, i * math.pi / 4 + math.pi / 8
        pts += [
            (cx + r * math.cos(a), cy + r * math.sin(a)),
            (cx + k * math.cos(b), cy + k * math.sin(b)),
        ]
        flags += [flagOnCurve, 0]
    return _wind(pts, flags, cw)


def bar(p, q, w, cw=True):
    """A `w`-wide rectangle from point `p` to point `q` - a graph edge."""
    dx, dy = q[0] - p[0], q[1] - p[1]
    n = math.hypot(dx, dy)
    ox, oy = -dy / n * w / 2.0, dx / n * w / 2.0
    pts = [
        (p[0] - ox, p[1] - oy),
        (q[0] - ox, q[1] - oy),
        (q[0] + ox, q[1] + oy),
        (p[0] + ox, p[1] + oy),
    ]
    return _wind(pts, [flagOnCurve] * 4, cw)


# ── the three glyphs ────────────────────────────────────────────────────

L, B = CX - HALF_W, CY - HALF_H  # 341, 384
R, T = CX + HALF_W, CY + HALF_H  # 1707, 1664


def table():
    """**Table**: a bordered grid with a heavier rule under the header row.

    Ported from the retired `draw_table_glyph` (see the module docstring).
    Everything was preserved except the outline:
    the pixel version STROKES a box and then draws the rules ACROSS it, which
    in a font would be overlapping contours. The same mark is expressed here
    as one rounded outer contour with the six CELLS cut out of it, so the
    frame and the rules are the gaps and no two contours overlap. The frame
    stroke, the heavier header rule and the thinner dividers keep their
    authored ratios (1.4 / 2.0 / 1.2 of a 15-wide box).

    The ONE correction: the pixel version puts its dividers at exact thirds of
    the OUTER box, which - because the outer thirds each lose a frame stroke -
    makes the middle column 27% wider than its neighbours. Here the INTERIOR
    is divided in three equal cells, which is what "a 3-column grid" draws.
    """
    frame, rule, divider, radius = px(1.4), px(2.0), px(1.2), px(1.5)

    # The header rule sits a third of the way down the outer box, as authored.
    rule_y = T - (T - B) / 3.0
    bands = [
        (round(rule_y + rule / 2), T - frame),  # header row
        (B + frame, round(rule_y - rule / 2)),  # body
    ]
    # Three equal interior cells with two dividers between them.
    x0, x1 = L + frame, R - frame
    cell_w = (x1 - x0 - 2 * divider) / 3.0
    cuts = []
    x = x0
    for i in range(3):
        cuts.append((round(x), round(x + cell_w)))
        x += cell_w + divider

    out = [rounded_rect(L, B, R, T, radius, cw=True)]
    for lo, hi in bands:
        for cx0, cx1 in cuts:
            out.append(rect(cx0, lo, cx1, hi, cw=False))
    return out


def props():
    """**Props**: three property-sheet rows, each a LABEL and a VALUE.

    Ported from the retired `draw_props_glyph`, proportions unchanged (half-width 6.5,
    stroke 1.6, rows 5.0 apart, a 3.0 label and a 3.0 gap - all in the
    original's pixels, rescaled by the same factor the Table is). The split
    row is the whole design and the reason it is not three plain bars: three
    equal bars is the hamburger mark, and at 28px a viewer reads the
    silhouette, not the intent.
    """
    half_w, t, step, label_w, gap = px(6.5), px(1.6), px(5.0), px(3.0), px(3.0)
    x0, x1 = CX - half_w, CX + half_w
    out = []
    for i in (-1, 0, 1):
        y = CY + i * step
        lo, hi = round(y - t / 2), round(y + t / 2)
        out.append(pill(x0, lo, x0 + label_w, hi))
        out.append(pill(x0 + label_w + gap, lo, x1, hi))
    return out


def graph():
    """**Graph**: three nodes joined by two edges - one above, two below.

    New, and drawn to complete the trio rather than ported. The constraint it
    is solving is not "be recognisable" but "be unmistakable BESIDE the other
    two at 16px", where a silhouette is all a viewer gets:

    * Table's silhouette is a filled RECTANGLE, Props' is horizontal STRIPES.
      This one is an open figure of DOTS AND DIAGONALS - a different primitive
      vocabulary from either, not a third arrangement of the same one.
    * The nodes are filled discs, not rings: a ring's counter is under a pixel
      at 16px and closes up, at which point the mark is three blobs and could
      be anything.
    * One parent over two children is the smallest arrangement that shows an
      EDGE RELATION rather than just several nodes, and it fills the live box
      to all four corners so the glyph's optical size matches the other two.

    The proportions were chosen by rendering, not by reasoning: at the first
    weights tried (a 4.5px disc on a 1.4px edge) the mark collapsed into a
    plain chevron at 16px - the edges carried the silhouette and the nodes
    read as nothing more than thickened ends. Discs of 5.5 authored pixels
    across, on edges at the Table's DIVIDER weight - the lightest stroke
    anywhere in the trio, because an edge is a connector and not a body -
    leaves 3px of visible edge between two 4px discs at 16px, which is where
    it starts reading as nodes-joined rather than as a caret.

    The discs and the edges DO overlap here, which the other two glyphs
    carefully avoid; msdfgen's sign correction resolves it (checked in the
    baked cell by `the_baked_graph_cell_is_three_discs_and_two_edges`).
    """
    r, edge = px(2.75), px(1.2)
    parent = (CX, T - r)
    kids = [(L + r, B + r), (R - r, B + r)]
    out = [bar(parent, k, edge) for k in kids]
    out.append(circle(*parent, r))
    for k in kids:
        out.append(circle(*k, r))
    return out


def screen():
    """**Screen**: an empty rounded frame - one screen of the app being built.

    Tim, 2026-08-16: "to contrast this on the screens pane, use a box for the
    screen." It is the Screens pane's half of the design/code toggle, and its
    whole job is to be UNMISTAKABLE for the rename pencil that used to sit
    there - a closed area against a diagonal stroke, which is as far apart as
    two 28px marks get.

    Drawn rather than borrowed, for the reason `props` and `table` are: nothing
    Material publishes MEANS "a screen". `crop_square` means crop-to-square,
    `check_box_outline_blank` means an unticked checkbox and `rectangle` means
    a rectangle; taking any of them would put a name in the codebase that says
    something the mark does not. See `../src/font/mod.rs`'s `highbay_codepoint`.

    It is the Table's outer frame with nothing inside it - same live box, same
    corner radius, same 1.4px frame stroke - so a viewer reads Table as "this
    screen, with rows in it" rather than as an unrelated mark. The two never
    appear together (Table is the Data pane's, this is the Screens pane's).
    """
    frame, radius = px(1.4), px(1.5)
    return [
        rounded_rect(L, B, R, T, radius, cw=True),
        rounded_rect(L + frame, B + frame, R - frame, T - frame, radius - frame, cw=False),
    ]


# name -> (codepoint, contour builder), sorted by NAME so it reads as a list and
# so the manifest in `../src/font/mod.rs` can binary-search the same order.
#
# **Codepoints are DECLARED here, never derived from a position in this list.**
# They used to be `ICON_BASE + index`, which is only correct while the list is
# never inserted into: adding `screen` alphabetically would have renumbered
# `table` from U+F802 to U+F803 - silently, in a face that had already shipped,
# for every caller that had already resolved it. Written down, a new icon takes
# the next free codepoint above every existing one and nothing moves.
ICONS = [
    ("graph", 0xF800, graph),
    ("props", 0xF801, props),
    ("screen", 0xF803, screen),
    ("table", 0xF802, table),
]


def build(contours):
    g = Glyph()
    g.numberOfContours = len(contours)
    pts = [p for c, _ in contours for p in c]
    flags = [f for _, fs in contours for f in fs]
    g.coordinates = GlyphCoordinates(pts)
    g.flags = bytearray(flags)
    ends, n = [], 0
    for c, _ in contours:
        n += len(c)
        ends.append(n - 1)
    g.endPtsOfContours = ends
    # An empty hinting program, not None: `glyf` compiles `program.getBytecode()`
    # unconditionally. These glyphs are geometry, so there is nothing to hint.
    g.program = Program()
    g.program.fromBytecode(b"")
    return g


def main():
    base_path, out_path = sys.argv[1], sys.argv[2]
    font = TTFont(base_path)

    if font["head"].unitsPerEm != UPEM:
        sys.exit(f"face is {font['head'].unitsPerEm}upem; the icon coordinates are {UPEM}upem")

    have = set(font.getGlyphOrder())
    # De-duplicated by identity, as `marker.py` does: a face can carry several
    # unicode subtables that fontTools backs with the SAME dict, so writing one
    # writes them all.
    cmaps, seen_ids = [], set()
    for t in font["cmap"].tables:
        if t.isUnicode() and id(t.cmap) not in seen_ids:
            seen_ids.add(id(t.cmap))
            cmaps.append(t)
    if not cmaps:
        sys.exit("face has no unicode cmap subtable")

    # Append-only: a new icon must sit ABOVE every one already in the face, so
    # the bake queue (which scans the Private Use Area in codepoint order)
    # never has a cell inserted before an existing one. Checked rather than
    # remembered, because the cost of getting it wrong is every icon cell in
    # every shipped atlas moving.
    settled = [cp for _, cp, _ in ICONS if f"uni{cp:04X}" in have]
    ceiling = max(settled, default=ICON_BASE - 1)

    added = []
    for label, cp, builder in ICONS:
        name = f"uni{cp:04X}"
        if name in have:
            continue  # already shipped in this face; never re-derived
        if not ICON_BASE <= cp <= ICON_LIMIT:
            sys.exit(f"{label}: U+{cp:04X} is outside the icon block")
        if cp <= ceiling:
            sys.exit(
                f"{label}: U+{cp:04X} sits below U+{ceiling:04X}, which the face already "
                f"carries - allocate upward or every icon cell above it moves in the atlas"
            )
        contours = builder()

        # Outer contours CW, counters CCW - and every glyph must start with an
        # outer one, or the fill is inverted.
        if signed_area2(contours[0][0]) >= 0:
            sys.exit(f"{label}: the first contour is not an outer (clockwise) one")

        # The contract `../src/font/mod.rs` asserts against the shipped bytes.
        xs = [p[0] for c, _ in contours for p in c]
        ys = [p[1] for c, _ in contours for p in c]
        if min(xs) + max(xs) != UPEM:
            sys.exit(f"{label}: ink is not centred on x={CX}; spans [{min(xs)}, {max(xs)}]")
        if min(ys) + max(ys) != UPEM:
            sys.exit(f"{label}: ink is not centred on y={CY}; spans [{min(ys)}, {max(ys)}]")
        if max(ys) > CEILING:
            sys.exit(
                f"{label}: ink reaches y={max(ys)}, above the {CEILING} ceiling - "
                f"the top {PX_RANGE / 2:.0f}px of the distance field would be cut off"
            )

        glyph = build(contours)
        glyph.recalcBounds(font["glyf"])
        font["glyf"].glyphs[name] = glyph
        font["hmtx"].metrics[name] = (UPEM, glyph.xMin)
        for t in cmaps:
            if cp in t.cmap:
                sys.exit(f"U+{cp:04X} is already mapped to {t.cmap[cp]}")
            t.cmap[cp] = name
        added.append((label, cp, name))

    order = font.getGlyphOrder() + [n for _, _, n in added]
    font.setGlyphOrder(order)
    font["glyf"].glyphOrder = order
    font["maxp"].numGlyphs = len(order)

    os2 = font["OS/2"]
    all_cps = [cp for t in cmaps for cp in t.cmap]
    os2.usFirstCharIndex = min(all_cps)
    os2.usLastCharIndex = min(0xFFFF, max(all_cps))
    os2.ulUnicodeRange2 |= 1 << (60 - 32)  # bit 60: Private Use Area (plane 0)

    font.save(out_path)
    listing = ", ".join(f"{lbl} U+{cp:04X}" for lbl, cp, _ in added)
    print(f"{out_path}: {len(order)} glyphs (+{len(added)}), icons {listing}")
    print(f"(cell ceiling y={CEILING})")


if __name__ == "__main__":
    main()
