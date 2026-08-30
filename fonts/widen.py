#!/usr/bin/env -S uv run --quiet --with fonttools python3
"""Widen a baked Highbay face to the declared text coverage, and give glyph 0 a box.

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

# It is IDEMPOTENT, which is what lets the coverage grow twice

`TEXT_RANGES` below mirrors the Rust declaration, and the script appends only
the glyphs the base face does not already carry. So re-running it after a
widening adds the NEW block and touches nothing else: a face that has already
shipped Latin-1 comes back with the same glyph ids for every one of those
glyphs, and only the new codepoints land above them.

That is the property the atlas needs, not just a convenience. Cells are placed
in `(set, glyph id)` order, so a glyph id that moved would move a CELL, and the
whole "a re-bake is a strict superset" claim in `add_shipped_coverage` would be
false for the half of the face that had already been baked.
"""

import sys

from fontTools.pens.recordingPen import RecordingPen
from fontTools.ttLib import TTFont
from fontTools.ttLib.tables._g_l_y_f import Glyph, GlyphCoordinates, flagOnCurve

# **The declared text coverage**, mirroring `../src/font/mod.rs`'s
# `TEXT_RANGES` - the same mirror `style.py` keeps, and checked against the
# Rust side by `a_style_face_covers_the_declared_text_ranges`. Printable ASCII
# is in the list because the check below proves the base already draws it
# identically; the script then adds only what is missing.
TEXT_RANGES = [
    (0x0020, 0x007E),  # printable ASCII
    (0x00A0, 0x00FF),  # Latin-1 Supplement
    (0x2013, 0x2014),  # en dash, em dash
    (0x2018, 0x2019),  # single quotation marks
    (0x201C, 0x201D),  # double quotation marks
    (0x2022, 0x2022),  # bullet
    (0x2026, 0x2026),  # horizontal ellipsis
    (0x20AC, 0x20AC),  # euro sign
    (0x2122, 0x2122),  # trade mark sign
]

# OS/2 `ulUnicodeRange` bits for the blocks the ranges above reach into, so a
# widened face DECLARES what it covers rather than only carrying it. Nothing in
# this repo reads these - the atlas asks the `cmap` - but a face that lies about
# its coverage is a trap for any other tool that opens it.
#
# Bit numbers are the OS/2 spec's: 0..31 live in `ulUnicodeRange1`, 32..63 in
# `ulUnicodeRange2`.
OS2_RANGE_BITS = {
    (0x00A0, 0x00FF): 1,   # Latin-1 Supplement
    (0x2000, 0x206F): 31,  # General Punctuation
    (0x20A0, 0x20CF): 33,  # Currency Symbols
    (0x2100, 0x214F): 35,  # Letterlike Symbols
}


def declared_codepoints():
    """Every codepoint `TEXT_RANGES` names, in order."""
    return [cp for lo, hi in TEXT_RANGES for cp in range(lo, hi + 1)]


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
    base_cmap = base.getBestCmap()
    src_cmap = src.getBestCmap()
    declared = declared_codepoints()
    missing = [cp for cp in declared if cp not in src_cmap]
    if missing:
        sys.exit(f"source does not cover {len(missing)} declared codepoints: {missing[:8]}")

    # **The closure is taken over the codepoints being ADDED, not over the whole
    # declared coverage**, and the difference is 11 dead glyphs per run.
    #
    # `have` can only recognise a glyph by NAME, and a name is only reliable for
    # a glyph some `cmap` points at: the upstream face's `post` table is format
    # 3.0, so fontTools synthesises `glyphNNNNN` from the glyph INDEX for
    # everything else - and the base's indices are not the source's. So the
    # accent components of a Latin-1 composite the base ALREADY carries come
    # back from `closure` under a name `have` has never seen, and get appended a
    # second time: byte-identical duplicates that no `cmap` entry and no
    # composite in the face refers to.
    #
    # Restricting the closure to codepoints the base cannot already draw removes
    # the question. Anything reached from there is genuinely new, or is an
    # AGL-named glyph (`period` under `ellipsis`) whose name DOES resolve. The
    # first widening is unaffected - on an ASCII-only base every Latin-1
    # codepoint is new, which is exactly the set that run used.
    wanted = [src_cmap[cp] for cp in declared if cp not in base_cmap]
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

    # Only codepoints the base does not already name, for the same reason the
    # glyph append is filtered: re-pointing an entry that already resolves is a
    # no-op at best and, if the two faces ever disagreed, a silent substitution.
    for table in base["cmap"].tables:
        if table.isUnicode():
            for cp in declared:
                if cp not in table.cmap:
                    table.cmap[cp] = src_cmap[cp]

    os2 = base["OS/2"]
    os2.usFirstCharIndex = min(
        cp for t in base["cmap"].tables if t.isUnicode() for cp in t.cmap
    )
    os2.usLastCharIndex = min(
        0xFFFF, max(cp for t in base["cmap"].tables if t.isUnicode() for cp in t.cmap)
    )
    for (blo, bhi), bit in OS2_RANGE_BITS.items():
        if not any(blo <= cp <= bhi for cp in declared):
            continue
        if bit < 32:
            os2.ulUnicodeRange1 |= 1 << bit
        else:
            os2.ulUnicodeRange2 |= 1 << (bit - 32)

    base.save(out_path)
    print(
        f"{out_path}: {len(order)} glyphs (+{len(added)}), "
        f"{len(wanted)} codepoints added, {len(declared)} declared"
    )


if __name__ == "__main__":
    main()
