#!/usr/bin/env -S uv run --quiet --with fonttools python3
"""Merge BORROWED Material Symbols glyphs into a baked Highbay face.

**This, `icon.py`, `marker.py` and `widen.py` are how `fonts/*.ttf` are made,
and all four are additive by construction.** Existing glyphs are never
re-derived: this appends new outlines after every glyph already there, so no
glyph id moves and `GDEF`/`GPOS`/`GSUB` (kerning, the fi/fl/ffi/ffl ligatures)
are carried through untouched.

    ./msymbols.py <base.ttf> <out.ttf> <MaterialSymbolsOutlined[...].ttf>

Run for the MERGED face only (`Roboto-Regular-ascii-msymbols.ttf`). The other
bundled face is deliberately Roboto plus only what this repo DREW - see
`../src/font/mod.rs`, which states which face is which. `icon.py` and
`marker.py` run for both; this one does not, and that asymmetry is the entire
difference between the two faces.

# It reproduces the face that already shipped, and PROVES it

The first nine of the eighteen icons in `MSYMBOLS_ICONS` were merged in by an
uncommitted script (`48c58da`), so the first job here is to be that script
rather than a second one that merely looks similar. (This one has run twice
since, for four icons and then for five, and the shipped face's glyph ids show
all three waves: 111..=119, 231..=234, 247..=251.) Every icon already in the
base face is compared outline-for-outline and advance-for-advance against the
source, and any difference aborts the run - the same provenance check
`widen.py` makes against Roboto, and for the same reason: a face built half
from one upstream build and half from another is a defect nobody would see
until a glyph looked subtly wrong.

Source: **the `variablefont/` asset of
<https://github.com/google/material-design-icons>** - Material Symbols
Outlined, the variable face, instanced at its default location
(FILL 0, GRAD 0, opsz 24, wght 400) and scaled from 960 to Roboto's 2048 upem.
Those two steps in that order are what the check above pins.

# Codepoints are Material's, and are DECLARED by Material

Each name's codepoint is the one Material's own `.codepoints` manifest gives
it, not the lowest or the highest alias in the `cmap` (several names carry
legacy Material Icons aliases as well: `edit` answers to five codepoints, of
which `U+F097` is the declared one). Taking the declared value is what keeps
`MSYMBOLS_ICONS` a statement about a published catalogue rather than about
whichever alias a scan happened to pick.

`check` is the sharp case, because the alias is the one everybody remembers:
the face draws it at both `U+E5CA` and `U+E668`, `U+E5CA` is the Material
*Icons* codepoint, and the Material *Symbols* manifest declares NO name at
`U+E5CA` at all. Both reach the same outline today, so taking the alias would
have drawn the right tick under a codepoint the published catalogue does not
use, and nothing here or downstream would have said so.

# What is NOT here

A name Material does not publish. Those are drawn in `icon.py` at codepoints
this repo owns, and the two vocabularies never fall through to one another -
`../src/font/mod.rs`'s `highbay_codepoint` says why at length.

Material Symbols is (c) Google, licensed Apache-2.0 - see
`LICENSE-MaterialSymbols.txt`.
"""

import sys

from fontTools.pens.recordingPen import RecordingPen
from fontTools.ttLib import TTFont
from fontTools.ttLib.scaleUpem import scale_upem
from fontTools.varLib.instancer import instantiateVariableFont

UPEM = 2048

# The variable face's default location - the instance every shipped icon was
# taken at, and the one the provenance check below re-derives.
LOCATION = {"FILL": 0, "GRAD": 0, "opsz": 24, "wght": 400}

# **The borrowed coverage manifest, mirrored by `MSYMBOLS_ICONS` in
# `../src/font/mod.rs`.** Sorted by name, as that list is. Codepoints are
# Material's declared ones (see the module docstring).
#
# Kept deliberately small: a name outside this list has no glyph, which is a
# missing-asset error for the caller to report rather than substitute.
ICONS = [
    ("chat", 0xE0C9),
    ("check", 0xE668),
    ("chevron_left", 0xE5CB),
    ("chevron_right", 0xE5CC),
    ("code", 0xE86F),
    ("edit", 0xF097),
    ("expand_less", 0xE5CE),
    ("expand_more", 0xE5CF),
    ("home", 0xE9B2),
    ("library_books", 0xE02F),
    ("menu", 0xE5D2),
    ("more_vert", 0xE5D4),
    ("person", 0xF0D3),
    ("redo", 0xE15A),
    ("search", 0xEF7A),
    ("send", 0xE163),
    ("settings", 0xE8B8),
    ("undo", 0xE166),
]


def source_face(path):
    """Material Symbols Outlined at the default instance, at Roboto's upem."""
    src = TTFont(path)
    if "fvar" not in src:
        sys.exit(f"{path} is not the VARIABLE Material Symbols face")
    instantiateVariableFont(src, LOCATION, inplace=True, updateFontNames=False)
    scale_upem(src, UPEM)
    return src


def unicode_cmaps(font):
    """The font's unicode cmap subtables, de-duplicated by identity.

    A face can carry several unicode subtables that fontTools backs with the
    SAME dict, so writing one writes them all - `icon.py` and `marker.py`
    de-duplicate identically.
    """
    out, seen = [], set()
    for t in font["cmap"].tables:
        if t.isUnicode() and id(t.cmap) not in seen:
            seen.add(id(t.cmap))
            out.append(t)
    if not out:
        sys.exit("face has no unicode cmap subtable")
    return out


def check_already_merged(base, src, present):
    """Abort unless `src` draws every ALREADY-MERGED icon exactly as `base` does.

    The additive premise rests on this: if the two builds disagreed by even a
    unit, the face would end up half from one upstream release and half from
    another, and nothing downstream could tell.
    """
    bc, bg, sg = base.getBestCmap(), base.getGlyphSet(), src.getGlyphSet()
    bad = []
    for name, cp in present:
        bn = bc[cp]
        p1, p2 = RecordingPen(), RecordingPen()
        bg[bn].draw(p1)
        sg[name].draw(p2)
        if p1.value != p2.value:
            bad.append((name, "outline"))
        if base["hmtx"][bn] != src["hmtx"][name]:
            bad.append((name, "advance"))
    if bad:
        sys.exit(
            f"source is not the build the shipped icons came from: {len(bad)} diffs, {bad[:8]}"
        )
    return len(present)


def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__.strip().splitlines()[2].strip())
    base_path, out_path, src_path = sys.argv[1:4]

    base = TTFont(base_path)
    if base["head"].unitsPerEm != UPEM:
        sys.exit(f"base is {base['head'].unitsPerEm}upem; this merge targets {UPEM}upem")
    src = source_face(src_path)

    cmaps = unicode_cmaps(base)
    have_cp = base.getBestCmap()
    have_names = set(base.getGlyphOrder())

    present = [(n, cp) for n, cp in ICONS if cp in have_cp]
    missing = [(n, cp) for n, cp in ICONS if cp not in have_cp]
    checked = check_already_merged(base, src, present)

    src_glyf, src_hmtx = src["glyf"], src["hmtx"]
    added = []
    for name, cp in missing:
        if name not in src_glyf:
            sys.exit(f"{name} is not a glyph in the source face")
        glyph = src_glyf[name]
        if glyph.isComposite():
            # Every Material Symbols outline is simple at the default
            # instance; a composite would need its components merged too, and
            # silently dropping them would draw a blank icon.
            sys.exit(f"{name} is a composite glyph - components are not merged")
        out_name = f"uni{cp:04X}"
        if out_name in have_names:
            sys.exit(f"{out_name} is already a glyph name in the base face")
        base["glyf"].glyphs[out_name] = glyph
        base["hmtx"].metrics[out_name] = src_hmtx[name]
        for t in cmaps:
            t.cmap[cp] = out_name
        have_names.add(out_name)
        added.append((name, cp, out_name))

    if not added:
        print(f"{out_path}: nothing to add ({checked} icons already merged and verified)")

    order = base.getGlyphOrder() + [n for _, _, n in added]
    base.setGlyphOrder(order)
    base["glyf"].glyphOrder = order
    base["maxp"].numGlyphs = len(order)

    os2 = base["OS/2"]
    all_cps = [cp for t in cmaps for cp in t.cmap]
    os2.usFirstCharIndex = min(all_cps)
    os2.usLastCharIndex = min(0xFFFF, max(all_cps))
    os2.ulUnicodeRange2 |= 1 << (60 - 32)  # bit 60: Private Use Area (plane 0)

    base.save(out_path)
    listing = ", ".join(f"{n} U+{cp:04X}" for n, cp, _ in added)
    print(
        f"{out_path}: {len(order)} glyphs (+{len(added)}), "
        f"{checked} already merged and verified{', added ' + listing if added else ''}"
    )


if __name__ == "__main__":
    main()
