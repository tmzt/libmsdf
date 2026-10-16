#!/usr/bin/env python3
"""Make the test faces for the atlas layout tables' extraction and check tests.

    uv run --with fonttools python3 -I scripts/make-layout-test-face.py

Writes `tests/fixtures/layout/roboto-multiple-subst.ttf`: the bundled
`fonts/Roboto-Regular-ascii.ttf` with its GSUB replaced by one `liga` lookup
of type 2 (multiple substitution, `A` -> `A B`), under `DFLT`. The atlas
layout tables cannot carry a multiple substitution, so
`FaceLayout::extract` must refuse this face with `UnsupportedLookup`
(docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md, step 3).

Also writes `tests/fixtures/layout/roboto-ordered-singles.ttf`: the same
face with its GSUB replaced by two single-substitution lookups under
`liga` whose order matters (`a` -> `b`, then `b` -> `c`), for step 5's test
that reordering two Gsub lookups is caught by the equivalence check.

The output is deterministic for a given fontTools; it is committed so the
tests need no Python.
"""

import pathlib

from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.ttLib import TTFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "fonts" / "Roboto-Regular-ascii.ttf"
DIR = ROOT / "tests" / "fixtures" / "layout"

FACES = {
    "roboto-multiple-subst.ttf": "feature liga { sub A by A B; } liga;\n",
    "roboto-ordered-singles.ttf": (
        "lookup AB { sub a by b; } AB;\n"
        "lookup BC { sub b by c; } BC;\n"
        "feature liga { lookup AB; lookup BC; } liga;\n"
    ),
}

DIR.mkdir(parents=True, exist_ok=True)
for name, fea in FACES.items():
    font = TTFont(SRC, recalcTimestamp=False)
    del font["GSUB"]
    addOpenTypeFeaturesFromString(font, fea, tables=["GSUB"])
    out = DIR / name
    font.save(out)
    print(f"wrote {out.relative_to(ROOT)}")
