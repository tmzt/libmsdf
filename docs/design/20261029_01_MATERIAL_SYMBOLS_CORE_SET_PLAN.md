---
title: A core set of Material Symbols in the merged Roboto face
date: 2026-10-29
status: Draft
covers: >
  Growing the borrowed half of the merged face (`font-apache-roboto-msymbols`:
  `ROBOTO_ASCII_MSYMBOLS`, `roboto-msymbols-48.atlas`,
  `roboto-msymbols-styled-48.atlas`) from 18 Material Symbols to 71, a
  common Android and Material 3 set, keeping every icon merged today. Covers
  the icon list with each declared codepoint, checked against the pinned
  upstream; the two proposed names that cannot be taken and what replaces
  them; one manifest file read by the merge script and by a test; the merge
  script's checks; the cell box the two merged atlases need so that
  25 of the 71 icons are not clipped at the top; the re-bake and the atlas
  sizes; `MSYMBOLS_ICONS` and the tests; the README; the release; and
  RoundTwo as the first user.
sources:
  - >
    Owner (Tim), 2026-10-29, in the RoundTwo app's session: "does the symbol
    font offer the standard/m3 versions and we just dont include them in the
    atlas? is there a standard set of approx 64 main android/m3 symbols?"
    Told that the merged face carries only a curated list, with no
    content_copy, mic, play, stop, close or delete, and that there is no
    official 64-icon core list but a commonly used set can be named, he
    answered "yes" to writing a libmsdf plan for it. The proposed set of
    section 3.2 was given with the request.
  - >
    Measurements made for this plan on 2026-10-29 with fontTools 4.67.0 of
    fonts/upstream/MaterialSymbolsOutlined[FILL,GRAD,opsz,wght].ttf
    (google/material-design-icons at 49d4db35df873165d6bd6ba09b063c7dafbac2f4,
    sha256 77ff1c20…), instanced at FILL 0, GRAD 0, opsz 24, wght 400 and
    scaled to 2048 units per em as fonts/msymbols.py does, and of the
    variablefont/MaterialSymbolsOutlined[FILL,GRAD,opsz,wght].codepoints
    manifest at the same commit (4,301 names, sha256 0c9dae3e…).
  - >
    fonts/msymbols.py (`ICONS`, `LOCATION`, the provenance check),
    fonts/icon.py (`folder`, `plus`), scripts/fetch-upstream-fonts.sh
    (`MSYMBOLS_COMMIT`), scripts/rebuild-fonts.sh (`FIXED`),
    font/src/lib.rs (`MSYMBOLS_ICONS`, `msymbols_codepoint`, `HIGHBAY_ICONS`,
    `PRIVATE_USE`, `OWNED_BLOCKS`, `GlyphSet::BorrowedIcons`),
    font/src/faces.rs (`ROBOTO_ASCII_MSYMBOLS`), font/src/bundled.rs
    (`ROBOTO_MSYMBOLS`, `BundledAtlas::cell`, `TEXTURINA_CELL`,
    `GRENZE_GOTISCH_CELL`), font/src/atlas.rs (`ATLAS_COLS`, `ATLAS_ROWS`,
    `CAP_TOP_FRAC`, `CapPlacement::window`, `glyph_projection_with`,
    `add_shipped_coverage`), font/src/bake.rs (`ink_outside`, the report),
    src/drawlist/mod.rs (`push_icon`), tests/coverage.rs,
    tests/highbay_icons.rs, tests/styled_faces.rs, tests/bundled_fonts.rs,
    tests/bundled_bake.rs, FONTS-LICENSE-APACHE.txt, fonts/atlases/
    (roboto-msymbols-48.atlas 2,414,684 bytes; roboto-msymbols-styled-48.atlas
    7,269,308 bytes; roboto-regular-48.atlas 2,413,984 bytes).
related:
  - docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md (the
    `font-apache-roboto-msymbols` feature, its two atlases and the committed
    bake; this plan grows that face)
  - docs/design/20261023_02_PER_ATLAS_CELL_BOX_PLAN.md (the cell box and
    format 5:0.2.0; this plan gives the two merged atlases one, as that plan
    did for Texturina and Grenze Gotisch)
  - docs/design/20261028_01_CODING_AGENT_GLYPHS_PLAN.md (the last coverage
    widening and its 0.6.1-or-0.7.0 question, which this plan follows)
  - RoundTwo, docs/design/20261029_06_MOCK_INTERVIEW_SESSION_PLAN.md (the
    first user: Copy buttons on the mock interview results need
    `content_copy`)
---

# A core set of Material Symbols in the merged Roboto face

## 1. Goal

The merged face draws Material Symbols only for a curated list, so an app
that wants an ordinary Android or Material 3 control icon (copy, close,
mic, play, stop, delete) finds no glyph and draws it by hand. RoundTwo is
the case in hand: its mock interview results need Copy buttons
(`content_copy`), and it draws ☰, ⋮, the chevrons and × itself.

Material publishes no official "core 64". This plan names a common set of
71 (the 18 merged today and 53 more), merges them from the pinned upstream
the face already comes from, and makes sure each one draws whole.

## 2. What is merged today

- **Borrowed** (`MSYMBOLS_ICONS`, `fonts/msymbols.py` `ICONS`): 18 Material
  Symbols, at Material's declared codepoints: chat, check, chevron_left,
  chevron_right, code, edit, expand_less, expand_more, home, library_books,
  menu, more_vert, person, redo, search, send, settings, undo. (The request
  counted 19; the list is 18.)
- **Drawn here** (`HIGHBAY_ICONS`, `fonts/icon.py`): 14 icons in the owned
  block U+F800..U+F80D: avatar, branch, database, device, folder, form,
  graph, plus, props, screen, sitemap, sparkle, table, widget. (The request
  counted 13; `folder` and `plus` were the last two added.) They are in both
  Roboto Regular faces and are not touched by this plan.
- **The source:** the `variablefont/` Material Symbols Outlined face of
  google/material-design-icons at commit
  `49d4db35df873165d6bd6ba09b063c7dafbac2f4`
  (`scripts/fetch-upstream-fonts.sh`, `MSYMBOLS_COMMIT`; sha256
  `77ff1c20896afc6f1abf7d57fc408234dfd021ab050b63e0442e71d6c9f0d6b8`),
  instanced at FILL 0, GRAD 0, opsz 24, wght 400 and scaled from 960 to
  2048 units per em. Material Symbols is © Google, Apache-2.0
  (`FONTS-LICENSE-APACHE.txt`), the same licence as Roboto, so the feature
  and its licence records do not change.
- **The source already has every proposed name** but one is at a codepoint
  this face cannot use (section 3.3). The atlas carries only what the face
  merges: `add_shipped_coverage` queues U+E000..U+F7FF as whatever the face
  maps there, so a name is in the atlas exactly when `msymbols.py` merged
  it.

## 3. Findings (measured 2026-10-29)

### 3.1 The declared codepoint

As today, each name takes the codepoint Material's `.codepoints` manifest
declares for it, not a legacy Material Icons alias. Many of the new names
have such an alias in the face (for example `delete` U+E872, `mic`
U+E029, `account_circle` U+E853, `check_circle` U+E86C, `warning` U+E002,
`star` U+E838, `lock` U+E897, `help` U+E887). The manifest is not
committed or fetched today; the checks below need it (step 1).

Every name of section 3.2 was checked against the source:
- it is declared in the manifest at the codepoint given;
- the source's `cmap` maps that codepoint to the glyph of that name;
- the glyph is simple (not composite) and has contours at the default
  instance;
- its advance is 2048, like every icon merged today;
- the codepoint is not already mapped in the merged face.

### 3.2 The set: 71 icons

"kept" are the 18 merged today; the rest are new. "Top" is the glyph's
highest ink, in font units, used in section 3.5.

**Navigation (14)**

| Name | Codepoint | | Top |
| --- | --- | --- | --- |
| arrow_back | U+E5C4 | new | 1707 |
| arrow_forward | U+E5C8 | new | 1707 |
| arrow_upward | U+E5D8 | new | 1707 |
| arrow_downward | U+E5DB | new | 1707 |
| close | U+E5CD | new | 1621 |
| menu | U+E5D2 | kept | 1536 |
| more_vert | U+E5D4 | kept | 1707 |
| more_horiz | U+E5D3 | new | 1195 |
| expand_more | U+E5CF | kept | 1367 |
| expand_less | U+E5CE | kept | 1367 |
| chevron_left | U+E5CB | kept | 1536 |
| chevron_right | U+E5CC | kept | 1536 |
| home | U+E9B2 | kept | 1792 |
| open_in_new | U+E89E | new | 1792 |

**Actions (23)**

| Name | Codepoint | | Top |
| --- | --- | --- | --- |
| add | U+E145 | new | 1621 |
| remove | U+E15B | new | 1109 |
| check | U+E668 | kept | 1538 |
| done_all | U+E877 | new | 1538 |
| edit | U+F097 | kept | 1792 |
| delete | U+E92E | new | 1792 |
| content_copy | U+E14D | new | 1877 |
| content_paste | U+E14F | new | 1963 |
| share | U+E80D | new | 1877 |
| search | U+EF7A | kept | 1792 |
| refresh | U+E5D5 | new | 1707 |
| undo | U+E166 | kept | 1707 |
| redo | U+E15A | kept | 1707 |
| download | U+F090 | new | 1707 |
| upload | U+F09B | new | 1707 |
| send | U+E163 | kept | 1707 |
| attach_file | U+E226 | new | 1877 |
| link | U+E250 | new | 1451 |
| filter_list | U+E152 | new | 1536 |
| sort | U+E164 | new | 1536 |
| tune | U+E429 | new | 1792 |
| settings | U+E8B8 | kept | 1877 |
| logout | U+E9BA | new | 1792 |

**Media and voice (11)**

| Name | Codepoint | | Top |
| --- | --- | --- | --- |
| mic | U+E31D | new | 1877 |
| mic_off | U+E02B | new | 1877 |
| play_arrow | U+E037 | new | 1621 |
| pause | U+E034 | new | 1621 |
| stop | U+E047 | new | 1536 |
| replay | U+E042 | new | 1963 |
| skip_next | U+E044 | new | 1536 |
| volume_up | U+E050 | new | 1773 |
| volume_off | U+E04F | new | 1809 |
| record_voice_over | U+E91F | new | 1873 |
| graphic_eq | U+E1B8 | new | 1877 |

**Status (15)**

| Name | Codepoint | | Top |
| --- | --- | --- | --- |
| info | U+E88E | new | 1877 |
| help | U+E8FD | new | 1877 |
| warning | U+F083 | new | 1877 |
| report | U+F052 | new, in place of `error` | 1792 |
| check_circle | U+F0BE | new | 1877 |
| cancel | U+E888 | new | 1877 |
| lock | U+E899 | new | 1963 |
| visibility | U+E8F4 | new | 1707 |
| visibility_off | U+E8F5 | new | 1809 |
| notifications | U+E7F5 | new | 1877 |
| schedule | U+EFD6 | new | 1877 |
| timer | U+E425 | new | 1963 |
| history | U+E8B3 | new | 1792 |
| star | U+F09A | new | 1877 |
| bookmark | U+E8E7 | new | 1792 |

**Things (8)**

| Name | Codepoint | | Top |
| --- | --- | --- | --- |
| person | U+F0D3 | kept | 1707 |
| account_circle | U+F20B | new | 1877 |
| chat | U+E0C9 | kept | 1877 |
| mail | U+E159 | new | 1707 |
| description | U+E873 | new | 1877 |
| lightbulb | U+E90F | new | 1877 |
| code | U+E86F | kept | 1536 |
| library_books | U+E02F | kept | 1877 |

Every glyph's ink lies within x 85..1963 and y 119..1963, inside the
2048-unit em box. The highest codepoint is `account_circle` U+F20B.

### 3.3 Dropped and substituted

- **`error` is dropped, and `report` (U+F052) takes its place.** Material
  declares `error` (and `error_outline`) only at **U+F8B6**, which is
  inside this repo's owned blocks (`OWNED_BLOCKS`, U+F800..U+F8FF;
  `HIGHBAY_ICONS_BLOCK` is U+F800..U+F8EF). Borrowed icons must sit below
  U+F800 (`owned_blocks_are_the_top_of_the_carveout`), and the bake queues
  U+F800..U+F8EF as owned icons. The face also maps `error` at the legacy
  U+E000 and U+E001, but the manifest declares no name there, and
  `covers_still_says_no_for_an_uncovered_codepoint` relies on U+E000
  staying uncovered. `report` is the octagonal alert mark, declared at
  U+F052. A circled `!` at a usable declared codepoint does not exist in
  this build. Open question 2.
- **`folder` is dropped.** `HIGHBAY_ICONS` already has a `folder` (U+F80C,
  the toolbar's Open mark, drawn in `fonts/icon.py`), and
  `a_name_never_crosses_between_the_two_vocabularies` forbids one name in
  both manifests. `icon.py` says that when Material's `folder` is bundled,
  the drawn one should be renamed. Renaming a published `HIGHBAY_ICONS`
  name is not additive, so it is left to open question 3. Material's
  `folder` (U+E2C7) is available for when that is decided.
- `add` and the drawn `plus` are different names, so both stay. They draw
  nearly the same mark.

### 3.4 Capacity

The merged face's shipped coverage is **249 cells** today
(`a_style_does_not_fit_beside_the_shipped_coverage_today`). The layer holds
320 (`ATLAS_COLS` 8 × `ATLAS_ROWS` 40). With 53 new icons it is **302 of
320**, 18 free. The texture keeps its size (400 × 2000 texels per layer, 3
channels) and no other cell moves: `GlyphSet::BorrowedIcons` is the last
set of the Regular layer, so new cells go after every existing one. The
two style layers of the styled atlas (215 cells each) carry no icons and
are unchanged in content. A later wave of more than 18 needs more columns
or a layer of its own (open question 5).

### 3.5 The cell clips icons at the top

The two merged atlases use the line box as their cell. With
`CapPlacement::DEFAULT` the cell holds y −807.04..1855.36 in font units (the
cap height, 1456 from `A`, sits 15% of the 1.3 em cell below its top). One
texel at the 48 px cell is 2662.4 / 48 = 55.47 units. `push_icon` centres
the em box (0..2048, Material's 24 dp square) in the target rect, so an
icon's position does not depend on the cell, but ink above 1855.36 is
clipped by the cell.

- **25 of the 71 icons reach above it.** 21 reach 1873–1877 (22 dp), about
  0.4 texel over. This includes three icons merged today: chat,
  library_books and settings. Four reach **1963** (23 dp), 107.6 units or
  1.9 texels over: content_paste, lock, replay and timer. At a 24 dp draw
  that cuts the top 1.27 dp off the lock's shackle and the clipboard's
  clip.
- The bake does not report it. `ink_outside` runs only for coverage-group
  bakes, and the Roboto atlases use shipped coverage, so their reports
  read "shipped coverage (no coverage groups)".
- By the same arithmetic, Roboto's `Å` (U+00C5, top 1939; 1940 in the
  italic) reaches 84 units over in every Roboto atlas. That is a finding
  for the plain atlases too, outside this plan (open question 4).

**The fix is a cell box, as for Grenze Gotisch** (per-atlas cell box plan):
move the 1.3 em cell up by 0.1 em, `CellBox { above_em: 0.1, below_em:
-0.1 }`. The cell then holds y −602.24..2060.16. It keeps its size, so there
is no loss of resolution, and the whole em box (0..2048) is inside it.
Margins:

| Ink | Extent | Margin |
| --- | --- | --- |
| icons, highest | 1963 | 97 units (1.75 texels) |
| Roboto `Å`, highest text | 1939 (Regular), 1940 (Italic) | 120 units or more |
| the edge markers, lowest (U+F8F0) | −512 | 90 units (1.63 texels) |
| Roboto `§`, lowest text | −495 (Regular), −497 (Italic), −465 (Bold, `ç`) | 105 units or more |

Any shift between 0.080 and 0.117 em leaves a texel of margin at both
ends; 0.1 is taken. The baseline stays where the line box puts it, so text
and icons draw at the same positions as before, whole. The two atlases
then carry a `CELL` table and are written at format 5:0.2.0 (section 6).

### 3.6 Atlas sizes

| Atlas | Now | Estimated after |
| --- | --- | --- |
| `roboto-msymbols-48.atlas` | 2,414,684 bytes | about 2,416,600 (+1.9 KB) |
| `roboto-msymbols-styled-48.atlas` | 7,269,308 bytes | about 7,271,200 (+1.9 KB) |

- The pixel block has a fixed size: 400 × 2000 × 3 = 2,400,000 bytes per
  layer, 1 layer and 3 layers. Only the tables grow.
- The merged atlas is 700 bytes larger than `roboto-regular-48.atlas`
  (2,413,984). The two differ by the 18 icons and one licence record, so an
  icon costs about 36 bytes: its 32-byte glyph record and its layout-table
  entries. 53 × 36 ≈ 1.9 KB, plus about 32 bytes for the `CELL` table and
  its directory record.
- The estimate is checked in step 5. Each file is measured with `wc -c`
  before and after the bake, and with `gzip -9 -c <file> | wc -c` (the
  cells' new texels are what compress worse). Both are recorded in the
  CHANGES.md landing addendum. The GPU memory of the texture does not
  change.

## 4. Design

### 4.1 One manifest file: `fonts/msymbols-icons.txt`

The list of section 3.2 moves out of `msymbols.py` into a data file, read by
the script and compared with `MSYMBOLS_ICONS` by a test, as
`fonts/coverage-groups.txt` is for the coverage groups:

```text
# The borrowed Material Symbols of Roboto-Regular-ascii-msymbols.ttf, one
# per line: <name> U+XXXX <group>. Sorted by name. The codepoint is the one
# Material's .codepoints manifest declares at MSYMBOLS_COMMIT
# (scripts/fetch-upstream-fonts.sh), always below U+F800. Read by
# fonts/msymbols.py; tests/msymbols_icons.rs compares it with MSYMBOLS_ICONS.
# docs/design/20261029_01_MATERIAL_SYMBOLS_CORE_SET_PLAN.md, section 3.2.
account_circle U+F20B things
add U+E145 actions
...
```

There are 71 lines, sorted by name (byte order, as `binary_search` needs).
Each group is one of `navigation`, `actions`, `media`, `status` or
`things`. Blank lines and `#` lines are ignored.

### 4.2 The upstream manifest

`scripts/fetch-upstream-fonts.sh` fetches
`variablefont/MaterialSymbolsOutlined[FILL,GRAD,opsz,wght].codepoints` at
`MSYMBOLS_COMMIT` into `fonts/upstream/` (gitignored), checked against sha256
`0c9dae3ed678a81549c3be046bc8045335e5f57f2cbe2287360fbf0c2cc1355f`
(79,354 bytes, 4,301 lines `<name> <hex>`). Add it to `DIRECT` and to the
header comment.

### 4.3 `fonts/msymbols.py`

New usage:

```text
./msymbols.py <base.ttf> <out.ttf> <MaterialSymbolsOutlined[...].ttf> <MaterialSymbolsOutlined[...].codepoints>
```

- `ICONS` is read from `msymbols-icons.txt` beside the script
  (`Path(__file__).with_name(...)`), not written in the script.
- Before merging, the script collects **every** failure and then exits
  non-zero with the full list, so no failure stops at the first. It
  checks:
  1. the file is sorted by name, with no repeated name or codepoint;
  2. every codepoint is in U+E000..U+F7FF;
  3. every `(name, codepoint)` is declared in the `.codepoints` manifest;
  4. the source `cmap` maps the codepoint to the glyph `name`;
  5. the glyph is simple and has contours (the existing composite check,
     and a new empty check);
  6. a codepoint the base face already maps must be an icon already merged
     (the provenance check, unchanged).
- After merging, the script checks the face it is about to save. The
  borrowed range U+E000..U+F7FF of its `cmap` must be **exactly** the
  file's 71 codepoints: none missing, none extra (no legacy alias). Each
  must map to a glyph with contours.
- It prints one line: `<out>: <n> glyphs (+53), 18 already merged and
  verified, added <name U+XXXX, ...>`. On the next run it prints `nothing
  to add (71 icons already merged and verified)`.
- The module docstring is updated: the three waves become four, there are
  71 icons, and it names the manifest file. The appended glyphs follow the
  file's (name) order, as the existing loop does.

`scripts/rebuild-fonts.sh` passes the `.codepoints` file as the fourth
argument, and its `FIXED` entry takes the new face's sha256.

### 4.4 The face

`fonts/Roboto-Regular-ascii-msymbols.ttf` is regenerated by running the
script over the committed face with
`SOURCE_DATE_EPOCH=1789684994` (its `head.modified`, as in `FIXED`):

```sh
SOURCE_DATE_EPOCH=1789684994 uv run --quiet --no-project --with fonttools==4.67.0 python3 -I \
  fonts/msymbols.py fonts/Roboto-Regular-ascii-msymbols.ttf out/bundled/msymbols.ttf \
  "fonts/upstream/MaterialSymbolsOutlined[FILL,GRAD,opsz,wght].ttf" \
  "fonts/upstream/MaterialSymbolsOutlined[FILL,GRAD,opsz,wght].codepoints"
```

The output is then copied over the committed face. The run is additive:
no glyph id moves, and the 262 glyphs of today keep their ids, so the new
ones are 262..=314. The new face's sha256 goes into
`font/src/bundled.rs` (`ROBOTO_MSYMBOLS.sha256`) and `rebuild-fonts.sh`
(`FIXED`). `FONTS-LICENSE-APACHE.txt`'s entry is updated together with the
face (AGENTS.md, "Bundled fonts and atlases", item 3): it names the
pinned commit `49d4db35…` and "71 icons listed in
fonts/msymbols-icons.txt".

### 4.5 `libmsdf-font`: the manifest constant and the cell box

- `MSYMBOLS_ICONS` (font/src/lib.rs) becomes the 71 entries of the file,
  sorted by name. Its type, `&[(&str, char)]`, and `msymbols_codepoint` do
  not change. No new public item is added; a caller resolves
  `msymbols_codepoint("content_copy")` as it resolves `"send"` today.
- The doc comments that count or list the set are updated:
  - `PRIVATE_USE` and `OWNED_BLOCKS`: "the eighteen … top out at U+F0D3"
    becomes the 71, topping out at U+F20B, still more than 1,500
    codepoints below U+F800;
  - `ATLAS_COLS` and `ATLAS_ROWS`: the budget table becomes 302 of 320,
    18 free;
  - `ROBOTO_ASCII_MSYMBOLS` (font/src/faces.rs): its icon table names the
    groups and points at `fonts/msymbols-icons.txt`, and it says why
    `error` and `folder` are not there (section 3.3).
- `font/src/bundled.rs` gains
  `const ROBOTO_MSYMBOLS_CELL: CellBox = CellBox { above_em: 0.1,
  below_em: -0.1 };` with a doc comment giving section 3.5's window and
  margins. `BundledAtlas::cell` returns it for `RobotoMsymbols` and
  `RobotoMsymbolsStyled`. `Baker::bundled_options` already takes the cell
  from `cell()`, so the bake needs no other change.

### 4.6 The re-bake

The atlases are re-baked by the one command AGENTS.md allows:
`cargo run -p libmsdf-font --features bake --bin libmsdf-bake -- bundled
--out fonts/atlases --hashes`. Only `roboto-msymbols-48.atlas`,
`roboto-msymbols-styled-48.atlas` and their reports may change bytes; the
other seven must be byte-identical. The new hashes go into
`fonts/bundled.sha256` in their own commit, after the owner agrees
(AGENTS.md, item 2).

### 4.7 README

In "Bundled fonts", add a short **Icons** paragraph after the terminal
text one:
- `font-apache-roboto-msymbols` carries 71 Material Symbols (Outlined, FILL
  0, weight 400), named by `MSYMBOLS_ICONS`, in five groups;
- names are resolved with `msymbols_codepoint`, and the icons this repo drew
  with `highbay_codepoint`;
- `None` is a missing asset, not a fallback.

With one example:

```rust
let icons = libmsdf::FontAtlas::from_bytes(libmsdf::ROBOTO_MSYMBOLS_ATLAS)?;
let shaper = libmsdf::TextShaper::new(libmsdf::ROBOTO_ASCII_MSYMBOLS.to_vec())?;
let copy = libmsdf::msymbols_codepoint("content_copy").expect("bundled icon");
let run = shaper.shape(copy.encode_utf8(&mut [0; 4]));
list.push_icon(&run, &icons, [x, y], [24.0, 24.0], 24.0, px_range, white);
```

It also says the two merged atlases use a cell 0.1 em above the line box,
so the whole 24 dp square draws. The Architectural decisions row is added
with this plan and updated with each status change.

## 5. Tests

**`tests/msymbols_icons.rs`, new**, `#![cfg(all(feature =
"font-apache-roboto-msymbols", feature = "shaper"))]`:
- `the_constant_is_the_manifest_file`: `MSYMBOLS_ICONS` equals the
  `(name, codepoint)` pairs of
  `include_str!("../fonts/msymbols-icons.txt")`, in order.
- `seventy_one_sorted_one_to_one_below_the_owned_blocks`:
  - 71 entries, sorted by name, no repeated name or codepoint;
  - every codepoint in U+E000..U+F7FF;
  - every group is one of the five.
- `the_core_set_resolves`: every name of section 3.2 resolves to its
  codepoint, `content_copy` to U+E14D among them. `error` and `folder` give
  `None` from `msymbols_codepoint`; `folder` gives `Some` from
  `highbay_codepoint`.
- `every_icon_is_in_the_face_and_both_atlases`: for each icon,
  - `ROBOTO_ASCII_MSYMBOLS` covers it;
  - shaped through `AtlasShaper` of `ROBOTO_MSYMBOLS_ATLAS` (and, with
    `font-apache-roboto-bold` and `-italic`, of
    `ROBOTO_MSYMBOLS_STYLED_ATLAS`), it gives one glyph that is not the
    placeholder and has a cell (`get_glyph` is `Some`).
- `every_carried_glyph_fits_the_cell`: for every glyph of the merged face
  that `ROBOTO_MSYMBOLS_ATLAS` has a cell for, its bounding box lies inside
  `BundledAtlas::RobotoMsymbols.cap().window(&face,
  BundledAtlas::RobotoMsymbols.cell())` with at least one texel (55.47
  units) to spare at top and bottom, and it is no wider than the cell. The
  failure message names each glyph and how far it is over. This is the
  check the shipped-coverage report does not make (section 3.5). It runs
  on the Bold and Italic faces with the styled atlas too.

**Updated:**
- `tests/styled_faces.rs`, `a_style_does_not_fit_beside_the_shipped_coverage_today`:
  249 becomes **302**, and its comment says why. A style (215) still does
  not fit.
- `tests/bundled_fonts.rs`:
  - `every_committed_atlas_carries_its_cell_box` already reads
    `BundledAtlas::cell()`; its doc comment names the two merged atlases
    beside Texturina and Grenze Gotisch;
  - the reference hashes come from the new `fonts/bundled.sha256`.
- `tests/bundled_bake.rs`: a `the_merged_atlases_take_the_cell_box` test,
  next to the Texturina and Grenze one, asserts `cell()` and
  `bundled_options(..).cell` for both atlases. It also asserts that the
  baseline row of a text glyph is where a line-box bake puts it, as that
  test does.

**Unchanged, and must still pass:**
- `a_name_never_crosses_between_the_two_vocabularies` (it is why `folder`
  is dropped);
- `owned_blocks_are_the_top_of_the_carveout` (why `error` is dropped);
- `an_unknown_icon_name_never_becomes_a_codepoint` and
  `covers_still_says_no_for_an_uncovered_codepoint` (U+E000 stays
  uncovered);
- the merged-versus-plain test, which checks that the merged bake's extra
  cells are exactly `MSYMBOLS_ICONS.len()` borrowed cells, all at the end.

## 6. Versioning (Rule 5)

**Adding glyphs to a bundled atlas is additive.** The 0.6.2 release, by the
owner's answer to that plan's open question 1, shipped 63 new codepoints
on the terminal page as a patch: no type, signature or field changed, and
every codepoint that drew before drew the same outline. The 53 icons alone
are the same kind of change, and would ship as **0.6.3**. The 53 new names
in `MSYMBOLS_ICONS` and `msymbols_codepoint` are additions to a slice, not
to an exhaustive enum.

**The cell box is not additive, so this plan proposes 0.7.0**, for
`libmsdf` and `libmsdf-font` in lockstep (tag `v0.7.0`). The 0.6.0 entry
marked the same change for Texturina and Grenze Gotisch **Breaking**:
- **Breaking:** `ROBOTO_MSYMBOLS_ATLAS` and `ROBOTO_MSYMBOLS_STYLED_ATLAS`
  move to atlas format 5:0.2.0 with a `CELL` table, which libmsdf 0.5 and
  earlier refuse. `BundledAtlas::RobotoMsymbols.cell()` and
  `RobotoMsymbolsStyled.cell()` change from the line box.
- **Breaking (rendering):** text and icons from those two atlases are drawn
  from a cell 0.1 em higher. Positions do not change. What was clipped at
  the top now draws: the 22 dp and 23 dp icons, and `Å`.
- **Changed:** `MSYMBOLS_ICONS` gains 53 entries (71), and
  `msymbols_codepoint` resolves them. The bytes of `ROBOTO_ASCII_MSYMBOLS`
  and the two atlases change. The other seven atlases are byte-identical.

Under Rule 5.5, libcontrols, which pins libmsdf, releases a new minor
version when it takes 0.7.0. RoundTwo pins libcontrols together with the
libmsdf it pins, so it moves when libcontrols does. Without the cell box
(open question 1) the release is 0.6.3, and libcontrols takes it with no
release of its own.

## 7. Steps

1. **The upstream manifest.**
   - Section 4.2 in `scripts/fetch-upstream-fonts.sh`.
   - Commit: the script alone (repository level).

   **Done when:** running the script downloads the `.codepoints` file once
   and then reports it present, its sha256 is `0c9dae3e…`, it has 4,301
   lines, and a corrupted copy fails with the expected-hash message.

2. **The manifest file and the merge script.**
   - `fonts/msymbols-icons.txt` with section 3.2's 71 lines.
   - Section 4.3 in `fonts/msymbols.py`.
   - The fourth argument in `scripts/rebuild-fonts.sh`. Its `FIXED` hash
     changes in step 3.
   - Commit: these three files (tooling and scripts).

   **Done when:**
   - run over the committed face, the script adds 53 icons and verifies 18;
   - run over its own output, it adds nothing;
   - each of these, made on a scratch copy of the file, makes it exit
     non-zero naming the line: `error U+F8B6` (outside U+E000..U+F7FF), a
     wrong codepoint for a declared name, an undeclared name, and two
     lines out of order;
   - two runs give the same sha256.

3. **The face (`libmsdf-font`).**
   - Regenerate the face with section 4.4's command.
   - Its sha256 goes into `font/src/bundled.rs`.
   - The `FONTS-LICENSE-APACHE.txt` entry, and section 4.5's
     `MSYMBOLS_ICONS` and doc comments.
   - Commit: the face, `font/src/`, the licence file (AGENTS.md item 3).
   - Separate commit: the `FIXED` hash in `scripts/rebuild-fonts.sh`.

   **Done when:**
   - `scripts/rebuild-fonts.sh` (without `--atlases`) reports the committed
     merged face a fixed point;
   - `cargo fmt -p libmsdf-font -- --check` and `cargo clippy -p
     libmsdf-font --all-targets` are clean, with no new warnings;
   - `cargo test -p libmsdf-font` passes.

4. **The cell box (`libmsdf-font`).**
   - `ROBOTO_MSYMBOLS_CELL` and the two `cell()` arms (section 4.5).
   - The `tests/bundled_bake.rs` test of section 5 lands with step 5,
     since it is in the root crate.
   - Commit: `font/src/bundled.rs`.

   **Done when:** fmt, clippy and `cargo test -p libmsdf-font` pass.

5. **The re-bake (`libmsdf`).**
   - Record the sizes first: `wc -c` and `gzip -9 -c | wc -c` of the two
     atlases.
   - Bake with section 4.6's command (several minutes for all nine; never
     `cargo clean`).
   - Check with `-- bundled --check fonts/atlases`.
   - Record the sizes again.
   - Commit: `fonts/atlases/` with the tests of section 5 that read the
     atlases (`tests/msymbols_icons.rs`, `tests/styled_faces.rs`,
     `tests/bundled_bake.rs`).
   - Separate commit: `fonts/bundled.sha256`, once the owner agrees to the
     byte change.

   **Done when:**
   - `--check` passes, and only the two merged atlases and their reports
     changed;
   - the new sizes are within 0.5 KB of section 3.6's estimates, or the
     difference is explained in the landing addendum;
   - `cargo test --features fonts-apache-roboto,fonts-ofl-ledger,fonts-ofl-terminal
     --test bundled_fonts` passes after the hash commit;
   - `cargo test --features fonts-apache-roboto --test msymbols_icons
     --test styled_faces` passes, with every icon asserted and no glyph
     past its cell;
   - fmt and clippy are clean for `libmsdf`.

6. **Evidence.**
   - A sheet of the 71 icons drawn by libmsdf from the new
     `roboto-msymbols-48.atlas`, in the five groups, each labelled with
     its name, at 24 and 48 px.
   - Beside it, the four 23 dp icons (content_paste, lock, replay, timer)
     and `chat` drawn from the old and the new atlas, showing the clipped
     and whole tops.
   - Use `examples/font_sheets.rs` if it exists by then (font sheet
     examples plan). Otherwise copy the scratch tool of
     `out/fonts/20261029_01_agent_glyphs/tool/` into the run's `tool/`.
   - Output goes to the next free `out/fonts/YYYYMMDD_NN_msymbols_core/`,
     with an entry in `out/fonts/LOG.md`.

   **Done when:**
   - every icon draws whole and none is the placeholder;
   - the before and after tops differ as section 3.5 says;
   - the log entry is written.

7. **README and release.**
   - Section 4.7 in `README.md`.
   - Both crates' `version` at the agreed version (0.7.0, or 0.6.3), with
     the `VERSIONS.md` entry of section 6.
   - Tag `v<version>` on the version commit.
   - A CHANGES.md landing addendum with the sizes of step 5.
   - This plan's amendment marking it Implemented, with the commits of each
     step.
   - The release waits for the owner's approval.

   **Done when:**
   - fmt, clippy with no new warnings, the full test suite with
     `gpu-tests`, and `scripts/check-font-features.sh` all pass at the
     tagged commit;
   - the tag is on the version commit.

8. **RoundTwo, the first user.** RoundTwo (`~/src/RoundTwo`, branch
   `ui-shell`) depends on libmsdf at `bd7383d` with
   `features = ["font-apache-roboto-regular"]`. It will:
   - enable `font-apache-roboto-msymbols`;
   - draw its mock interview results' Copy buttons with
     `msymbols_codepoint("content_copy")` (its mock interview session
     plan);
   - replace its hand-drawn ☰, ⋮, chevrons and × with `menu`, `more_vert`,
     `chevron_left`/`chevron_right` and `close`.

   That work is planned and committed in RoundTwo under its own rules, not
   here. It moves its libmsdf and libcontrols pins together (section 6).

   **Done when:** the release commit and tag are given to RoundTwo's
   session, and RoundTwo has recorded the change in its own CHANGES.md or
   plan.

## 8. Open questions

1. **The cell box, or 0.6.3 without it?** Section 3.5 proposes the cell box
   so that no icon is clipped, which makes the release 0.7.0 and
   libcontrols a minor release. Without it the release is strictly
   additive (0.6.3). Then:
   - the 21 icons at 22 dp lose about 0.4 texel at the top, as chat,
     library_books and settings already do;
   - content_paste, lock, replay and timer lose 1.9 texels, unless they
     are left out of this wave.

   Which?
2. **`report` for `error`?** Material's `error` is only at U+F8B6, inside
   the owned blocks. `report` (the octagon) is proposed in its place. The
   alternative is to leave out an error mark, using `warning` and
   `cancel`. Which?
3. **Material's `folder`.** It is left out because the drawn
   `HIGHBAY_ICONS` `folder` has the name. `icon.py` says the drawn one
   should then be renamed. That changes a published name, so it is not
   additive. Leave it out, or rename the drawn one in a later breaking
   release and add Material's then?
4. **`Å` in the plain Roboto atlases.** It reaches 84 units past the top of
   the cell in `roboto-regular`, `-bold` and `-italic` too. This plan fixes
   it only in the merged atlases. Should the same 0.1 em cell box go to the
   three plain ones (another breaking change, and the plain and merged
   atlases stay alike), in this release or a later one?
5. **Headroom.** 302 of 320 cells leaves 18. The next wave of more than 18
   icons needs 16 columns (a re-layout of every cell) or the icons on a
   layer of their own. Is 18 enough for now?
6. **FILL 0 only.** The face is instanced at FILL 0, so `star`,
   `bookmark`, `play_arrow`, `stop` and `check_circle` are outlines. M3
   uses filled forms for selected states. A filled set would be a second,
   separately named set of glyphs, not in this plan. Wanted later?
