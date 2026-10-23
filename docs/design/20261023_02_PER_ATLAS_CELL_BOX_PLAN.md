---
title: A per-atlas cell box, and atlas format 5:0.2.0
date: 2026-10-23
status: Implemented
covers: >
  Let an atlas's glyph cells be taller than, or offset from, the 1.3 em
  line box, set per atlas at bake time, so that a face whose ink does not
  fit the line box (Texturina's accents, Grenze Gotisch's rings) bakes
  whole without moving its baseline; record the box in a new atlas-wide
  `CELL` table, written only when the box is not the line box, at a new
  atlas format version 5:0.2.0 that older readers refuse; draw text from
  the cell box rather than the line box; re-bake the two bundled faces
  that clip; and the libmsdf 0.6.0 release this needs.
sources:
  - >
    Owner (Tim), 2026-10-23, in the libcontrols session: "do we need to
    expand the glyph box or should we have that as a setting per atlas and
    bump the magic version?", then: "write the plan to bump the format
    which will also bump the version as you said when implemented".
  - >
    docs/FONT_TEXTURINA.md and docs/FONT_GRENZE_GOTISCH.md, the
    2026-10-23 12:37 EDT entries, with the renders and per-case metrics in
    out/fonts/20261023_01_cap_clipping/ (section 3).
  - >
    font/src/atlas.rs (`CELL_EM_RATIO`, `CAP_TOP_FRAC`, `CapPlacement`,
    `glyph_projection_with`, `default_cell_metrics_with`, `SetMetrics`
    with `px_per_em_frac`, `AtlasFormatVersion::reads`,
    `ATLAS_FORMAT_VERSION` 5:0.1.0, the table directory and
    `ATLAS_TABLE_FACE_ALL`, `min_antialiased_font_size`), font/src/bake.rs
    (`BakeOptions`, `FaceReport::ink_outside`), font/src/bundled.rs
    (`BundledAtlas::cap`), font/src/terminal.rs (`TerminalShaper::new`),
    src/drawlist/mod.rs (`push_shaped_text`, `push_marker`, `push_icon`,
    `screen_px_range`, `X_MARGIN_FRAC`), src/drawlist/stream.rs (the
    stream lowering's `line_box_h`), src/gpu/sdf_render.wgsl (case 8u:
    `scale = atlas_gh / line_h` with `line_h = cmd.size.y`).
related:
  - docs/design/20261023_01_TERMINAL_FONTS_PLAN.md (`CapPlacement`, the
    bake report's ink lines, and the finding this plan answers)
  - docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md (the bundled
    atlases, their hashes and the format-5 table scheme)
---

# A per-atlas cell box, and atlas format 5:0.2.0

## 1. Goal

Every atlas cell is the line box today: 1.3 em tall and wide
(`CELL_EM_RATIO`), drawn onto exactly one line box (`font_size ×
LINE_BOX_RATIO`). Ink outside that window is not in the atlas. Two bundled
faces have such ink (section 3). Moving the cap height down the cell
(`CapPlacement`) fits Grenze Gotisch but moves its text down within the
line, and cannot fit Texturina at all.

After this plan:

- a bake can give an atlas a **cell box**: how far its cells reach above
  the line box's top and below its bottom, in em. The line box itself, the
  line pitch and where the baseline sits in the line box do not change;
  only the cell grows or shifts around it;
- an atlas with a cell box that is not the line box carries a `CELL` table
  and is written at format **5:0.2.0**, which libmsdf 0.5 and earlier
  refuse instead of drawing it wrong. Every other atlas is written exactly
  as today, at 5:0.1.0, byte for byte;
- text, markers and icons are drawn from the cell box;
- Texturina and Grenze Gotisch are re-baked with cell boxes that fit all
  their ink with at least one texel to spare, with their baselines where
  today's atlases put them.

## 2. Why per atlas, not a larger cell for every atlas

Measured against the alternatives on 2026-10-23 (owner's question above):

- **A larger cell everywhere** re-bakes all nine bundled atlases, moves
  every face within its line, and spreads each 48-texel cell over more em
  (at 1.41 em, 7.5% fewer texels per em) for faces that fit today.
- **Moving the cap** (`CapPlacement`, already shipped) fits only faces
  whose ink spans less than the cell, and moves their text.
- **Scaling a face down** inside the cell draws it smaller than its font
  size.

A per-atlas box costs only the faces that need it. The format already
records each set's em-to-cell ratio (`SetMetrics::px_per_em_frac`, which is
1/1.3 in every atlas today) and every glyph's `px_per_em` and
`baseline_row`; the shader already maps whatever box an instance gives it
onto the cell (`scale = atlas_gh / cmd.size.y`). What is missing is where
the cell sits relative to the line box, and a version that stops an older
reader from assuming 1.3.

## 3. Findings (measured 2026-10-23)

From docs/FONT_TEXTURINA.md and docs/FONT_GRENZE_GOTISCH.md (units per em
1000 for both; the default window has the cap height at 0.15 of the cell):

| Face | Cap height | Ink, y | Ink span | Default window | Clipped today |
|---|---|---|---|---|---|
| Texturina | 720 | −282 (`¿`) to 1064 (`ĥ ĺ`) | 1346 | −385 to 915 | 53 glyphs at the top, worst 149; `‰` 28 too wide |
| Grenze Gotisch | 620 | −271 to 862 (`Å Ů`) | 1133 | −485 to 815 | 2 glyphs at the top, 47 |

Texturina's span is larger than a 1300-unit cell. Re-baking both faces at
the default placement reproduces the committed atlases byte for byte, so
the renders show what ships.

One texel of a 48-texel cell is `cell_em × 1000 / 48` units: 27.1 at
1.3 em, 29.3 at 1.406 em. A glyph whose ink reaches the cell edge loses the
outside of its antialiasing ramp there (the sample is clamped to the
cell's last texel centre), so a fitted box keeps one texel of margin.

## 4. Design

### 4.1 The cell box

In font/src/atlas.rs:

```rust
/// Where an atlas's cells sit relative to the line box, in em.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellBox {
    /// How far the cell's top is above the line box's top (negative: below).
    pub above_em: f64,
    /// How far the cell's bottom is below the line box's bottom (negative:
    /// above).
    pub below_em: f64,
}

impl CellBox {
    /// The cell is the line box: every atlas before this plan.
    pub const LINE_BOX: Self = Self { above_em: 0.0, below_em: 0.0 };
    /// The cell's height in em: `CELL_EM_RATIO + above_em + below_em`.
    pub fn cell_em(&self) -> f64;
}
impl Default for CellBox { /* LINE_BOX */ }
```

Cells stay square (`glyph_size` × `glyph_size` texels), so a taller cell is
also wider by the same amount; the ink stays centred horizontally as today.

`CapPlacement::top_frac` keeps its meaning relative to the **line box**:
the cap height sits `top_frac × CELL_EM_RATIO` em below the line box's top,
whatever the cell box. The baseline in the line box is therefore the same
for `CellBox::LINE_BOX` and any other box; only the cell around it changes.

### 4.2 The `CELL` table and format 5:0.2.0

- Tag `CELL`, face byte `ATLAS_TABLE_FACE_ALL`, 8 bytes: `above_em` then
  `below_em`, each `f32` little-endian.
- Written only when the box is not `CellBox::LINE_BOX`. An atlas without it
  has the line box.
- `ATLAS_FORMAT_VERSION` becomes 5:0.2.0: the newest version this build
  reads. The writer writes **5:0.1.0 when there is no `CELL` table** and
  5:0.2.0 when there is. Under `AtlasFormatVersion::reads`, a newer minor
  before 1.0 is refused, so libmsdf 0.5 and earlier refuse a 5:0.2.0 file
  with "atlas format version is newer than this build reads", rather than
  skipping the table and drawing the cells onto a 1.3 em box.
- The loader refuses: a `CELL` table in a 5:0.1.x file; a body that is not
  8 bytes; a non-finite value; a `cell_em` outside 0.5 to 4.0; and a set
  whose `px_per_em_frac` differs from `1 / cell_em` by more than 1e-5 (and,
  with no table, from `1 / CELL_EM_RATIO`).
- `FontAtlas::cell_box() -> CellBox`: the table's box, or `LINE_BOX`.

### 4.3 Baking

- `BakeOptions::cell: CellBox`, default `CellBox::LINE_BOX`;
  `FontAtlasBuilder::with_cell_box(CellBox)` and `cell_box()`.
- `glyph_projection_with` and `default_cell_metrics_with` take the box:
  `em_scale = gs / (upem × cell_em)`, and the cap height lands
  `gs × (top_frac × CELL_EM_RATIO + above_em) / cell_em` texels from the
  cell top. With `LINE_BOX` both reduce to today's expressions, and the
  bytes are identical.
- `SetMetrics::px_per_em_frac` and `baseline_frac` are written for the cell
  as today (they describe the cell, not the line box).
- `CapPlacement::window` becomes the **cell** window, `(top − cell, top)`
  with `top = cap + (top_frac × CELL_EM_RATIO + above_em) × upem` and
  `cell = cell_em × upem`, and takes the box. The bake report's "ink past the
  cell window" lines measure against it.

### 4.4 Drawing

In src/drawlist/mod.rs and src/drawlist/stream.rs, for an atlas with box
`b` at font size `s`:

- **Text** (`push_shaped_text`, and the stream lowering): the instance box
  is `s × b.cell_em()` tall and its top is `b.above_em × s` above the line
  box top the caller passed: `position.y = pos.y − b.above_em × s`. The
  horizontal margin is `X_MARGIN_FRAC × box height`, the same expression
  the shader inverts (`line_x = pos.x + x_margin_frac × cmd.size.y`). The
  return value (the advance) does not change.
- **Markers and icons** (`push_marker`, `push_icon`): the conversion from
  cell texels to screen pixels, `line_h / cell_px` today, becomes
  `s / entry.px_per_em`, which is the same number for a 1.3 em cell.
- **Antialiasing**: `screen_px_range` keeps its signature and documents
  that it is for 1.3 em cells; a new
  `screen_px_range_in(atlas_px_range, px_per_em, font_size)` computes
  `atlas_px_range × font_size / px_per_em`, and libmsdf's own callers use
  it. `FontAtlas::min_antialiased_font_size` divides by the atlas's
  `cell_em` instead of `LINE_BOX_RATIO`.
- **The shader does not change.** Case 8u already maps `cmd.size.y` onto
  the cell. Its vertical fade (`ombre_y`) spans the instance box, which is
  the cell box: for a `LINE_BOX` atlas, exactly as today.
- Line pitch, layout, measurement and every caller that sizes a row from
  `LINE_BOX_RATIO` are unchanged: ink above or below the line box overhangs
  it, as accents do in any text system.

`TerminalShaper::new` also refuses a pair whose cell boxes differ
(`TerminalError::Mismatch { what: "cell box", .. }`); both terminal atlases
keep the line box.

### 4.5 The bundled faces

`BundledAtlas::cell() -> CellBox`: `LINE_BOX` for every atlas except:

| Atlas | `above_em` | `below_em` | `cell_em` | Cell window, y | Margin top / bottom | One texel |
|---|---|---|---|---|---|---|
| Texturina | 0.179 | −0.073 | 1.406 | −312 to 1094 | 30 / 30 | 29.3 |
| Grenze Gotisch | 0.075 | −0.075 | 1.300 | −410 to 890 | 28 / 139 | 27.1 |

Both keep `CapPlacement::DEFAULT`, so their baselines stay exactly where
today's committed atlases put them; neither face's text moves. Texturina's
cell is also 1406 units wide, which holds `‰` (1328). Grenze Gotisch's box
only shifts the cell up, so its texel density is unchanged; Texturina's
falls by 7.5% (34.1 texels per em instead of 36.9).

`fonts/atlases/texturina-48.atlas` and `grenze-gotisch-48.atlas` are
re-baked (5:0.2.0) with their reports, and `fonts/bundled.sha256` updated
for those two; the other seven atlases are byte-identical.

## 5. Tests

- **Format** (font/src/atlas.rs tests): a `CELL` table round-trips; an atlas
  with `LINE_BOX` writes 5:0.1.0 and no table; a 5:0.2.0 file is refused by
  `AtlasFormatVersion { 5, 0, 1, 0 }.reads`; each refusal in section 4.2
  has a case.
- **Bundled** (tests/bundled_fonts.rs, tests/bundled_bake.rs): the seven
  `LINE_BOX` atlases re-bake byte-identical to the committed files; the
  Texturina and Grenze Gotisch reports list no ink past the cell; every
  glyph's ink is at least one texel inside its cell.
- **Drawing** (src/drawlist tests): for a `LINE_BOX` atlas, every instance
  field is as before (and the golden scene in tests/golden_render.rs is
  unchanged byte for byte); for Texturina, the baseline lands at the same
  `y` as with today's atlas for the same `pos` and font size, within 1e-3
  px, and the instance box is `1.406 × s` tall with its top `0.179 × s`
  above `pos.y`.
- **GPU** (`gpu-tests`, a new tests/cell_box_render.rs): `Â Ĺ ĥ ĺ Å Ů ¿ ‰`
  at 12, 16, 24 and 56 px, scale 1 and 2: the topmost row with coverage is
  within 1 px of the FreeType outline's top at that size (no flat cut), and
  the bottommost within 1 px of its bottom. Frames are written under
  `out/fonts/<YYYYMMDD_NN>_cell_box/`, logged in `out/fonts/LOG.md`, and
  compared with today's renders in a dated addition to each FONT document.

## 6. Versioning (Rule 5)

Not strictly additive, so libmsdf (and libmsdf-font, in lockstep) moves to
**0.6.0**:

- **Breaking:** the atlas format moves to 5:0.2.0; the bundled Texturina
  and Grenze Gotisch atlases are written at it, and libmsdf 0.5 and earlier
  refuse them.
- **Breaking:** `BakeOptions` gains the public field `cell`.
- **Breaking (rendering):** text from those two atlases draws ink that was
  clipped, outside the line box.
- Added: `CellBox`, `FontAtlas::cell_box`, `FontAtlasBuilder::with_cell_box`
  and `cell_box`, `BundledAtlas::cell`, `screen_px_range_in`.

libcontrols draws no text instance itself and needs no code change, but
takes 0.6.0 as a semver-incompatible move and releases 0.7.0 under
Rule 5.5, in its own repository, after this plan.

## 7. Steps

1. **The cell box and the format.** `CellBox`, the `CELL` table, the
   writer's version choice, `ATLAS_FORMAT_VERSION` 5:0.2.0, the loader's
   checks and `FontAtlas::cell_box`.
   **Done when:** the format tests in section 5 pass and every committed
   atlas loads and re-serialises byte for byte.
2. **Baking with a box.** `BakeOptions::cell`, `with_cell_box`, the
   projection, the default cell metrics and the cell window.
   **Done when:** every bundled atlas re-bakes byte-identical with
   `LINE_BOX`, and an in-memory Texturina bake with section 4.5's box
   reports no ink past the cell.
3. **Drawing from the box.** Section 4.4 in the draw list, the stream
   lowering, `screen_px_range_in`, `min_antialiased_font_size` and
   `TerminalShaper::new`.
   **Done when:** the drawing tests pass and the golden scene is unchanged.
4. **The bundled faces.** `BundledAtlas::cell`, the two re-bakes, their
   reports, `fonts/bundled.sha256`, the GPU test and its evidence, and a
   dated addition to docs/FONT_TEXTURINA.md and
   docs/FONT_GRENZE_GOTISCH.md.
   **Done when:** the bundled and GPU tests pass, the seven other atlases
   are unchanged, and the renders show every listed glyph whole.
5. **Release.** Both crates at 0.6.0, the VERSIONS.md entry, tag `v0.6.0`,
   the CHANGES.md landing addendum, the plan marked Implemented.
   **Done when:** fmt, clippy (no new warnings), the full test suite with
   `gpu-tests` and `scripts/check-font-features.sh` pass at the tagged
   commit.

## 8. Open questions

1. **The 0.5.1 cap fix.** The owner chose libmsdf 0.5.1 / libcontrols
   0.6.1 for moving the cap (Grenze Gotisch to 0.20, Texturina to 0.225 or
   unchanged), which moves those faces' text down. This plan fixes both
   with their baselines where they are today. If 0.5.1 ships first, 0.6.0
   moves the text back up. Proposed: skip the cap move, and fix both faces
   only here.
2. **The margin.** One texel is proposed (section 3). Two texels would make
   Texturina's cell 1.468 em (11% fewer texels per em) for a softer outer
   ramp at large sizes; the GPU test in step 4 is the check.

## Amendment — 2026-10-23 13:23 EDT: approved with the owner's changes; implementation started

The owner approved implementing this plan for libmsdf 0.6.0 in the
libcontrols session ("let's get 0.6.0 out first"), with the changes below,
agreed there before this amendment was written (Rule 2). The release itself,
step 5, waits for the owner's separate approval ("don't release until
approved"): steps 1 to 4 are committed, and nothing is bumped, tagged or
pushed until then. Where this amendment and the text above
disagree, this amendment wins. Evidence for each change is in
docs/FONT_TEXTURINA.md, docs/FONT_GRENZE_GOTISCH.md (13:04 EDT entries),
docs/FONT_NOTO_SANS_MONO.md and docs/FONT_TERMINAL_SYMBOLS.md (13:17 EDT).

**Open question 1, the cap fix: dropped.** No cap-placement release comes
first ("we're dropping the 0.5.1 plan, the next release will be 0.6.0").
Texturina and Grenze Gotisch are fixed only by their cell boxes, with
`CapPlacement::DEFAULT`, so neither face's text moves.

**Open question 2, the margin: one texel**, as proposed. A prototype of
sections 4.3 and 4.4 (out/fonts/20261023_02_cell_box_prototype/) baked both
faces with section 4.5's boxes: no ink past the cell, each line's top ink
within 1 px of the outlines, baselines unchanged to five places, and no
visible softening at 16 px for either one or two texels.

**Section 4.3, the width check.** The bake report's "wider" figure measures
ink width against the **cell** width, `cell_em × upem`, not
`CELL_EM_RATIO × upem`. (The prototype found it still flagged `‰` in a
1.406 em cell.)

**New section 4.6, coverage.** Added to this release, on the owner's word
("it's worth doing all of that now"):

- **TEXT** gains `U+0218-021B` (`Ș ș Ț ț`, Romanian) in
  `fonts/coverage-groups.txt` and `COVERAGE_TEXT`, policy `fail`. Every face
  that bakes the TEXT group has them upstream: Noto Sans Mono, Grenze
  Gotisch and Texturina. Their subsets are rebuilt with
  `scripts/rebuild-fonts.sh` from the pinned upstream files, and their
  atlases re-baked; the hashes in `font/src/faces.rs`,
  `scripts/rebuild-fonts.sh` and `fonts/bundled.sha256` are updated. The
  Roboto atlases use shipped coverage and do not change.
- **TERMINAL** gains the eight glyphs of Claude Code's interface that no
  bundled terminal atlas carried: `※` U+203B, `↯` U+21AF, `∙` U+2219,
  `∴ ∵` U+2234-2235, `∷` U+2237, `⌕` U+2315 and `⑂` U+2442. They come from
  the faces `fonts/terminal_symbols.py` already reads, in its existing
  order (Noto Sans Mono, then Noto Sans Symbols 2, Noto Sans Symbols, Noto
  Sans Math). `COVERAGE_TERMINAL` grows from 583 to 591 codepoints.
- **TERMINAL_EMOJI** gains `✨` U+2728 from Noto Emoji, two cells wide.
  `COVERAGE_TERMINAL_EMOJI` grows from 23 to 24.
- The symbol page grows from 607 to 616 cells, inside its 640-cell layer.

**Section 5, a further test.** `tests/terminal_glyphs.rs`: every codepoint
of Claude Code's interface glyph table and both of its spinners (70
codepoints, read from Claude Code 2.1.309 and listed in the test with that
version) shapes through `TerminalShaper` with no placeholder, from the text
atlas or the symbol page; the starburst frames `· ✢ ✳ ✶ ✻ ✽` and `*` are
one cell each, and `✨` two. The GPU test of section 5 also draws both
spinners' frames.

**Section 6, versioning.** Unchanged: 0.6.0, breaking. The VERSIONS.md
entry also lists the coverage additions and that the Noto Sans Mono and
terminal symbol atlases change bytes.

**Section 7, steps.** Step 4 also covers section 4.6 and the new test; its
**Done when** adds: every bundled atlas other than Grenze Gotisch,
Texturina, Noto Sans Mono and the terminal symbol page is byte-identical,
and `tests/terminal_glyphs.rs` passes.

**Not in this release: sheet examples.** The owner wants evidence sheets
generated by libmsdf examples rather than scratch tools ("move in that
direction"). That is a separate plan and release after 0.6.0.

Implementation starts with this amendment; the document is now immutable
except by the owner's explicit instruction (Rule 1.5).

## Amendment — 2026-10-23 14:51 EDT: implemented

Every step is done and its **Done when** holds; released as 0.6.0
(`4de319c`, tag `v0.6.0`) on the owner's approval. The commits:

- Step 1, the cell box and the format: `07f642c`, `f403dc1`.
- Step 2, baking with a box: `d756a62`, `4978313`.
- Step 3, drawing from the box: `087314d`, `6f690e5`.
- Step 4, the bundled faces, section 4.6's coverage and the tests:
  `bb13dd7`, `78c8cd3`, `9c47e68`, `0037efb`; evidence in
  out/fonts/20261023_04_cell_box_release/ and the FONT documents
  (`6997d85`).
- Step 5, the release: `4de319c`; departures recorded in CHANGES.md
  (`37e0e7e`).
