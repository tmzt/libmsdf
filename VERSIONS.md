# VERSIONS

Each change of the crate version and the public API changes it carries
(AGENTS.md, Rule 5). Internal changes are not listed, and there is no entry
without a version change. Entries are appended, oldest first, and never
edited.

## 0.2.0 — 2026-07-24 (`2434ec0`, tag `v0.2.0`)

The extraction of the render core and typography from matter-stream.
Recorded retroactively on 2026-10-16 from the commit, which changed the
version from the 0.1.0 of the stub seed `e90bbbe`; the tag was added on
2026-10-16.

The crate then stayed at 0.2.0 through `69e6c5c` (the commit consumers pin)
while its public API grew, before Rule 5 existed. Those changes are not
listed per commit; the API below is the one at `69e6c5c`, so the next entry
lists the changes from there.

- Crate root: `PHASE_STATUS`; modules `core`, `drawlist`, `font`, `gpu`,
  `pane`.
- `core`: `SdfDrawCmd` and the `DRAW_TYPE_*` constants (`BOX`, `CIRCLE`,
  `TEXT`, `OUTLINE`, `SLAB`, `LINE`, `BEZIER`, `MSDF_TEXT`), the CPU SDF
  mirrors in `core::sdf`, `Anim`, `ClipRect`, `GpuFont`, `RenderFrame`,
  `Rasterizer`, colour helpers (`rgba`, `rgba_unpack`, `color_u32_to_f32`).
- `drawlist`: `DrawList` (push, clip, rotate, translate and effects scopes,
  `translate_from`, `append_from`, `lower`), `SdfInstance`, `SdfKind`,
  `SdfFrame`, `SdfRotate`, `Elevation`, `DrawEffects`, `rotate_rect`, and the
  `stream` module's 32-bit command lowering.
- `font`: `FontAtlas`, `FontAtlasBuilder`, `AtlasLayer`, `AtlasManager`,
  `AtlasRegion`, `GlyphEntry`, `GlyphSet`, `GlyphStyle`, `CellKey`,
  `SetMetrics`, `ShapedRun`, `ShapedGlyph`, `TextShaper`, `StyledShaper`,
  `ShelfPacker`, outline extraction (`Edge`, `EdgeKind`, `GlyphOutline`,
  `extract_outline`), the bundled faces and icon sets (`ROBOTO_*`,
  `HIGHBAY_ICONS`, `MSYMBOLS_ICONS`, `MARKERS`, ...) and the atlas format
  constants and errors re-exported at the crate root.
- `gpu`: `GpuSdfRenderer`, `MsdfCompute`, `BlurPass`, `blur_params`.
- `pane`: `PaneGate`, `PaneMark`.

## 0.3.0 — 2026-10-16 (tag `v0.3.0`)

Filled Bézier paths (`docs/design/20261015_01_FILLED_BEZIER_PATH_PLAN.md`).
The commit is the one tagged `v0.3.0`: the entry is written in that commit,
which cannot name its own hash. No dependency changed version.

- **Breaking:** `SdfKind` gains `PathFill { seg_start, seg_count }` and is
  `#[non_exhaustive]`: a `match` on it outside libmsdf needs a `_` arm.
- Added: the `drawlist::path` module, re-exported from `drawlist`:
  `Path` (`contours`, `bounds`, `mapped`), `PathBuilder` (`new`, `move_to`,
  `line_to`, `quad_to`, `cubic_to`, `close`, `build`), `Contour`,
  `PathSegment` (`Line`, `Quad`, `Cubic`; `end`), `PathError` (`Empty`,
  `NoCurrentPoint`, `NonFinite`, `TooManySegments`; `Display`, `Error`),
  `GpuPathSegment` (`LINE`, `QUAD`, `to_rows`) and the constants
  `PATH_MAX_SEGMENTS`, `PATH_CUBIC_TOLERANCE`, `PATH_MAX_QUADS_PER_CUBIC`,
  `PATH_LINE_DEMOTE`. The crate root re-exports `Path`, `PathBuilder`,
  `Contour`, `PathSegment`, `PathError`, `PATH_MAX_SEGMENTS` and
  `PATH_CUBIC_TOLERANCE`.
- Added: `DrawList::push_path_fill(&Path, color, Elevation) -> Result<(),
  PathError>` (even-odd fill) and `DrawList::path_segments()`.
- Added: `core::sdf::{DRAW_TYPE_PATH_FILL, PARAM_BANK_CAPACITY,
  sd_path_fill}`, re-exported from `core`; the crate root re-exports
  `DRAW_TYPE_PATH_FILL` and `PARAM_BANK_CAPACITY`. `sdf_eval_with_params`
  evaluates draw type 12.
