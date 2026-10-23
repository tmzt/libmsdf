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
