# libmsdf

The wgpu SDF/MSDF rendering engine and typography stack, extracted from
matter-stream's render crates (`matterstream-common` / `-font` / `-ui-gpu`)
with all VM, skills, and card code pruned: SDF primitive instancing
(box / rounded-box / circle / line / Bézier stroke / filled Bézier path), MSDF text via font
atlases (baked at build time **and** generated at runtime by a WGSL compute
shader), rustybuzz shaping, and the `DrawList` contract that highbay_ui's
node-graph lowers into. Runs native and on wasm32 (WebGPU); includes the lo-fi
"pencil sketch" noise-distortion hook point (PLAN.md Phase 5).

Status: Phase 5 complete — extracted from matter-stream (VM/skills/cards pruned),
wgpu 29 SDF/MSDF renderer, runtime compute-shader MSDF generation matching the CPU
msdfgen baseline, dynamic atlas management, cubic-Bézier arc strokes, and the
`DrawList` render contract. Native + wasm32 (WebGPU) compile-clean; GPU tests run
headless on Metal. See `Cargo.toml` for the `cpu-bake` / `gpu-tests` feature notes.

## Drawing into a DrawList

`libmsdf::Render` is the one shared name for something that draws itself
into a `DrawList`: `render_into(&self, out)` appends its drawing (never
clearing), and `render_mut(&mut self, out)` is the same for a renderer that
settles state as it draws. There is no context and no clip: a renderer
holds what it draws with, and a caller clips around the call. A kept
`DrawList` is itself a `Render`, appended at top level.

```rust
struct Underline { from: [f32; 2], to: [f32; 2] }
impl libmsdf::Render for Underline {
    fn render_into(&self, out: &mut libmsdf::drawlist::DrawList) {
        out.push_line(self.from, self.to, 2.0, [1.0; 4]);
    }
}
```

### Several atlases in one list

An `AtlasSet` holds several atlases as one texture array and one glyph
table. Draw from an atlas in it through its handle; one renderer uploads
the set once and draws any list in it, so text and icons from different
atlases interleave in a single list.

```rust
let mut set = libmsdf::AtlasSet::new();
let text = set.register(FontAtlas::from_bytes(ROBOTO)?)?;
let icons = set.register(FontAtlas::from_bytes(SYMBOLS)?)?;
let (text, icons) = (set.atlas(text).unwrap(), set.atlas(icons).unwrap());
let run = text.shaper(GlyphStyle::Regular)?.shape("Done");
list.push_shaped_text_with(&run, text, [10.0, 10.0], 24.0, px_range, white);
list.push_icon_with(&check, icons, [80.0, 6.0], [32.0, 32.0], 32.0, px_range, white);

let renderer = GpuSdfRenderer::new_with_atlas_set(&device, format, &set);
renderer.upload_atlas_set(&queue, &set);
renderer.render_draw_list_in(&device, &queue, &view, w, h, 1.0, &list, &set, 0.0);
```

## Licensing

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Bundled fonts are optional

libmsdf can bundle font files and the MSDF atlases baked from them. Every
one is behind a cargo feature that is **off by default**, and the
feature's name says which licence it brings:

| Feature | Face | Licence | Licence file |
|---|---|---|---|
| `font-apache-roboto-regular` | Roboto Regular | Apache-2.0 | [FONTS-LICENSE-APACHE.txt](FONTS-LICENSE-APACHE.txt) |
| `font-apache-roboto-bold` | Roboto Bold | Apache-2.0 | [FONTS-LICENSE-APACHE.txt](FONTS-LICENSE-APACHE.txt) |
| `font-apache-roboto-italic` | Roboto Italic | Apache-2.0 | [FONTS-LICENSE-APACHE.txt](FONTS-LICENSE-APACHE.txt) |
| `font-apache-roboto-msymbols` | Roboto Regular with Material Symbols | Apache-2.0 | [FONTS-LICENSE-APACHE.txt](FONTS-LICENSE-APACHE.txt) |
| `font-ofl-grenze-gotisch` | Grenze Gotisch (subset) | OFL-1.1 | [FONTS-LICENSE-OFL.txt](FONTS-LICENSE-OFL.txt) |
| `font-ofl-texturina` | Texturina (subset) | OFL-1.1 | [FONTS-LICENSE-OFL.txt](FONTS-LICENSE-OFL.txt) |
| `font-ofl-noto-sans-mono` | Noto Sans Mono Regular and Bold (instances, subset) | OFL-1.1 | [FONTS-LICENSE-OFL.txt](FONTS-LICENSE-OFL.txt) |
| `font-ofl-noto-terminal-symbols` | The terminal symbol page, merged from five Noto faces | OFL-1.1 | [FONTS-LICENSE-OFL.txt](FONTS-LICENSE-OFL.txt) |

`fonts-apache-roboto` enables the four Roboto features,
`fonts-ofl-ledger` Grenze Gotisch and Texturina, and `fonts-ofl-terminal`
the two terminal features.

A font's licence applies to a build only when its feature is enabled.
With no font feature enabled, the compiled crate contains no font data and
is licensed purely under MIT OR Apache-2.0. If you enable one and
distribute the result, distribute the matching licence file with it.

The repository itself contains the font files (under `fonts/`, and Roboto
atlases under `tests/fixtures/` that the tests use); they stay under their
own licences there whether or not a feature is enabled.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

## Bundled fonts

Enable the features you use, for example:

```toml
[dependencies]
libmsdf = { git = "https://github.com/tmzt/libmsdf", rev = "...", features = ["fonts-ofl-ledger", "font-apache-roboto-regular"] }
```

Each feature brings that face's MSDF atlas (`GRENZE_GOTISCH_ATLAS`,
`ROBOTO_REGULAR_ATLAS`, ...: format 5, with layout tables, licence records
and, for the OFL faces, coverage groups), and with `shaper` (on by default)
its TTF (`GRENZE_GOTISCH_TTF`, `ROBOTO_REGULAR_ASCII`, ...). A consumer that
lays text out from the atlas alone turns default features off:

```rust
let atlas = libmsdf::FontAtlas::from_bytes(libmsdf::GRENZE_GOTISCH_ATLAS)?;
let shaper = libmsdf::AtlasShaper::new(&atlas, libmsdf::GlyphStyle::Regular)?;
let run = shaper.shape("Treasury 50G");
```

The atlases are baked by `libmsdf::bake::Baker` and committed under
`fonts/atlases/`, so enabling a font feature needs nothing on the build
host beyond Rust: no C++ compiler and no bake at build time. (Baking them in
`build.rs` was the plan's first choice; it was too slow for a build script
at the default `opt-level` - see CHANGES.md.) The OFL atlases are 16 cell
columns wide (800 x 2000 texels); `FontAtlas::padded_to` pads an 8-column
atlas to match before stacking atlases into one texture array.

**Your own faces.** A build script can bake with the same baker by
depending on the `libmsdf-font` crate (no wgpu) with its `bake` feature:

```toml
[build-dependencies]
libmsdf-font = { git = "https://github.com/tmzt/libmsdf", rev = "...", features = ["bake"] }
```

```rust
// build.rs
use libmsdf_font::bake::{BakeOptions, Baker, LicenceSpec};
use libmsdf_font::{GlyphStyle, LayoutOptions};

fn main() {
    let ttf = std::path::Path::new("assets/MyFace.ttf");
    println!("cargo:rerun-if-changed={}", ttf.display());
    let mut opts = BakeOptions::new(48, 6.0);
    opts.licences.push(LicenceSpec { spdx: "OFL-1.1".into(), source: "My Face".into(), styles: vec![GlyphStyle::Regular] });
    opts.layout = Some(vec![(GlyphStyle::Regular, LayoutOptions::default())]);
    let bytes = Baker::bake_face(ttf, &opts).expect("bake");
    std::fs::write(std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("myface.atlas"), bytes).unwrap();
}
```

Baking needs a host C++ compiler (msdfgen) and a macOS or Linux-gnu host.
On a nightly Cranelift dev profile, add to the workspace manifest:

```toml
[profile.dev.build-override]
codegen-backend = "llvm"
```

`[profile.dev.build-override] opt-level = 2` makes such a bake several
times faster.

**Terminal text.** `fonts-ofl-terminal` brings Noto Sans Mono (Regular, and
Bold as a style layer) and a symbol page on the same 0.6 em cell and
baseline: box drawing, block elements, Braille and geometric shapes in
full, and curated arrows, technical symbols, dingbats and monochrome emoji
(`COVERAGE_TERMINAL`, `COVERAGE_TERMINAL_EMOJI`). A `TerminalShaper` lays a
line out on the cell grid, taking each character from the text atlas or the
page, and `push_terminal_line` draws it:

```rust
let mut set = libmsdf::AtlasSet::new();
let text = FontAtlas::from_bytes(libmsdf::NOTO_SANS_MONO_ATLAS)?;
let page = FontAtlas::from_bytes(libmsdf::TERMINAL_SYMBOLS_ATLAS)?;
let shaper = libmsdf::TerminalShaper::new(&text, &page)?;
let (text, page) = (set.register(text)?, set.register(page)?);
let pitch = shaper.line_pitch_em() * font_size;
for (i, s) in ["╭─ ⠋ Working… ─╮", "│ ✔ done  🤖  │", "╰──────────────╯"].iter().enumerate() {
    let line = shaper.line(s, GlyphStyle::Regular);
    let pos = [x, y + i as f32 * pitch];
    list.push_terminal_line(&line, set.atlas(text).unwrap(), set.atlas(page).unwrap(), pos, font_size, px_range, white);
}
```

Box drawing joins only when lines are `line_pitch_em()` (1.3 em, the line
box) apart. Emoji are two cells, except `⚠` and `➡`, which are one cell and
become two when U+FE0F follows them, as in terminals. Italic draws upright.

**Verifying the bundled atlases:**
`cargo run -p libmsdf-font --features bake --bin libmsdf-bake -- bundled --check fonts/atlases`
re-bakes them in memory and compares, and
`cargo test --features fonts-apache-roboto,fonts-ofl-ledger,fonts-ofl-terminal --test bundled_fonts`
checks them against `fonts/bundled.sha256`.

**Rebuilding the bundled faces:** `scripts/fetch-upstream-fonts.sh` fetches
the upstream faces (google/fonts, Roboto v2.138, Material Symbols, Noto) at
pinned sources with SHA-256 checks, and `scripts/rebuild-fonts.sh` rebuilds
the OFL subsets, the terminal symbol face and the Roboto Bold and Italic
faces from them and compares them
with the committed bytes, checks that the scripts behind the two Regular
faces return them unchanged, and with `--atlases` also runs the atlas check
above. It needs `uv`.

## Architectural decisions

| Document | Date | Status | Decision |
|---|---|---|---|
| [Filled Bézier paths as an SDF primitive](docs/design/20261015_01_FILLED_BEZIER_PATH_PLAN.md) | 2026-10-15 | Implemented | Adds `SdfKind::PathFill`: closed line/quadratic/cubic paths filled by an exact even-odd signed distance, segments in the aux param bank; every step done, released as 0.3.0 (`v0.3.0`). |
| [Atlas-carried layout tables and a shaper without rustybuzz](docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md) | 2026-10-16 | In implementation | Atlas format 5 (split, version-gated field) with tagged `Head`/`Cmap`/`Hmtx`/`Gsub`/`Gpos`, `LICS` and `LICT` tables, no VM data, and a pure-Rust `AtlasShaper` proven equal to rustybuzz at bake; approved by the owner, in implementation on `impl/0.4.0`. |
| [Bundled fonts as optional, licence-named features, baked at build time](docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md) | 2026-10-16 | In implementation | Every bundled face (four Roboto, Apache-2.0; Grenze Gotisch and Texturina, OFL-1.1, subset) becomes an opt-in, licence-named cargo feature whose format-5 atlas is baked at build time by a public `Baker` in a split-out `libmsdf-font` crate, with coverage groups and a `CVRG` table; approved, in implementation on `impl/0.4.0`. |
| [Stateful components that render into DrawLists](docs/design/20261021_01_COMPONENTS_PLAN.md) | 2026-10-21 | Superseded (by [A minimal Render trait](docs/design/20261021_02_RENDER_TRAIT_PLAN.md)) | A design-neutral component trait with context, gating and placement; superseded before implementation when the owner chose only a minimal render trait for libmsdf, components being UI hosting rather than rendering. |
| [A minimal Render trait](docs/design/20261021_02_RENDER_TRAIT_PLAN.md) | 2026-10-21 | Implemented | One public trait, `Render`, with `render_into(&self, out)` and `render_mut(&mut self, out)` (by default `render_into`), no context or clip, implemented for `DrawList`; every step done, released as 0.4.1 (`v0.4.1`). |
| [Several atlases in one draw list, cached in the renderer](docs/design/20261021_03_ATLAS_SET_PLAN.md) | 2026-10-21 | Implemented | An `AtlasSet` laid out as one texture array and one glyph table, handles (`AtlasRef`: an id naming its set, and the atlas) passed to new `push_shaped_text_with` / `push_icon_with` calls that tag each instance (amended from a scope), `lower_with(&set)` offsetting glyph indices, and a renderer that uploads the set once; a shaping helper on the handle; every step done, released as 0.4.2 (`v0.4.2`). |
| [A terminal face and a symbol page, from Noto](docs/design/20261023_01_TERMINAL_FONTS_PLAN.md) | 2026-10-23 | Implemented | Noto Sans Mono (OFL) Regular and Bold as one two-layer terminal atlas, a 607-cell symbol page (box drawing, blocks, Braille and geometric shapes in full, curated arrows, technical symbols, dingbats and monochrome emoji, merged from Noto faces onto the mono cell), a per-bake cap placement so accents and box glyphs fit the cell, a `TerminalShaper` on a fixed cell grid, and a `fonts-ofl-terminal` preset; breaking (0.5.0); approved with the owner's answers (1.3 em pitch, two-cell `⚠️`/`➡️` wide forms, the extra Noto faces); every step done, released as 0.5.0 (`v0.5.0`). |
| [A per-atlas cell box, and atlas format 5:0.2.0](docs/design/20261023_02_PER_ATLAS_CELL_BOX_PLAN.md) | 2026-10-23 | Implemented | Cells may reach above or below the 1.3 em line box, set per atlas at bake time and recorded in a `CELL` table at format 5:0.2.0 (written only when used, so older readers refuse it and every other atlas stays 5:0.1.0), with text drawn from the cell box; re-bakes Texturina and Grenze Gotisch whole with their baselines unchanged; adds Ș ș Ț ț, eight Claude Code interface glyphs and ✨ to the coverage; breaking (0.6.0); approved with the owner's changes; every step done, released as 0.6.0 (`v0.6.0`). |
| [Font evidence sheets from a libmsdf example](docs/design/20261023_03_FONT_SHEET_EXAMPLES_PLAN.md) | 2026-10-23 | Draft | One example, `examples/font_sheets.rs`, with `cells`, `terminal` and `glyphs` subcommands, draws every panel of the FONT documents' evidence sheets with libmsdf (the reference as the glyph's outline filled as a path), labels and guides included, into the next free `out/fonts/YYYYMMDD_NN_<name>/` with metrics and a log entry, replacing the scratch tools; no public API change; drafted for the owner's review. |
