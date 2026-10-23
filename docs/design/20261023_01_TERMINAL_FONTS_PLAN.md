---
title: A terminal face and a symbol page, from Noto
date: 2026-10-23
status: Draft
covers: >
  A bundled monospace face for terminal-style text, Noto Sans Mono (SIL OFL
  1.1) Regular and Bold, baked as one two-layer atlas; a symbol page, one
  16-column atlas layer of its own, holding the box drawing, block elements,
  Braille patterns and geometric shapes in full, plus curated arrows,
  technical symbols, dingbats and monochrome emoji that coding-agent
  terminals print, merged from Noto faces into one face set on Noto Sans
  Mono's cell; a per-bake cap placement so that the face's accented capitals
  and the box-drawing glyphs fit the cell window; a `TerminalShaper` that
  lays a line out on a fixed cell grid across the two atlases; a drawing
  helper; a one-feature preset; the tests that prove coverage, joins and
  alignment; and the 0.5.0 release this needs.
sources:
  - >
    Owner (Tim), 2026-10-23, relayed by another session: "roboto mono with a
    full atlas page for the common symbols Claude prefers (such as the box
    or spinners)". Then, in order: for the fallback face, "probably Noto";
    "no cjk and mono emoji where useful (the robot, etc)"; and Noto Sans
    Mono for all terminal text in place of Roboto Mono (section 2).
  - >
    The request that came with it: Roboto Mono (now Noto Sans Mono) Regular
    and Bold with TEXT coverage as for Roboto; a full page for U+2500-257F,
    U+2580-259F, U+2800-28FF, U+25A0-25FF, U+2190-21FF, U+2300-23FF,
    U+2700-27BF and the bullets and ellipsis; a SYMBOLS-style group that
    skips and reports a missing codepoint; a "terminal" preset; a bake test
    for coverage; a render check of box-drawing joins and Braille alignment
    at phone sizes.
  - >
    font/src/atlas.rs (`glyph_projection`, `CAP_TOP_FRAC` 0.15,
    `CELL_EM_RATIO` 1.3, `ATLAS_COLS`, `ATLAS_ROWS`, `with_columns`,
    `add_styled_coverage`, `add_styled_face`, `SetMetrics::baseline_frac`),
    font/src/bake.rs (`BakeOptions`), font/src/bundled.rs (`BundledFace`,
    `BundledAtlas`, which is exhaustive), font/src/coverage.rs
    (`CoverageGroupId`, `#[non_exhaustive]`), font/src/atlas_shaper.rs
    (`AtlasShaper`, `is_default_ignorable`), font/src/shaper.rs
    (`ShapedRun`, `ShapedGlyph`), src/drawlist/mod.rs
    (`push_shaped_text_with`), src/gpu/sdf_render.wgsl (the line box mapped
    to the cell; per-glyph `baseline_row`), fonts/coverage-groups.txt,
    scripts/fetch-upstream-fonts.sh, Cargo.toml (the font features),
    fonts/atlases/ (a 16-column atlas is 4,830,216 bytes).
  - >
    Measurements made for this plan on 2026-10-23 with fontTools 4.60.1 and
    Python's unicodedata (Unicode 16.0.0), of the files in section 3.1
    (section 3).
related:
  - docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md (the bundled-face
    scheme, coverage groups, `CVRG`, the bake report and the OFL subsets
    this follows)
  - docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md (`AtlasShaper` and
    the layout tables the terminal shaper uses)
  - docs/design/20261021_03_ATLAS_SET_PLAN.md (`AtlasSet` and `AtlasRef`,
    which put the two atlases in one draw list)
---

# A terminal face and a symbol page, from Noto

## 1. Goal

A viewer that shows coding-agent sessions in a terminal style needs crisp
monospace text and the symbols those agents print: box-drawing frames,
block bars, Braille spinners, bullets, check marks, arrows and a few emoji.
libmsdf bundles no monospace face, and none of its faces covers those
blocks.

This plan adds:
1. **The terminal face**: Noto Sans Mono Regular and Bold, with the TEXT,
   SYMBOLS and MATH coverage the bundled OFL faces have;
2. **the symbol page**: one atlas layer for the terminal symbols, drawn on
   the same cell grid as the text, so box-drawing lines join and spinners
   do not jitter;
3. **a shaper and a draw call** that lay a line out on that grid, taking
   each character from the text atlas or from the symbol page;
4. **one preset feature** that brings all of it.

Out of scope: CJK (owner's decision, section 2), colour emoji, and baking
on demand (open question 4).

## 2. Decisions made by the owner (2026-10-23)

Relayed by another session, in this order:

1. "roboto mono with a full atlas page for the common symbols Claude
   prefers (such as the box or spinners)".
2. For the face that fills in what the text face lacks: "probably Noto":
   Noto Sans Mono for box drawing and block elements, Noto Sans Symbols 2
   for Braille, technical symbols and dingbats. Recorded then as a
   preference, with the metric check to be made by this plan.
3. "no cjk and mono emoji where useful (the robot, etc)": no CJK; no colour
   emoji and no full emoji range; monochrome outlines of a curated set from
   Noto Emoji, the black-and-white face, in the text colour, with the
   variation selectors U+FE0E and U+FE0F mapping to the same glyph.
4. **Noto Sans Mono for all terminal text**, replacing Roboto Mono. Roboto
   Mono is dropped from this plan, and the existing Roboto features are
   unaffected. The symbol page takes box drawing and block elements from
   Noto Sans Mono itself, Braille, technical symbols and dingbats from Noto
   Sans Symbols 2, and the emoji from Noto Emoji. The metric question
   remains only for the Symbols 2 and Emoji glyphs.

Section 3.6 finds that Noto Sans Symbols 2 lacks some of the requested
technical symbols and arrows. This plan fills them from two more Noto faces,
Noto Sans Symbols and Noto Sans Math. Both are OFL and from the same
project. That is a proposal for the owner, not one of the decisions above.

## 3. Findings (measured 2026-10-23)

### 3.1 Sources

Everything comes from google/fonts at commit
`e7db6ca603fd4862010383ff17ffd6cc4ffac1c0`. Each file's METADATA.pb says
`license: "OFL"`, and each OFL.txt names no Reserved Font Name.

| File | sha256 | Upstream |
|---|---|---|
| `ofl/notosansmono/NotoSansMono[wdth,wght].ttf` | `2cb2adb378a8f574213e23df697050b83c54c27df465a2015552740b2769a081` | notofonts/latin-greek-cyrillic `9b7310b8f99fcd2583c49606e6aefca13a391350` |
| `ofl/notosanssymbols2/NotoSansSymbols2-Regular.ttf` | `7d5fb73b7ca67a6798101741f5d280a3d016a56a197afcd4199dbb57b4b82a21` | notofonts/symbols `25b00f0d3f40873a005287514ba2a48558655314` |
| `ofl/notosanssymbols/NotoSansSymbols[wght].ttf` | `f7e7e04b4a24b6c78893d50cbfd2b2f6cae49617ab047bfef668d252adb128f7` | notofonts/symbols |
| `ofl/notosansmath/NotoSansMath-Regular.ttf` | `3f495fe933c06786e4d5f6d86b8ee70b6753a68ee3b9d87528726de0f6e2c47d` | notofonts/math `4b78263eac2301b04367d600a07b521cfbff0867` |
| `ofl/notoemoji/NotoEmoji[wght].ttf` | `de6c18832938afc99caf132b39d6a30a19bac7f2e812e28db2535b4608d27551` | (no repository named in METADATA.pb) |

**Roboto Mono, for the record.** google/fonts now ships Roboto Mono under
`ofl/robotomono/` with `license: "OFL"`, although its upstream repository's
LICENSE says Apache-2.0. A `font-apache-roboto-mono` feature would have had
to settle which licence applies first. The owner's switch to Noto Sans Mono
makes that moot.

### 3.2 Noto Sans Mono

- **Axes:** `wght` 100-900 (default 400) and `wdth` 62.5-100 (default
  100). The bake uses `wdth` 100 with `wght` 400 for Regular and 700 for
  Bold.
- **Metrics:** upem 1000. Every TEXT glyph has a 600-unit advance, so the
  cell is exactly 0.6 em. `hhea` and typo ascent and descent are 1069 and
  -293 (1.362 em); win ascent and descent are 1229 and 389. Cap height
  (`A`'s top) is 717 in both weights.
- **Coverage of the existing groups**, the same at both weights:
  - TEXT: 286 of 287. It lacks U+00AD soft hyphen.
  - SYMBOLS: 63 of 63.
  - MATH: 73 of 77. It lacks U+220F, U+2211, U+2213 and U+222B.
- **GSUB features:** `aalt c2sc ccmp dnom frac locl numr onum ordn rtlm
  smcp ss03 subs sups zero`. Only `ccmp`, `locl` and `rtlm` are applied by
  default. The equivalence check of the atlas layout tables plan decides
  whether they can be expressed, at step 3.
- **Ink extent of TEXT:**
  - Regular: -245 (`g`) to 995 (`Å`);
  - Bold: -245 to 1004 (`Å`).

### 3.3 The cell window clips the accented capitals

`glyph_projection` maps a 1.3 em square onto each cell, and puts the cap
height at `CAP_TOP_FRAC` (0.15) of the cell from its top. The drawn line box
is that same 1.3 em (`LINE_BOX_RATIO`). For a face with a cap height of 717,
the cell holds y = -388 to 912 font units.

**166 of Noto Sans Mono's TEXT glyphs reach above 912**, at both weights:
the accented capitals À-Ý and Ă-Ŵ (944 for an acute, 995 to 1004 for a
ring), and `ĥ`/`ĺ` at 990. A bake at the default placement would cut the
tops off their accents by up to 0.09 em, about 1.4 px at 16 px.

The window has room, since the TEXT ink spans 1.249 em of the 1.3 em cell.
It is only placed for a face whose accents sit lower. Section 4.2 makes the
placement a bake option.

### 3.4 Box drawing joins at 1.213 em, not at the face's line height

In Noto Sans Mono:
- `█` spans y -240 to 973, so the block design pitch is 1.213 em.
- `│` and `┼` span -246 to 979, overlapping that pitch by 6 units at each
  end.
- `─` spans x -6 to 606 on its 600 advance.

So the glyphs join when lines are 1.213 em apart. That is neither the face's
own line height (1.362 em) nor libmsdf's line box (1.3 em). Nor do they fit
the cell window: 973 is above 912. Sharing the face fixes the horizontal
grid, but not the vertical one. Section 4.4 remaps the box and block glyphs
onto the line box.

### 3.5 Widths

- **Braille** (Noto Sans Symbols 2) has a 700-unit advance. Its ink spans
  x 152 to 548 (0.396 em) and y -191 to 699 across all 256 patterns. It fits
  one 0.6 em cell once it is centred.
- **40 geometric shapes in Noto Sans Mono** have a 1200-unit (two-cell)
  advance: ▢-▩, ▭, ▰, ▱, △, ▶, ▷, ▻, ▽, ◀, ◁, ◅-◈, ◍, ◢-◥, ◧-◳ and ◯. Their
  East Asian Width is N or A, which is one cell in a non-CJK terminal.
  Their ink is 0.585 to 1.0 em wide.
- **Borrowed technical symbols and dingbats** are wider than one cell. For
  example:
  - `✔` ink 0.828 em, `⌘` 0.712, `⌫` 1.095;
  - `➜` 0.907, `❖` 1.006, `☐` 0.719.
  Fitting them into one cell less a 0.03 em margin each side scales them by
  0.49 (`⌫`) to 0.97 (`⏶`); the script computes each one.
- **Noto Emoji** glyphs have a 2600-unit advance at upem 2048 (1.27 em).
  Their ink is y -340 to 1740 (-0.166 to 0.850 em) and at most 2204 units
  (1.076 em) wide, except `🧪` at 2424 (1.18 em). So every curated emoji
  but `🧪` fits two cells less margin (1.14 em) without scaling, and `🧪`
  is scaled by 0.96. `⚠` U+26A0 and `➡` U+27A1 have East Asian Width N (one
  cell); the other 19 curated emoji are W (two cells).

### 3.6 Who covers the requested blocks

Taking each codepoint from the first face in the order Noto Sans Mono,
Noto Sans Symbols 2, Noto Sans Symbols, Noto Sans Math:

| Block | Covered | From |
|---|---|---|
| Box drawing U+2500-257F | 128/128 | Mono 128 |
| Block elements U+2580-259F | 32/32 | Mono 32 |
| Braille U+2800-28FF | 256/256 | Symbols 2 256 |
| Geometric shapes U+25A0-25FF | 96/96 | Mono 96 |
| Arrows U+2190-21FF | 112/112 | Math 74, Mono 23, Symbols 2 11, Symbols 4 |
| Misc technical U+2300-23FF | 253/256 | Mono 118, Symbols 98, Symbols 2 37 |
| Dingbats U+2700-27BF | 180/192 | Symbols 2 141, Symbols 35, Mono 4 |

**Symbols 2 alone lacks some named symbols:** `↳` (Math only), `⎿` and `⎇`
(Noto Sans Symbols only). Noto Sans Mono lacks Braille entirely, as the
request expected. Noto Emoji covers all 21 curated emoji.

### 3.7 The page budget

A 16-column layer at 48 px is 800 × 2000 texels and holds 640 cells. The
seven requested blocks in full come to 1,057 codepoints, so **they do not
fit one page**.

The four blocks the terminal look depends on do fit:
- box drawing, block elements, Braille and geometric shapes: 512;
- the curated arrows, technical symbols, dingbats and other symbols of
  section 4.5: 71;
- the curated emoji: 21;
- the placeholder: 1.

That is **605 of 640, with 35 cells free**. The bullets and ellipsis
(`•` U+2022, `·` U+00B7, `…` U+2026) are in SYMBOLS, so they are in the text
atlas already and take no cell here.

## 4. Design

### 4.1 Files

```text
fonts/NotoSansMono-Regular-subset.ttf       new: instance wght 400, wdth 100,
                                            subset to TEXT ∪ SYMBOLS ∪ MATH
fonts/NotoSansMono-Bold-subset.ttf          new: instance wght 700, wdth 100,
                                            the same subset
fonts/LibmsdfTerminalSymbols-Regular.ttf    new: the merged symbol face
                                            (section 4.4)
fonts/terminal_symbols.py                   new: makes it from the upstream
                                            faces, reproducibly
fonts/ofl_subset.py                         gains the Noto Sans Mono instances
fonts/coverage-groups.txt                   gains the TERMINAL and
                                            TERMINAL_EMOJI groups
fonts/bundled.sha256                        gains the three faces
scripts/fetch-upstream-fonts.sh             gains the five files of section 3.1,
                                            with their sha256s
FONTS-LICENSE-OFL.txt                       gains the Noto copyright lines and
                                            the derivation note
```

The upstream variable faces are not committed. As with Grenze Gotisch and
Texturina, they are fetched into the gitignored `fonts/upstream/`, and the
committed subsets record their upstream sha256 (`BundledFace::
upstream_sha256`). The merged face is renamed (family "Libmsdf Terminal
Symbols"). No Noto face has a Reserved Font Name, so the rename is for
clarity, not a licence requirement.

### 4.2 Cap placement: a bake option

In `libmsdf-font`, `font/src/atlas.rs`:

```rust
/// Where a bake puts a face's cap height in the cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapPlacement {
    /// The cap height's distance from the cell top, as a fraction of the
    /// cell. `CAP_TOP_FRAC` (0.15) by default.
    pub top_frac: f64,
    /// The cap height in em, or `None` to measure the face's own `A`.
    pub height_em: Option<f64>,
}

impl CapPlacement {
    pub const DEFAULT: Self = Self { top_frac: CAP_TOP_FRAC, height_em: None };
}

/// `glyph_projection` with a placement. `glyph_projection` is this with
/// `CapPlacement::DEFAULT`, unchanged.
pub fn glyph_projection_with(face: &ttf_parser::Face, glyph_id: u16,
    glyph_size: u32, cap: CapPlacement) -> Option<GlyphProjection>;
```

- `default_cell_metrics` gains the same parameter through a `_with`
  variant.
- `FontAtlasBuilder::with_cap_placement(cap)` sets it for every cell of the
  bake, and `BakeOptions` gains `pub cap: CapPlacement`.
- The shader already reads each glyph's `baseline_row` from the glyph
  table, and `SetMetrics::baseline_frac` already records each set's
  baseline. **No format change, no shader change.**

**The terminal placement:** `top_frac = 0.24`. For a cap height of 717 the
window is then y -271 to 1029.
- Bold `Å` (1004) keeps 25 units of headroom, about 0.9 texel at 48 px.
- `g` (-245) keeps 26 units below it.

That is the most even split of the 51 spare units in the 1.3 em cell.

**The bake report gains a line per face:** the glyphs whose ink passes the
cell window, with the amount in font units. Steps 2 and 3 use it.

### 4.3 The terminal text atlas

- **One atlas, two layers:** Regular as the face and Bold as a style
  layer, 16 columns and 48 px cells, `CapPlacement { top_frac: 0.24,
  height_em: None }`.
- **Coverage:** `Coverage::Groups(vec![COVERAGE_TEXT, COVERAGE_SYMBOLS,
  COVERAGE_MATH])` for both layers, with layout tables and licence records
  as for the other OFL atlases.
- **Waiver:** `(Regular, '\u{AD}', "soft hyphen: Noto Sans Mono has no
  glyph; default-ignorable, drawn as nothing")`, and the same for Bold.
- **Cells:** about 406 mapped codepoints plus the shaped closure and glyph
  0. That is more than 320, so it needs 16 columns. Step 3 logs the exact
  count.
- **Size:** two 800 × 2000 layers, about 9.7 MB uncompressed in the
  binary, by the 4,830,216 bytes of a 16-column layer today.

**Bold through the style mechanism.** The Bold layer is reached as
`GlyphStyle::Bold` through `AtlasShaper::new(&atlas, GlyphStyle::Bold)`,
not as a separate atlas. It is one feature and one atlas, unlike Roboto's
separate Bold atlas, because the terminal shaper switches style within a
line.

### 4.4 The symbol face

`fonts/terminal_symbols.py` builds `LibmsdfTerminalSymbols-Regular.ttf`.
It takes no arguments and is deterministic: the same inputs give the same
bytes, which step 4 checks against `bundled.sha256`.

The build:
1. Instance Noto Sans Mono (`wght` 400, `wdth` 100), Noto Sans Symbols
   (`wght` 400) and Noto Emoji (`wght` 400). Noto Sans Symbols 2 and Noto
   Sans Math are static.
2. For each codepoint of the TERMINAL and TERMINAL_EMOJI groups, take the
   glyph from the first face that maps it:
   - TERMINAL: in the order of section 3.6;
   - TERMINAL_EMOJI: from Noto Emoji only.
3. Scale every outline to upem 1000, Noto Sans Mono's.
4. Normalise each glyph onto the cell, as below.
5. Write a face with `cmap`, `hmtx`, `glyf` and the name records, and no
   GSUB or GPOS. Every glyph stands alone.

Constants, in font units at upem 1000:
- cell C = 600;
- window W = [-271, 1029] (section 4.2);
- margin M = 30;
- overhang O = 100;
- design span of Noto Sans Mono's blocks B = [-240, 973].

**Rule 1: box drawing and block elements** (U+2500-259F, from Noto Sans
Mono):
- **Remap vertically:** map y linearly from B onto W, by y' = -271 + (y +
  240) × 1300 / 1213. That is a scale of 1.072, so horizontal strokes are
  7% thicker than Noto's.
- **Extend the touching edges outward**, except for the shades U+2591-2593:
  - points with x ≤ 6 move to x = -40, and points with x ≥ 594 to x = 640;
  - points with y' ≥ 1029 - 8 move to 1029 + O, and points with
    y' ≤ -271 + 8 to -271 - O.
- **Advance:** 600.

How the joins work:
- **Vertical.** The shader clips each glyph to its cell, and the cell is
  the line box. A stroke that runs past the window is therefore fully
  inside right up to the cell edge. Adjacent lines abut, and each pixel
  belongs to one line's quad, so there is no seam.
- **Horizontal.** The cell is 1.3 em wide but the advance is 0.6 em, so
  neighbouring glyphs overlap. The 0.04 em extension puts each glyph's
  antialiased edge inside its neighbour's solid stroke.
- **The shades** are dot patterns. An overlap would double their alpha in
  a stripe, so they are only remapped.

**Rule 2: everything else** (Braille, geometric shapes, curated symbols,
emoji):
- The width in cells is n = 2 for East Asian Width W or F, otherwise 1. For
  the curated set that makes exactly the 19 W emoji wide (section 3.5).
- **Advance:** n × C.
- **Fit:** where the ink is wider than n × C - 2M, scale uniformly about
  the ink centre to that width. Where the ink then passes W - M vertically,
  scale further, about the same centre, to fit.
- **Centre** the ink horizontally in the n cells, and keep its vertical
  position (its baseline relation).

The advance therefore encodes the width. The terminal shaper reads cells
from `Hmtx` as `round(advance / C)`, and needs no Unicode width table.

### 4.5 The symbol page

New coverage groups in `font/src/coverage.rs`, appended to the
non-exhaustive `CoverageGroupId`:
- `TerminalSymbols = 3`, with `COVERAGE_TERMINAL`, policy Skip;
- `TerminalEmoji = 4`, with `COVERAGE_TERMINAL_EMOJI`, policy Skip.

Both are written to `CVRG` with these ids, and to `fonts/coverage-groups.txt`
for the script, where the existing test keeps the two in step.

`COVERAGE_TERMINAL`, 583 codepoints:

```text
U+2500-257F   box drawing, all 128
U+2580-259F   block elements, all 32
U+25A0-25FF   geometric shapes, all 96
U+2800-28FF   Braille patterns, all 256
arrows (25)   U+2190-2199 ← ↑ → ↓ ↔ ↕ ↖ ↗ ↘ ↙   U+21B0-21B3 ↰ ↱ ↲ ↳
              U+21B5 ↵   U+21BA-21BB ↺ ↻   U+21D0-21D5 ⇐ ⇑ ⇒ ⇓ ⇔ ⇕
              U+21E5 ⇥   U+21E7 ⇧
technical (15) U+2303 ⌃  U+2318 ⌘  U+2325 ⌥  U+2328 ⌨  U+232B ⌫  U+2387 ⎇
              U+238B ⎋  U+23BF ⎿  U+23CE ⏎  U+23F5-23FA ⏵ ⏶ ⏷ ⏸ ⏹ ⏺
dingbats (24) U+2713-2718 ✓ ✔ ✕ ✖ ✗ ✘   U+2722-2727 ✢ ✣ ✤ ✥ ✦ ✧
              U+2731-2733 ✱ ✲ ✳   U+2736 ✶   U+273B-273D ✻ ✼ ✽   U+2756 ❖
              U+276E-276F ❮ ❯   U+279C ➜   U+27A4 ➤
other (7)     U+2605-2606 ★ ☆   U+2610-2612 ☐ ☑ ☒   U+2699 ⚙   U+29C9 ⧉
```

`COVERAGE_TERMINAL_EMOJI`, 21 codepoints, extendable by appending (35
cells are free):

```text
U+1F916 🤖  U+2705 ✅  U+274C ❌  U+26A0 ⚠  U+1F527 🔧  U+1F50D 🔍
U+1F4DD 📝  U+1F4C1 📁  U+1F4C4 📄  U+1F680 🚀  U+1F4A1 💡  U+1F41B 🐛
U+23F3 ⏳  U+231B ⌛  U+1F389 🎉  U+1F44D 👍  U+1F44E 👎  U+27A1 ➡
U+2B50 ⭐  U+1F512 🔒  U+1F9EA 🧪
```

Every codepoint of both groups is covered by the faces of section 3.6, so
the bake report lists none skipped. Skip stays the policy, so a later
addition that no face covers is reported rather than failing the bake.

**The atlas.** The merged face is baked as the Regular style, one layer of
16 columns at 48 px, about 4.8 MB, with:
- `Coverage::Groups(vec![COVERAGE_TERMINAL, COVERAGE_TERMINAL_EMOJI])`;
- `CapPlacement { top_frac: 0.24, height_em: Some(0.717) }`.

The face has no `A`. With the text face's cap height given explicitly, its
baseline and window are exactly the text atlas's. A test checks that 0.717
is the Noto Sans Mono subset's `A` top divided by its upem.

**Variation selectors.** U+FE0E and U+FE0F are default-ignorable, so
`AtlasShaper` gives them zero advance and draws nothing. A curated emoji
followed by either draws its one glyph, at its one width (open question 2).

### 4.6 Features and constants

In both `Cargo.toml`s, following section 4.2 of the bundled fonts plan:

```toml
font-ofl-noto-sans-mono = ["libmsdf-font/font-ofl-noto-sans-mono"]
font-ofl-noto-terminal-symbols = ["libmsdf-font/font-ofl-noto-terminal-symbols"]
# The terminal preset: Noto Sans Mono text and the symbol page.
fonts-ofl-terminal = ["font-ofl-noto-sans-mono", "font-ofl-noto-terminal-symbols"]
```

| Constant | Contents | Gate |
|---|---|---|
| `NOTO_SANS_MONO_REGULAR_TTF` | TTF | `all(feature = "font-ofl-noto-sans-mono", feature = "shaper")` |
| `NOTO_SANS_MONO_BOLD_TTF` | TTF | as above |
| `NOTO_SANS_MONO_ATLAS` | atlas, 2 layers | `feature = "font-ofl-noto-sans-mono"` |
| `TERMINAL_SYMBOLS_TTF` | TTF | `all(feature = "font-ofl-noto-terminal-symbols", feature = "shaper")` |
| `TERMINAL_SYMBOLS_ATLAS` | atlas, 1 layer | `feature = "font-ofl-noto-terminal-symbols"` |

`BundledAtlas` gains `NotoSansMono` and `TerminalSymbols`. It is
exhaustive, so this is breaking (section 6), and it becomes
`#[non_exhaustive]` in the same release so that later faces are additive.

### 4.7 The terminal shaper

In `libmsdf-font`, new module `terminal`. It needs no `shaper` feature,
since it is built on `AtlasShaper`.

```rust
/// Which atlas a segment's glyphs are in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalSource { Text, Symbols }

/// A run of glyphs from one atlas, starting at a column.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalSegment {
    pub source: TerminalSource,
    pub column: u32,
    /// Advances snapped to whole cells (font units).
    pub run: ShapedRun,
}

/// One laid-out line.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalLine {
    pub segments: Vec<TerminalSegment>,
    /// The columns the line takes.
    pub columns: u32,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TerminalError {
    /// An atlas has no layout tables.
    NoLayout(TerminalSource),
    /// The atlases disagree on upem, cell advance or baseline.
    Mismatch { what: &'static str, text: f32, symbols: f32 },
}

pub struct TerminalShaper { /* an AtlasShaper per text style, one for the page */ }

impl TerminalShaper {
    /// Checks that the two atlases share upem, cell advance (the text
    /// face's `0`) and `baseline_frac`.
    pub fn new(text: &FontAtlas, symbols: &FontAtlas) -> Result<Self, TerminalError>;
    /// The cell advance in em (0.6 for Noto Sans Mono).
    pub fn cell_em(&self) -> f32;
    /// The line pitch at which box drawing joins: `LINE_BOX_RATIO` em.
    pub fn line_pitch_em(&self) -> f32;
    /// Lay `text` out in `style`. A style the text atlas lacks uses Regular.
    pub fn line(&self, text: &str, style: GlyphStyle) -> TerminalLine;
}
```

`line`:
1. For each character, choose the source: the text atlas if its `Cmap` has
   it, otherwise the symbol page if its `Cmap` has it, otherwise the text
   atlas (which draws the placeholder).
2. A default-ignorable character joins the segment before it.
3. Split the text into maximal same-source segments, and shape each with
   its source's `AtlasShaper`. The symbol page is always shaped as Regular:
   symbols are style-invariant, like the icons.
4. Snap each glyph's advance to `max(round(x_advance / cell), 0) × cell`,
   keeping `x_offset` and `y_offset`, so marks and zero-width glyphs stay
   at zero. Each segment's `column` is the sum of the columns before it.

In libmsdf, `src/drawlist/mod.rs`:

```rust
impl DrawList {
    /// Draw a laid-out line at `pos` (the line box's top left): each segment
    /// with `push_shaped_text_with`, from `text` or `symbols`, at
    /// `pos.x + column * cell_em * font_size`. Returns the width drawn.
    pub fn push_terminal_line(&mut self, line: &TerminalLine,
        text: AtlasRef<'_>, symbols: AtlasRef<'_>, pos: [f32; 2],
        font_size: f32, px_range: f32, color: [f32; 4]) -> f32;
}
```

Both atlases are 800 wide, so an `AtlasSet` of the two needs no padding
(atlas set plan, 23:26 EDT amendment). The renderer holds three layers,
about 19 MB as RGBA8.

### 4.8 README and licences

- **README, "Bundled fonts":** gains the two features and the preset, and
  states that box drawing joins only at `line_pitch_em()`.
- **README:** gains a short example that lays out and draws
  `"╭─ ⠋ Working… ─╮"`.
- **FONTS-LICENSE-OFL.txt:** gains the copyright lines of the five Noto
  faces and a note that the subsets and the merged face are modified
  versions under the same licence.

## 5. Tests

**CPU, with no GPU and no bake**, reading the committed faces:
- **Coverage:** each subset maps exactly the codepoints of section 3.2, and
  the merged face maps exactly the 604 codepoints of section 4.5.
  `coverage-groups.txt` agrees with the Rust constants.
- **Cell geometry of the merged face:**
  - every advance is 600 or 1200, and 1200 exactly for the 19 wide emoji;
  - every Rule 2 glyph's ink lies within its cells less M, and within
    W - M;
  - every box and block glyph that touches an edge reaches past W by at
    least O - 8 vertically, or to -40 or 640 horizontally;
  - the shades stay inside W.
- **Placement:** the terminal placement puts the window at -271 to 1029
  for both subsets.

**With `cpu-bake`:**
- **The text atlas:**
  - its report lists exactly the waived soft hyphen and the four MATH skips
    of section 3.2;
  - "ink past the window" is empty at 0.24, and lists the 166 glyphs of
    section 3.3 at the default placement;
  - the layout equivalence check passes.
- **The symbol page:**
  - its report lists no skipped codepoint;
  - "ink past the window" lists exactly the overhanging box and block
    glyphs;
  - its `CVRG` has groups 3 and 4;
  - its `baseline_frac` equals the text atlas's.
- **The shaper:**
  - `"│ ⠋ a✅b"` lays out as segments Symbols@0, Text@1, Symbols@2, Text@3
    (two columns), Symbols@5 (two columns), Text@7, for 8 columns;
  - `"⚠\u{FE0F}x"` puts `x` at column 1;
  - each Braille spinner frame `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`, followed by `" Working"`, gives
    the same columns and the same `x_offset` for `W`;
  - an atlas pair with different baselines is refused with `Mismatch`.

**GPU, with `gpu-tests`,** at font sizes 12, 14, 16, 20 and 24 px and scale
factors 1, 2 and 3 (phone sizes up to 72 px), with lines `line_pitch_em()`
apart:
- **Blocks:** a 3 × 3 block of `█` has alpha ≥ 0.99 at every pixel more
  than 1 px inside its outer edge, so no seam shows between cells or lines.
- **Strokes:** in three lines of `┼─┼─┼`, the alpha along each stroke's
  centre line, sampled across every cell and line boundary, is at least 0.9
  of its value at mid-cell. `╭──╮`, `│  │`, `╰──╯` closes: the corners meet
  the strokes with the same bound.
- **Braille:** each dot's centroid in a spinner frame is within 0.5 px of
  its position in the other frames, and the text after it does not move.
- **Accents:** `Å` in Bold at 16 px and scale 3 draws its ring whole. The
  ring's top row of ink is below the cell's top edge.

## 6. Versioning (Rule 5)

**The release is not strictly additive, so it is 0.5.0** for `libmsdf` and
`libmsdf-font` in lockstep:
- **Breaking:** `BundledAtlas` gains two variants (it is exhaustive), and
  becomes `#[non_exhaustive]`.
- **Breaking:** `BakeOptions` gains `cap`. Its fields are public and it is
  exhaustive, so struct literals stop compiling.
- **Added:**
  - `CapPlacement`, `glyph_projection_with`, `default_cell_metrics_with`
    and `FontAtlasBuilder::with_cap_placement`;
  - `CoverageGroupId::{TerminalSymbols, TerminalEmoji}`,
    `COVERAGE_TERMINAL` and `COVERAGE_TERMINAL_EMOJI`;
  - the `terminal` module (`TerminalShaper`, `TerminalLine`,
    `TerminalSegment`, `TerminalSource`, `TerminalError`) and
    `DrawList::push_terminal_line`;
  - the features and constants of section 4.6, and the bake report's
    ink line.

Under Rule 5.5, a crate with a public API that takes this release bumps its
own minor version.

## 7. Steps

1. **Faces.**
   - `fetch-upstream-fonts.sh` gains the five files.
   - `ofl_subset.py` makes the two Noto Sans Mono subsets.
   - `FONTS-LICENSE-OFL.txt` gains the Noto entries.
   - `bundled.sha256` gains the subsets.

   **Done when:** a fresh fetch reproduces both subsets byte for byte; each
   maps exactly the codepoints of section 3.2; and "Ål g ½ № 5 € ₽" shapes
   the same in the subset as in the upstream instance.
2. **Cap placement.**
   - `CapPlacement`, the `_with` functions, `with_cap_placement`,
     `BakeOptions::cap`, and the report's ink line.

   **Done when:** every existing bundled atlas bakes byte-identical to
   `bundled.sha256` at the default; and the Noto Sans Mono Regular subset
   reports the 166 glyphs at the default and none at 0.24.
3. **The terminal text atlas.**
   - The feature `font-ofl-noto-sans-mono`, `BundledAtlas::NotoSansMono`,
     the constants and the waivers.

   **Done when:** the build-time bake passes its equivalence check; its
   report is as in section 5; and the cell count is logged in CHANGES.md.
4. **The symbol face and page.**
   - `terminal_symbols.py` and the merged face.
   - The two coverage groups and `coverage-groups.txt`.
   - The feature `font-ofl-noto-terminal-symbols`,
     `BundledAtlas::TerminalSymbols` and the constants.

   **Done when:** the script reproduces the committed face byte for byte;
   the CPU geometry tests pass; and the page bakes 605 cells with the
   report of section 5.
5. **The shaper and the draw call.**
   - The `terminal` module and `push_terminal_line`.

   **Done when:** the shaper tests of section 5 pass.
6. **The preset and the render checks.**
   - `fonts-ofl-terminal`, the README additions, and the GPU tests.

   **Done when:** the GPU tests pass on this machine's adapter, with the
   renders and their log under `out/terminal/`; and
   `scripts/check-font-features.sh` passes with the preset on and off.
7. **Release.**
   - 0.5.0 for both crates, the `VERSIONS.md` entry, and the tag `v0.5.0`.

   **Done when:** the tag is on the commit that changes the version.

## 8. Open questions

1. **The line pitch is 1.3 em.** Box drawing joins only at
   `LINE_BOX_RATIO`, because the remap targets the line box. That is
   tighter than Noto's own 1.362 em and looser than its 1.213 em block
   design. A different terminal pitch means re-baking the box and block
   glyphs for it. Is 1.3 em right?
2. **Emoji width and the selectors.** As decided, a selector does not
   change the glyph. With one glyph per codepoint, the width is also fixed:
   `⚠` and `➡` are one cell (scaled to about 0.53 of their emoji size), whether
   or not U+FE0F follows, and the other 19 are two cells. Terminals that
   follow `wcwidth` or string-width draw `⚠️` and `➡️` (with U+FE0F) two
   columns wide, so a column-aligned line after them would be one column
   short. The alternative bakes a second, two-cell glyph for those two
   codepoints, chosen by the shaper when U+FE0F follows. That costs 2 cells
   and departs from "the same glyph". Keep one glyph?
3. **The rest of arrows, technical symbols and dingbats** (about 480
   more codepoints that the faces cover) do not fit this page. Is a second
   symbol page wanted, or are the curated sets enough?
4. **Baking on demand** for rare codepoints, at run time with the compute
   MSDF path. It is not proposed here. Is it wanted later?
5. **Italic.** Noto Sans Mono has no italic. Should italic terminal text
   draw upright (as this plan does, through the Regular fallback), or
   slanted at draw time?
6. **The two extra faces.** Noto Sans Symbols and Noto Sans Math fill what
   Symbols 2 lacks (section 2). Are they acceptable, or should those
   symbols be dropped from the curated sets?
