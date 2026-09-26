#!/usr/bin/env python3
"""Bake libmsdf's MSDF atlases into `out/`, and say what moved.

    ./scripts/bake-atlases.py                   # bake both into out/, report the cell delta
    ./scripts/bake-atlases.py --reference-dir D # ...comparing against D/<name>.atlas too
    ./scripts/bake-atlases.py --update-fixture  # ...and refresh tests/fixtures from out/
    ./scripts/bake-atlases.py --check           # verify the fixture only; writes nothing

**This is libmsdf's copy of highbay's `scripts/bake-atlases.py`.** That one
writes each artifact straight into the crate that commits it; this one writes
only into `out/`, which `.gitignore` keeps out of the repo, so a bake never
dirties a tree it does not own. Moving an artifact from `out/` to where it is
committed is a separate, deliberate copy:

    out/roboto-msymbols-48.atlas  ->  highbay  crates/libhbui/assets/
    out/roboto-ascii-48.atlas     ->  highbay  src/assets/
                                  ->  libmsdf  tests/fixtures/  (--update-fixture)

The plain bake's delta is reported against the committed fixture. The merged
bake has no committed copy in this repo, so pass `--reference-dir` (highbay's
`crates/libhbui/assets`, say) to get its delta - that is the one whose cells
moving would matter most.

The rest of this docstring is the parent script's and still holds, except that
the section on where the script lives is answered above.

**This exists because the atlas half of the font pipeline had no producer.**
`deps/libmsdf/fonts/{msymbols,icon,marker,widen}.py` make the `.ttf` faces and
each aborts on a provenance mismatch. The `.atlas` files were made by a
paragraph in a doc comment that somebody had to know to run - and on 2026-08-30
somebody did not. `6b483299` added `check` and four chevrons to
`libmsdf::MSYMBOLS_ICONS` and to the merged face; the committed atlas was not
re-baked, so for ten days those five names RESOLVED and drew nothing at all,
reported as a `DrawFinding::MissingIcon` that no frame and no fixture surfaces.
A red test told readers to replace working strokes with an invisible `<Icon>`.

`libhbui::draw`'s icon path takes TWO doors - the manifest resolves the name,
the ATLAS supplies the cell - and only the first had been widened.
`TextShaper::covers` reads the `cmap` and cannot see a missing cell, which is
exactly how the gap survived every test in the repo. So this script's real job
is not to save typing: it is to make "the face changed" and "the atlas changed"
one action.

# Why this is in `scripts/` and not in `deps/libmsdf/fonts/`

The font scripts write files INSIDE libmsdf, beside their own sources. An atlas
bake writes into two crates OUTSIDE it - `crates/libhbui/assets/` and
`src/assets/` - as well as a libmsdf test fixture. Producing another package's
committed artifact is a repo-level concern, so it lives with the repo's other
tooling.

# The `--style` flags are NOT optional, and leaving them off is destructive

`crates/libhbui/assets/roboto-msymbols-48.atlas` is a LAYERED (v4) atlas:
`(48, Regular)`, `(48, Bold)` and `(48, Italic)`, which is what
`libhbui::surface::BAKED_STYLES` declares and `crates/libhbui/tests/emphasis.rs`
holds it to. Baking it without `--style bold --style italic` produces a valid,
loadable, single-layer v3 file 430 cells smaller, and every `**bold**` and
`*italic*` run in the repo silently loses its cut.

`bake_atlas.rs`'s header carried exactly that incantation - it was written
before layers existed and was never revised - so the "standard command" would
have quietly undone the emphasis work. The flags belong in a file that runs,
not in prose that has to be remembered.

# The bake is REPRODUCIBLE, and that is what makes `--check` sound

Verified by baking twice and comparing: byte-identical. What is NOT established
is reproducibility across ARCHITECTURES - MSDF generation is float math and an
arm64 and an x86 host have never been compared. Run this where the artifact was
baked. If it ever fails elsewhere on pixels alone, compare the INDEX (glyph
ids, cell coordinates, dimensions) that this script prints rather than raising a
tolerance.
"""

from __future__ import annotations

import argparse
import os
import pathlib
import struct
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent  # libmsdf
OUT = ROOT / "out"
FIXTURE = "tests/fixtures/roboto-ascii-48.atlas"

MERGED_FACE = "fonts/Roboto-Regular-ascii-msymbols.ttf"
PLAIN_FACE = "fonts/Roboto-Regular-ascii.ttf"

# (name in out/, face, extra flags, committed copy in this repo or None, what it is)
ATLASES = [
    (
        "roboto-msymbols-48.atlas",
        MERGED_FACE,
        ["--style", "bold", "--style", "italic"],
        None,
        "merged face, 3 layers (Regular/Bold/Italic) - libhbui::surface::BAKED_ATLAS",
    ),
    (
        "roboto-ascii-48.atlas",
        PLAIN_FACE,
        [],
        FIXTURE,
        "plain face, 1 layer - libmsdf's test fixture and highbay's src/assets",
    ),
]

GLYPH_SIZE = "48"
PX_RANGE = "6.0"


def index(path: pathlib.Path):
    """The atlas's header and per-glyph cell table - everything but the pixels.

    A cell is `(x, y, w, h, layer)`. Comparing these is what answers the only
    question that matters after a coverage widening: did anything that already
    drew MOVE?
    """
    d = path.read_bytes()
    if d[0:4] != b"hbfa":
        raise SystemExit(f"{path} is not an atlas (bad magic)")
    ver, w, h, ng, ch, sc = struct.unpack_from("<6I", d, 4)
    off = 28
    layers = []
    if ver == 4:
        (lc,) = struct.unpack_from("<I", d, off)
        off += 4
        for _ in range(lc):
            size_px, style = struct.unpack_from("<HB", d, off)
            layers.append((size_px, style))
            off += 4
    else:
        layers = [(int(GLYPH_SIZE), 0)]
    sets = []
    for _ in range(sc):
        s, gc = struct.unpack_from("<HH", d, off)
        sets.append((s, gc))
        off += 36
    cells = {}
    for _ in range(ng):
        gid, ax, ay, aw, ah, lay = struct.unpack_from("<6H", d, off)
        cells[gid] = (ax, ay, aw, ah, lay)
        off += 32
    return {
        "version": ver,
        "width": w,
        "height": h,
        "glyphs": ng,
        "sets": sets,
        "layers": layers,
        "cells": cells,
        "bytes": len(d),
    }


def describe(a) -> str:
    return (
        f"v{a['version']} {a['width']}x{a['height']} "
        f"{a['glyphs']} cells, {len(a['layers'])} layer(s), {a['bytes']} bytes"
    )


def bake(face: str, out: pathlib.Path, extra: list[str], check: bool) -> None:
    cmd = [
        "cargo", "run", "--quiet", "--manifest-path", str(ROOT / "Cargo.toml"),
        "--features", "cpu-bake", "--example", "bake_atlas", "--",
        face, str(out), GLYPH_SIZE, PX_RANGE,
    ] + extra
    if check:
        cmd.append("--check")
    # Flushed so `bake_atlas`'s own lines land under the heading they belong to
    # rather than ahead of every heading at exit.
    sys.stdout.flush()
    result = subprocess.run(cmd, cwd=ROOT, env={**os.environ, "CARGO_INCREMENTAL": "0"})
    if result.returncode != 0:
        raise SystemExit(f"bake_atlas failed for {out} (exit {result.returncode})")


def report(name: str, before, after) -> bool:
    """Print the delta. Returns True if any EXISTING cell moved."""
    added = sorted(set(after["cells"]) - set(before["cells"]))
    removed = sorted(set(before["cells"]) - set(after["cells"]))
    moved = [
        g for g in sorted(set(before["cells"]) & set(after["cells"]))
        if before["cells"][g] != after["cells"][g]
    ]
    print(f"    before: {describe(before)}")
    print(f"    after:  {describe(after)}")
    print(f"    cells:  {before['glyphs']} -> {after['glyphs']}")
    if added:
        print(f"    ADDED   gids {added[0]}..{added[-1]} ({len(added)})" if len(added) > 3
              else f"    ADDED   gids {added}")
    if removed:
        print(f"    REMOVED gids {removed}")
    if moved:
        print(f"    *** {len(moved)} EXISTING CELL(S) MOVED: {moved[:12]}")
        for g in moved[:5]:
            print(f"          gid {g}: {before['cells'][g]} -> {after['cells'][g]}")
        return True
    print("    moved:  none - every existing cell kept its coordinates")
    return False


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument(
        "--check", action="store_true",
        help=f"verify {FIXTURE} is what the shipped face bakes to; write nothing",
    )
    ap.add_argument(
        "--reference-dir", type=pathlib.Path,
        help="also report each bake's delta against <dir>/<name>.atlas "
             "(e.g. highbay's crates/libhbui/assets for the merged bake)",
    )
    ap.add_argument(
        "--update-fixture", action="store_true",
        help=f"after baking, copy out/roboto-ascii-48.atlas over {FIXTURE}",
    )
    args = ap.parse_args()

    any_moved = False
    failures = []
    for name, face, extra, committed, what in ATLASES:
        if not (ROOT / face).is_file():
            failures.append(f"{name}: face {face} is missing")
            print(f"\n{name}\n    MISSING FACE: {face}")
            continue

        if args.check:
            if committed is None:
                continue  # nothing of this bake is committed here to verify
            print(f"\n{committed}\n    {what}")
            try:
                bake(face, ROOT / committed, extra, check=True)
                print(f"    OK - committed bytes are what this bake produces "
                      f"({describe(index(ROOT / committed))})")
            except SystemExit as e:
                failures.append(f"{committed}: {e}")
                print(f"    STALE - {e}")
            continue

        out = OUT / name
        print(f"\nout/{name}\n    {what}")
        # Bake to a scratch path FIRST, so the report is against what was there
        # and a failed bake leaves the previous out/ file intact.
        with tempfile.TemporaryDirectory() as tmp:
            candidate = pathlib.Path(tmp) / name
            bake(face, candidate, extra, check=False)
            after = index(candidate)
            references = []
            if committed is not None:
                references.append((committed, ROOT / committed))
            if args.reference_dir is not None:
                references.append((str(args.reference_dir / name), args.reference_dir / name))
            for label, ref in references:
                print(f"  against {label}")
                if ref.is_file():
                    any_moved |= report(label, index(ref), after)
                else:
                    print("    (no such file - nothing to compare)")
            if not references:
                print(f"    NEW: {describe(after)} (no reference; see --reference-dir)")
            OUT.mkdir(exist_ok=True)
            out.write_bytes(candidate.read_bytes())
        print(f"    wrote out/{name}")
        if args.update_fixture and committed is not None:
            (ROOT / committed).write_bytes(out.read_bytes())
            print(f"    updated {committed}")

    if failures:
        print("\n" + "\n".join(f"FAIL {f}" for f in failures))
        return 1
    if any_moved:
        print(
            "\n*** A CELL THAT ALREADY DREW HAS MOVED.\n"
            "    Every text frame in the repo depends on cells NOT moving, and\n"
            "    `bake_atlas.rs`'s header explains why they normally cannot:\n"
            "    cells are laid out in `(set, glyph id)` order, so a widening that\n"
            "    lands in the last set of a layer is a strict superset. If a cell\n"
            "    moved, that property no longer holds - say so loudly, and expect\n"
            "    frames to move."
        )
        return 1
    if args.check:
        print("\nOK - the committed fixture is what the shipped face bakes to.")
        return 0
    print("\nOK - all atlases baked into out/, no existing cell moved.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
