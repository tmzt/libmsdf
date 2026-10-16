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

## 0.4.0 — 2026-10-16 (tag `v0.4.0`)

Atlas-carried layout tables (`docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md`)
and bundled fonts as optional features
(`docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md`), in one release.
The workspace now has two crates versioned in lockstep, `libmsdf` and
`libmsdf-font`, both 0.4.0 and both covered by this entry and this tag. The
commit is the one tagged `v0.4.0`. No dependency changed version (Rule
5.5): msdfgen and ttf-parser 0.18 are pinned to the versions already in use.

**Format:** writes atlas format 5:0.1.0 (file-major 5; tables `Head`,
`Cmap`, `Hmtx`, `Gsub`, `Gpos`, `LICS`, `LICT`, `CVRG`); reads the legacy
v3 and v4 files and 5:0.1.x (unknown tables skipped). Releases before 0.4.0
refuse every file this one writes.

- **Breaking (format):** every atlas is written at 5:0.1.0, whatever its
  layer count; v3 and v4 are no longer written. A runtime shell
  (`FontAtlas::empty`) now keeps its declared `size_px: 0` through a round
  trip instead of having the size measured off its cells.
- **Breaking:** `FontAtlas` gains the public fields `layout:
  Option<AtlasLayout>`, `licences: Vec<LicenceRecord>`, `read_version:
  AtlasFileVersion` and `coverage: Vec<FaceCoverage>`: a struct literal
  outside libmsdf needs them (or `..FontAtlas::empty(..)`).
- **Breaking:** `FontAtlasBuilder::build` refuses a bake with a style that
  no licence covers; call `add_licence` (`bake_atlas --licence`).
- **Breaking:** rustybuzz and ttf-parser 0.25 are optional behind a new
  `shaper` feature, on by default. A consumer with `default-features =
  false` loses `TextShaper`, `StyledShaper`, `NoFaceForStyle`,
  `FontAtlasBuilder`, `glyph_projection`, `cap_height`, `extract_outline`,
  `bundled_style_face` and the layout extraction until it enables `shaper`.
- **Breaking:** `ROBOTO_REGULAR_ASCII`, `ROBOTO_BOLD_ASCII`,
  `ROBOTO_ITALIC_ASCII` and `ROBOTO_ASCII_MSYMBOLS` exist only with their
  `font-apache-roboto-*` feature and `shaper`. Migration: enable
  `fonts-apache-roboto`, or the single features used.
- **Breaking:** `bundled_style_face` returns `None`, and
  `StyledShaper::bundled` carries no face, for a style whose feature is off.
- Changed: the font module is the `libmsdf-font` crate, re-exported as
  `libmsdf::font` with every path unchanged; `LINE_BOX_RATIO` is defined
  there and re-exported as `libmsdf::drawlist::LINE_BOX_RATIO`; `cpu-bake`
  enables `shaper` and `libmsdf-font/bake`.
- Changed (not API): `fonts/LICENSE-Roboto.txt` and
  `fonts/LICENSE-MaterialSymbols.txt` are replaced by
  `FONTS-LICENSE-APACHE.txt`; `FONTS-LICENSE-OFL.txt` is added.
- Added, the format: `AtlasFormatVersion`, `AtlasFileVersion` (`of`),
  `ATLAS_FILE_MAJOR`, `ATLAS_FORMAT_VERSION`, `ATLAS_HEADER_SIZE_V5`,
  `MAX_ATLAS_TABLES`, `MAX_ATLAS_TABLE_BYTES`, `ATLAS_TABLE_FACE_ALL`;
  `FontAtlas::{file_version, licences, coverage, try_to_bytes, padded_to}`.
- Added, layout: `AtlasLayout` (`from_faces`, `face`, `faces`, `styles`),
  `FaceLayout` (`validate`; `extract` with `shaper`), `GsubLookup`,
  `GsubRules`, `Ligature`, `ContextSide`, `ContextSingle` and the layout
  limits (`MIN_UNITS_PER_EM`, `MAX_UNITS_PER_EM`, `MAX_GSUB_LOOKUPS`,
  `MIN_LIGATURE_COMPONENTS`, `MAX_LIGATURE_COMPONENTS`, `LAYOUT_TAGS`);
  `AtlasShaper` (`new`, `from_face`, `style`, `face`, `glyph_id_for_char`,
  `shape`), `RunShaper` (implemented by `AtlasShaper` and `TextShaper`),
  `NoLayout`, `is_default_ignorable`; with `shaper`: `LayoutOptions`
  (`extra_features`, `coverage`), `LayoutError`, and
  `layout_bake::{check_equivalence, LAYOUT_CHECK_PANGRAMS,
  LAYOUT_CHECK_LEDGER, LAYOUT_CHECK_NO_LETTERS, RUSTYBUZZ_DEFAULT_FEATURES,
  TEXT_SHAPER_DEFAULT_FEATURES}`; `ShapedGlyph` and `ShapedRun` derive
  `PartialEq` and `Eq`.
- Added, licences and coverage: `LicenceRecord`, `LICENCE_REGISTRY`,
  `LicenceKind`, `licence_by_spdx`, `licence_by_key`, `LicenceTextError`;
  `CoverageGroupId`, `MissingPolicy`, `CoverageGroup`, `COVERAGE_TEXT`,
  `COVERAGE_SYMBOLS`, `COVERAGE_MATH`, `Coverage`, `FaceCoverage`,
  `GroupCoverage`.
- Added, the builder: `FontAtlasBuilder::{with_layout, with_style_layout,
  bakes_layout, add_licence, face_data, with_columns, columns,
  add_styled_face}`.
- Added, bundled fonts: the features `font-apache-roboto-regular`,
  `font-apache-roboto-bold`, `font-apache-roboto-italic`,
  `font-apache-roboto-msymbols`, `font-ofl-grenze-gotisch`,
  `font-ofl-texturina` and the groups `fonts-apache-roboto`,
  `fonts-ofl-ledger` (all off by default); `GRENZE_GOTISCH_TTF`,
  `TEXTURINA_TTF`; `ROBOTO_REGULAR_ATLAS`, `ROBOTO_BOLD_ATLAS`,
  `ROBOTO_ITALIC_ATLAS`, `ROBOTO_MSYMBOLS_ATLAS`,
  `ROBOTO_MSYMBOLS_STYLED_ATLAS`, `GRENZE_GOTISCH_ATLAS`,
  `TEXTURINA_ATLAS`; `BUNDLED_ATLAS_GLYPH_SIZE`, `BUNDLED_ATLAS_PX_RANGE`;
  `libmsdf::bundled`; `libmsdf::font::faces`;
  `libmsdf::font::bundled` (`BundledAtlas`, `BundledFace`, `hex_of`).
- Added, the baker, with `cpu-bake` (`libmsdf-font`'s `bake`):
  `libmsdf::bake` (`Baker` with `bake_face`, `bake_face_report`,
  `bake_bundled`, `bake_bundled_report`, `bundled_options`; `BakeOptions`,
  `LicenceSpec`, `BakeError`, `BakeReport`, `FaceReport`, `GroupReport`,
  `sha256`), and the `libmsdf-bake` binary of `libmsdf-font`.
