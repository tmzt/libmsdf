#!/usr/bin/env python3
"""Merge the terminal symbol page's face from the Noto faces.

**This is how `fonts/LibmsdfTerminalSymbols-Regular.ttf` is made**
(docs/design/20261023_01_TERMINAL_FONTS_PLAN.md, section 4.4 and the
02:32 EDT amendment):

    uv run --with fonttools==4.67.0 python3 -I fonts/terminal_symbols.py \\
        <upstream dir> <out.ttf>

The upstream directory is `fonts/upstream/`, filled by
`scripts/fetch-upstream-fonts.sh`. Every input's sha256 is checked first. The
output is deterministic: the same inputs give the same bytes.

1. Noto Sans Mono (wght 400, wdth 100), Noto Sans Symbols (wght 400) and
   Noto Emoji (wght 400) are made static instances; Noto Sans Symbols 2 and
   Noto Sans Math are static already. Each instance is round-tripped through
   the file format, so its coordinates are the integers a saved face stores.
2. For each codepoint of the TERMINAL group in `fonts/coverage-groups.txt`,
   the glyph comes from the first of Noto Sans Mono, Noto Sans Symbols 2,
   Noto Sans Symbols and Noto Sans Math that maps it; for TERMINAL_EMOJI,
   from Noto Emoji. A wide form (U+F0000 on) takes its base's emoji glyph.
3. Outlines are scaled to upem 1000 and set on Noto Sans Mono's 600-unit
   cell:
   - box drawing and block elements are remapped from their design span
     [-240, 973] onto the 1.3 em line box [-271, 1029], and every edge that
     touches the cell is pushed out (to x -40 or 640, y -371 or 1129) so
     cells join without a seam; the shades U+2591-2593 are only remapped;
   - every other glyph takes n cells (2 for a wide emoji or a wide form,
     otherwise 1), is fitted inside them less 30 units a side and inside the
     window less 30, and has its source advance box centred in them, so a
     family such as Braille keeps one position for every pattern (its ink
     is nudged inside the margins only where it would cross them).
4. The face has cmap, glyf, hmtx and names only: no GSUB, GPOS or hinting.
5. The output's sha256 is printed.
"""

import hashlib
import io
import pathlib
import sys

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.cu2quPen import Cu2QuPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

HERE = pathlib.Path(__file__).resolve().parent

# file in fonts/upstream | sha256 | instance (None for a static face)
SOURCES = {
    "mono": ("NotoSansMono[wdth,wght].ttf",
             "2cb2adb378a8f574213e23df697050b83c54c27df465a2015552740b2769a081",
             {"wght": 400, "wdth": 100}),
    "symbols2": ("NotoSansSymbols2-Regular.ttf",
                 "7d5fb73b7ca67a6798101741f5d280a3d016a56a197afcd4199dbb57b4b82a21", None),
    "symbols": ("NotoSansSymbols[wght].ttf",
                "f7e7e04b4a24b6c78893d50cbfd2b2f6cae49617ab047bfef668d252adb128f7",
                {"wght": 400}),
    "math": ("NotoSansMath-Regular.ttf",
             "3f495fe933c06786e4d5f6d86b8ee70b6753a68ee3b9d87528726de0f6e2c47d", None),
    "emoji": ("NotoEmoji[wght].ttf",
              "de6c18832938afc99caf132b39d6a30a19bac7f2e812e28db2535b4608d27551",
              {"wght": 400}),
}
TERMINAL_ORDER = ("mono", "symbols2", "symbols", "math")

# The wide forms of the narrow curated emoji: wide form -> base. The same
# pairs as libmsdf-font's `terminal::TERMINAL_WIDE_FORM`, which a test holds
# equal to this face's cmap.
WIDE_FORMS = {0xF0000: 0x26A0, 0xF0001: 0x27A1}
# The curated emoji that are narrow under wcwidth (East Asian Width N): one
# cell. Every other curated emoji is two.
NARROW_EMOJI = {0x26A0, 0x27A1}

UPEM = 1000
CELL = 600
WINDOW = (-271, 1029)   # the 1.3 em line box at cap 717, top_frac 0.24
DESIGN = (-240, 973)    # Noto Sans Mono's block design span
MARGIN = 30
OVERHANG = 100
EDGE_X = (-40, 640)
SHADES = range(0x2591, 0x2594)
BOX_BLOCK = range(0x2500, 0x25A0)

VERSION = "Version 1.000; libmsdf terminal symbols, merged from Noto"
COPYRIGHT = (
    "Copyright 2022 The Noto Project Authors "
    "(https://github.com/notofonts/latin-greek-cyrillic, "
    "https://github.com/notofonts/symbols); Copyright 2022 Google LLC "
    "(Noto Sans Math); Copyright 2013 Google LLC (Noto Emoji)"
)
DESCRIPTION = (
    "Merged by libmsdf from Noto Sans Mono, Noto Sans Symbols 2, Noto Sans "
    "Symbols, Noto Sans Math and Noto Emoji: the terminal symbol page, set on "
    "Noto Sans Mono's cell. Outlines scaled, remapped and fitted; a Modified "
    "Version under the SIL Open Font License 1.1."
)
LICENSE = (
    "This Font Software is licensed under the SIL Open Font License, Version "
    "1.1. This license is available with a FAQ at: https://openfontlicense.org"
)
LICENSE_URL = "https://openfontlicense.org"


def groups() -> dict[str, list[tuple[int, int]]]:
    out: dict[str, list[tuple[int, int]]] = {}
    for line in (HERE / "coverage-groups.txt").read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        group, _policy, rng = line.split()
        lo, _, hi = rng.removeprefix("U+").partition("-")
        out.setdefault(group, []).append((int(lo, 16), int(hi or lo, 16)))
    return out


def expand(ranges: list[tuple[int, int]]) -> list[int]:
    return sorted({cp for lo, hi in ranges for cp in range(lo, hi + 1)})


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load(updir: pathlib.Path, key: str) -> TTFont:
    name, want, instance = SOURCES[key]
    data = (updir / name).read_bytes()
    if sha256(data) != want:
        sys.exit(f"{name}: sha256 {sha256(data)}, expected {want} - not the recorded upstream")
    font = TTFont(io.BytesIO(data), recalcTimestamp=False)
    if instance is not None:
        font = instantiateVariableFont(font, instance)
        buf = io.BytesIO()
        font.save(buf)
        buf.seek(0)
        font = TTFont(buf, recalcTimestamp=False)
    return font


class Bounds:
    """The ink bounds of a glyph drawn through a transform."""

    def __init__(self, font: TTFont, gname: str):
        from fontTools.pens.boundsPen import BoundsPen

        pen = BoundsPen(font.getGlyphSet())
        font.getGlyphSet()[gname].draw(pen)
        self.box = pen.bounds


def draw(font: TTFont, gname: str, point) -> object:
    """The glyph `gname` of `font` as a TrueType glyph, every point mapped
    through `point(x, y) -> (x, y)`."""
    tt = TTGlyphPen(None)
    pen = Cu2QuPen(tt, max_err=1.0, reverse_direction=False) if "glyf" not in font else tt

    class PointPen:
        def __init__(self, out):
            self.out = out

        def moveTo(self, p):
            self.out.moveTo(point(*p))

        def lineTo(self, p):
            self.out.lineTo(point(*p))

        def qCurveTo(self, *ps):
            self.out.qCurveTo(*(None if p is None else point(*p) for p in ps))

        def curveTo(self, *ps):
            self.out.curveTo(*(point(*p) for p in ps))

        def closePath(self):
            self.out.closePath()

        def endPath(self):
            self.out.endPath()

        def addComponent(self, *a):
            raise AssertionError("components are drawn decomposed")

    from fontTools.pens.recordingPen import DecomposingRecordingPen

    rec = DecomposingRecordingPen(font.getGlyphSet())
    font.getGlyphSet()[gname].draw(rec)
    rec.replay(PointPen(pen))
    return tt.glyph()


def box_block(font: TTFont, gname: str, cp: int):
    s = UPEM / font["head"].unitsPerEm
    lo, hi = DESIGN
    k = (WINDOW[1] - WINDOW[0]) / (hi - lo)

    def point(x, y):
        x, y = x * s, y * s
        y = WINDOW[0] + (y - lo) * k
        if cp not in SHADES:
            if x <= 6:
                x = EDGE_X[0]
            elif x >= CELL - 6:
                x = EDGE_X[1]
            if y >= WINDOW[1] - 8:
                y = WINDOW[1] + OVERHANG
            elif y <= WINDOW[0] + 8:
                y = WINDOW[0] - OVERHANG
        return (round(x), round(y))

    return draw(font, gname, point), CELL


def fitted(font: TTFont, gname: str, cells: int):
    s = UPEM / font["head"].unitsPerEm
    box = Bounds(font, gname).box
    if box is None:
        return draw(font, gname, lambda x, y: (round(x * s), round(y * s))), cells * CELL
    x0, y0, x1, y1 = (v * s for v in box)
    # The advance box's centre, not the ink's: every glyph of a family
    # (Braille above all) keeps its place in the cell.
    cx, cy = font["hmtx"][gname][0] * s / 2, (y0 + y1) / 2
    k = 1.0
    room = cells * CELL - 2 * MARGIN
    if x1 - x0 > room:
        k = room / (x1 - x0)
    top, bottom = WINDOW[1] - MARGIN, WINDOW[0] + MARGIN
    half = (y1 - y0) * k / 2
    fit = min(top - cy, cy - bottom)
    if half > fit:
        k *= fit / half
    mid = cells * CELL / 2
    # Nudge the ink inside the margins if the advance box's centring put it
    # across one (k already makes it narrow enough to fit).
    left, right = (x0 - cx) * k + mid, (x1 - cx) * k + mid
    if left < MARGIN:
        mid += MARGIN - left
    elif right > cells * CELL - MARGIN:
        mid -= right - (cells * CELL - MARGIN)

    def point(x, y):
        return (round((x * s - cx) * k + mid), round((y * s - cy) * k + cy))

    return draw(font, gname, point), cells * CELL


def name_of(cp: int) -> str:
    if cp in WIDE_FORMS:
        return name_of(WIDE_FORMS[cp]) + ".wide"
    return f"uni{cp:04X}" if cp <= 0xFFFF else f"u{cp:05X}"


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    updir, out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    g = groups()
    terminal, emoji = expand(g["TERMINAL"]), expand(g["TERMINAL_EMOJI"])
    assert len(terminal) == 591, f"TERMINAL is {len(terminal)} codepoints, not 591"
    assert len(emoji) == 24, f"TERMINAL_EMOJI is {len(emoji)} codepoints, not 24"
    assert all(cp in emoji for cp in WIDE_FORMS), "a wide form outside TERMINAL_EMOJI"

    fonts = {key: load(updir, key) for key in SOURCES}
    cmaps = {key: f.getBestCmap() for key, f in fonts.items()}
    mono = fonts["mono"]

    order = [".notdef"]
    glyphs = {".notdef": draw(mono, ".notdef", lambda x, y: (round(x), round(y)))}
    advances = {".notdef": CELL}
    cmap = {}
    sources: dict[str, int] = {}
    for cp in terminal:
        key = next((k for k in TERMINAL_ORDER if cp in cmaps[k]), None)
        if key is None:
            sys.exit(f"U+{cp:04X}: no Noto face maps it")
        font, gname = fonts[key], cmaps[key][cp]
        if cp in BOX_BLOCK:
            assert key == "mono", f"U+{cp:04X} is box drawing but not from Noto Sans Mono"
            glyph, adv = box_block(font, gname, cp)
        else:
            glyph, adv = fitted(font, gname, 1)
        name = name_of(cp)
        order.append(name)
        glyphs[name], advances[name], cmap[cp] = glyph, adv, name
        sources[key] = sources.get(key, 0) + 1
    for cp in emoji:
        base = WIDE_FORMS.get(cp, cp)
        if base not in cmaps["emoji"]:
            sys.exit(f"U+{base:04X}: Noto Emoji does not map it")
        cells = 1 if cp in NARROW_EMOJI else 2
        glyph, adv = fitted(fonts["emoji"], cmaps["emoji"][base], cells)
        name = name_of(cp)
        order.append(name)
        glyphs[name], advances[name], cmap[cp] = glyph, adv, name
        sources["emoji"] = sources.get("emoji", 0) + 1

    for gl in glyphs.values():
        gl.recalcBounds(None)
    fb = FontBuilder(UPEM, isTTF=True)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap(cmap)
    fb.setupGlyf(glyphs)
    fb.setupHorizontalMetrics(
        {n: (advances[n], getattr(glyphs[n], "xMin", 0)) for n in order}
    )
    hhea, os2 = mono["hhea"], mono["OS/2"]
    fb.setupHorizontalHeader(ascent=hhea.ascent, descent=hhea.descent, lineGap=hhea.lineGap)
    fb.setupOS2(
        sTypoAscender=os2.sTypoAscender,
        sTypoDescender=os2.sTypoDescender,
        sTypoLineGap=os2.sTypoLineGap,
        usWinAscent=os2.usWinAscent,
        usWinDescent=os2.usWinDescent,
        sxHeight=os2.sxHeight,
        sCapHeight=os2.sCapHeight,
        usWeightClass=400,
        achVendID="NONE",
    )
    fb.setupNameTable(
        {
            "copyright": COPYRIGHT,
            "familyName": "Libmsdf Terminal Symbols",
            "styleName": "Regular",
            "uniqueFontIdentifier": "LibmsdfTerminalSymbols-Regular;1.000",
            "fullName": "Libmsdf Terminal Symbols Regular",
            "psName": "LibmsdfTerminalSymbols-Regular",
            "version": VERSION,
            "description": DESCRIPTION,
            "licenseDescription": LICENSE,
            "licenseInfoURL": LICENSE_URL,
        },
        mac=False,
    )
    fb.setupPost(keepGlyphNames=True)
    head = fb.font["head"]
    head.created = head.modified = mono["head"].created
    fb.font.recalcTimestamp = False
    fb.save(out)

    built = TTFont(out)
    assert len(built.getBestCmap()) == 591 + 24
    print(f"{out}: {len(order)} glyphs, {len(cmap)} codepoints; from {sources}")
    print(f"sha256 {sha256(out.read_bytes())}")


if __name__ == "__main__":
    main()
