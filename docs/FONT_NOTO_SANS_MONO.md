# Noto Sans Mono

Render notes for the bundled terminal text face (`font-ofl-noto-sans-mono`,
`fonts/atlases/noto-sans-mono-48.atlas`, Regular and Bold as two layers,
from `fonts/NotoSansMono-Regular-subset.ttf` and
`fonts/NotoSansMono-Bold-subset.ttf`), the text half of the
`fonts-ofl-terminal` preset. Every image is drawn from the baked atlas with
libmsdf's own GPU renderer, through `TerminalShaper` and
`DrawList::push_terminal_line` as a terminal would draw it, so it shows what
libmsdf draws, not what the TTF looks like.

The images are generated evidence under `out/fonts/` (gitignored).
Entries are appended under dated headings and never edited.

## 2026-10-23 13:17 EDT — Samples (`out/fonts/20261023_03_terminal_samples/`)

The atlas as committed at `a152858` (libmsdf 0.5.0's atlas, unchanged since),
cap placed at 0.24 of the cell (`CapPlacement::TERMINAL_TOP_FRAC`). The bake
report lists no ink past the cell for either style: the accented capitals,
`ĥ ĺ` and the descenders fit. Cell advance 0.6 em, line pitch 1.3 em.

The top panel is the face's own outlines (FreeType) on the same baselines;
the panel under it is the atlas. Regular, then Bold, with the line box in
red and the baseline in blue:

![Noto Sans Mono at 56 px: the outlines, then the atlas through the terminal shaper](./out/fonts/20261023_03_terminal_samples/noto-sans-mono_56.png)

At a phone size, 14 px at scale 2:

![Noto Sans Mono at 14 px, scale 2](./out/fonts/20261023_03_terminal_samples/noto-sans-mono_14px.png)

With the symbol page on one cell grid (docs/FONT_TERMINAL_SYMBOLS.md), at
16 px scale 2 and at 48 px:

![A terminal mock-up at 16 px, scale 2](./out/fonts/20261023_03_terminal_samples/terminal_mock_16px.png)

![The same mock-up at 48 px](./out/fonts/20261023_03_terminal_samples/terminal_mock_48px.png)

Every line of the mock-up is 41 columns by the shaper's own count
([frames.jsonl](./out/fonts/20261023_03_terminal_samples/frames.jsonl)), so
the right-hand border stands in one column across text, symbols and two-cell
emoji.

**Found:** `ș` and `ț` (U+0219, U+021B, Romanian) draw as the placeholder
box, as they do from the subset TTF itself. The upstream Noto Sans Mono has
them; the shared TEXT coverage group (`fonts/coverage-groups.txt`,
`COVERAGE_TEXT`) does not include U+0218–021B, so no bundled atlas carries
them. Adding them is a coverage change for every bundled face, not proposed
here.

The spec, the renderer (a scratch crate against libmsdf) and the sheet
script are in
[out/fonts/20261023_03_terminal_samples/tool/](./out/fonts/20261023_03_terminal_samples/tool/).

## 2026-10-23 14:12 EDT — Ș ș Ț ț, and Claude Code's interface (`out/fonts/20261023_04_cell_box_release/`)

Correcting the 13:17 entry's finding: the TEXT coverage group now includes
`U+0218-021B`, and `fonts/atlases/noto-sans-mono-48.atlas` (libmsdf
`9c47e68`) carries `Ș ș Ț ț` in Regular and Bold
(`tests/terminal_glyphs.rs`). The atlas keeps the line box and format
5:0.1.0. With the symbol page, it draws every glyph of Claude Code
2.1.309's interface, shown in a session mock-up at 16 px, scale 2, and at
24 px:

![A Claude Code session at 16 px, scale 2](./out/fonts/20261023_04_cell_box_release/claude-code_session_16px.png)

![The same at 24 px](./out/fonts/20261023_04_cell_box_release/claude-code_session_24px.png)
