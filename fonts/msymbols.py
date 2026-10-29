#!/usr/bin/env -S uv run --quiet --with fonttools python3
"""Merge BORROWED Material Symbols glyphs into a baked Highbay face.

    ./msymbols.py <base.ttf> <out.ttf> <MaterialSymbolsOutlined[...].ttf> <MaterialSymbolsOutlined[...].codepoints>

**This, `icon.py`, `marker.py` and `widen.py` are how `fonts/*.ttf` are made,
and all four are additive by construction.** Existing glyphs are never
re-derived: this appends new outlines after every glyph already there, so no
glyph id moves and `GDEF`/`GPOS`/`GSUB` (kerning, the fi/fl/ffi/ffl ligatures)
are carried through untouched.

Run for the MERGED face only (`Roboto-Regular-ascii-msymbols.ttf`). The other
bundled face is deliberately Roboto plus only what this repo DREW - see
`../font/src/faces.rs`, which states which face is which. `icon.py` and
`marker.py` run for both; this one does not, and that asymmetry is the entire
difference between the two faces.

# The icons are listed in `msymbols-icons.txt`

The borrowed coverage manifest is the data file `msymbols-icons.txt` beside
this script (`<name> U+XXXX <group>`, sorted by name), which
`tests/msymbols_icons.rs` compares with `MSYMBOLS_ICONS` in
`../font/src/lib.rs`. There are 71 icons in five groups
(docs/design/20261029_01_MATERIAL_SYMBOLS_CORE_SET_PLAN.md, section 3.2). A
name outside the file has no glyph, which is a missing-asset error for the
caller to report rather than substitute.

# It reproduces the face that already shipped, and PROVES it

The first nine of the icons were merged in by an uncommitted script
(`48c58da`), so the first job here is to be that script rather than a second
one that merely looks similar. This one has run three times since: for four
icons, for five, and for the 53 of the core set, and the shipped face's glyph
ids show all four waves: 111..=119, 231..=234, 247..=251, 262..=314. Every
icon already in the base face is compared outline-for-outline and
advance-for-advance against the source, and any difference aborts the run -
the same provenance check `widen.py` makes against Roboto, and for the same
reason: a face built half from one upstream build and half from another is a
defect nobody would see until a glyph looked subtly wrong.

Source: **the `variablefont/` asset of
<https://github.com/google/material-design-icons>** - Material Symbols
Outlined, the variable face, instanced at its default location
(FILL 0, GRAD 0, opsz 24, wght 400) and scaled from 960 to Roboto's 2048 upem.
Those two steps in that order are what the check above pins.

# Codepoints are Material's, and are DECLARED by Material

Each name's codepoint is the one Material's own `.codepoints` manifest gives
it (the fourth argument, fetched by `../scripts/fetch-upstream-fonts.sh` at
the same commit), not the lowest or the highest alias in the `cmap` (several
names carry legacy Material Icons aliases as well: `edit` answers to five
codepoints, of which `U+F097` is the declared one). Taking the declared value
is what keeps `MSYMBOLS_ICONS` a statement about a published catalogue rather
than about whichever alias a scan happened to pick. Every line of
`msymbols-icons.txt` is checked against that manifest before anything is
merged.

`check` is the sharp case, because the alias is the one everybody remembers:
the face draws it at both `U+E5CA` and `U+E668`, `U+E5CA` is the Material
*Icons* codepoint, and the Material *Symbols* manifest declares NO name at
`U+E5CA` at all. Both reach the same outline today, so taking the alias would
have drawn the right tick under a codepoint the published catalogue does not
use, and nothing here or downstream would have said so.

# The checks

Before merging, every failure is collected and reported together, each
naming its line of `msymbols-icons.txt`:
1. the file is sorted by name, with no repeated name or codepoint;
2. every codepoint is in U+E000..U+F7FF (U+F800..U+F8FF are this repo's
   owned blocks);
3. every `(name, codepoint)` is declared in the `.codepoints` manifest;
4. the source `cmap` maps the codepoint to the glyph `name`;
5. the glyph is simple (not composite) and has contours;
6. a codepoint the base face already maps is an icon already merged, drawn
   exactly as the source draws it (the provenance check).
After merging, the borrowed range U+E000..U+F7FF of the face about to be
saved must map exactly the file's codepoints - none missing, none extra - each
to a glyph with contours.

# What is NOT here

A name Material does not publish. Those are drawn in `icon.py` at codepoints
this repo owns, and the two vocabularies never fall through to one another -
`../font/src/lib.rs`'s `highbay_codepoint` says why at length. Also not here:
Material's `error`, declared only at U+F8B6 inside the owned blocks (`report`
takes its place), and Material's `folder`, whose name the drawn icon holds.

Material Symbols is (c) Google, licensed Apache-2.0 - see
`../FONTS-LICENSE-APACHE.txt`.
"""

import sys
from pathlib import Path

from fontTools.pens.recordingPen import RecordingPen
from fontTools.ttLib import TTFont
from fontTools.ttLib.scaleUpem import scale_upem
from fontTools.varLib.instancer import instantiateVariableFont

UPEM = 2048

# The variable face's default location - the instance every shipped icon was
# taken at, and the one the provenance check below re-derives.
LOCATION = {"FILL": 0, "GRAD": 0, "opsz": 24, "wght": 400}

# The borrowed range: the BMP private use area below this repo's owned blocks.
BORROWED = range(0xE000, 0xF800)

GROUPS = ("navigation", "actions", "media", "status", "things")

MANIFEST = Path(__file__).with_name("msymbols-icons.txt")


def read_icons(path, errors):
    """`[(line number, name, codepoint)]` of `msymbols-icons.txt`.

    Malformed lines, and checks 1 and 2 of the module docstring, append to
    `errors` rather than stopping.
    """
    icons = []
    for no, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if (
            len(parts) != 3
            or not parts[1].startswith("U+")
            or parts[2] not in GROUPS
        ):
            errors.append(f"{path.name}:{no}: `{line}` is not `<name> U+XXXX <group>`")
            continue
        try:
            cp = int(parts[1][2:], 16)
        except ValueError:
            errors.append(f"{path.name}:{no}: `{parts[1]}` is not a codepoint")
            continue
        icons.append((no, parts[0], cp))

    seen_names, seen_cps = {}, {}
    for i, (no, name, cp) in enumerate(icons):
        if i and name.encode() <= icons[i - 1][1].encode() and name not in seen_names:
            errors.append(
                f"{path.name}:{no}: `{name}` is out of order (after `{icons[i - 1][1]}`)"
            )
        if name in seen_names:
            errors.append(f"{path.name}:{no}: `{name}` repeats line {seen_names[name]}")
        if cp in seen_cps:
            errors.append(f"{path.name}:{no}: U+{cp:04X} repeats line {seen_cps[cp]}")
        seen_names.setdefault(name, no)
        seen_cps.setdefault(cp, no)
        if cp not in BORROWED:
            errors.append(
                f"{path.name}:{no}: {name} U+{cp:04X} is outside U+E000..U+F7FF"
            )
    return icons


def read_declared(path):
    """Material's `.codepoints` manifest: `{name: codepoint}`."""
    declared = {}
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if line.strip():
            name, hexcp = line.split()
            declared[name] = int(hexcp, 16)
    return declared


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


def has_contours(glyf, name):
    g = glyf[name]
    return not g.isComposite() and g.numberOfContours > 0


def check_source(icons, declared, src, errors):
    """Checks 3 to 5 of the module docstring."""
    sc, sglyf = src.getBestCmap(), src["glyf"]
    for no, name, cp in icons:
        where = f"{MANIFEST.name}:{no}: {name} U+{cp:04X}"
        if name not in declared:
            errors.append(f"{where}: `{name}` is not declared in the .codepoints manifest")
            continue
        if declared[name] != cp:
            errors.append(
                f"{where}: the .codepoints manifest declares `{name}` at U+{declared[name]:04X}"
            )
            continue
        if sc.get(cp) != name:
            errors.append(f"{where}: the source cmap maps U+{cp:04X} to {sc.get(cp)!r}")
            continue
        if sglyf[name].isComposite():
            # Every Material Symbols outline is simple at the default
            # instance; a composite would need its components merged too, and
            # silently dropping them would draw a blank icon.
            errors.append(f"{where}: a composite glyph (components are not merged)")
        elif sglyf[name].numberOfContours <= 0:
            errors.append(f"{where}: the source glyph has no contours")


def check_already_merged(base, src, present, errors):
    """Check 6: `src` draws every ALREADY-MERGED icon exactly as `base` does.

    The additive premise rests on this: if the two builds disagreed by even a
    unit, the face would end up half from one upstream release and half from
    another, and nothing downstream could tell.
    """
    bc, bg, sg = base.getBestCmap(), base.getGlyphSet(), src.getGlyphSet()
    for no, name, cp in present:
        where = f"{MANIFEST.name}:{no}: {name} U+{cp:04X}"
        bn = bc[cp]
        if name not in sg:
            continue  # reported by check_source
        p1, p2 = RecordingPen(), RecordingPen()
        bg[bn].draw(p1)
        sg[name].draw(p2)
        if p1.value != p2.value:
            errors.append(f"{where}: the base face's outline differs from the source")
        if base["hmtx"][bn] != src["hmtx"][name]:
            errors.append(f"{where}: the base face's advance differs from the source")


def fail(errors):
    sys.exit(
        f"msymbols.py: {len(errors)} failure(s), nothing written:\n  " + "\n  ".join(errors)
    )


def main():
    if len(sys.argv) != 5:
        sys.exit(__doc__.strip().splitlines()[2].strip())
    base_path, out_path, src_path, codepoints_path = sys.argv[1:5]

    errors = []
    icons = read_icons(MANIFEST, errors)
    declared = read_declared(codepoints_path)

    base = TTFont(base_path)
    if base["head"].unitsPerEm != UPEM:
        sys.exit(f"base is {base['head'].unitsPerEm}upem; this merge targets {UPEM}upem")
    src = source_face(src_path)
    check_source(icons, declared, src, errors)

    cmaps = unicode_cmaps(base)
    have_cp = base.getBestCmap()
    have_names = set(base.getGlyphOrder())

    present = [(no, n, cp) for no, n, cp in icons if cp in have_cp]
    missing = [(no, n, cp) for no, n, cp in icons if cp not in have_cp]
    check_already_merged(base, src, present, errors)
    for no, name, cp in missing:
        if f"uni{cp:04X}" in have_names:
            errors.append(
                f"{MANIFEST.name}:{no}: uni{cp:04X} is already a glyph name in the base face"
            )
    if errors:
        fail(errors)

    src_glyf, src_hmtx = src["glyf"], src["hmtx"]
    added = []
    for _, name, cp in missing:
        out_name = f"uni{cp:04X}"
        base["glyf"].glyphs[out_name] = src_glyf[name]
        base["hmtx"].metrics[out_name] = src_hmtx[name]
        for t in cmaps:
            t.cmap[cp] = out_name
        have_names.add(out_name)
        added.append((name, cp, out_name))

    order = base.getGlyphOrder() + [n for _, _, n in added]
    base.setGlyphOrder(order)
    base["glyf"].glyphOrder = order
    base["maxp"].numGlyphs = len(order)

    os2 = base["OS/2"]
    all_cps = [cp for t in cmaps for cp in t.cmap]
    os2.usFirstCharIndex = min(all_cps)
    os2.usLastCharIndex = min(0xFFFF, max(all_cps))
    os2.ulUnicodeRange2 |= 1 << (60 - 32)  # bit 60: Private Use Area (plane 0)

    # The face about to be saved: its borrowed range is exactly the file.
    want = {cp for _, _, cp in icons}
    for t in cmaps:
        got = {cp for cp in t.cmap if cp in BORROWED}
        for cp in sorted(want - got):
            errors.append(f"after merging: U+{cp:04X} is not mapped")
        for cp in sorted(got - want):
            errors.append(
                f"after merging: U+{cp:04X} is mapped (to {t.cmap[cp]}) but not in {MANIFEST.name}"
            )
        for cp in sorted(want & got):
            if not has_contours(base["glyf"], t.cmap[cp]):
                errors.append(f"after merging: U+{cp:04X} maps to a glyph with no contours")
    if errors:
        fail(errors)

    base.save(out_path)
    if added:
        listing = ", ".join(f"{n} U+{cp:04X}" for n, cp, _ in added)
        print(
            f"{out_path}: {len(order)} glyphs (+{len(added)}), "
            f"{len(present)} already merged and verified, added {listing}"
        )
    else:
        print(f"{out_path}: nothing to add ({len(present)} icons already merged and verified)")


if __name__ == "__main__":
    main()
