---
title: Font evidence sheets from a libmsdf example
date: 2026-10-23
status: Draft
covers: >
  One example, `examples/font_sheets.rs`, that bakes, draws and composes the
  font evidence sheets libmsdf's FONT documents cite, replacing the scratch
  Rust crates and Pillow scripts kept beside each sheet under out/fonts/.
  Its subcommands cover the three kinds of sheet made so far: a face's cells
  against its outlines (with candidate cell boxes or cap placements baked in
  memory), terminal samples through `TerminalShaper`, and a glyph-list
  sheet. The reference panel is the glyph's own outline filled as a path by
  libmsdf, not FreeType; labels and guides are libmsdf instances; output
  goes to the next free `out/fonts/YYYYMMDD_NN_<name>/` with metrics and a
  log entry. No public API change.
sources:
  - >
    Owner (Tim), 2026-10-23, in the libcontrols session: "we should also move
    to generating these sheets via examples, not conjured python each time";
    then "let's get 0.6.0 out first then we'll plan and release that examples
    change, I just want to move in that direction"; then, after 0.6.0, "move
    the examples refactor to a subagent".
  - >
    The scratch tools kept with the 2026-10-23 evidence:
    out/fonts/20261023_01_cap_clipping/tool/ (clipviz: in-memory bakes at
    candidate cap placements, sheet.py), 20261023_02_cell_box_prototype/tool/
    (cellviz against a patched copy, cellsheet.py, which also measured each
    line's ink extents against FreeType), 20261023_03_terminal_samples/tool/
    (termviz through TerminalShaper, termspec.py, termsheet.py) and
    20261023_04_cell_box_release/tool/ (relviz, relsheet.py; ccspec.py,
    ccsheet.py).
  - >
    src/drawlist/mod.rs (`push_path_fill`, `push_shaped_text_with`,
    `push_terminal_line`, `SdfInstance`, `Elevation`), src/drawlist/path.rs
    (`PathBuilder`: move_to, line_to, quad_to, cubic_to, close, build;
    even-odd fill; `PATH_MAX_SEGMENTS` = 256; `PathError`),
    font/src/outline.rs (`extract_outline` over ttf-parser to a
    `GlyphOutline` of lines, quads and cubics in font units, y-up, by
    contour), font/src/bake.rs (`Baker::bundled_options`,
    `bake_face_report`, `FaceReport::ink_outside`), font/src/bundled.rs
    (`BundledAtlas`), tests/terminal_render.rs and tests/cell_box_render.rs
    (the headless GPU render and readback this reuses), Cargo.toml (the
    `bake_atlas` example's `required-features`; `png` and `pollster` as
    dev-dependencies).
related:
  - docs/design/20261023_02_PER_ATLAS_CELL_BOX_PLAN.md (the cell box the
    cell sheets show, and the evidence this replaces the tools for)
  - docs/design/20261023_01_TERMINAL_FONTS_PLAN.md (the terminal atlases
    and `TerminalShaper` the terminal sheets draw through)
---

# Font evidence sheets from a libmsdf example

## 1. Goal

Every FONT document's images so far (docs/FONT_TEXTURINA.md,
FONT_GRENZE_GOTISCH.md, FONT_NOTO_SANS_MONO.md, FONT_TERMINAL_SYMBOLS.md)
were made by a scratch Rust crate written for that run plus a Python script
composing the sheet with Pillow, and the reference panel was FreeType
through Pillow. They are kept beside the evidence, but they cannot be re-run
from the repository: they point at absolute paths, one patches a copy of
libmsdf, and the compositing needs Python and a system font.

After this plan, `cargo run --release --example font_sheets -- <subcommand>`
makes each kind of sheet from the repository alone, with libmsdf drawing
every panel, the reference included, and writes it where Rule 3.5 says
evidence goes.

## 2. Findings (2026-10-23)

- **Three kinds of sheet were made.**
  1. *Cells against outlines* (clipviz, cellviz, relviz): a face's tall
     glyphs in lines at 56 px, one panel for the reference and one per
     atlas (the committed one, candidates baked in memory with another cap
     placement or cell box, or an older committed atlas from git), with the
     line box, the cell box and the baseline drawn as guides; a 2x close-up
     and a 16 px panel for softness; per-line top and bottom ink rows
     compared with the reference (cellsheet.py's `extents.json`).
  2. *Terminal samples* (termviz): lines through `TerminalShaper` and
     `push_terminal_line` at 14 to 56 px, scale 1 and 2, light on dark;
     a block-by-block sheet of every glyph on the symbol page and a
     terminal mock-up whose line widths in columns are recorded.
  3. *A glyph list* (ccspec/ccsheet): a given list of codepoints, here the
     70 of Claude Code 2.1.309's interface, drawn in rows with its spinners.
- **libmsdf can draw the reference itself.** `extract_outline` gives a
  glyph's contours as lines, quadratics and cubics in font units, and
  `PathBuilder` takes exactly those segments; `push_path_fill` fills the
  result on the GPU. Two limits: the fill is **even-odd**, while TrueType
  outlines are drawn **nonzero**, so a glyph with overlapping contours (an
  unmerged variable-font outline or a composite whose parts overlap) would
  show holes where they overlap; and one path lowers to at most
  **256** GPU segments, which an emoji outline can exceed.
- **The renderer and readback exist twice in tests** (terminal_render.rs,
  cell_box_render.rs), each with its own 8-bit PNG writer. `png` is already
  a dev-dependency, so an example can use it.
- **The scratch tools disagreed on nothing that matters,** but each kept its
  own copy of the line layout and guide geometry.

## 3. Design

### 3.1 The example

`examples/font_sheets.rs`, registered in Cargo.toml with
`required-features = ["cpu-bake", "gpu-tests", "fonts-ofl-ledger",
"fonts-ofl-terminal", "fonts-apache-roboto"]` (the `gpu-tests` feature is
what gates headless GPU use elsewhere; see open question 3). It is an
example, not library code: nothing it defines is public API.

```text
cargo run --release --example font_sheets -- cells <atlas> [--box A,B]... [--cap F]... [--git <rev>]... [--lines <file>] [--name <name>]
cargo run --release --example font_sheets -- terminal [--lines <file>] [--sizes 14,24] [--scales 1,2] [--name <name>]
cargo run --release --example font_sheets -- glyphs <file> [--size 24] [--scale 2] [--name <name>]
```

- `<atlas>` is a `BundledAtlas` by its file stem (`texturina`,
  `grenze-gotisch`, ...). Each `--box A,B` and `--cap F` adds a panel baked
  in memory with `Baker::bundled_options` and that `CellBox` or
  `CapPlacement::top_frac`; each `--git <rev>` adds a panel from the
  committed atlas at that revision (read with `git show`). The committed
  atlas is always a panel. Without `--lines`, the lines are the glyphs the
  committed bake report lists past the cell window, then a fixed pangram
  and descender line, as clipviz chose them.
- `terminal` draws the samples, the symbol page block by block and the
  mock-up of termviz; `--lines` replaces the mock-up.
- `glyphs` draws the codepoints listed in `<file>` (one per line, `U+XXXX`
  or the character, `#` comments) in rows, through `TerminalShaper` when
  every one is on the terminal atlases and through the face's own shaper
  otherwise.

### 3.2 Panels and guides

- **Reference panel.** Each glyph of a line is laid out with the face's
  shaper (the same advances as the atlas panel), its outline taken with
  `extract_outline`, mapped from font units to the panel (`x = pen +
  x_font * size / upem`, `y = baseline - y_font * size / upem`) and filled
  with `push_path_fill`. A glyph whose path is refused (`PathError`), or
  whose outline has overlapping contours (section 2; detected by comparing
  the even-odd fill's ink with the outline's bounding box on a test raster,
  or by open question 1's answer), is drawn instead from the atlas at four
  times the size, scaled down, and the panel's label says which glyphs
  that applies to.
- **Atlas panels.** `push_shaped_text_with` (or `push_terminal_line`) from
  an `AtlasSet` holding the panel's atlas, so the cell box is honoured as
  in 0.6.0.
- **Guides.** The line box (top and bottom), the cell box (dashed: short
  `SdfKind::Box` dashes) and the baseline, as rect instances one physical
  pixel tall, in fixed colours. The baseline is the line box's (`(baseline_frac
  × cell_em − above_em) / LINE_BOX_RATIO`), the same in every panel of a
  face unless a candidate moves it.
- **Labels.** Each panel's caption in Roboto Regular from the bundled
  atlas, saying what it is: the atlas, its format version, its box or cap,
  and the bake report's ink-past-the-cell count. A caption never contains
  a glyph Roboto lacks (the ✨ caption in 20261023_04 was a box).
- **Composition.** All panels of a sheet are drawn into one `DrawList` and
  one render target, stacked with fixed gaps; the light sheet (cells) and
  the dark sheet (terminal, glyphs) use the 2026-10-23 colours.

### 3.3 Measurement

For every line of every panel, the first and last physical rows with
coverage at or above 64 (a quarter, as `tests/cell_box_render.rs` uses) are
recorded, with the reference panel's rows for the same line.
`metrics.json` beside the sheet holds them per panel and line, with each
panel's atlas, version, box, cap and ink-past-the-cell list, and for
terminal lines the shaper's column counts.

### 3.4 Output

- The directory is the next free `out/fonts/YYYYMMDD_NN_<name>/` for today
  (the date from the host clock, `NN` one more than the highest existing
  for that date); `<name>` defaults to the subcommand and face.
- It holds the sheet PNGs (8-bit RGB through `png`), `metrics.json`, and
  `command.txt` with the exact command line and the libmsdf commit
  (`git rev-parse HEAD`, and whether the tree was dirty).
- The example prints a log entry for `out/fonts/LOG.md` (heading with the
  timestamp and directory, `./`-relative links, one paragraph stating what
  each sheet shows and the measured extents), for the author to check and
  paste. It does not write LOG.md or any FONT document itself.

### 3.5 What goes away

The scratch tools stay where they are, as the record of how the 2026-10-23
sheets were made; no new scratch tool is written for a sheet this example
can make.

## 4. Tests

- **The example builds in CI-equivalent runs:** `cargo build --examples
  --features <required>` is added to `scripts/check-font-features.sh`'s
  build matrix (it builds, it does not render).
- **The reference panel is the outline:** a unit test inside the example
  module, run by `cargo test --example font_sheets --features <required>`,
  fills `H` and `O` of Roboto Regular as paths at 56 px and checks their ink
  rows and columns against `glyph_bounding_box` within 1 px, and checks that
  `O`'s counter is empty.
- **Parity with the scratch tools** is step 3's check, not a test: the
  scratch tools need Python and absolute paths.

## 5. Versioning (Rule 5)

None. An example is not public API, and nothing public is added or changed,
so Rule 5.4 asks for no release. If implementing it shows that a helper
belongs in the library (the PNG writer, the outline-to-path mapping), that
is a separate, additive change: a patch release, or a minor one if it
changes anything public.

## 6. Steps

1. **Render, guides, labels and output.** The example skeleton: argument
   parsing, the headless GPU render and readback, PNG output, the output
   directory rule, `command.txt`, guides and Roboto captions.
   **Done when:** `font_sheets cells texturina` writes a sheet with the
   committed atlas's panel, its guides and caption into the next free
   directory, and prints a log entry.
2. **The reference panel and measurement.** Outline-to-path filling with
   the fallback, and `metrics.json`; the unit test of section 4.
   **Done when:** the unit test passes, and the Texturina sheet's reference
   panel shows every glyph of its lines filled from its outline (or labelled
   as drawn from the atlas).
3. **All three subcommands, at parity.** `--box`, `--cap` and `--git`
   panels; `terminal`; `glyphs`; the build line in
   `check-font-features.sh`.
   **Done when:** the example regenerates the 2026-10-23 sheets
   (20261023_01 with `--git 26a6a57`-era atlases and `--cap 0.225,0.247,0.265`,
   20261023_02's boxes, 20261023_03's terminal samples, 20261023_04's
   0.5-against-0.6 and Claude Code sheets), and every line's top and bottom
   ink rows agree with the scratch tools' recorded extents (cellsheet.py's
   `extents.json`, termviz's column counts) within 1 px, with the
   differences, if any, listed in the run's log entry. Pixels need not match
   Pillow's.
4. **Use it.** The FONT documents' next dated entries cite sheets made by
   the example, and the README's bundled-fonts section names it.
   **Done when:** one new dated FONT entry is made entirely from the
   example's output.

## 7. Open questions

1. **Overlapping contours.** Even-odd filling shows holes where a
   nonzero-wound outline's contours overlap. Options: (a) fall back to the
   atlas for such glyphs, as section 3.2 does; (b) add a nonzero fill rule
   to `PathBuilder` / the path shader, a library change with its own plan;
   (c) merge overlaps on the CPU before filling. Proposed: (a) now, and (b)
   only if many glyphs need the fallback.
2. **Emoji and long outlines.** An outline past 256 GPU segments is
   refused. Proposed: split by contour into several paths when the contours
   do not overlap, otherwise the atlas fallback.
3. **The GPU feature.** The example needs a GPU and the same LLVM codegen
   note as the GPU tests. Proposed: require `gpu-tests`, as the tests do,
   rather than add an `examples` feature; or name a new `sheets` feature if
   the owner prefers that `gpu-tests` stay test-only.
4. **Which atlases count as "libmsdf 0.5's".** `--git <rev>` takes any
   revision; the sheets so far used 26a6a57. Proposed: keep it a free
   revision, recorded in `command.txt`.
