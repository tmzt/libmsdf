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
