# Terminal symbols

Render notes for the bundled terminal symbol page
(`font-ofl-noto-terminal-symbols`, `fonts/atlases/terminal-symbols-48.atlas`,
one 16-column layer, from `fonts/LibmsdfTerminalSymbols-Regular.ttf`, which
`fonts/terminal_symbols.py` merges from Noto Sans Mono, Noto Sans Symbols,
Noto Sans Symbols 2, Noto Sans Math and Noto Emoji onto Noto Sans Mono's
cell), the symbol half of the `fonts-ofl-terminal` preset. Every image is
drawn from the baked atlas with libmsdf's own GPU renderer, through
`TerminalShaper` and `DrawList::push_terminal_line`, so it shows what libmsdf
draws.

The images are generated evidence under `out/fonts/` (gitignored).
Entries are appended under dated headings and never edited.

## 2026-10-23 13:17 EDT — The whole page (`out/fonts/20261023_03_terminal_samples/`)

The atlas as committed at `a152858` (libmsdf 0.5.0's, unchanged since).
Every codepoint the face maps that Noto Sans Mono does not, 606 of them,
grouped by block and drawn at 24 px, scale 2, with a space between glyphs
(so the box-drawing joins are not shown here; the mock-up below shows
them):

| Block | Glyphs |
|---|---|
| Box drawing U+2500–257F | 128 (all) |
| Block elements U+2580–259F | 32 (all) |
| Braille patterns U+2800–28FF | 256 (all) |
| Geometric shapes U+25A0–25FF | 96 (all) |
| Arrows U+2190–21FF | 25 |
| Miscellaneous technical U+2300–23FF | 17 |
| Miscellaneous symbols U+2600–26FF | 7 |
| Dingbats U+2700–27BF | 27 |
| Everything else, emoji included | 16, and the two wide forms |

The last row also shows `⚠` and `➡` alone (one cell) and followed by VS16
(their two-cell wide forms, U+F0000 and U+F0001).

![Every glyph on the terminal symbol page, by block, at 24 px, scale 2](./out/fonts/20261023_03_terminal_samples/terminal-symbols_page.png)

On one cell grid with Noto Sans Mono, at 16 px scale 2 and at 48 px: box
drawing joins across cells and lines, the Braille spinner frame, the
eighth-block progress bar, shades, the curated emoji (`🤖 🎉 ✅`) and the
wide forms all keep to the 41-column grid:

![A terminal mock-up at 16 px, scale 2](./out/fonts/20261023_03_terminal_samples/terminal_mock_16px.png)

![The same mock-up at 48 px](./out/fonts/20261023_03_terminal_samples/terminal_mock_48px.png)

The bake report's 135 glyphs with "ink past the cell window" are the box
drawing and block elements, which `terminal_symbols.py` remaps with a
100-unit vertical overhang so that the cell edge cuts them and neighbouring
cells meet with no seam (terminal fonts plan, section 4.4). They are
intended.

**Found:** `✨` (U+2728) is not on the page. The terminal shaper falls back
to Noto Sans Mono's placeholder box, one cell wide where an emoji would be
two, so a line containing it is one column short; the first mock-up drawn
for this entry had exactly that. Noto Emoji (one of the face's sources) has
it; it is not in the curated `COVERAGE_TERMINAL_EMOJI` list. Adding it is a
coverage change, not proposed here.

The spec, the renderer (a scratch crate against libmsdf) and the sheet
script are in
[out/fonts/20261023_03_terminal_samples/tool/](./out/fonts/20261023_03_terminal_samples/tool/).

## 2026-10-23 14:12 EDT — Claude Code's glyphs, and ✨ (`out/fonts/20261023_04_cell_box_release/`)

Correcting the 13:17 entry's finding: `✨` U+2728 is on the page now, two
cells wide, from Noto Emoji. The page also gains the eight glyphs of Claude
Code 2.1.309's interface that no terminal atlas carried: `※ ↯ ∙ ∴ ∵ ∷ ⌕ ⑂`
(the thinking animation is `∴ ∷ ∵ ∷`). `fonts/atlases/terminal-symbols-48.atlas`
(libmsdf `9c47e68`) has 616 cells, 591 symbols and 24 emoji; the bake report's
135 overhanging box and block glyphs are as before.

Claude Code's "starburst" spinner is not Braille: its frames are `· ✢ ✳ ✶ ✻ ✽`
and back, with `*` for `✳` on some terminals, Dingbats asterisks that were
already on the page. Its other spinner is Braille, `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`.
`tests/terminal_glyphs.rs` holds all 70 interface glyphs to drawing from
the two terminal atlases with no placeholder, and `tests/cell_box_render.rs`
draws both spinners' frames and checks the text after them never moves.

![Claude Code's 70 interface glyphs, then the starburst, Braille and thinking spinners and the sparkle, at 24 px, scale 2](./out/fonts/20261023_04_cell_box_release/claude-code_glyphs.png)

## 2026-10-29 02:19 EDT — The coding agents' glyphs (`out/fonts/20261029_01_agent_glyphs/`)

The page gains the 77 codepoints of the coding-agent glyphs plan
(`docs/design/20261028_01_CODING_AGENT_GLYPHS_PLAN.md`, section 3.1),
and is now one 18-column layer: 63 symbols (`COVERAGE_TERMINAL`, now 654) and 14 emoji with
one wide form (`COVERAGE_TERMINAL_EMOJI`, now 39). The face
(sha256 `0d912327…`) maps 693 codepoints; every glyph that was on the page
keeps its outline and advance, checked glyph by glyph against the previous
face. The atlas is 900 x 2000 (18 columns, computed from its 694 cells) and
5,432,000 bytes; its bake report carries 654 of 654 and 39 of 39, and its
135 overhanging glyphs are the same box and block codepoints, by the same
amounts, as before.

**Where each glyph came from.** By the merge's face order, with one
override:
- Noto Sans Mono: `ℹ ≡ ⌐ ‖` (4). Its `ℹ` is drawn as a serifed
  letter i, which is Noto's design, not a fallback.
- Noto Sans Symbols 2: the 34 control pictures and `␣`, and
  `⌦ ⛁ ☰ ⬢ ♥ ✋ ⏲ ⛔ ⦿`, and `⊙` (45). `⊙` is the page's one source
  override: Noto Sans Mono maps it first, with a different circle, so it
  is taken from Noto Sans Symbols 2, the face of `⦿`. The two have equal
  bounding boxes, x 30 to 570 and y 92 to 632.
- Noto Sans Symbols: `ⓘ Ⓘ ⓞ ⓧ ⌂ ♪ ☺` (7).
- Noto Sans Math: `⋮ ⋯ ⊷ ⟳ ⊠ ⇞ ⇟` (7).
- Noto Emoji: the 14 emoji and the wide form of `🛠` at U+F0002.

**Widths.** `☰ ⛔ ✋` are two cells (1200 units), the first two-cell
symbols on the page; every other new symbol is one. Of the emoji, 13 are
two cells; `🛠` is one, and its wide form, which the shaper uses when
U+FE0F follows, is two. `ℹ ♥ ☺ ⏲` stay one cell with U+FE0F.

**Scale factors** (Rule 2 fitting, plan section 3.4), as measured for the
plan; the merged face's ink matches them:

| Scale | Glyphs |
|---|---|
| 1.00 (unscaled) | `⋮ ℹ ≡ ⌐ ‖ ⇞ ⇟ ♪`; 30 of the 34 control pictures; `✋ ⛔ ☰` in two cells; 12 of the 14 emoji in two cells; `🛠`'s wide form |
| 0.94 to 0.99 | `␂ ␆ ␕ ␚`; `📂` 0.98 |
| 0.85 to 0.91 | `␣` 0.91, `⌂` 0.88, `♥` 0.85 |
| 0.74 to 0.83 | `⊠` 0.83, `☺` 0.78, `⬢` 0.75, `⋯` 0.74 |
| 0.64 to 0.68 | `⊙ ⦿` 0.68, `⊷` 0.67, `ⓘ Ⓘ ⓞ ⓧ` 0.66, `⛁` 0.64 |
| 0.49 to 0.58 | `⏲` 0.58, `⟳` 0.53, `⌦` 0.49, `🛠` 0.49 in one cell |

![The 77 coding-agent glyphs by row, at 24 px, scale 2: interface marks, key hints, the control pictures, Claude Code's figures fallbacks, the text-presentation symbols, Gemini CLI's emoji, and the empty and filled circles](./out/fonts/20261029_01_agent_glyphs/agent_glyphs.png)

![One mock-up line per agent at 24 px between column rulers, every line 53 columns with its bar at column 52](./out/fonts/20261029_01_agent_glyphs/agent_lines_24px.png)

![The same lines at 16 px, scale 2](./out/fonts/20261029_01_agent_glyphs/agent_lines_16px.png)

`tests/terminal_glyphs.rs` holds all 77 to drawing from the page at their
widths, with the selector cases; the log entry is in
`out/fonts/LOG.md`, and the tool in
[out/fonts/20261029_01_agent_glyphs/tool/](./out/fonts/20261029_01_agent_glyphs/tool/).
