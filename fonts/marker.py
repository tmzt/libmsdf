#!/usr/bin/env -S uv run --quiet --with fonttools python3
"""Add the repo's own EDGE MARKER glyphs to a baked Highbay face.

**This and `widen.py` are how `fonts/*.ttf` are made, and both are additive by
construction.** Existing glyphs are never re-derived: this appends new outlines
after every glyph already there, so no glyph id moves and `GDEF`/`GPOS`/`GSUB`
(kerning, the fi/fl/ffi/ffl ligatures) are carried through untouched.

    ./marker.py <base.ttf> <out.ttf>

Run for BOTH faces (see `../src/font/mod.rs` for what each one is). A marker is
not an icon: it is geometry this repo DRAWS WITH, so both bundled faces carry
the identical set and the two bakes still differ only by the Material Symbols
half.

# Why a glyph rather than a shader shape

An arrowhead sits at an arbitrary tangent angle on a curve, at a size that
changes with the diagram's zoom. That is exactly what an MSDF cell is for: the
field reconstructs a sharp corner at any scale, and the glyph path already
carries rotation (`SdfRotate`), colour, clipping and the transform stack. A new
`SdfKind` would have bought a new shader arm and a new wire-format entry to
redo work the text path already does.

# The design contract, and it is declared HERE

`DrawList::push_marker` anchors a marker by its ADVANCE-WIDTH POINT ON THE
BASELINE and scales it so the caller's `size` is the advance. Both of those
numbers are read back off the baked atlas entry, never restated in Rust - so
this file is the single place the geometry is decided. Every marker glyph
therefore obeys:

* ink spans `x in [0, advance]`, so the anchor is the leading edge and the tail
  runs backwards along -x (which is what a rotation about the anchor sweeps);
* ink is symmetric about `y = 0`, the baseline, so the anchor sits on the
  marker's axis and rotating about it does not swing the shape off the curve;
* `advance` is the marker's along-edge extent, which is what `size` means.

`marker_contract_holds` in `../src/font/mod.rs` asserts exactly this against the
shipped bytes, so a redesign here that forgets to move Rust fails a test rather
than drawing the arrow a few pixels off its curve.

# Sizing, and why the numbers are what they are

Cells are baked at 48px with `px_range` 6.0, from a 2048upem face, at
`em_scale = 48 / (2048 * 1.3)`; the cell's baseline row is `48*0.15 +
em_scale*cap_height`, about 33.4. Ink centred on the baseline therefore sits LOW
in the cell, and the binding constraint is the bottom edge: the distance field
needs `px_range/2` = 3px of clear cell below the ink or the field is truncated
and the lower edge stops antialiasing. A half-width of 512 units leaves 5.3px.
That is the ceiling this design is written under, and it is why the arrow is
0.625em long rather than a full em.
"""

import sys

from fontTools.ttLib import TTFont
from fontTools.ttLib.tables._g_l_y_f import Glyph, GlyphCoordinates, flagOnCurve
from fontTools.ttLib.tables.ttProgram import Program

UPEM = 2048

# The repo-owned marker block, at the TOP of the Private Use Area and mirrored
# by `MARKERS` in `../src/font/mod.rs`. Material Symbols are BORROWED at their
# own upstream codepoints, which are all far below this; allocating markers
# upward from F8F0 keeps them sorted last, and the bake queue's append-only
# ordering with them (`FontAtlasBuilder::add_shipped_coverage`).
MARKER_BASE = 0xF8F0

# name -> (advance, [contour, ...]) in font units, y-up, outer contours CW
# (checked against Roboto's own glyphs: every outer contour there winds CW).
#
# ARROW: a plain filled triangle. Not a barbed or notched head - a concave
# throat is a feature roughly a pixel wide at the 8-14px an arrowhead is drawn
# at in a diagram, so it reads as mud exactly where the shape has to stay
# unmistakably directional. Three straight edges and three corners is also what
# MSDF reproduces essentially exactly. The apex is 2*atan(512/1280) = 43.6
# degrees, a little blunter than graphviz's 38.6, which is deliberate: it holds
# its direction at smaller sizes.
MARKERS = {
    0xF8F0: ("uniF8F0", 1280, [[(0, 512), (1280, 0), (0, -512)]]),
}


def signed_area2(points):
    """Twice the shoelace area: negative is clockwise in a y-up frame."""
    total = 0
    for i, (x1, y1) in enumerate(points):
        x2, y2 = points[(i + 1) % len(points)]
        total += x1 * y2 - x2 * y1
    return total


def build(contours):
    g = Glyph()
    g.numberOfContours = len(contours)
    pts = [p for c in contours for p in c]
    g.coordinates = GlyphCoordinates(pts)
    g.flags = bytearray([flagOnCurve] * len(pts))
    ends, n = [], 0
    for c in contours:
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
        sys.exit(f"face is {font['head'].unitsPerEm}upem; the marker coordinates are {UPEM}upem")

    have = set(font.getGlyphOrder())
    # De-duplicated by identity: a face can carry several unicode subtables
    # (the plain face has both (0,3) and (3,1) format 4) that fontTools backs
    # with the SAME dict, so writing one writes them all - and a naive
    # already-mapped check would then trip on the entry it had just made.
    cmaps, seen_ids = [], set()
    for t in font["cmap"].tables:
        if t.isUnicode() and id(t.cmap) not in seen_ids:
            seen_ids.add(id(t.cmap))
            cmaps.append(t)
    if not cmaps:
        sys.exit("face has no unicode cmap subtable")

    added = []
    for cp in sorted(MARKERS):
        name, advance, contours = MARKERS[cp]
        if name in have:
            sys.exit(f"{name} is already in the face - this script is additive, not idempotent")
        for c in contours:
            if signed_area2(c) >= 0:
                sys.exit(f"{name}: outer contour is not clockwise in the y-up frame")
        # The contract `../src/font/mod.rs` asserts against the shipped bytes.
        xs = [p[0] for c in contours for p in c]
        ys = [p[1] for c in contours for p in c]
        if (min(xs), max(xs)) != (0, advance):
            sys.exit(f"{name}: ink must span x in [0, advance]; got [{min(xs)}, {max(xs)}]")
        if min(ys) != -max(ys):
            sys.exit(f"{name}: ink must be symmetric about the baseline; got [{min(ys)}, {max(ys)}]")

        glyph = build(contours)
        glyph.recalcBounds(font["glyf"])
        font["glyf"].glyphs[name] = glyph
        font["hmtx"].metrics[name] = (advance, min(xs))
        for t in cmaps:
            if cp in t.cmap:
                sys.exit(f"U+{cp:04X} is already mapped to {t.cmap[cp]}")
            t.cmap[cp] = name
        added.append(name)

    order = font.getGlyphOrder() + added
    font.setGlyphOrder(order)
    font["glyf"].glyphOrder = order
    font["maxp"].numGlyphs = len(order)

    os2 = font["OS/2"]
    all_cps = [cp for t in cmaps for cp in t.cmap]
    os2.usFirstCharIndex = min(all_cps)
    os2.usLastCharIndex = min(0xFFFF, max(all_cps))
    os2.ulUnicodeRange2 |= 1 << (60 - 32)  # bit 60: Private Use Area (plane 0)

    font.save(out_path)
    print(f"{out_path}: {len(order)} glyphs (+{len(added)}), markers {', '.join(added)}")


if __name__ == "__main__":
    main()
