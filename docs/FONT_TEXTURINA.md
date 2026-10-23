# Texturina

Render notes for the bundled Texturina atlas (`font-ofl-texturina`,
`fonts/atlases/texturina-48.atlas`, from `fonts/Texturina[opsz,wght]-subset.ttf`).
Every image is drawn from a baked atlas with libmsdf's own GPU renderer, so
it shows what libmsdf draws, not what the TTF looks like; the top panel of
each sheet is the face's own outlines (FreeType) for comparison.

The images are generated evidence under `out/fonts/` (gitignored).
Entries are appended under dated headings and never edited.

## 2026-10-23 12:37 EDT — Accents clipped by the cell (`out/fonts/20261023_01_cap_clipping/`)

A bundled atlas cell is 1.3 em tall, and every bake puts the face's cap
height 0.15 of the cell from its top (`CapPlacement::DEFAULT`). For
Texturina (1000 units per em, cap height 720) that window is y −385 to 915.
Its accented capitals rise to 968 and `ĥ`/`ĺ` to 1064, so the bake report
lists 54 glyphs with ink past the cell:

| Glyphs | Ink top | Clipped today |
|---|---|---|
| `ÁÂÀĆČĈĎÉĚÊÈĜĤÍÎÌĴĹŃŇÓÔÒŐŔŘŚŠŜŤÚÛÙŰŴÝŶŹŽ` (39) | 968 | 53 units |
| `ÅŮ` | 959 | 44 |
| `ĊĖĠİŻ` | 944 | 29 |
| `ÃĨÑÕŨ` | 916 | 1 |
| `ĥĺ` | 1064 | 149 |
| `‰` | — | 28 units too wide (not a placement matter) |

The face's ink spans y −282 (`¿`) to 1064 (`ĥ`, `ĺ`): 1346 units, more than
the 1300-unit cell. **No cap placement fits all of it**; the placements
below trade which glyphs lose their tips. Each was baked in memory from the
committed face with only `cap.top_frac` changed; the 0.15 bake reproduces
the committed atlas byte for byte.

| Cap placement | Clipped at the top | Clipped at the bottom | Text moves down |
|---|---|---|---|
| 0.15 (today) | 53 glyphs, worst 149 units | — | — |
| 0.225 | `ĥ ĺ`, 52 units | — | 0.0975 em (1.6 px at 16 px) |
| 0.247 | `ĥ ĺ`, 23 units | `¿ ¡`, 24 units | 0.126 em |
| 0.265 | — | 11 glyphs, worst 47 units | 0.150 em |

"Text moves down" is how far the baseline moves within the line box:
`push_shaped_text` places a run by its line-box top, so a lower cap draws
every glyph of the face lower by that much.

![The face's outlines, today's atlas and three candidate placements, with the line box (red) and baseline (blue)](./out/fonts/20261023_01_cap_clipping/texturina_placements.png)

Close up (2×), today's atlas: the capitals' accents are cut flat and `ĥ`/`ĺ`
keep only the bottom of theirs.

![Today's atlas at 2x: accents cut flat at the cell top](./out/fonts/20261023_01_cap_clipping/texturina_0.150_closeup.png)

![Cap at 0.225 at 2x: every capital accent whole; the tips of ĥ and ĺ still clip](./out/fonts/20261023_01_cap_clipping/texturina_0.225_closeup.png)

![Cap at 0.247 at 2x](./out/fonts/20261023_01_cap_clipping/texturina_0.247_closeup.png)

![Cap at 0.265 at 2x: everything above fits; the tails of ¿ and ¡ and the low descenders clip](./out/fonts/20261023_01_cap_clipping/texturina_0.265_closeup.png)

### Would a larger cell fix it?

Fully, yes: Texturina needs a cell of at least 1.35 em (1346 units, plus a
little margin). But the cell is the line box for every atlas
(`CELL_EM_RATIO` = `LINE_BOX_RATIO` = 1.3): the shader maps a cell onto
exactly one line box, and the draw list positions runs by it. A taller cell
for one face means a per-set cell ratio in the atlas format, glyph quads
that overhang the line box, and draw-list positioning that knows about it:
a format change and its own plan, to save the accent tips of two lowercase
letters. Scaling Texturina down to 96.5% inside the cell would also fit, but
would make it draw smaller than its font size. Neither is proposed here; the
0.225 placement fixes every glyph but `ĥ` and `ĺ` without either.

The tools that made these images (a scratch crate against libmsdf at
`60d925c`, and the Pillow script that composes the sheets) are kept beside
them in [out/fonts/20261023_01_cap_clipping/tool/](./out/fonts/20261023_01_cap_clipping/tool/);
the per-case metrics are in
[cases.jsonl](./out/fonts/20261023_01_cap_clipping/cases.jsonl).

## 2026-10-23 13:04 EDT — The per-atlas cell box, prototyped (`out/fonts/20261023_02_cell_box_prototype/`)

A throwaway prototype of `docs/design/20261023_02_PER_ATLAS_CELL_BOX_PLAN.md`
(the bake and the text box only, no format change; its diff is in
[tool/prototype.diff](./out/fonts/20261023_02_cell_box_prototype/tool/prototype.diff))
baked Texturina with the plan's cell box, 0.179 em above the line box and
0.073 em less below (a 1.406 em cell), and with a two-texel variant (0.212
above, 0.042 less below, 1.470 em). The bake report lists **no ink past the
cell** for either, `‰` included, against 54 glyphs today, and the baseline
sits exactly where today's atlas puts it (0.70385 of the line box in all
three).

Each line's topmost ink, compared with the face's own outlines at 56 px
([extents.json](./out/fonts/20261023_02_cell_box_prototype/extents.json)):

| Line | Outlines | Today | Cell box |
|---|---|---|---|
| `ÁÂÀÅ…` | row 13 | row 16 (3 px cut) | row 12 |
| `ŃŇÑÓ…` | row 118 | row 121 (3 px cut) | row 117 |
| `ĥĺ ¿¡ ‰ …` | row 218 | row 226 (8 px cut) | row 217 |

The bottoms agree with the outlines within 1 px in every case.

![Outlines, today's atlas, the cell box and the two-texel cell box: the line box in red, the cell box dashed in green, the baseline in blue](./out/fonts/20261023_02_cell_box_prototype/texturina_cell_box.png)

At 16 px the larger cell (7.5% fewer texels per em, 11% for two texels)
is not visibly softer than today:

![The last line at 16 px, 4x without smoothing: today, the cell box, the two-texel cell box](./out/fonts/20261023_02_cell_box_prototype/texturina_16px.png)

This corrects the 12:37 entry's conclusion that only a format change and its
own plan could save `ĥ` and `ĺ`: the plan is written, and the prototype
shows it fixes every glyph without moving the text.

## 2026-10-23 14:12 EDT — The committed cell-box atlas (`out/fonts/20261023_04_cell_box_release/`)

`fonts/atlases/texturina-48.atlas` is now baked with
`BundledAtlas::cell`'s box, 0.179 em above the line box and 0.073 em less
below (a 1.406 em cell, format 5:0.2.0), at libmsdf `9c47e68`, and carries
`Ș ș Ț ț`. Its bake report lists **no ink past the cell**, `‰` included,
and its baseline sits where libmsdf 0.5's did (0.70385 of the line box).
`tests/cell_box_render.rs` checks every tall glyph's top and bottom against
its outline at 12 to 56 px, scale 1 and 2; libmsdf 0.5's atlas fails that at
every size, `ĥ ĺ` by up to 8.8 px at 56 px.

![Outlines, libmsdf 0.5's atlas and the committed cell-box atlas: the line box in red, the cell box dashed in green, the baseline in blue](./out/fonts/20261023_04_cell_box_release/texturina_0.5_vs_0.6.png)

This supersedes the 12:37 entry's proposed cap placements, which were
dropped; the 13:04 prototype is what shipped.
