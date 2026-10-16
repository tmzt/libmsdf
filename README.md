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

`fonts-apache-roboto` enables the four Roboto features and
`fonts-ofl-ledger` the two OFL ones.

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

**Verifying the bundled atlases:**
`cargo run -p libmsdf-font --features bake --bin libmsdf-bake -- bundled --check fonts/atlases`
re-bakes them in memory and compares, and
`cargo test --features fonts-apache-roboto,fonts-ofl-ledger --test bundled_fonts`
checks them against `fonts/bundled.sha256`.

**Rebuilding the bundled faces:** `scripts/fetch-upstream-fonts.sh` fetches
the upstream faces (google/fonts, Roboto v2.138, Material Symbols) at pinned
sources with SHA-256 checks, and `scripts/rebuild-fonts.sh` rebuilds the
OFL subsets and the Roboto Bold and Italic faces from them and compares them
with the committed bytes, checks that the scripts behind the two Regular
faces return them unchanged, and with `--atlases` also runs the atlas check
above. It needs `uv`.

## Architectural decisions

| Document | Date | Status | Decision |
|---|---|---|---|
| [Filled Bézier paths as an SDF primitive](docs/design/20261015_01_FILLED_BEZIER_PATH_PLAN.md) | 2026-10-15 | Implemented | Adds `SdfKind::PathFill`: closed line/quadratic/cubic paths filled by an exact even-odd signed distance, segments in the aux param bank; every step done, released as 0.3.0 (`v0.3.0`). |
| [Atlas-carried layout tables and a shaper without rustybuzz](docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md) | 2026-10-16 | In implementation | Atlas format 5 (split, version-gated field) with tagged `Head`/`Cmap`/`Hmtx`/`Gsub`/`Gpos`, `LICS` and `LICT` tables, no VM data, and a pure-Rust `AtlasShaper` proven equal to rustybuzz at bake; approved by the owner, in implementation on `impl/0.4.0`. |
| [Bundled fonts as optional, licence-named features, baked at build time](docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md) | 2026-10-16 | In implementation | Every bundled face (four Roboto, Apache-2.0; Grenze Gotisch and Texturina, OFL-1.1, subset) becomes an opt-in, licence-named cargo feature whose format-5 atlas is baked at build time by a public `Baker` in a split-out `libmsdf-font` crate, with coverage groups and a `CVRG` table; approved, in implementation on `impl/0.4.0`. |
