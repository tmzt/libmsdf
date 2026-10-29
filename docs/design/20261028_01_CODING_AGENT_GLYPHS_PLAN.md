---
title: Coding-agent interface glyphs on the terminal symbol page
date: 2026-10-28
status: Draft
covers: >
  Adding the interface glyphs that Claude Code, Codex, Gemini CLI and
  Antigravity print, and that no bundled terminal atlas carries, to the
  terminal symbol page: 63 codepoints to `COVERAGE_TERMINAL` (interface
  marks, key hints, the control pictures, Claude Code's figures fallbacks,
  three text-presentation symbols, and the owner's ⊙ and ⦿) and 14 to
  `COVERAGE_TERMINAL_EMOJI` (with one more wide form, for 🛠). Covers which
  Noto face supplies each one, the width of each in cells, the page's
  capacity and the 18-column page it now needs, the regenerated face and
  atlas, the tests that assert every new codepoint, the evidence sheet,
  and the libmsdf 0.6.1 release.
sources:
  - >
    Owner (Tim), 2026-10-28, through flankr's session: "add the empty and
    filled version [⊙ U+2299, ⦿ U+29BF] and the dingbats/gemini logo to the
    common symbols, other symbols used by Claude, codex, agy, etc.", then
    "yes" to writing this plan with the list of section 3.
  - >
    A survey of the glyphs printed by Claude Code 2.1.312, Codex 0.162.1,
    Gemini CLI 0.38.2 and Antigravity 1.4.7, made in flankr's session on
    2026-10-28, checked codepoint by codepoint against the bundled coverage
    groups and the Noto faces (section 3.1).
  - >
    Measurements made for this plan on 2026-10-28 with fontTools 4.67.0,
    wcwidth 0.9.4 and Python's unicodedata (Unicode 15.1.0), of the
    pinned upstream faces in fonts/upstream/ (section 3).
  - >
    fonts/terminal_symbols.py (`TERMINAL_ORDER`, `WIDE_FORMS`,
    `NARROW_EMOJI`, `fitted`, the 591 and 24 asserts), fonts/coverage-groups.txt,
    font/src/coverage.rs (`COVERAGE_TERMINAL`, `COVERAGE_TERMINAL_EMOJI`,
    the counts test), font/src/terminal.rs (`TERMINAL_WIDE_FORM`),
    font/src/bundled.rs (`BundledAtlas::columns`, `TERMINAL_SYMBOLS`),
    font/src/atlas.rs (`ATLAS_COLS`, `ATLAS_ROWS`, the per-layer check in
    `build`), src/drawlist/atlas_set.rs (`AtlasSet::extent`),
    tests/bundled_fonts.rs, tests/terminal_glyphs.rs,
    tests/terminal_shaper.rs, fonts/atlases/terminal-symbols-48.atlas
    (800 x 2000, 616 glyphs, 3 channels, 4,828,224 bytes).
related:
  - docs/design/20261023_01_TERMINAL_FONTS_PLAN.md (the symbol page, its
    face merge, Rule 2 fitting, the 640-cell layer and the wide forms of
    the 02:32 EDT amendment; this plan extends its curated sets)
  - docs/design/20261023_02_PER_ATLAS_CELL_BOX_PLAN.md (its 13:23 EDT
    amendment last added glyphs to the page; this plan follows it)
  - docs/design/20261023_03_FONT_SHEET_EXAMPLES_PLAN.md (the `font_sheets`
    example step 5 uses for the evidence sheet, if it exists by then)
  - docs/design/20261021_03_ATLAS_SET_PLAN.md (`AtlasSet`, which pads
    atlases of different sizes to the widest)
---

# Coding-agent interface glyphs on the terminal symbol page

## 1. Goal

flankr draws coding-agent sessions with libmsdf's terminal atlases. The
agents print glyphs that neither the Noto Sans Mono text atlas nor the
terminal symbol page carries, so they draw as the placeholder box:
- flankr's own subagent dropdown (⊙) and plugin menu button (⋮);
- Codex's and Gemini CLI's status marks and emoji;
- Claude Code's key hints, control pictures and the fallbacks of its
  `figures` table.

After this plan, every codepoint of section 3 draws from the symbol page at
its terminal width, and nothing that draws today changes shape or width.

Out of scope:
- colour emoji;
- a second symbol page;
- any change to the text atlas or the other bundled atlases;
- the one-cell width of the existing ◽ ◾ (open question 3).

## 2. Already covered

The owner asked for "the dingbats/gemini logo". **These are on the page
already**, from the terminal fonts plan's section 4.5, and need nothing:
- Gemini's mark, `✦` U+2726, from Noto Sans Symbols 2. `✧` U+2727 is
  there too.
- Claude Code's spinner dingbats `· ✢ ✳ ✶ ✻ ✽` (with `*`), and its
  Braille spinner. `tests/terminal_glyphs.rs` asserts them, at Claude Code
  2.1.309.

## 3. Findings (measured 2026-10-28)

### 3.1 What is missing

The survey's results, per codepoint, are in flankr's session scratchpad
(`check.txt`). Each codepoint below is in no bundled coverage group.

| # | Group | Codepoints | First printed by |
|---|---|---|---|
| 1 | TERMINAL | interface marks: `⋮` U+22EE, `⋯` U+22EF, `⊷` U+22B7, `⟳` U+27F3, `⊠` U+22A0, `⛁` U+26C1, `ℹ` U+2139, `ⓘ` U+24D8, `≡` U+2261, `⌐` U+2310, `‖` U+2016 | Codex (`⋮ ⓘ ‖`), Antigravity (`⋯ ⟳ ⊠ ⛁`), Gemini CLI (`⊷ ⌐`), Claude Code (`ℹ ≡`) |
| 2 | TERMINAL | key hints: `⌦` U+2326, `⇞` U+21DE, `⇟` U+21DF, `␣` U+2423 | Claude Code |
| 3 | TERMINAL | control pictures: U+2400-241F (32), `␡` U+2421, `␤` U+2424 | Claude Code |
| 4 | TERMINAL | Claude Code's `figures` fallbacks: `☰` U+2630, `⌂` U+2302, `⬢` U+2B22, `♥` U+2665, `♪` U+266A, `☺` U+263A, `Ⓘ` U+24BE, `ⓞ` U+24DE, `ⓧ` U+24E7 | Claude Code |
| 5 | TERMINAL | text presentation: `✋` U+270B, `⏲` U+23F2, `⛔` U+26D4 | Gemini CLI |
| 6 | TERMINAL_EMOJI | `🟢` U+1F7E2, `🟡` U+1F7E1, `🔴` U+1F534, `🔄` U+1F504, `💭` U+1F4AD, `🧠` U+1F9E0, `🛠` U+1F6E0, `🧑` U+1F9D1, `💻` U+1F4BB, `📎` U+1F4CE, `🚧` U+1F6A7, `🔔` U+1F514, `📂` U+1F4C2, `🔐` U+1F510 | Gemini CLI |
| 7 | TERMINAL | the owner's `⊙` U+2299 and `⦿` U+29BF | flankr |

**Totals:**
- TERMINAL: 11 + 4 + 34 + 9 + 3 + 2 = **63** codepoints;
- TERMINAL_EMOJI: **14** codepoints, and one wide form (section 3.4).

### 3.2 Which face supplies each

The merge takes each TERMINAL codepoint from the first face that maps it,
in the order Noto Sans Mono, Noto Sans Symbols 2, Noto Sans Symbols, Noto
Sans Math. Every new codepoint is mapped by one of them:

| Face | Codepoints |
|---|---|
| Noto Sans Mono | `ℹ ≡ ⌐ ‖` (4); also `⊙` first, but see section 4.2 |
| Noto Sans Symbols 2 | the 34 control pictures and `␣`; `⌦ ⛁ ☰ ⬢ ♥ ✋ ⏲ ⛔ ⦿` (44 with `␣`) |
| Noto Sans Symbols | `ⓘ Ⓘ ⓞ ⓧ ⌂ ♪ ☺` (7) |
| Noto Sans Math | `⋮ ⋯ ⊷ ⟳ ⊠ ⇞ ⇟` (7) |

That is 4 + 1 (`⊙`) + 44 + 7 + 7 = 63. Noto Emoji maps all 14 emoji of
row 6.

Noto Sans Mono maps `ℹ ≡ ⌐ ‖`, but the Noto Sans Mono subsets keep only
TEXT, SYMBOLS and MATH, so the text atlas lacks them. They go on the page
from Noto Sans Mono's own outlines, at its 600-unit advance, unscaled.

No new face, no new upstream file, and no licence change are needed.

### 3.3 Widths

The width in cells is what `wcwidth` (0.9.4, Unicode 16 tables) gives,
which is what the agents' own layout code counts:

- **Two cells:**
  - `✋` U+270B and `⛔` U+26D4: East Asian Width W.
  - `☰` U+2630: W since Unicode 16.0, which moved the trigrams U+2630-2637
    to W. Python 3.12's unicodedata (15.1.0) still says N.
- **One cell:** the other 60 TERMINAL codepoints (East Asian Width N or A).
- **Two cells:** 13 of the 14 emoji (W).
- **One cell, two with U+FE0F:** `🛠` U+1F6E0 (East Asian Width N; `wcwidth`
  gives 2 with U+FE0F), the same case as `⚠` and `➡`.

**Narrow symbols that are wide with U+FE0F.** `ℹ ♥ ☺ ⏲` are one cell, and
`wcwidth` gives 2 when U+FE0F follows. The 02:32 EDT amendment of the
terminal fonts plan settled this for TERMINAL: its symbols "stay one cell
whatever selector follows". This plan keeps that rule (open question 4).

**How the page handles width today.**
- A TERMINAL glyph is fitted to one cell (`fitted(font, gname, 1)`).
- A TERMINAL_EMOJI glyph is fitted to two cells, except those in
  `NARROW_EMOJI` (one cell). Each of those also has a two-cell wide form
  at U+F0000 on (`WIDE_FORMS`, `TERMINAL_WIDE_FORM`), which the shaper
  uses when U+FE0F follows.
- The advance encodes the width, and the shaper reads `round(advance /
  600)`.

So no TERMINAL glyph is two cells yet. The page's ◽ U+25FD and ◾ U+25FE
are W but drawn in one cell (open question 3). This plan adds a set of
two-cell TERMINAL codepoints for the three new W ones (section 4.2).

### 3.4 Fit (Rule 2 of the terminal fonts plan)

Each glyph is scaled about its centre to fit its cells less 30 units a side
(540 units in one cell, 1140 in two), and the window less 30. Scale factors
for the new glyphs, at upem 1000:

| Scale | Glyphs |
|---|---|
| 1.00 (unscaled) | `⋮ ℹ ≡ ⌐ ‖ ⇞ ⇟ ♪`; 30 of the 34 control pictures; `✋ ⛔ ☰` in two cells; 12 of the 14 emoji in two cells; `🛠`'s wide form |
| 0.94 to 0.99 | `␂ ␆ ␕ ␚` (ink 0.546 to 0.572 em); `📂` 0.98 (ink 1.163 em) |
| 0.85 to 0.91 | `␣` 0.91, `⌂` 0.88, `♥` 0.85 |
| 0.74 to 0.83 | `⊠` 0.83, `☺` 0.78, `⬢` 0.75, `⋯` 0.74 |
| 0.64 to 0.68 | `⊙ ⦿` 0.68, `⊷` 0.67, `ⓘ Ⓘ ⓞ ⓧ` 0.66, `⛁` 0.64 |
| 0.49 to 0.58 | `⏲` 0.58, `⟳` 0.53, `⌦` 0.49 (as `⌫` today), `🛠` 0.49 in one cell |

Every glyph fits the window vertically at these scales. The tallest is `✋`,
y -116 to 978, inside the window less the margin, -241 to 999.

### 3.5 The empty and filled pair

`⊙` comes first from Noto Sans Mono, and `⦿` only from Noto Sans Symbols 2
(and Math). The two faces draw different circles:
- Noto Sans Mono's `⊙`: a 1200-unit advance, ink x 253 to 947, y 6 to 700.
- Noto Sans Symbols 2's `⊙` and `⦿` share one outline box: a 894-unit
  advance, ink x 51 to 843, y -34 to 758. It is also the box of its `○`
  and `◉`.

Taken by the default order, the pair would differ in stroke weight and
sit 9 units apart vertically. flankr uses them as a pair (the dropdown's
icon and its radio dots).

### 3.6 Capacity

The page holds 591 + 24 + 1 = **616 cells** in one 16-column layer of 640
(40 rows, pinned). This plan adds:
- 63 TERMINAL cells;
- 14 emoji and 1 wide form (U+F0002);

**78 cells in all, for 694. That is 54 more than one 16-column layer
holds.**

`ATLAS_ROWS` explains why the page cannot grow in rows: 40 rows of 50
texels is 2000, under the 2048 floor of `max_texture_dimension_2d`. It
grows in columns, or in layers. The costs, against today's page (pixel
data 800 × 2000 × 3 = 4,800,000 bytes, plus 28,224 bytes of tables, about
46 bytes a cell):

| Option | Cells (free) | Page | Atlas file | GPU, text + page set (RGBA8, 3 layers) |
|---|---|---|---|---|
| Today: 16 columns | 640 (24) | 800 × 2000 | 4,828,224 | 19.2 MB |
| **18 columns** | **720 (26)** | **900 × 2000** | **about 5,432,000 (+604,000)** | **21.6 MB (+2.4 MB)** |
| 20 columns | 800 (106) | 1000 × 2000 | about 6,032,000 (+1,204,000) | 24.0 MB (+4.8 MB) |
| A second 16-column layer | 1280 (586) | 800 × 2000 × 2 | about 9,632,000 (+4,804,000) | 25.6 MB (+6.4 MB) |

Notes on the table:
- Each column adds 40 cells and 50 texels of width, so +300,000 file bytes.
- The GPU column is for an `AtlasSet` holding the text atlas and the page.
  `AtlasSet` makes every layer as wide as its widest atlas, so the text
  atlas's two layers are padded to the page's width too.
- The file is compiled into the binary (`include_bytes!`), so the binary
  grows by the same amount.
- The widest a 2048 texture allows is 40 columns (2000 texels).
- A second layer would also need the builder to put cells of one style on
  two layers, which it does not do today (a layer is one size and style).

## 4. Design

### 4.1 Coverage

`fonts/coverage-groups.txt` and `font/src/coverage.rs` gain the codepoints
of section 3.1, inserted in ascending order (`CVRG` requires it; per-atlas
cell box plan, CHANGES.md 14:13 EDT). Each one is its own line or range.
Adjacent ranges are not merged, as with U+2500-257F and U+2580-259F today.

**TERMINAL gains these lines:**

```text
U+2016  U+2139  U+21DE-21DF  U+2261  U+2299  U+22A0  U+22B7  U+22EE-22EF
U+2302  U+2310  U+2326  U+23F2  U+2400-241F  U+2421  U+2423-2424
U+24BE  U+24D8  U+24DE  U+24E7  U+2630  U+263A  U+2665  U+266A  U+26C1
U+26D4  U+270B  U+27F3  U+29BF  U+2B22
```

That is 63 codepoints, so **`COVERAGE_TERMINAL` grows from 591 to 654**.

**TERMINAL_EMOJI gains these lines:**

```text
U+1F4AD  U+1F4BB  U+1F4C2  U+1F4CE  U+1F504  U+1F510  U+1F514  U+1F534
U+1F6A7  U+1F6E0  U+1F7E1  U+1F7E2  U+1F9D1  U+1F9E0
```

In addition, `U+F0000-F0001` becomes `U+F0000-F0002`. That is 15
codepoints, so **`COVERAGE_TERMINAL_EMOJI` grows from 24 to 39**.

The doc comments of both constants, and the comment above the TERMINAL
lines in `coverage-groups.txt`, name this plan and list what it adds, as
they do for the 13:23 EDT amendment's eight.

### 4.2 The symbol face (`fonts/terminal_symbols.py`)

The merge is unchanged except for these four changes:

1. **Two-cell TERMINAL glyphs.**
   - New constant `WIDE_TERMINAL = {0x2630, 0x26D4, 0x270B}`.
   - A TERMINAL codepoint in it is fitted with `fitted(font, gname, 2)`,
     for a 1200 advance. Every other TERMINAL codepoint keeps one cell.
   - The set is explicit, not computed from unicodedata, so the output
     does not depend on the Python version's Unicode tables (section
     3.3, `☰`).
   - No existing TERMINAL glyph changes width.
2. **One source override.**
   - New constant `SOURCE_OVERRIDE = {0x2299: "symbols2"}`.
   - It is checked before `TERMINAL_ORDER`, so `⊙` comes from Noto Sans
     Symbols 2, the same face and circle as `⦿` (section 3.5).
   - Both are then fitted at 0.68 to the same 540 × 540 ink box.
3. **The third wide form.**
   - `WIDE_FORMS` gains `0xF0002: 0x1F6E0`, and `NARROW_EMOJI` gains
     `0x1F6E0`.
   - So `🛠` is one cell (scaled 0.49), and its wide form two (unscaled).
   - New wide forms are appended in this order, as the 02:32 EDT amendment
     says.
4. **The counts.** The asserts become 654 and 39 (693 mapped
   codepoints).

**Unchanged glyphs.** Every glyph already on the page keeps its outline and
advance. The face's bytes change (new glyphs, glyph order and cmap), and
its sha256 changes. The sha256 is recorded in three places:
- `font/src/bundled.rs` (`TERMINAL_SYMBOLS`);
- `scripts/rebuild-fonts.sh`;
- the face's line in `fonts/bundled.sha256`, if it has one. Today that
  file lists atlases only.

### 4.3 The page: 18 columns

**Decision: the symbol page is baked at 18 columns.** That gives 720 cells,
694 used and 26 free, at 900 × 2000 texels.
- It is the smallest page that fits section 3.6's cells.
- It costs about 604,000 file bytes and 2.4 MB of GPU memory for a
  text-and-page set (section 3.6). Open question 2 is 20 columns.

The changes:
- `font/src/bundled.rs`: `BundledAtlas::columns` returns 18 for
  `TerminalSymbols`, from a new private constant
  `TERMINAL_SYMBOLS_COLUMNS: u32 = 18`, and 16 for the other OFL faces as
  before. Its doc comment says why.
- `libmsdf-bake bundled` already takes the columns from
  `BundledAtlas::columns` (`opts.columns = atlas.columns()`), so the bake
  needs no other change.
- `src/drawlist/atlas_set.rs`'s module comment ("400 x 2000, or 800 x 2000
  for 16 columns") adds 900 × 2000 for the terminal symbol page. It also
  says that a set of the terminal atlases pads the text atlas to 900.
- No format change and no shader change. The width is in the atlas
  header, and every glyph entry holds texel coordinates.

**What moves.** Cells are numbered in codepoint order, so most existing
glyphs move to new cells. That is internal: a glyph is found through the
glyph table, not by cell number. Every other bundled atlas is
byte-identical.

### 4.4 The shaper

`font/src/terminal.rs`: `TERMINAL_WIDE_FORM` gains `('\u{1F6E0}',
'\u{F0002}')`, matching the script's `WIDE_FORMS`. Nothing else changes.
- The two-cell TERMINAL glyphs need no code: the shaper already takes a
  glyph's width from its advance.
- The new codepoints are not in the text atlas, so the shaper's source
  choice ("the text atlas if its Cmap has it") sends them all to the page.

### 4.5 README

- **"Bundled fonts":** the emoji sentence says `⚠`, `➡` and `🛠` are one
  cell and two with U+FE0F.
- **"Bundled fonts":** a sentence says that `☰ ⛔ ✋` on the symbol page
  are two cells.
- **"Bundled fonts":** the symbol page is 900 × 2000.
- **The decision table** gains this plan's row.

## 5. Tests

**CPU, no bake** (`tests/bundled_fonts.rs`, `font/src/coverage.rs`):
- **The counts test:** `COVERAGE_TERMINAL` is 654 and
  `COVERAGE_TERMINAL_EMOJI` 39, and `coverage-groups.txt` agrees with the
  Rust constants.
- **`the_face_maps_exactly_the_terminal_groups`:** 693 mapped codepoints,
  equal to the two groups.
- **`every_glyph_sits_on_its_cells`:** `wide()` becomes:
  - the emoji, less `⚠ ➡ 🛠`;
  - the wide forms;
  - `WIDE_TERMINAL`'s `☰ ⛔ ✋`.

  Each glyph's advance is 1200 for those and 600 otherwise, and the
  existing bounds checks apply to every glyph.
- **New, `the_circled_pair_shares_a_circle`:** `⊙` and `⦿` in the merged
  face have equal bounding boxes.
- **`the_atlas_is_one_full_page`:**
  - 900 × 2000;
  - one layer;
  - 694 glyphs ("654 symbols, 39 emoji and the placeholder");
  - `CVRG` groups 3 and 4.
- **`terminal_shaper.rs`, `a_line_draws_each_segment_from_its_atlas`:**
  the extent becomes `[900, 2000]`, and the message becomes "the set pads
  the text atlas to the page's width". It asserted that the two atlases
  are the same width (terminal fonts plan, section 4.7), which this plan
  ends.

**New, in `tests/terminal_glyphs.rs`:**
- A list `CODING_AGENTS_2026_10_28` of the 77 codepoints of section 3.1,
  each with its width in cells. Its doc comment names the four agents and
  their versions (Claude Code 2.1.312, Codex 0.162.1, Gemini CLI 0.38.2,
  Antigravity 1.4.7), and which agent printed each row.
- `the_list_is_the_survey_s_seventy_seven`: 77 entries, no duplicates.
- `every_coding_agent_glyph_draws_from_the_page`: each codepoint, alone,
  shapes through `TerminalShaper` as one `Symbols` segment, with a glyph
  that is not the placeholder, and takes its listed columns.
- `selectors_and_widths`:
  - `"🛠x"` and `"🛠\u{FE0E}x"` put `x` at column 1;
  - `"🛠\u{FE0F}x"` puts `x` at column 2;
  - `"ℹ\u{FE0F}x"` and `"♥\u{FE0F}x"` put `x` at column 1 (section 3.3);
  - `"✋x"`, `"⛔x"` and `"☰x"` put `x` at column 2;
  - `"⊙ ⦿ ⋮"` takes 5 columns.
- The existing Claude Code 2.1.309 tests are unchanged and still pass.

**With the bake** (`tests/bundled_bake.rs`, `libmsdf-bake bundled
--check`):
- The page's report lists 654 of 654 and 39 of 39 carried, none skipped.
- Its "ink past the cell window" lists only the box and block glyphs it
  lists today, by codepoint. The glyph numbers change.
- The eight other atlases are byte-identical.

## 6. Versioning (Rule 5)

**Proposed: 0.6.1**, for `libmsdf` and `libmsdf-font` in lockstep. This is
the owner's "0.6.1 (additive coverage)". The reasons:
- No type, signature or field changes. No variant is added to an exhaustive
  enum (`CoverageGroupId` is `#[non_exhaustive]` and gains nothing).
- Every codepoint that draws today draws the same outline at the same width
  in the same column.
- Only codepoints that drew the placeholder now draw a glyph.

What a caller can see change, listed in `VERSIONS.md` as **Changed**:
- the values of `COVERAGE_TERMINAL`, `COVERAGE_TERMINAL_EMOJI` and
  `TERMINAL_WIDE_FORM`;
- the bytes of `TERMINAL_SYMBOLS_TTF` and `TERMINAL_SYMBOLS_ATLAS`;
- the page's size (900 × 2000) and `BundledAtlas::TerminalSymbols.columns()`
  (18);
- the extent of an `AtlasSet` that holds the page.

**This is a judgement the owner should confirm (open question 1).** The
0.6.0 entry marked its coverage additions **Breaking**, together with the
changed atlas bytes. Rule 5.4 also counts "behaviour a caller can observe
changing", and a caller can read the page's width.

If the owner reads it that way:
- the release is 0.7.0;
- libcontrols, which takes libmsdf, then releases 0.8.0 under Rule 5.5.

Under 0.6.1, libcontrols takes the patch with no release of its own.

## 7. Steps

1. **Coverage.**
   - Section 4.1 in `fonts/coverage-groups.txt` and
     `font/src/coverage.rs`, with the counts test at 654 and 39.
   - `TERMINAL_WIDE_FORM` gains the `🛠` pair (section 4.4).

   **Done when:**
   - `cargo test -p libmsdf-font` passes, with the counts test at 654 and
     39 and the file matching the constants;
   - `cargo fmt -p libmsdf-font -- --check` and `cargo clippy -p
     libmsdf-font --all-targets` are clean.
2. **The symbol face.**
   - Section 4.2 in `fonts/terminal_symbols.py`.
   - Regenerate `fonts/LibmsdfTerminalSymbols-Regular.ttf` with the
     script's documented command.
   - Update the face's sha256 in `font/src/bundled.rs` and
     `scripts/rebuild-fonts.sh`.
   - The CPU tests of section 5 that read the face.

   This is cheap: one fontTools run, no atlas bake.

   **Done when:**
   - the script reports 693 codepoints, with sources Noto Sans Mono 4,
     Symbols 2 45, Symbols 7, Math 7 for the additions (with `⊙` under
     Symbols 2);
   - running it twice gives the same sha256;
   - `scripts/rebuild-fonts.sh` (without `--atlases`) reproduces the
     committed face;
   - the face tests pass.
3. **The page atlas.**
   - `TERMINAL_SYMBOLS_COLUMNS` and `BundledAtlas::columns` (section 4.3).
   - Re-bake with `cargo run -p libmsdf-font --features bake --bin
     libmsdf-bake -- bundled --out fonts/atlases --hashes` (AGENTS.md,
     "Bundled fonts and atlases", item 1). It re-bakes all nine atlases,
     which costs CPU time but no extra disk beyond the existing `target/`.
     Never `cargo clean`.
   - Check with `-- bundled --check fonts/atlases`.
   - Update the atlas-set comment, `the_atlas_is_one_full_page` and the
     shaper's extent test.

   **Done when:**
   - `--check` passes;
   - only `terminal-symbols-48.atlas` and its report change bytes;
   - the report reads as in section 5;
   - `cargo test --features fonts-apache-roboto,fonts-ofl-ledger,fonts-ofl-terminal
     --test bundled_fonts` fails only on the page's reference hash.

   The new hash goes into `fonts/bundled.sha256` in its own commit, after
   the owner agrees to the byte change (AGENTS.md, item 2). After that, the
   same test passes.
4. **The glyph tests.**
   - The new tests of section 5 in `tests/terminal_glyphs.rs`.

   **Done when:** `cargo test --features fonts-ofl-terminal --test
   terminal_glyphs` passes, and every codepoint of section 3.1 is asserted.
5. **Evidence.**
   - **The sheet.** If `examples/font_sheets.rs` exists by then (font sheet
     examples plan, still Draft), run `cargo run --release --example
     font_sheets -- glyphs <list> --size 24 --scale 2 --name agent_glyphs`
     with the 77 codepoints, and `-- terminal --lines <file>` with one line
     per agent of what it prints.
   - Otherwise copy the scratch tool of
     `out/fonts/20261023_04_cell_box_release/tool/` into the new run's
     `tool/`, and draw the same two panels with it.
   - The output goes to the next free `out/fonts/YYYYMMDD_NN_agent_glyphs/`,
     with an entry in `out/fonts/LOG.md`.
   - A dated section in `docs/FONT_TERMINAL_SYMBOLS.md` (Rule 3.6) shows
     the sheet. It says where each glyph came from, and lists the scale
     factors of section 3.4.

   **Done when:**
   - the sheet shows every new glyph whole and not the placeholder, with
     `⊙ ⦿` matched;
   - the two-cell glyphs take two columns in the agent lines;
   - the FONT section and the log entry are written.
6. **Release 0.6.1.**
   - Both crates' `version` at 0.6.1, and the `VERSIONS.md` entry
     (section 6).
   - The README changes (section 4.5).
   - Tag `v0.6.1` on the commit that changes the version.
   - A CHANGES.md landing addendum, and this plan's amendment marking it
     Implemented with the commits of each step.
   - The release waits for the owner's approval, as 0.6.0 did.

   **Done when:**
   - fmt, clippy (no new warnings), the full test suite with `gpu-tests`
     and `scripts/check-font-features.sh` all pass at the tagged commit;
   - the tag is on the version commit.

## 8. Open questions

1. **0.6.1 or 0.7.0?** Section 6 proposes 0.6.1, as asked. The 0.6.0
   precedent and Rule 5.4's "observable behaviour" (the page's width,
   `columns()`, a set's extent) could make this 0.7.0, and then libcontrols
   0.8.0. Which?
2. **18 or 20 columns?** 18 leaves 26 free cells. That is enough for
   another round like the 13:23 EDT amendment's nine, but not for one like
   this. 20 leaves 106 free, for about 600,000 more file bytes and 2.4 MB
   more GPU memory per terminal set. Each change of width moves every cell
   again, so going to 20 now saves one later re-layout. Which?
3. **◽ U+25FD and ◾ U+25FE** are W (two columns in a terminal) but drawn in
   one cell today. Making them two cells changes glyphs that draw now, so
   it is not in this release. Should it be done in the next breaking one?
4. **`ℹ ♥ ☺ ⏲` with U+FE0F** stay one cell, by the 02:32 EDT rule for
   TERMINAL symbols, where `wcwidth` says two. Wide forms for them would
   cost 4 more cells. Gemini CLI prints `⏲` without a selector, and Claude
   Code's `figures` table prints `ℹ ♥ ☺` without one. Keep one cell?
5. **`☰` under older Unicode tables.** It is two cells by Unicode 16
   (`wcwidth` 0.9.4, current `string-width`). A terminal with pre-16 tables
   draws it in one column, so a line would drift by a column there. Two
   cells, as proposed?
6. **`⊙` from Noto Sans Symbols 2** (section 4.2, item 2) is the only
   source override on the page. It is chosen so the pair matches. The
   alternative keeps the face order, and Noto Sans Mono's `⊙` then differs
   from `⦿` in weight. Keep the override?
