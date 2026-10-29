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

## 0.4.1 — 2026-10-21 (tag `v0.4.1`)

A minimal Render trait (`docs/design/20261021_02_RENDER_TRAIT_PLAN.md`).
Strictly additive, so a patch release (Rule 5.4). `libmsdf` and
`libmsdf-font` stay in lockstep: both are 0.4.1 and both are covered by
this entry and this tag, though `libmsdf-font` itself is unchanged. The
commit is the one tagged `v0.4.1`. No dependency changed version.

- Added: `libmsdf::drawlist::Render` (re-exported as `libmsdf::Render`),
  with `render_into(&self, out: &mut DrawList)` and
  `render_mut(&mut self, out: &mut DrawList)` (by default `render_into`);
  the module `libmsdf::drawlist::render`; `impl Render for DrawList`
  (appending with `append_from`).

## 0.4.2 — 2026-10-21 (tag `v0.4.2`)

Several atlases in one draw list, cached in the renderer
(`docs/design/20261021_03_ATLAS_SET_PLAN.md`). Strictly additive, so a
patch release (Rule 5.4). `libmsdf` and `libmsdf-font` stay in lockstep:
both are 0.4.2 and both are covered by this entry and this tag, though
`libmsdf-font` itself is unchanged. The commit is the one tagged `v0.4.2`.
No dependency changed version.

- Added: the module `libmsdf::drawlist::atlas_set`, with `AtlasSet`
  (`new`, `number`, `register`, `atlas`, `len`, `is_empty`, `extent`,
  `layers`, `glyphs`, `table_bases`, `layer_bases`, `glyph_table_u32s`,
  `to_rgba_bytes`; `Default`), `AtlasId` (`set`, `index`), `AtlasRef`
  (`id`, `atlas`; `Deref<Target = FontAtlas>`; `shaper(style)`),
  `AtlasSetError` (`TooManyLayers`, `TooManyGlyphs`, `Full`; `Display`,
  `Error`) and `MAX_SET_GLYPHS`, re-exported from `libmsdf::drawlist`;
  `AtlasId`, `AtlasRef`, `AtlasSet` and `AtlasSetError` also at the root.
- Added: `DrawList::push_shaped_text_with`, `DrawList::push_icon_with`,
  `DrawList::instance_atlas`, `DrawList::lower_with`.
- Added: `GpuSdfRenderer::new_with_atlas_set`,
  `GpuSdfRenderer::upload_atlas_set`, `GpuSdfRenderer::render_draw_list_in`.
- Unchanged for existing callers: `DrawList::lower` lowers a list of plain
  calls exactly as before. It now debug-asserts that no text was pushed
  from a set's handle; lists that use no set never reach that assertion.

## 0.5.0 — 2026-10-23 (tag `v0.5.0`)

A terminal face and a symbol page, from Noto
(`docs/design/20261023_01_TERMINAL_FONTS_PLAN.md`). Not strictly additive,
so a minor release (Rule 5.4). `libmsdf` and `libmsdf-font` stay in
lockstep: both are 0.5.0 and both are covered by this entry and this tag.
The commit is the one tagged `v0.5.0`. No dependency changed version. The
atlas format is unchanged (5:0.1.0), and every atlas bundled before this
release is byte-identical.

- **Breaking:** `BundledAtlas` gains `NotoSansMono` and `TerminalSymbols`,
  and is now `#[non_exhaustive]`: a downstream exhaustive match stops
  compiling. `BundledAtlas::ALL` is `[BundledAtlas; 9]` (was 7).
- **Breaking:** `BakeOptions` gains the public field `cap: CapPlacement`
  (default `CapPlacement::DEFAULT`, the placement every earlier bake used),
  and `FaceReport` gains `ink_outside: Vec<InkOutside>`: struct literals of
  either outside libmsdf need them.
- **Breaking (rendering):** a text run's glyphs are composited as one
  shape (each pixel takes the largest coverage any glyph of the run gives
  it, composited once), and a glyph sample is clamped to its cell's last
  texel centre. Pixels where two glyphs of one run overlap with partial
  coverage draw lighter than before (the golden scene: 4 of 320,000 pixels,
  by at most 25 levels); everything else draws as before.
- Added: `CapPlacement` (`top_frac`, `height_em`; `DEFAULT`,
  `TERMINAL_TOP_FRAC`, `cap_units`, `window`), `glyph_projection_with`,
  `default_cell_metrics_with`, `FontAtlasBuilder::with_cap_placement` and
  `cap_placement`, and `InkOutside` (`glyph_id`, `above`, `below`,
  `wider`); the bake report's "ink past the cell window" lines.
- Added: `CoverageGroupId::TerminalSymbols` (3) and `TerminalEmoji` (4)
  (the enum was already `#[non_exhaustive]`), with `COVERAGE_TERMINAL` (583
  codepoints) and `COVERAGE_TERMINAL_EMOJI` (23).
- Added: `BundledAtlas::coverage`, `columns` and `cap`; `is_ofl` is true
  for the two new atlases.
- Added: the module `libmsdf_font::terminal` (`libmsdf::font::terminal`),
  with `TerminalShaper` (`new`, `cell_em`, `line_pitch_em`, `line`),
  `TerminalLine`, `TerminalSegment`, `TerminalSource`, `TerminalError`,
  `TERMINAL_WIDE_FORMS` and `TERMINAL_WIDE_FORM`, re-exported from
  `libmsdf::font`; the five types also at the `libmsdf` root.
- Added: `DrawList::push_terminal_line`.
- Added: the features `font-ofl-noto-sans-mono`,
  `font-ofl-noto-terminal-symbols` and the preset `fonts-ofl-terminal`,
  with `NOTO_SANS_MONO_REGULAR_TTF`, `NOTO_SANS_MONO_BOLD_TTF`,
  `TERMINAL_SYMBOLS_TTF` (each with `shaper`), `NOTO_SANS_MONO_ATLAS` and
  `TERMINAL_SYMBOLS_ATLAS`, at the root and in `libmsdf::font`.

## 0.6.0 — 2026-10-23 (tag `v0.6.0`)

A per-atlas cell box and atlas format 5:0.2.0, and wider terminal and text
coverage (`docs/design/20261023_02_PER_ATLAS_CELL_BOX_PLAN.md`). Not
strictly additive, so a minor release (Rule 5.4). `libmsdf` and
`libmsdf-font` stay in lockstep: both are 0.6.0 and both are covered by this
entry and this tag. The commit is the one tagged `v0.6.0`. No dependency
changed version.

- **Breaking:** the atlas format moves to 5:0.2.0 (`ATLAS_FORMAT_VERSION`).
  An atlas whose cells are not the line box carries a `CELL` table and is
  written at 5:0.2.0, which libmsdf 0.5 and earlier refuse. Every other atlas
  is written at 5:0.1.0 (`ATLAS_FORMAT_VERSION_LINE_BOX`), exactly as
  before.
- **Breaking:** the bundled Texturina and Grenze Gotisch atlases are baked
  with cell boxes and written at 5:0.2.0. Their accents and rings, clipped
  before, now draw whole, overhanging the line box; their baselines do not
  move.
- **Breaking:** `FontAtlas` gains the public field `cell_box`, and
  `BakeOptions` the public field `cell`: struct literals of either need them.
  `glyph_projection_with`, `default_cell_metrics_with` and
  `CapPlacement::window` take a `CellBox`.
- **Breaking (rendering):** text, markers and icons from an atlas with a
  cell box are drawn in the cell box rather than the line box. Atlases whose
  cells are the line box draw exactly as before (the golden scene is
  unchanged).
- **Breaking:** the TEXT coverage group gains `Ș ș Ț ț` (U+0218-021B), so the
  Noto Sans Mono, Grenze Gotisch and Texturina subsets, and their atlases,
  change bytes. The terminal symbol page gains `※ ↯ ∙ ∴ ∵ ∷ ⌕ ⑂`
  (`COVERAGE_TERMINAL`, 591 codepoints) and `✨` (`COVERAGE_TERMINAL_EMOJI`,
  24), and its atlas changes bytes (616 cells). The five Roboto atlases are
  byte-identical.
- **Breaking:** `TerminalShaper::new` refuses a text atlas and symbol page
  whose cell boxes differ (`TerminalError::Mismatch { what: "cell box" }`).
- Added: `CellBox` (`above_em`, `below_em`; `LINE_BOX`, `cell_em`,
  `is_line_box`), `FontAtlas::cell_box`, `FontAtlasBuilder::with_cell_box`
  and `cell_box`, `BundledAtlas::cell`, `CapPlacement::top_frac_in`,
  `ATLAS_FORMAT_VERSION_LINE_BOX`, `CELL_TABLE_TAG`; `CellBox` and
  `ATLAS_FORMAT_VERSION_LINE_BOX` also at the `libmsdf` root.
- Added: `drawlist::screen_px_range_in` and
  `drawlist::stream::lower_stream_with_cell_box`.
- Changed: `FontAtlas::min_antialiased_font_size` uses the atlas's cell
  height.

## 0.6.1 — 2026-10-28 (tag `v0.6.1`)

A scoped append, for kept lists placed under open scopes
(`docs/design/20261028_02_SCOPED_APPEND_PLAN.md`). Strictly additive, so a
patch release (Rule 5.4). `libmsdf` and `libmsdf-font` stay in lockstep:
both are 0.6.1 and both are covered by this entry and this tag, though
`libmsdf-font` itself is unchanged. The commit is the one tagged `v0.6.1`.
No dependency changed version.

- Added: `DrawList::append_scoped(&mut self, other: &DrawList, offset:
  [f32; 2])`, which splices `other` as if its calls were made now, moved by
  `offset`, inside the clip, effects and rotate/translate scopes open on
  this list. `append_from` is unchanged and still asserts that nothing is
  open.

## 0.6.2 — 2026-10-29 (tag `v0.6.2`)

Coding-agent interface glyphs on the terminal symbol page
(`docs/design/20261028_01_CODING_AGENT_GLYPHS_PLAN.md`). Additive coverage,
so a patch release, by the owner's answer to the plan's open question 1
(Rule 5.4): no type, signature or field changes, and every codepoint that
drew before draws the same outline at the same width. `libmsdf` and
`libmsdf-font` stay in lockstep: both are 0.6.2 and both are covered by this
entry and this tag. The commit is the one tagged `v0.6.2`. No dependency
changed version.

- Changed: `COVERAGE_TERMINAL` gains 63 codepoints (654): the interface
  marks `⋮ ⋯ ⊷ ⟳ ⊠ ⛁ ℹ ⓘ ≡ ⌐ ‖`, the key hints `⌦ ⇞ ⇟ ␣`, the control
  pictures U+2400-241F, `␡` and `␤`, `☰ ⌂ ⬢ ♥ ♪ ☺ Ⓘ ⓞ ⓧ`, `✋ ⏲ ⛔`, and
  `⊙ ⦿`. `☰ ⛔ ✋` are two cells; the rest are one.
- Changed: `COVERAGE_TERMINAL_EMOJI` gains 15 (39): `🟢 🟡 🔴 🔄 💭 🧠 🛠 🧑
  💻 📎 🚧 🔔 📂 🔐` and the wide form U+F0002.
- Changed: `TERMINAL_WIDE_FORM` gains `('\u{1F6E0}', '\u{F0002}')`: `🛠` is
  one cell, and two when U+FE0F follows it.
- Changed: the bytes of `TERMINAL_SYMBOLS_TTF` (sha256 `0d912327…`) and
  `TERMINAL_SYMBOLS_ATLAS` (694 cells). The page is 900 x 2000 texels, and
  `BundledAtlas::TerminalSymbols.columns()` is 18, computed from the
  page's cells (at most 40); an `AtlasSet` holding the page is 900 wide and
  pads a 16-column atlas to it. The other eight bundled atlases are
  byte-identical.
