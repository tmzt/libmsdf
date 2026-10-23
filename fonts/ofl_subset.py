#!/usr/bin/env python3
"""Subset an upstream OFL face to the bundled coverage groups.

**This is how `fonts/GrenzeGotisch[wght]-subset.ttf`,
`fonts/Texturina[opsz,wght]-subset.ttf` and
`fonts/NotoSansMono-{Regular,Bold}-subset.ttf` are made**
(docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md, 13:34 and 13:40 EDT
amendments; docs/design/20261023_01_TERMINAL_FONTS_PLAN.md, section 4.1):

    uv run --with fonttools==4.67.0 python3 -I fonts/ofl_subset.py \\
        <upstream.ttf> <out.ttf> <expected-upstream-sha256> \\
        [--instance <axis>=<value>[,<axis>=<value>...]]

uv 0.10 or later and Python 3.11 or later. The upstream files are build
inputs, fetched to the gitignored `fonts/upstream/` from google/fonts at the
commits named in `FONTS-LICENSE-OFL.txt`.

1. The upstream file's sha256 must equal the expected value.
2. With `--instance`, the face is first made a static instance at those axis
   values (every axis named; the Noto Sans Mono subsets use
   `wght=400,wdth=100` and `wght=700,wdth=100`), and the variation data goes
   with it. Without it, the variable face is kept whole.
3. The face is subset to the union of TEXT, SYMBOLS and MATH in
   `fonts/coverage-groups.txt` (411 codepoints), keeping every layout
   feature, glyph names, the whole `name` table and any variation data, and
   dropping hinting (`prep`, `fpgm`, `cvt `, glyph instructions), `gasp` and
   `DSIG`. The `head` timestamp is not recalculated, so the output is
   reproducible.
4. `name` IDs 5 and 10 record the modification (an OFL Modified Version;
   no face subset here declares a Reserved Font Name).
5. Provenance: every kept glyph's outline and advance, and every kept
   codepoint's glyph name, must equal the upstream file's (its instance,
   with `--instance`).
6. The output's sha256 is printed.
"""

import hashlib
import io
import pathlib
import shutil
import sys

if shutil.which("uv") is None:
    sys.exit("this needs `uv` (https://docs.astral.sh/uv/) on PATH")

from fontTools import subset  # noqa: E402
from fontTools.pens.recordingPen import RecordingPen  # noqa: E402
from fontTools.ttLib import TTFont  # noqa: E402
from fontTools.varLib.instancer import instantiateVariableFont  # noqa: E402

HERE = pathlib.Path(__file__).resolve().parent
VERSION_SUFFIX = "; libmsdf subset (U+0020-017F, punctuation)"
DESCRIPTION = (
    "Subset by libmsdf to Latin (U+0020-017F) and common punctuation and "
    "symbols; outlines, metrics and layout unchanged."
)


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


# The text groups a subset keeps. The file also lists the terminal symbol
# page's groups, which fonts/terminal_symbols.py reads.
TEXT_GROUPS = ("TEXT", "SYMBOLS", "MATH")


def codepoints() -> list[int]:
    cps = set()
    for group, ranges in groups().items():
        if group not in TEXT_GROUPS:
            continue
        for lo, hi in ranges:
            cps.update(range(lo, hi + 1))
    return sorted(cps)


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def parse_instance(arg: str) -> dict[str, float]:
    axes = {}
    for part in arg.split(","):
        tag, _, value = part.partition("=")
        axes[tag] = float(value)
    return axes


def load(src: pathlib.Path, instance: dict[str, float] | None) -> TTFont:
    font = TTFont(src, recalcTimestamp=False)
    if instance is not None:
        tags = {a.axisTag for a in font["fvar"].axes}
        if set(instance) != tags:
            sys.exit(f"--instance must name every axis: {sorted(tags)}")
        font = instantiateVariableFont(font, instance)
        # Round-trip through the file format, so that the instance's
        # coordinates are the integers a saved face stores, for the subset and
        # for the provenance check alike.
        buf = io.BytesIO()
        font.save(buf)
        buf.seek(0)
        font = TTFont(buf, recalcTimestamp=False)
    return font


def main() -> None:
    args = sys.argv[1:]
    instance = None
    if len(args) == 5 and args[3] == "--instance":
        instance = parse_instance(args[4])
        args = args[:3]
    if len(args) != 3:
        sys.exit(__doc__)
    src, out, expected = pathlib.Path(args[0]), pathlib.Path(args[1]), args[2]
    found = sha256(src)
    if found != expected:
        sys.exit(f"{src}: sha256 {found}, expected {expected} - not the recorded upstream")

    cps = codepoints()
    assert len(cps) == 411, f"the coverage groups' union is {len(cps)} codepoints, not 411"

    options = subset.Options()
    options.layout_features = ["*"]
    options.glyph_names = True
    options.name_IDs = ["*"]
    options.name_languages = ["*"]
    options.name_legacy = True
    options.notdef_outline = True
    options.hinting = False
    options.drop_tables += ["gasp", "DSIG"]
    options.recalc_timestamp = False

    font = load(src, instance)
    subsetter = subset.Subsetter(options=options)
    subsetter.populate(unicodes=cps)
    subsetter.subset(font)

    name = font["name"]
    suffix, description = VERSION_SUFFIX, DESCRIPTION
    if instance is not None:
        at = ",".join(f"{t}={v:g}" for t, v in instance.items())
        suffix += f"; instance {at}"
        description += f" Static instance at {at}."
    for rec in list(name.names):
        if rec.nameID == 5:
            rec.string = str(rec.toUnicode()) + suffix
    name.setName(description, 10, 3, 1, 0x409)
    name.setName(description, 10, 1, 0, 0)
    for tag in ("prep", "fpgm", "cvt ", "gasp", "DSIG"):
        assert tag not in font, f"{tag} survived the subset"

    font.save(out)

    # Provenance: the kept glyphs are the upstream glyphs.
    up, sub = load(src, instance), TTFont(out)
    up_cmap, sub_cmap = up.getBestCmap(), sub.getBestCmap()
    for cp, gname in sub_cmap.items():
        if up_cmap.get(cp) != gname:
            sys.exit(f"U+{cp:04X} maps to {gname} in the subset, {up_cmap.get(cp)} upstream")
    up_gs, sub_gs = up.getGlyphSet(), sub.getGlyphSet()
    for gname in sub.getGlyphOrder():
        a, b = RecordingPen(), RecordingPen()
        up_gs[gname].draw(a)
        sub_gs[gname].draw(b)
        if a.value != b.value:
            sys.exit(f"{gname}: outline differs from upstream")
        if up["hmtx"][gname] != sub["hmtx"][gname]:
            sys.exit(f"{gname}: advance differs from upstream")

    mapped = sorted(cp for cp in cps if cp in sub_cmap)
    print(f"{out}: {len(sub.getGlyphOrder())} glyphs, maps {len(mapped)} of {len(cps)} codepoints")
    print(f"sha256 {sha256(out)}")


if __name__ == "__main__":
    main()
