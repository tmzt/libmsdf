# Grenze Gotisch

Render notes for the bundled Grenze Gotisch atlas (`font-ofl-grenze-gotisch`,
`fonts/atlases/grenze-gotisch-48.atlas`, from
`fonts/GrenzeGotisch[wght]-subset.ttf`). Every image is drawn from a baked
atlas with libmsdf's own GPU renderer, so it shows what libmsdf draws, not
what the TTF looks like; the top panel of each sheet is the face's own
outlines (FreeType) for comparison.

The images are generated evidence under `out/fonts/` (gitignored).
Entries are appended under dated headings and never edited.

## 2026-10-23 12:37 EDT — Rings clipped by the cell (`out/fonts/20261023_01_cap_clipping/`)

A bundled atlas cell is 1.3 em tall, and every bake puts the face's cap
height 0.15 of the cell from its top (`CapPlacement::DEFAULT`). For Grenze
Gotisch (1000 units per em, cap height 620) that window is y −485 to 815.
The rings of `Å` and `Ů` rise to 862, so the bake report lists both, each
clipped by 47 units: the top of the ring is cut flat.

The face's ink spans y −271 to 862, 1133 units, well inside the 1300-unit
cell, so a lower cap height fits all of it: any placement from 0.187 to
0.314. At **0.20** the bake report lists no ink past the cell, with 18 units
to spare at the top and 149 at the bottom. Each placement was baked in
memory from the committed face with only `cap.top_frac` changed; the 0.15
bake reproduces the committed atlas byte for byte.

| Cap placement | Clipped | Text moves down |
|---|---|---|
| 0.15 (today) | `Å Ů`, 47 units at the top | — |
| 0.20 | nothing | 0.065 em (about 1 px at 16 px) |

"Text moves down" is how far the baseline moves within the line box:
`push_shaped_text` places a run by its line-box top, so a lower cap draws
every glyph of the face lower by that much.

![The face's outlines, today's atlas and the 0.20 placement, with the line box (red) and baseline (blue)](./out/fonts/20261023_01_cap_clipping/grenze-gotisch_placements.png)

![Today's atlas at 2x: the rings of Å and Ů cut flat](./out/fonts/20261023_01_cap_clipping/grenze-gotisch_0.150_closeup.png)

![Cap at 0.20 at 2x: the rings whole](./out/fonts/20261023_01_cap_clipping/grenze-gotisch_0.200_closeup.png)

No larger cell is needed for this face.

The tools that made these images are kept beside them in
[out/fonts/20261023_01_cap_clipping/tool/](./out/fonts/20261023_01_cap_clipping/tool/);
the per-case metrics are in
[cases.jsonl](./out/fonts/20261023_01_cap_clipping/cases.jsonl).

## 2026-10-23 13:04 EDT — The per-atlas cell box, prototyped (`out/fonts/20261023_02_cell_box_prototype/`)

A throwaway prototype of `docs/design/20261023_02_PER_ATLAS_CELL_BOX_PLAN.md`
(the bake and the text box only, no format change; its diff is in
[tool/prototype.diff](./out/fonts/20261023_02_cell_box_prototype/tool/prototype.diff))
baked Grenze Gotisch with the plan's cell box: the 1.3 em cell shifted up
0.075 em. The bake report lists **no ink past the cell**, the rings of `Å`
and `Ů` are whole (top ink at row 13, as in the face's outlines, against
row 16 today), and the baseline is exactly where today's atlas puts it
(0.62692 of the line box). Unlike the 12:37 entry's 0.20 cap placement, the
text does not move, and the texel density is unchanged.

![Outlines, today's atlas and the cell box: the line box in red, the cell box dashed in green, the baseline in blue](./out/fonts/20261023_02_cell_box_prototype/grenze-gotisch_cell_box.png)
