#!/usr/bin/env -S uv run --quiet --with fonttools python3
"""Widen a baked Highbay face to Latin-1 Supplement, and give glyph 0 a box.

**This is how `fonts/*.ttf` are made, and it is additive by construction.**
The existing glyphs are never re-derived: the script COPIES new outlines into
an existing face, appending them after every glyph already there. So no glyph
id moves, `GDEF`/`GPOS`/`GSUB` (kerning, the fi/fl/ffi/ffl ligatures) are
carried through untouched, and every ASCII advance in every frame is the one
that was already shipping. A re-subset from upstream would have had to
reproduce all of that by luck.

Source of the new outlines: **Roboto v2.138, the `roboto-android.zip` asset of
<https://github.com/googlefonts/roboto/releases/tag/v2.138>**. That is the
build the shipped ASCII half came from, and the script proves it rather than
trusting it: every printable-ASCII outline and advance is compared against the
base face and any difference aborts the run. (The `roboto-unhinted.zip` asset
of the SAME tag differs in 46 outlines, and Roboto v2.137 differs in 67
advances - which is why the check is not a formality.)

    ./widen.py <base.ttf> <out.ttf> <roboto-android/Roboto-Regular.ttf>

Run for both faces; see `../src/font/mod.rs` for what each one is.
"""

import sys

from fontTools.pens.recordingPen import RecordingPen
from fontTools.ttLib import TTFont
from fontTools.ttLib.tables._g_l_y_f import Glyph, GlyphCoordinates, flagOnCurve

# Latin-1 Supplement. The declared widening, and the only text range added:
# it is what a European name needs, it is contiguous, and it stops nowhere
# near the Private Use Area the icon half lives in.
LATIN1 = range(0x00A0, 0x0100)


def check_ascii_identical(base, src):
    """Abort unless `src` draws printable ASCII exactly as `base` already does.

    The whole additive premise rests on this: components of the new composites
    (`Aacute` is `A` + `acute`) resolve BY NAME to glyphs already in the base,
    so if the two faces disagreed by even a unit, the accents would sit on
    outlines from one build over bodies from another.
    """
    bc, sc = base.getBestCmap(), src.getBestCmap()
    bg, sg = base.getGlyphSet(), src.getGlyphSet()
    bad = []
    for cp in range(0x20, 0x7F):
        bn, sn = bc.get(cp), sc.get(cp)
        if bn is None or sn is None:
            bad.append((cp, "unmapped"))
            continue
        p1, p2 = RecordingPen(), RecordingPen()
        bg[bn].draw(p1)
        sg[sn].draw(p2)
        if p1.value != p2.value:
            bad.append((cp, "outline"))
        if base["hmtx"][bn] != src["hmtx"][sn]:
            bad.append((cp, "advance"))
    if bad:
        sys.exit(f"source is not the base face's build: {len(bad)} diffs, {bad[:8]}")


def closure(src, names):
    """`names` plus, recursively, every glyph their composites reference."""
    glyf, out, seen = src["glyf"], [], set()

    def add(n):
        if n in seen:
            return
        seen.add(n)
        out.append(n)
        g = glyf[n]
        if g.isComposite():
            for c in g.components:
                add(c.glyphName)

    for n in names:
        add(n)
    return out


def tofu(src):
    """Glyph 0's box: Roboto's own `.notdef` frame, X removed, at stem weight.

    Roboto draws `.notdef` as a filled rectangle `(100,0)-(808,1456)` with four
    triangular counters cut out of it, which read as an X. Keeping contour 0 and
    replacing those four counters with ONE rectangular counter turns it into a
    hollow box - U+25A1 WHITE SQUARE's form - at the outer size, cap height and
    advance the type designer already chose for this exact purpose.

    The one thing NOT reused is the frame's 54-unit stroke, which exists because
    the X carries the weight in the original. Standing alone it is a third of
    Roboto's stem and goes to a hairline: at 48px atlas cells it is under one
    texel, so at UI sizes the box would fade out exactly where it is needed. The
    stroke is therefore the face's own capital stem, measured off `I` - so the
    placeholder sits at the same optical weight as the text it appears in, and
    no number here is invented.
    """
    nd = src["glyf"][".notdef"]
    nd.expand(src["glyf"])
    outer_end = nd.endPtsOfContours[0]
    outer = [tuple(nd.coordinates[i]) for i in range(outer_end + 1)]
    x0, y0 = min(p[0] for p in outer), min(p[1] for p in outer)
    x1, y1 = max(p[0] for p in outer), max(p[1] for p in outer)

    cap_i = src["glyf"]["I"]
    cap_i.expand(src["glyf"])
    stroke = cap_i.xMax - cap_i.xMin

    # Outer contour is clockwise, so the counter must wind the other way.
    inner = [
        (x0 + stroke, y0 + stroke),
        (x1 - stroke, y0 + stroke),
        (x1 - stroke, y1 - stroke),
        (x0 + stroke, y1 - stroke),
    ]

    g = Glyph()
    g.numberOfContours = 2
    g.coordinates = GlyphCoordinates(outer + inner)
    g.flags = bytearray([flagOnCurve] * 8)
    g.endPtsOfContours = [3, 7]
    g.program = nd.program
    g.recalcBounds(src["glyf"])
    return g


def main():
    base_path, out_path, src_path = sys.argv[1], sys.argv[2], sys.argv[3]
    base, src = TTFont(base_path), TTFont(src_path)
    check_ascii_identical(base, src)

    have = set(base.getGlyphOrder())
    src_cmap = src.getBestCmap()
    wanted = [n for cp in LATIN1 if (n := src_cmap.get(cp))]
    if len(wanted) != len(LATIN1):
        sys.exit("source does not cover all of Latin-1 Supplement")

    added = [n for n in closure(src, wanted) if n not in have]
    order = base.getGlyphOrder() + added
    base.setGlyphOrder(order)
    base["glyf"].glyphOrder = order
    for n in added:
        base["glyf"][n] = src["glyf"][n]
        base["hmtx"][n] = src["hmtx"][n]
    base["maxp"].numGlyphs = len(order)

    # The placeholder. Glyph 0 and nothing else: that is where the shaper
    # already routes every codepoint the face cannot draw, so no renderer has
    # to learn a lookup that could drift from this bake.
    base["glyf"][".notdef"] = tofu(src)
    base["hmtx"][".notdef"] = src["hmtx"][".notdef"]

    for table in base["cmap"].tables:
        if table.isUnicode():
            for cp in LATIN1:
                table.cmap[cp] = src_cmap[cp]

    os2 = base["OS/2"]
    os2.usFirstCharIndex = min(
        cp for t in base["cmap"].tables if t.isUnicode() for cp in t.cmap
    )
    os2.usLastCharIndex = min(
        0xFFFF, max(cp for t in base["cmap"].tables if t.isUnicode() for cp in t.cmap)
    )
    os2.ulUnicodeRange1 |= 1 << 1  # Latin-1 Supplement

    base.save(out_path)
    print(f"{out_path}: {len(order)} glyphs (+{len(added)}), {len(wanted)} new codepoints")


if __name__ == "__main__":
    main()
