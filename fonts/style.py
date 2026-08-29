#!/usr/bin/env -S uv run --quiet --with fonttools python3
"""Subset an upstream Roboto STYLE face to the declared text coverage.

**This is how `fonts/Roboto-Bold-ascii.ttf` and `Roboto-Italic-ascii.ttf` are
made.** They are the same shapes the shipped regular face draws, in a heavier
or slanted cut, so that a `**bold**` run in prose can resolve to real bold
outlines instead of a synthesized slant or a fatter regular.

    ./style.py <roboto-android/Roboto-Bold.ttf> <out.ttf> \\
               <roboto-android/Roboto-Regular.ttf>

Unlike `widen.py`/`icon.py`/`marker.py`, this does NOT edit a shipped face -
it produces a NEW one, so nothing it does can move a glyph that has already
been baked. The additive rule those three keep is not needed here; what
replaces it is the provenance check below.

# It is the SIBLING of the shipped face, and the script proves it

A bold cut only reads as emphasis of the prose beside it if both come from the
same drawing. So the third argument is the `Roboto-Regular.ttf` shipped in the
same upstream drop as the style face, and every printable-ASCII outline and
advance of it is compared against `Roboto-Regular-ascii.ttf` - exactly
`widen.py`'s check, for exactly its reason. A style face from a different build
would pass every test in this repo and simply look wrong next to the text.

Source: **Roboto v2.138, the `roboto-android.zip` asset of
<https://github.com/googlefonts/roboto/releases/tag/v2.138>** - the build the
shipped ASCII half came from.

# What is NOT in a style face

* **The Private Use Area.** Markers, our own icons and the borrowed Material
  Symbols are style-INVARIANT: they are geometry a renderer reaches for and
  names an app asks for, not prose, and a bold arrowhead is not a thing.
  `FontAtlasBuilder::add_styled_coverage` queues text ranges only, so a styled
  face that carried them would still contribute no cell.
* **An outline on glyph 0.** The placeholder is style-invariant too - one
  glyph, addressed as glyph 0 in every style (`GlyphStyle::styled_glyph_id`) -
  so a styled `.notdef` would be a second box that nothing can ever reach.
  `the_placeholder_is_style_invariant` pins the empty outline against these
  bytes rather than trusting this comment.
* **Hinting.** TrueType instructions steer a rasterizer at small ppem; MSDF
  baking reads outlines and never runs them, so they are dead weight in a face
  whose only job is to be baked. Dropping them is most of why a subset face is
  ~19 KiB rather than ~64.
"""

import sys

from fontTools import subset
from fontTools.pens.recordingPen import RecordingPen
from fontTools.ttLib import TTFont

# The declared text coverage, and the one place it is written on this side of
# the wall: `../src/font/mod.rs`'s `TEXT_RANGES`. A style face covers exactly
# what the regular face covers, so `bold` is never narrower than the prose it
# emphasizes - `a_style_face_covers_the_declared_text_ranges` checks the two
# against each other.
TEXT_RANGES = [(0x0020, 0x007E), (0x00A0, 0x00FF)]


def check_ascii_identical(base, src):
    """Abort unless `src` draws printable ASCII exactly as `base` already does.

    Verbatim from `widen.py`, and load-bearing for the same reason there: it
    is what makes "the same upstream drop" a checked fact rather than a note
    about which zip someone downloaded.
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
        sys.exit(
            f"the sibling Regular is not the shipped face's build: "
            f"{len(bad)} diffs, {bad[:8]}"
        )


def main():
    here = __file__.rsplit("/", 1)[0]
    style_path, out_path, sibling_path = sys.argv[1], sys.argv[2], sys.argv[3]

    check_ascii_identical(
        TTFont(f"{here}/Roboto-Regular-ascii.ttf"), TTFont(sibling_path)
    )

    font = TTFont(style_path)
    unicodes = [cp for lo, hi in TEXT_RANGES for cp in range(lo, hi + 1)]
    missing = [cp for cp in unicodes if cp not in font.getBestCmap()]
    if missing:
        sys.exit(f"{style_path} does not cover {len(missing)} declared codepoints")

    options = subset.Options()
    # **Every feature the SHAPER can ask for, and no more.** The subsetter's
    # own default list is what HarfBuzz turns on for horizontal Latin (`ccmp`,
    # `liga`, `clig`, `calt`, `kern`, `mark`, `mkmk`, `locl`, `rlig`...), and
    # `TextShaper`'s two default features are the addition: `lnum`/`pnum`, the
    # ones `GlyphSet::ShapedText` exists because of.
    #
    # Keeping `["*"]` instead costs 133 glyphs and 12 KiB per face for
    # stylistic sets, small caps and oldstyle figures that nothing in this repo
    # can request - dead outlines that would still have to be shipped to a
    # browser. Narrowing it further is the risk this balances against: a
    # feature the shaper enables and the face has been stripped of shapes to a
    # glyph id the atlas never baked, which now draws the placeholder BOX.
    options.layout_features = subset.Options().layout_features + ["lnum", "pnum"]
    # Dead weight in a face that is only ever baked - see the module docstring.
    options.hinting = False
    # Glyph 0 stays outline-less: the placeholder is style-invariant.
    options.notdef_outline = False
    options.name_IDs = ["*"]
    options.recalc_bounds = True

    subsetter = subset.Subsetter(options=options)
    subsetter.populate(unicodes=unicodes)
    subsetter.subset(font)
    font.save(out_path)

    n = font["maxp"].numGlyphs
    print(f"{out_path}: {n} glyphs, {len(unicodes)} codepoints")


if __name__ == "__main__":
    main()
