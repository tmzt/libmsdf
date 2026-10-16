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
