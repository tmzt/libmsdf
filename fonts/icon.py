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

The SEVEN below them are the navigation rail's destination marks, ported
2026-09-17 from `crates/highbay_ui/src/zui/rail.rs`'s `draw_rail_glyph`. Unlike
Table and Props they were measured off a SCREEN CAPTURE of the shipping IDE
rather than off the Rust, because the two disagree; the block comment above
`sitemap` records how, what the port could not carry, and why `sitemap` and
`device` are new names rather than the `graph` and `screen` they resemble.

**`draw_rail_glyph` STAYS, and that is not an oversight.** `highbay_ui` is
behind a cargo feature: `--features zui` is the shipping shell and draws its own
rail from that function, and `--features ideroot` has no `highbay_ui` in its
graph at all. So these outlines and those strokes are two roots' answers to one
design rather than a duplicate to be collapsed, and deleting the strokes would
take the icons out of the default build for nothing. Converting the zui rail to
name these glyphs is a change of its own, with its own frames to look at.

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


def arc(cx, cy, rx, ry, a0, a1, n):
    """`n` quadratic segments of an ELLIPSE, from angle `a0` to `a1` (radians).

    The generalisation of `circle` in both axes at once, and the primitive the
    cylinder and the branch are drawn from. It returns an OPEN run - on-curve
    start, on-curve end - so arcs and straight runs concatenate into one
    contour, which is what lets a stroked figure be expressed as the closed
    outline of its ink rather than as an overlapping pile of bars.

    Off-curve points sit on each segment's bisector at `1/cos(step/2)` of the
    radii, the same construction `circle` uses with `step` fixed at 45 degrees.
    For a segment whose bisector is an axis - which every 45-degree segment of
    a half or quarter arc has - that scaling brings the control point's axis
    projection back to exactly `rx`/`ry`, so the CONTROL-POINT bounding box
    `recalcBounds` writes is the true one and the centring assertions below are
    measuring the ink rather than a hull around it.
    """
    step = (a1 - a0) / n
    k = 1.0 / math.cos(step / 2.0)
    pts = [(cx + rx * math.cos(a0), cy + ry * math.sin(a0))]
    flags = [flagOnCurve]
    for i in range(n):
        mid, end = a0 + step * (i + 0.5), a0 + step * (i + 1)
        pts += [
            (cx + rx * k * math.cos(mid), cy + ry * k * math.sin(mid)),
            (cx + rx * math.cos(end), cy + ry * math.sin(end)),
        ]
        flags += [0, flagOnCurve]
    return pts, flags


def ellipse(cx, cy, rx, ry, cw=True):
    """A closed ellipse, as `circle` is a closed circle."""
    pts, flags = arc(cx, cy, rx, ry, 0.0, 2.0 * math.pi, 8)
    # `arc` closes the ring by repeating the start point; a contour is implicitly
    # closed, so the duplicate would be a zero-length edge.
    return _wind(pts[:-1], flags[:-1], cw)


def joined(*runs):
    """Concatenate open runs into one closed contour.

    Each run is `(points, flags)` from `arc`, or a bare list of on-curve points
    for a straight section. A point equal to the one before it is dropped: two
    arcs that meet, or an arc and the line leaving its endpoint, name the shared
    point twice and a zero-length edge is not geometry.
    """
    pts, flags = [], []
    for run in runs:
        if isinstance(run, tuple):
            rp, rf = run
        else:
            rp, rf = list(run), [flagOnCurve] * len(run)
        for p, f in zip(rp, rf):
            if pts and _same(pts[-1], p):
                continue
            pts.append(p)
            flags.append(f)
    if len(pts) > 1 and _same(pts[0], pts[-1]):
        pts, flags = pts[:-1], flags[:-1]
    return pts, flags


def _same(p, q):
    return round(p[0]) == round(q[0]) and round(p[1]) == round(q[1])


# ── the glyphs ──────────────────────────────────────────────────────────

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


# ── the seven the navigation rail draws ─────────────────────────────────
#
# `crates/highbay_ui/src/zui/rail.rs`'s `draw_rail_glyph` draws these as
# `push_bezier` strokes and `SdfKind::Outline` boxes over a reserved rectangle,
# one arm per destination. It is still there and still shipping - see the module
# docstring for why a feature-gated second root makes that two answers rather
# than the drift `table` and `props` record.
#
# # They were MEASURED, not transcribed
#
# The geometry below was read off a 2x screen capture of the shipping IDE
# (`Screenshot 2026-09-14 at 5.14.06 PM.png`, rail column, scanned run by run)
# rather than recomputed from the Rust, because the two disagree and the
# SCREEN is what Tim asked the rail to match. Two divergences found that way,
# both reported rather than resolved here:
#
# * `SdfKind::Outline` strokes CENTRED on its rectangle, so every arm's ink is
#   one stroke wider and taller than the arm's own numbers say. That is why
#   Storyboard measures 20.7 x 15.5 where the arm computes 18.2 x 13.0.
# * the Changes arm's cubic leaves the trunk VERTICALLY and arrives at its node
#   vertically - an S. On screen the limb leaves the trunk PERPENDICULAR, runs
#   flat, and turns up into the node. The screen is drawn below.
#
# # One scale, because the relative sizes are the design
#
# The seven were designed together in one 24px box with ONE 2.5px stroke, and
# they are not the same size as each other: Widgets is 16.5 x 15.0 where Genius
# is 22.4 x 21.6. So they are mapped through a SINGLE scale rather than each
# being stretched to fill the live box - stretching would give the set one
# optical size the design deliberately does not have, and would make the stroke
# a different weight in every glyph.
#
# The scale is set by the tallest mark, the phone, which fills the live box's
# height exactly. Nothing else reaches the box on either axis, which is correct
# and is why `every_highbay_icon_shapes_and_is_baked_with_ink` no longer asks
# every glyph to fill it.
#
# # What the port could not carry
#
# **Strokes become outlines.** A glyph has no strokes, so each of these is the
# OUTLINE OF THE INK the rail's strokes lay down: a stroked rectangle becomes a
# ring, a stroked corner becomes a quarter annulus closed at both ends, a
# stroked ellipse becomes an elliptical one. Only `sparkle` is a judgement
# rather than a transcription, recorded there.
#
# **Two names were NOT enough on their own.** Storyboard and Screens look like
# `graph` and `screen`, which the face already carries - and they are different
# MARKS: `graph`'s nodes are filled discs and the rail's are outlined rounded
# squares; `screen` is a landscape frame and the rail's is a portrait phone.
# The face is additive by construction, so neither could be redrawn, and naming
# the rail's marks after them would have drawn the wrong picture. They are
# `sitemap` and `device`, named for what they DRAW.

RAIL_STROKE = 2.5  # design px, and it is the same in all seven
_TALLEST = 22.18  # the phone's ink height, the mark that sets the scale
RAIL_UNIT = (2 * HALF_H) / _TALLEST  # design px -> font units


def d(n):
    """`n` pixels of the rail's 24px design box, in font units."""
    return n * RAIL_UNIT


def centred(contours):
    """Place a figure's ink exactly on the em's centre, in whole units.

    Four of the seven are not symmetric about their own design box - the
    storyboard's parent node sits half a pixel right of centre, the widget's
    tab hangs off one side - so each is drawn where the design puts it and
    moved onto the em here.

    `main`'s centring check is EXACT (`min + max == UPEM`) and rounding two
    extremes independently can leave the sum one unit out, with a parity no
    uniform shift can fix. When that happens every point sitting ON the odd
    extreme moves one unit inward, which keeps a flat edge flat and is 1/2048
    of an em.
    """
    pts = [p for c, _ in contours for p in c]
    shift = [(UPEM - min(p[a] for p in pts) - max(p[a] for p in pts)) / 2.0 for a in (0, 1)]
    out = [
        ([(round(x + shift[0]), round(y + shift[1])) for x, y in c], f) for c, f in contours
    ]
    for axis in (0, 1):
        vals = [p[axis] for c, _ in out for p in c]
        residue = min(vals) + max(vals) - UPEM
        if residue == 0:
            continue
        if abs(residue) != 1:
            sys.exit(f"centring is {residue} units out, which is not a rounding residue")
        edge, step = (max(vals), -1) if residue == 1 else (min(vals), 1)
        out = [
            (
                [
                    (x + step, y) if axis == 0 and x == edge else (x, y + step)
                    if axis == 1 and y == edge
                    else (x, y)
                    for x, y in c
                ],
                f,
            )
            for c, f in out
        ]
    return out


def ring(cx, cy, w, h, r, t, cw=True):
    """A rounded rectangle STROKED `t` wide on its centreline - two contours.

    `w`/`h`/`r` are the centreline's, as `SdfKind::Outline`'s rectangle is, so
    an arm's own numbers go in unchanged and the ink comes out one stroke
    larger. A corner whose inner radius would be negative is squared off, which
    is what the shader draws there too.
    """
    ow, oh, ir = w + t, h + t, r - t / 2.0
    outer = rounded_rect(cx - ow / 2, cy - oh / 2, cx + ow / 2, cy + oh / 2, r + t / 2.0, cw=cw)
    x0, y0 = cx - (w - t) / 2, cy - (h - t) / 2
    x1, y1 = cx + (w - t) / 2, cy + (h - t) / 2
    inner = (
        rounded_rect(x0, y0, x1, y1, ir, cw=not cw) if ir > 0 else rect(x0, y0, x1, y1, cw=not cw)
    )
    return [outer, inner]


def sitemap():
    """**Sitemap**: three screens, one over two, joined by two limbs.

    The Storyboard destination's mark - the app's screens and the paths between
    them. Measured at 20.7 x 15.5 of the 24px box, the widest of the seven and
    the only one that is wider than it is tall.

    **It is NOT `graph`, and the difference is the whole reason it has a name.**
    `graph` is three filled DISCS on straight edges, drawn for the toolbar at
    16px where "a ring's counter is under a pixel and closes up". This one's
    nodes are outlined rounded SQUARES, which at the rail's 24px keep a visible
    counter and read as screens rather than as points. Two marks, two meanings,
    two names; the face is additive, so `graph` could not have become this one
    even if the two had meant the same thing.

    The limbs run from centre to centre and therefore THROUGH the nodes, which
    is what fills most of each counter and leaves the slivers a viewer reads as
    depth. That overlap is the design and is reproduced rather than cleaned up.
    """
    node, t = 4.8, RAIL_STROKE
    corner = node * 0.22
    top = (0.48, 4.8)
    kids = [(-6.72, -3.36), (6.72, -3.36)]
    out = []
    for cx, cy in [top, *kids]:
        out += ring(d(cx), d(cy), d(node), d(node), d(corner), d(t))
    for kid in kids:
        out.append(bar((d(top[0]), d(top[1])), (d(kid[0]), d(kid[1])), d(t)))
    return centred(out)


def device():
    """**Device**: a portrait phone frame - one screen of the app, at its shape.

    The Screens destination's mark, measured at 14.5 x 22.2: the TALLEST of the
    seven, and the one that sets the scale every other mark is drawn through.

    **It is NOT `screen`.** That name is taken by a LANDSCAPE frame drawn for
    the toolbar's design/code toggle, and the two are not interchangeable at a
    glance: the whole of what this mark says is the aspect. A device is not a
    screen and a screen is not a device - one is the thing, the other is the
    surface - so the two names are honest beside each other rather than a split
    of one meaning.

    Its stroke measures one texel heavier on screen than the other six, which
    is the only place the rail's set is not uniform. Not reproduced: a single
    weight is what makes six marks a set, and the odd one out reads as a
    rendering artefact rather than as emphasis.
    """
    return centred(ring(0, 0, d(12.0), d(19.68), d(2.88), d(RAIL_STROKE)))


def form():
    """**Form**: a framed sheet with three field rows, the last one short.

    The Forms destination's mark, 18.3 x 20.7. Against `device` - the same
    frame with nothing in it - what separates it is that this screen has
    FIELDS; against `props` - three rows with no frame - what separates it is
    the frame. A property sheet is a panel of rows; a form is a screen of them.

    The rows are measured rather than derived: 2.0 tall, 10.1 and 10.1 and 5.6
    wide, on a 4.5 pitch, inset 4.0 from the frame's outer edge. Their group
    sits 1.25px ABOVE the sheet's centre on screen and is drawn that way here -
    unlike `table`, whose off-centre dividers were corrected, because there the
    artefact made one column 27% wider than its neighbours and here it is a
    placement a designer can have meant.
    """
    frame, rows = ring(0, 0, d(15.84), d(18.24), d(2.53), d(RAIL_STROKE))
    out = [frame, rows]
    left = d(-18.34 / 2 + 4.0)
    for i, width in enumerate([10.14, 10.14, 5.58]):
        cy = d(20.74 / 2 - (4.75 + i * 4.5))
        out.append(pill(left, cy - d(1.0), left + d(width), cy + d(1.0)))
    return centred(out)


def widget():
    """**Widget**: a rounded square with a connector tab on its right edge.

    The Widgets destination's mark, and the SMALLEST of the seven at 16.4 x
    15.0 - which is the design's own proportion and not a slip: a custom widget
    is a smaller thing than a screen, and the rail draws it that way.

    What separates it from `device` and `form`, both also rectangles, is that
    its silhouette is INTERRUPTED. The tab is a filled rounded square 0.26 of
    the block's side, overlapping it by 0.3 of its own width, so it reads as
    attached rather than as a second object.

    The tab reaches 0.1px past the frame's inner edge, as it does on screen.
    Left in: it is a transversal overlap of two filled contours - the `graph`
    case - and moving it would be inventing a proportion to avoid something
    msdfgen resolves.
    """
    t, side, tab = RAIL_STROKE, 12.48, 3.84
    out = ring(d(-1.44), 0, d(side), d(side), d(side * 0.16), d(t))
    right = -1.44 + side / 2
    out.append(
        rounded_rect(
            d(right - tab * 0.3), d(-tab / 2), d(right + tab * 0.7), d(tab / 2), d(tab * 0.35)
        )
    )
    return centred(out)


def database():
    """**Database**: the cylinder - a rim, two walls and a front bottom arc.

    The Data destination's mark, 18.8 x 21.9. It must not be confused with
    `table`, which is also about data and is a filled RECTANGLE; this one is
    curved on both ends and open in the middle, so the two share no silhouette.

    Expressed as ONE outer contour with TWO counters cut out of it, the way
    `table` is: the ring the walls and the two arcs bound, and the opening
    inside the rim. No stroked edge and no overlapping contour anywhere.

    The ellipse radius is 2.52 and not the arm's 3.36, and that is not a
    correction. The arm draws its arcs as CUBICS whose control points sit at
    `ry` - a curve that reaches three quarters of the way there - so 2.52 is
    what the design has always drawn. Measured on screen at 21.9 tall against
    the 23.6 the arm's numbers predict, which is how the discrepancy surfaced.
    """
    t = RAIL_STROKE
    rx, ry, wall = 8.16, 3.36 * 0.75, 14.4
    ox, oy = d(rx + t / 2), d(ry + t / 2)
    ix, iy = d(rx - t / 2), d(ry - t / 2)
    yt, yb = d(wall / 2), d(-wall / 2)

    # Each circuit's last leg - the LEFT wall, back to where the first arc
    # started - is the contour's implicit closing edge and is not named.
    outer = joined(
        arc(CX, CY + yt, ox, oy, math.pi, 0.0, 4),  # the back arc, over the top
        [(CX + ox, CY + yb)],  # down the right wall
        arc(CX, CY + yb, ox, oy, 0.0, -math.pi, 4),  # the front arc, under the bottom
    )
    # The body's opening. Its top is the rim's LOWER edge, which is the outer
    # ellipse's front arc between the two inner walls - so the arc is taken over
    # exactly the angles where it lies inside them.
    span = math.acos(ix / ox)
    body = joined(
        arc(CX, CY + yt, ox, oy, math.pi + span, 2.0 * math.pi - span, 4),
        [(CX + ix, CY + yb)],
        arc(CX, CY + yb, ix, iy, 0.0, -math.pi, 4),
    )
    return centred(
        [
            _wind(*outer, cw=True),
            _wind(*body, cw=False),
            ellipse(CX, CY + yt, ix, iy, cw=False),  # the opening inside the rim
        ]
    )


def sparkle():
    """**Sparkle**: a four-pointed star with a smaller one off its upper right.

    The Genius destination's mark and the largest of the seven at 22.4 x 21.6.
    Nothing else in either vocabulary is a star and nothing else pairs two
    marks, which matters more here than for most: the pane it names has no
    other visual identity.

    Four tips joined by inward-bowed quadratics whose control point is a
    quarter of the way out along the bisector of the two tips - the arm's own
    construction, which puts each edge's midpoint at 0.53r against a straight
    edge's 0.71r.

    **The ONE departure: these are FILLED, where the rail strokes them.** A
    stroked four-pointed star at this weight is two hairlines a quarter of a
    pixel apart by the time the mark is 22px wide, and they merge into a grey
    smear that reads as a blob. Filled, the silhouette IS the star, which is
    what a viewer reads at that size and what the screen capture shows. The tip
    radii are grown by half a stroke so the filled silhouette lands on the
    stroked one's own bounds; the other six transcribe their strokes because a
    stroked frame at 22px is still a frame.
    """
    grow = 22.41 / 19.92  # the stroked bounds, over the tips' own

    def star(cx, cy, r):
        tips = [(0, -r), (r, 0), (0, r), (-r, 0)]
        pts, flags = [], []
        for i, (ax, ay) in enumerate(tips):
            bx, by = tips[(i + 1) % 4]
            pts += [(cx + ax, cy + ay), (cx + (ax + bx) * 0.25, cy + (ay + by) * 0.25)]
            flags += [flagOnCurve, 0]
        return _wind(pts, flags, cw=True)

    return centred(
        [
            star(d(-1.92 * grow), d(-1.68 * grow), d(7.92 * grow)),
            star(d(6.48 * grow), d(6.0 * grow), d(3.6 * grow)),
        ]
    )


def branch():
    """**Branch**: a trunk with a node at each end, and a limb to a third.

    The Changes destination's mark, 14.5 x 19.25 - source control, and the
    narrowest of the seven. It shares a vocabulary with `graph` and `sitemap`
    (nodes joined by strokes) and is told apart by the arrangement: those two
    are one node over two, symmetric; this is an UPRIGHT with one limb. At 22px
    the silhouettes are a chevron, an arch and a stem.

    **Drawn from the screen and not from the arm, because they disagree.** The
    arm's cubic leaves the trunk vertically and arrives at the node vertically,
    an S. What the shipping IDE draws - scanned row by row - is a limb leaving
    the trunk PERPENDICULAR, running flat for two thirds of its reach, then a
    quarter turn up into the node. That is the shape here. Which of the two is
    intended is not this file's to decide and is reported with the port.

    The limb is the quarter annulus that turn describes, and the rectangle it
    starts from overlaps it rather than meeting it, for the reason the three
    discs overlap what they cap: two contours that merely TOUCH are the case
    msdfgen has no answer for.
    """
    t, r = RAIL_STROKE, 1.75
    trunk_x, limb_y = -5.5, 3.125  # the trunk's centreline, and the flat run's
    y_top, y_bot = 7.875, -7.875
    node = (5.5, 6.125)  # the limb's own node, and the quarter turn's end
    turn = (1.75, node[1])  # the turn's centre; the flat run ends below it
    rx, ry = node[0] - turn[0], node[1] - limb_y

    out = [
        rect(d(trunk_x - t / 2), d(y_bot), d(trunk_x + t / 2), d(y_top), cw=True),
        # The flat run, from inside the trunk to inside the turn.
        rect(d(trunk_x - t / 2), d(limb_y - t / 2), d(turn[0] + t / 4), d(limb_y + t / 2)),
    ]
    band = joined(
        arc(d(turn[0]), d(turn[1]), d(rx + t / 2), d(ry + t / 2), -math.pi / 2, 0.0, 2),
        [(d(turn[0] + rx - t / 2), d(turn[1]))],
        arc(d(turn[0]), d(turn[1]), d(rx - t / 2), d(ry - t / 2), 0.0, -math.pi / 2, 2),
    )
    out.append(_wind(*band, cw=True))
    for cx, cy in [(trunk_x, y_top), (trunk_x, y_bot), node]:
        out.append(circle(d(cx), d(cy), d(r)))
    return centred(out)


# ── the two the main toolbar draws ──────────────────────────────────────
#
# The IDE's top strip names eight marks. SIX of them are Material's and are
# already in the merged face - `code`, `edit`, `expand_more`, `undo`, `redo`,
# plus `device` from the rail set above, which the toolbar's Teststand button
# reuses because it IS the same mark. The two below are the ones Material does
# not publish in the cut this face carries, and they are drawn here for the
# same reason the rail's seven were: a name in no manifest draws nothing and is
# reported, never substituted.
#
# # `folder` is on the RAIL'S scale, and `plus` is on the LIVE BOX's
#
# The folder is drawn through `d()`, not a second unit: `device` is drawn
# through it and the toolbar draws `device`, so a toolbar-only scale would make
# one strip hold two sizes of our own marks for no reason a designer chose.
# What that buys is ONE em for the two PROPORTIONED marks the strip draws -
# `35`, the number `ide_rail_item.tsx` derives and for the identical reason.
#
# The plus is not proportioned, so it fills the live box like the four toolbar
# marks above and takes the borrowed set's own em instead. Its docstring is
# where that split is argued, because it is a fact about what a cross IS.
#
# # The measurements are the SHIPPING TOOLBAR's, read twice
#
# `crates/highbay_ui/src/zui/toolbar.rs` draws the folder from `push_bezier`
# strokes and `crates/highbay_ui/src/zui/selector.rs` draws the plus from two
# more, and both were read off the same 2x capture the rail's seven were
# (`Screenshot 2026-09-14 at 5.14.06 PM.png`, toolbar strip). The Rust and the
# screen AGREE here, unlike the rail - the folder's arm computes 19.8 x 18.8 of
# ink and the capture measures 20.0 x 19.0, the plus computes 12.9 square and
# the capture measures 13.0 - so there was no divergence to report and the
# numbers below are the arms' own.
#
# # A stroked figure becomes the OUTLINE OF ITS INK, and here it OVERLAPS
#
# The rail's marks were closed outlines with no two contours crossing. These
# two are not: the folder's tab bars run into the body's wall and the plus is
# two bars crossing at the centre. That is `graph`'s spelling rather than a new
# one - *"the discs and the edges DO overlap here ... msdfgen's sign correction
# resolves it"* - and it is what keeps the mark identical to the strokes it was
# ported from, where the same overlap is what a round-capped line pile draws.


def folder():
    """**Folder**: an open document tray with its tab at the upper left.

    The `Open` button's mark - *"an open folder is the universal 'open a
    document' affordance"* (`zui::toolbar::render_open_button`). Measured at
    19.8 x 18.8 of the design box against the capture's 20.0 x 19.0.

    **It is NOT Material's `folder`**, which this bake does not carry in any
    cut. The name is ours and it names what the mark DRAWS; the day Material's
    own `folder` is bundled, libmsdf's `a_name_never_crosses_between_the_two_
    vocabularies` is what will say so, and the answer then is to rename this
    one rather than to let two vocabularies answer one name.

    The tab is an OPEN three-sided figure, not a filled flap: the arm draws it
    as three separate strokes and leaves the space inside them empty, which is
    what distinguishes this from the solid-tab folder every file manager draws.
    Here that emptiness is a region enclosed by four CW contours and inside
    none of them, so its winding number is zero and it is a hole by
    construction rather than by a counter somebody has to keep pointing the
    right way.

    The body's corners are the one liberty taken. The arm strokes a square
    rectangle because `push_bezier` has no corner radius; at the 1.2px radius
    below the difference is a third of a pixel at the size this draws, and a
    square-cornered folder beside `device`'s 2.88px radius reads as a different
    set.
    """
    half_w, half_h, t = 9.0, 7.0, 1.8
    # The tab: `tab_l`/`tab_r` in the arm, raised 3.0 above the body's own top
    # edge and stopping 15% of the half-width short of centre.
    rise, tab_r_x = 3.0, -half_w * 0.15
    out = ring(0, 0, d(2 * half_w), d(2 * half_h), d(1.2), d(t))
    # Each bar is the ink of one stroke, and each runs INTO the body's top wall
    # rather than up to it - two CW contours overlapping wind to 2 and stay
    # filled, where a butt join would leave a seam the field has to resolve.
    inner_top = d(half_h - t / 2)
    top = d(half_h + rise + t / 2)
    out.append(rect(d(-half_w - t / 2), inner_top, d(-half_w + t / 2), top))
    out.append(rect(d(tab_r_x - t / 2), inner_top, d(tab_r_x + t / 2), top))
    out.append(rect(d(-half_w - t / 2), d(half_h + rise - t / 2), d(tab_r_x + t / 2), top))
    return centred(out)


def plus():
    """**Plus**: two round-capped bars crossing - the selector's `+`.

    `zui::selector`'s `Glyph::Plus`, whose own doc explains why it is geometry
    rather than a borrowed name: *"there is no Material `add` that differs from
    a horizontal and a vertical stroke, and borrowing four more glyphs would
    spend four atlas cells to draw the same pixels."* That argument was for a
    renderer drawing two `push_bezier` calls; a glyph spends the cell either
    way, so what decides it here is that an authored `<Icon name=…>` resolves
    through one manifest and cannot reach a renderer's private enum.

    Named `plus` and not `add`: `add` is Material's word for this button and
    the two manifests must stay disjoint, so the owned name says what the mark
    IS rather than what pressing it does.

    # It FILLS the live box, where `folder` beside it does not, and that is the
    # one place the two part company

    `folder` is drawn through `d()` because a folder has PROPORTIONS - 19.8 by
    18.8 of the design box, a shape a designer chose - and a mark drawn to fill
    its cell instead would be a different folder. A cross has none: it is two
    bars at right angles, and every size of it is the same mark. So this one is
    scaled like `table`, `props`, `graph` and `screen` - to the shared live box
    - and the design's 12.9px comes back as an EM rather than as geometry.

    That is not a saving of effort. Drawn on `d()` at its authored 12.9 this
    cell inks 14 texels, one below the floor
    `every_highbay_icon_shapes_and_is_baked_with_ink` sets at 15, and the
    honest fix for a mark with no proportion to keep is to draw it properly and
    pick the em - not to move a floor that exists so a glyph is not a speck in
    its own cell.

    The arms reach `SIGN_HALF + t/2` = 6.45 of 12.9 and the stroke is 1.9 of
    it, so both ratios are the arm's; the caps are the reason both bars are
    pills rather than rectangles, `push_bezier`'s line primitive being
    round-capped, which `pill`'s own docstring records as the difference
    between reproducing a mark and redrawing it. The two overlap at the centre
    and wind to 2, which stays filled.
    """
    # The mark's own 12.9-unit box, mapped onto the live box's height.
    unit = HALF_H / (5.5 + 1.9 / 2.0)
    reach, half = HALF_H, round(unit * 1.9 / 2.0)
    return centred(
        [
            pill(-reach, -half, reach, half),
            rounded_rect(-half, -reach, half, reach, half),
        ]
    )


def avatar():
    """**Avatar**: a filled person silhouette - the rail's account mark.

    Chrome rather than a destination: it stands at the bottom of the rail, below
    the spacer, and selects no pane. Measured with the seven above it, at 20.0 x
    22.5 of the design box - the largest mark the rail draws, which is what the
    account control is on screen.

    **It is NOT Material's `person`, which this bake already carries.** That one
    is the OUTLINED cut - a ring for the head and a hollow body, because the
    merged face instances Material Symbols at FILL 0 - and the rail draws the
    FILLED silhouette. Rendered both and compared before drawing: the two are
    not the same mark, and naming this `person` was never available anyway,
    since a name may not cross between the two vocabularies.

    A disc and a loaf, and the loaf's BOTTOM corners are square. That is the
    whole difference between a person and a pill at this size: the shoulders
    are cut off by the frame rather than closed, which is what says the figure
    continues past the mark.
    """
    head_r, head_y = 5.0, 6.25
    x, top, bottom, corner = 10.0, -2.25, -11.25, 3.5
    body = [
        ((-x, bottom), flagOnCurve),
        ((-x, top - corner), flagOnCurve),
        ((-x, top), 0),
        ((-x + corner, top), flagOnCurve),
        ((x - corner, top), flagOnCurve),
        ((x, top), 0),
        ((x, top - corner), flagOnCurve),
        ((x, bottom), flagOnCurve),
    ]
    return centred(
        [
            circle(d(0), d(head_y), d(head_r)),
            _wind([(d(px_), d(py)) for (px_, py), _ in body], [f for _, f in body], cw=True),
        ]
    )


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
    ("avatar", 0xF80B, avatar),
    ("branch", 0xF804, branch),
    ("database", 0xF805, database),
    ("device", 0xF809, device),
    ("folder", 0xF80C, folder),
    ("form", 0xF806, form),
    ("graph", 0xF800, graph),
    ("plus", 0xF80D, plus),
    ("props", 0xF801, props),
    ("screen", 0xF803, screen),
    ("sitemap", 0xF80A, sitemap),
    ("sparkle", 0xF807, sparkle),
    ("table", 0xF802, table),
    ("widget", 0xF808, widget),
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
