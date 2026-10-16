#!/usr/bin/env python3
"""Make the test face for the atlas layout tables' unsupported-lookup test.

    uv run --with fonttools python3 -I scripts/make-layout-test-face.py

Writes `tests/fixtures/layout/roboto-multiple-subst.ttf`: the bundled
`fonts/Roboto-Regular-ascii.ttf` with its GSUB replaced by one `liga` lookup
of type 2 (multiple substitution, `A` -> `A B`), under `DFLT`. The atlas
layout tables cannot carry a multiple substitution, so
`FaceLayout::extract` must refuse this face with `UnsupportedLookup`
(docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md, step 3). The output is
deterministic for a given fontTools; it is committed so the test needs no
Python.
"""

import pathlib

from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.ttLib import TTFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "fonts" / "Roboto-Regular-ascii.ttf"
OUT = ROOT / "tests" / "fixtures" / "layout" / "roboto-multiple-subst.ttf"

font = TTFont(SRC)
del font["GSUB"]
addOpenTypeFeaturesFromString(font, "feature liga { sub A by A B; } liga;\n", tables=["GSUB"])
OUT.parent.mkdir(parents=True, exist_ok=True)
font.save(OUT)
print(f"wrote {OUT.relative_to(ROOT)}")
