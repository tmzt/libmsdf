---
title: Bundled fonts as optional, licence-named features, baked at build time
date: 2026-10-16
status: In implementation
covers: >
  Every font face libmsdf bundles (Roboto Regular, Bold, Italic, Roboto
  Regular merged with Material Symbols, and two SIL OFL 1.1 faces, Grenze
  Gotisch and Texturina) becomes an opt-in cargo feature whose name carries
  its licence, all off by default. Each feature brings that face's MSDF
  atlas (format 5, with layout and licence tables), baked from the committed
  TTF by libmsdf's build script into OUT_DIR when the feature is on, and,
  together with the `shaper` feature, the TTF bytes. The baker becomes a
  public static entry point (`libmsdf::bake::Baker::bake_face`) used by the
  build script, by a consumer's build script and by a CLI; it lives in a new
  wgpu-free crate, `libmsdf-font`, split out of `src/font`. Also covered: a
  fallback (an explicit bake command and committed atlases) with exact
  switch criteria; instructions for agents in AGENTS.md; licence files at the
  repository root; a README Licensing section stating that the crate with no
  font feature is purely MIT OR Apache-2.0; checks that a build with no font
  feature carries no font bytes and bakes nothing; the consumer changes; and
  the joint 0.4.0 release with the atlas layout tables plan.
sources:
  - >
    Owner (Tim), 2026-10-16, relayed by the coordinating agent: "can we move
    the baked files and includes to libmsdf with a feature per include and
    updated README FONT-LICENSE-XXX.txt stating they're optional and without
    them license is MIT/Apache"; "yes, write the plan and hold the tag".
  - >
    Owner, 2026-10-16, relayed while this plan was drafted: "add ofl to the
    feature names"; on the Apache names that followed from it, "apache
    naming is fine, keep it. right the license so consumer opts in knowing
    it"; "and bake in build.rs if any enabled"; and "if we can't directly
    expose the baker as a crate static method (it can call python) and
    clearly instruct agents to do that" (section 2).
  - Cargo.toml (license "MIT OR Apache-2.0"; edition 2024; features
    default = [], cpu-bake = msdfgen 0.2 + ttf-parser 0.18, gpu-tests; the
    comment on the Cranelift mis-prefixing of msdfgen's C++ symbols;
    publish = false), build.rs (WGSL validation with naga)
  - >
    src/font/ (mod.rs with ROBOTO_REGULAR_ASCII, ROBOTO_ASCII_MSYMBOLS,
    ROBOTO_BOLD_ASCII, ROBOTO_ITALIC_ASCII as include_bytes! of fonts/*.ttf,
    bundled_style_face and the licence references; shaper.rs with
    StyledShaper::bundled; atlas.rs with FontAtlasBuilder; its references
    outside the module, section 3.5), src/drawlist/mod.rs, src/lib.rs
  - >
    tests/styled_faces.rs, tests/icons_agree_with_the_baker.rs,
    tests/atlas_layers.rs, examples/bake_atlas.rs, scripts/bake-atlases.py,
    fonts/ (the four Roboto TTFs, LICENSE-Roboto.txt and
    LICENSE-MaterialSymbols.txt, the *.py face scripts), .gitignore,
    README.md, VERSIONS.md, AGENTS.md
  - >
    docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md with all amendments
    to 13:03 EDT (format 5:0.1.0, Head/Cmap/Hmtx/Gsub/Gpos, LICS/LICT,
    AtlasShaper, the default-on `shaper` feature, per-face
    `--feature <tag>[@<style>,...]`, `--layout`, `--licence`, the bake-time
    equivalence check, and its section 3 measurements of the two OFL faces);
    its steps 1 and 2 are committed (ba766d8, f15a308).
  - >
    Measurements made for this plan on 2026-10-16 (section 3.4): bake time
    and reproducibility of `bake_atlas` at HEAD bd33d69 on an Apple Silicon
    Mac, stable cargo 1.94.1; msdfgen-sys 0.2.1's prebuilt bindings.
  - >
    Consumers, read only: libcontrols (git rev b375be3; uses no bundled face
    constant); highbay (uses ROBOTO_ASCII_MSYMBOLS, ROBOTO_REGULAR_ASCII and
    StyledShaper::bundled; commits its own three-layer
    roboto-msymbols-48.atlas; its dev profile is LLVM since 2026-07-29); the
    tower defense game (uses ROBOTO_REGULAR_ASCII; ships its own copies of
    Grenze Gotisch and Texturina, their v3 atlases, and a
    FONTS-LICENSE-OFL.txt at its repository root).
  - >
    Upstream sources: google/fonts ofl/grenzegotisch and ofl/texturina (SIL
    OFL 1.1, no Reserved Font Name); googlefonts/roboto v2.138
    roboto-android.zip; google/material-design-icons variablefont/.
related:
  - >
    docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md: this plan depends on
    its steps 1 to 7 and takes over its step 8 (the 0.4.0 release), held on
    the owner's instruction until both plans land (section 5).
  - docs/design/20261015_01_FILLED_BEZIER_PATH_PLAN.md (the 0.3.0 landing,
    whose consumer-build procedure step 9 reuses)
---

# Bundled fonts as optional, licence-named features, baked at build time

## 1. Purpose

libmsdf bundles four Roboto faces today, unconditionally: every build of the
crate, on every target, compiles about 85 KB of TTF into the library through
four `include_bytes!` constants, whether or not the consumer draws with
them. They are Apache-2.0 (Roboto and Material Symbols), compatible with the
crate's own MIT OR Apache-2.0, but still a second licence a consumer takes
on without choosing to.

A consumer that wants other faces must bake and commit its own atlases and
carry its own copies of the font files. The tower defense game does this for
its two ledger faces, Grenze Gotisch and Texturina (SIL OFL 1.1), and
highbay commits its own baked Roboto atlas. Committed atlases are 2.4 MB
binaries per layer that drift from the code that reads them: when the format
moves (as it does to version 5 in the atlas layout tables plan), every copy
must be re-baked by hand.

This plan makes every bundled face an opt-in cargo feature whose name says
which licence it brings, moves the game's two OFL faces into libmsdf on the
same terms, and bakes each enabled face's atlas from its committed TTF at
build time, so no bundled atlas is committed and none can be stale. It
states the licensing plainly: with no font feature enabled, the compiled
crate contains no font data, bakes nothing, and is MIT OR Apache-2.0 only.

## 2. Decisions made by the owner (2026-10-16)

1. **One cargo feature per bundled face, all off by default, with the
   licence in the name.** The owner: "add ofl to the feature names". The
   Apache faces follow the same pattern, which the owner confirmed: "apache
   naming is fine, keep it. right the license so consumer opts in knowing
   it". A consumer that enables a font feature has named, in its own
   manifest, the licence it takes on.

   | Feature | Face | Licence |
   |---|---|---|
   | `font-apache-roboto-regular` | Roboto Regular, subset | Apache-2.0 |
   | `font-apache-roboto-bold` | Roboto Bold, subset | Apache-2.0 |
   | `font-apache-roboto-italic` | Roboto Italic, subset | Apache-2.0 |
   | `font-apache-roboto-msymbols` | Roboto Regular merged with Material Symbols | Apache-2.0 (both) |
   | `font-ofl-grenze-gotisch` | Grenze Gotisch | OFL-1.1 |
   | `font-ofl-texturina` | Texturina | OFL-1.1 |

   Two group features, whose names also carry the licence:
   `fonts-apache-roboto` (the four Roboto features) and `fonts-ofl-ledger`
   (both OFL faces).
2. **Each feature brings that face's atlas** (format 5, with layout and
   licence tables), and its TTF bytes **only together with the `shaper`
   feature** (which `cpu-bake` implies), since without `shaper` nothing in
   libmsdf reads a TTF.
3. **The atlases are baked in `build.rs` when any font feature is enabled**
   ("and bake in build.rs if any enabled"), from the committed TTFs into
   `OUT_DIR`, and included from there. With no font feature enabled the
   build script bakes nothing and no baking dependency is built.
4. **The baker is a public static entry point of the crate**,
   `libmsdf::bake::Baker::bake_face`, usable from libmsdf's build script, a
   consumer's build script and a CLI. The owner allowed it to call Python
   where that is the cleaner path ("it can call python"); section 4.6 says
   why it does not need to. **If baking in `build.rs` proves infeasible**,
   the same entry point is run by an explicit, documented command, the
   atlases are committed to a known path and included from there, and
   AGENTS.md and the README instruct agents exactly when and how to run it
   and to never hand-edit or substitute an atlas. Section 4.10 gives the
   exact switch criteria. **Primary path: `build.rs`.**
5. **Licence files at the repository root**, one per licence, listing the
   faces, files, copyright lines and sources, then the full licence text.
   The owner wrote "FONT-LICENSE-XXX.txt"; this plan uses the plural
   `FONTS-LICENSE-OFL.txt` and `FONTS-LICENSE-APACHE.txt` to match the file
   the tower defense game already has for the same purpose, so one name
   means one thing in both repositories.
6. **README** states that libmsdf is MIT OR Apache-2.0, that the bundled
   fonts are optional, that each font's licence applies only when its
   feature is enabled, and that with no font feature the crate is purely
   MIT OR Apache-2.0.
7. **Consumers** enable the features they use; the tower defense game drops
   its own copies of the two OFL faces and their atlases.
8. **Hold the tag.** This plan lands in the same 0.4.0 release as the atlas
   layout tables plan, whose release step is held, on the owner's
   instruction ("hold the tag"), until both plans land, and is performed
   once, as this plan's last step.

## 3. Findings

### 3.1 Who uses the bundled face constants

| Where | Uses | Needs |
|---|---|---|
| libmsdf library code | `bundled_style_face` reads the Regular, Bold and Italic constants; `StyledShaper::bundled` calls it | feature-aware (section 4.4) |
| libmsdf unit tests | `ROBOTO_REGULAR_ASCII` in `font/shaper.rs`, `font/outline.rs`, `drawlist/mod.rs` | a test-only face (section 4.11) |
| libmsdf integration tests | all four, in `styled_faces.rs`, `icons_agree_with_the_baker.rs`, `atlas_layers.rs` | a test-only face (section 4.11) |
| `examples/bake_atlas.rs` | Bold and Italic constants for `--style bold` / `--style italic` without a path | the path form, or the features (section 4.6) |
| libcontrols | none: it draws boxes and lines and takes atlas and shaper from its caller | no font feature |
| highbay | `ROBOTO_ASCII_MSYMBOLS` (most uses), `ROBOTO_REGULAR_ASCII`, `StyledShaper::bundled` (so Bold and Italic) | `fonts-apache-roboto` |
| tower defense game | `ROBOTO_REGULAR_ASCII` (its status line); its own `include_bytes!` of the two OFL TTFs and their v3 atlases | `fonts-ofl-ledger`, `font-apache-roboto-regular` |

The game's copies, which libmsdf's must equal byte for byte (sha256):

```text
701b299d8dc002a2b4bea2ff0f1272c0e4081a2835914354804565c410d0c637  GrenzeGotisch[wght].ttf    193,492 bytes
478a15d7145cf94565cfc1aeb33596aeb0118c7d86a84d061ddf46de3d7dfda3  Texturina[opsz,wght].ttf   288,320 bytes
```

### 3.2 Copyright lines (`name` ID 0, atlas layout tables plan section 3.5)

- Grenze Gotisch: `Copyright 2020 The Grenze Gotisch Project Authors (https://github.com/Omnibus-Type/Grenze-Gotisch)`
- Texturina: `Copyright 2020 The Texturina Project Authors (https://github.com/Omnibus-Type/Texturina)`
- Roboto (all three cuts): `Copyright 2011 Google Inc. All Rights Reserved.`
- Material Symbols: not in the merged face's `name` table; to be read from
  the upstream variable font's `name` ID 0 in step 2.

Neither OFL face declares a Reserved Font Name.

### 3.3 Optional layout features in the ledger faces

Grenze Gotisch has `tnum`; **Texturina does not** (atlas layout tables plan,
section 3.1). The game sets its figures in Texturina, so tabular figures are
not available from the figure face, and nothing in the game turns `tnum` on.

### 3.4 Bake time and reproducibility (measured 2026-10-16)

`bake_atlas` from HEAD bd33d69 (writing format 5 without tables; the layout
extraction and its equivalence check did not exist yet), on an Apple Silicon
Mac, stable cargo 1.94.1, 48 px cells, px range 6.0, one layer:

| Face | release build | debug build (opt-level 0) |
|---|---|---|
| Roboto Regular ascii | 1.0 s user | 1.0 s user |
| Grenze Gotisch | 1.8 s user | not measured |
| Texturina | 2.3 s user | 2.7 s user |

- **Reproducible on one machine:** two release bakes of Texturina, and the
  release and debug bakes of Roboto Regular and of Texturina, are
  byte-identical (sha256 `e478381e...` for Roboto Regular, `e7514c56...` for
  Texturina). Unoptimised build-script code therefore does not change the
  bytes here. Reproducibility across CPU architectures is not established
  (scripts/bake-atlases.py says so too).
- **Not yet measured:** the equivalence check (atlas layout tables plan,
  section 4.9), which shapes about 40,000 corpus strings per face with both
  shapers. That plan budgets it at under 10 s per face in a release build.
  Build scripts compile at opt-level 0 by default, so in `build.rs` it may be
  several times slower; step 6 measures it against the switch criteria.

### 3.5 Can a build script reach the baker?

A build script cannot depend on the crate it builds, so libmsdf's `build.rs`
cannot call `libmsdf::FontAtlasBuilder`. The baker must live in a crate the
build script can name as a build-dependency. `src/font` is almost
self-contained: outside its own module it uses only
`crate::drawlist::LINE_BOX_RATIO` (one function, `min_antialiased_font_size`,
and one test) plus doc links to `drawlist` items; it has no wgpu or
bytemuck code; its dependencies are rustybuzz, ttf-parser 0.25,
unicode-segmentation, and msdfgen with ttf-parser 0.18 under `cpu-bake`. It
can move into a crate of its own with one constant moved alongside it.

### 3.6 Toolchain facts for a build-time bake

- **msdfgen is C++ built by `cc`.** As a build-dependency it is compiled for
  the host, so cross-compiling to Android or wasm32 needs only the host's
  C++ compiler (Xcode command line tools on macOS, g++ or clang on Linux).
  The target never sees it.
- **msdfgen-sys 0.2.1 ships prebuilt bindings only for these hosts:**
  `aarch64-macos`, `x86_64-macos`, and `{aarch64, arm, x86, x86_64, mips,
  mips64, powerpc, powerpc64, sparc64}-linux-gnu`. Other hosts (Windows,
  Linux musl) fail with "No prebuilt bindings" unless its `bindgen` feature
  is on, which needs libclang. This limit already applies to `cpu-bake`.
- **The Cranelift problem.** `Cargo.toml` records that under a Cranelift dev
  profile on macOS, msdfgen's C++ symbols are referenced with a doubled
  Mach-O underscore and linking fails. `codegen-backend` is a profile key
  that only works on nightly, and profiles are set by the top-level
  workspace, never by a dependency, so libmsdf cannot choose the backend for
  its build script. Build scripts and their dependencies are compiled under
  the `build-override` profile, which inherits `codegen-backend` from the
  profile it overrides. On stable (libmsdf, the tower defense game) the
  issue cannot arise. highbay moved its dev profile to LLVM on 2026-07-29.
  A consumer that does use Cranelift adds one line (section 4.9).
- **Feature separation.** With resolver 2 or later (the default since
  edition 2021; libmsdf is edition 2024), features of a build-dependency are
  not unified with the same crate as a normal dependency, so the bake
  feature of the build-time copy never reaches the target build.

### 3.7 Atlas sizes

A one-layer 48 px atlas is about 2.4 MB, and highbay's three-layer Roboto
atlas is 7,222,132 bytes. A consumer's binary grows by the raw size of every
atlas it enables. Baked into `OUT_DIR`, none of them is in the repository.

## 4. Design

### 4.1 Crate layout: `libmsdf-font`

The repository becomes a workspace with the root package `libmsdf` and one
member, `font/` (package `libmsdf-font`, `publish = false`, licence
`MIT OR Apache-2.0`, no wgpu):

```text
font/Cargo.toml
font/src/lib.rs          the contents of today's src/font/mod.rs
font/src/{atlas,glyph_table,layout,licence,manager,outline,packer,shaper}.rs
                         moved from src/font/ by git mv, `crate::font::` -> `crate::`
font/src/layout_bake.rs  (atlas layout tables plan, step 3)
font/src/atlas_shaper.rs (atlas layout tables plan, step 4)
font/src/bake.rs         new: Baker (section 4.5)
font/src/bundled.rs      new: the bundled-face table (section 4.5)
font/src/bin/libmsdf-bake.rs  new: the CLI (section 4.6)
```

`LINE_BOX_RATIO` moves into `libmsdf-font` (`libmsdf_font::LINE_BOX_RATIO`)
and `libmsdf::drawlist` re-exports it under its current path. libmsdf keeps
`pub use libmsdf_font as font;` and every crate-root re-export it has, so
**every public path is unchanged** (`libmsdf::FontAtlas`,
`libmsdf::font::atlas::FontAtlas`, ...). The types are the same types, not
copies.

`libmsdf-font` features:

```toml
[features]
default = ["shaper"]
shaper = ["dep:rustybuzz", "dep:ttf-parser"]
bake = ["shaper", "dep:msdfgen", "dep:ttf-parser-018", "dep:sha2"]
```

libmsdf's `shaper` becomes `["libmsdf-font/shaper"]` and `cpu-bake`
becomes `["shaper", "libmsdf-font/bake"]`.

### 4.2 Cargo features and build-dependencies of libmsdf

```toml
[dependencies]
libmsdf-font = { path = "font", default-features = false }

[build-dependencies]
naga = { version = "30.0.1", features = ["wgsl-in"] }
# The build-time baker: built for the HOST, only when a font feature is on.
# Renamed so `dep:` cannot be confused with the normal dependency.
libmsdf-font-bake = { package = "libmsdf-font", path = "font", features = ["bake"], optional = true }

[features]
default = ["shaper"]
shaper = ["libmsdf-font/shaper"]
cpu-bake = ["shaper", "libmsdf-font/bake"]
gpu-tests = []
# Bundled fonts. Every one is OFF by default. The licence is in the name:
# enabling one takes on that font's licence (FONTS-LICENSE-*.txt, README.md
# "Licensing"). With none enabled, the compiled crate contains no font data,
# the build script bakes nothing, and the crate is MIT OR Apache-2.0 only.
# Enabling one bakes its atlas in build.rs: the build host needs a C++
# compiler (README.md, "Bundled fonts").
font-apache-roboto-regular = ["dep:libmsdf-font-bake"]
font-apache-roboto-bold = ["dep:libmsdf-font-bake"]
font-apache-roboto-italic = ["dep:libmsdf-font-bake"]
font-apache-roboto-msymbols = ["dep:libmsdf-font-bake"]
font-ofl-grenze-gotisch = ["dep:libmsdf-font-bake"]
font-ofl-texturina = ["dep:libmsdf-font-bake"]
fonts-apache-roboto = [
    "font-apache-roboto-regular", "font-apache-roboto-bold",
    "font-apache-roboto-italic", "font-apache-roboto-msymbols",
]
fonts-ofl-ledger = ["font-ofl-grenze-gotisch", "font-ofl-texturina"]
```

A font feature does not enable `shaper`: a layout-only consumer
(`default-features = false`) gets the atlas, not the TTF, and no rustybuzz
or ttf-parser in its target build (they are in its host build only, inside
the build-time baker).

### 4.3 Files

```text
fonts/Roboto-Regular-ascii.ttf            unchanged
fonts/Roboto-Bold-ascii.ttf               unchanged
fonts/Roboto-Italic-ascii.ttf             unchanged
fonts/Roboto-Regular-ascii-msymbols.ttf   unchanged
fonts/GrenzeGotisch[wght].ttf             new, upstream bytes, unmodified
fonts/Texturina[opsz,wght].ttf            new, upstream bytes, unmodified
fonts/bundled.sha256                      new: reference hashes (section 4.8)
FONTS-LICENSE-APACHE.txt                  new, repository root
FONTS-LICENSE-OFL.txt                     new, repository root
fonts/LICENSE-Roboto.txt                  removed (section 4.12)
fonts/LICENSE-MaterialSymbols.txt         removed (section 4.12)
tests/fixtures/legacy/roboto-ascii-48-v3.atlas   kept (legacy-reader tests)
tests/fixtures/roboto-ascii-48.atlas      kept, format 5 (atlas layout tables
                                          plan step 7): tests that run with
                                          no feature and no bake load it
```

**No bundled atlas is committed** on the primary path. The two committed
fixtures stay because tests need atlases without baking: the legacy one
exercises the v3 reader on real historical bytes, the v5 one exercises
everything else and is also the byte reference for the Roboto Regular
build-time bake (section 4.8).

The OFL faces keep their upstream file names, brackets included, so their
provenance shows in the name. They are the full variable fonts, not
subsets: the game ships them unmodified today, the atlas is baked from the
default instance, and an unmodified file needs no derivation script
(open question 3).

### 4.4 Constants

In `src/bundled.rs` of libmsdf, re-exported from `libmsdf::font` and the
crate root. The four Roboto TTF constants move here from `src/font/mod.rs`
with their doc comments, keeping their names.

| Constant | Contents | Gate |
|---|---|---|
| `ROBOTO_REGULAR_ASCII` | TTF | `all(feature = "font-apache-roboto-regular", feature = "shaper")` |
| `ROBOTO_BOLD_ASCII` | TTF | `all(feature = "font-apache-roboto-bold", feature = "shaper")` |
| `ROBOTO_ITALIC_ASCII` | TTF | `all(feature = "font-apache-roboto-italic", feature = "shaper")` |
| `ROBOTO_ASCII_MSYMBOLS` | TTF | `all(feature = "font-apache-roboto-msymbols", feature = "shaper")` |
| `GRENZE_GOTISCH_TTF` | TTF | `all(feature = "font-ofl-grenze-gotisch", feature = "shaper")` |
| `TEXTURINA_TTF` | TTF | `all(feature = "font-ofl-texturina", feature = "shaper")` |
| `ROBOTO_REGULAR_ATLAS` | atlas | `feature = "font-apache-roboto-regular"` |
| `ROBOTO_BOLD_ATLAS` | atlas | `feature = "font-apache-roboto-bold"` |
| `ROBOTO_ITALIC_ATLAS` | atlas | `feature = "font-apache-roboto-italic"` |
| `ROBOTO_MSYMBOLS_ATLAS` | atlas | `feature = "font-apache-roboto-msymbols"` |
| `ROBOTO_MSYMBOLS_STYLED_ATLAS` | atlas, 3 layers | `all(` msymbols, bold, italic features `)` |
| `GRENZE_GOTISCH_ATLAS` | atlas | `feature = "font-ofl-grenze-gotisch"` |
| `TEXTURINA_ATLAS` | atlas | `feature = "font-ofl-texturina"` |
| `BUNDLED_ATLAS_GLYPH_SIZE` | `u32` = 48 | none |
| `BUNDLED_ATLAS_PX_RANGE` | `f32` = 6.0 | none |

All are `pub const`, as today, so they stay usable in `const` and `static`
initialisers (highbay builds a `static TextShaper` from one). TTF constants
are `include_bytes!("../fonts/<file>.ttf")`; atlas constants are
`include_bytes!(concat!(env!("OUT_DIR"), "/bundled/<file>.atlas"))`.

**What each atlas is.** 48 px cells, px range 6.0, layout tables with the
default features only, and the face's licence records:

- `ROBOTO_REGULAR_ATLAS`: the plain face as the Regular style, one layer;
  the same bake as `tests/fixtures/roboto-ascii-48.atlas`.
- `ROBOTO_BOLD_ATLAS`, `ROBOTO_ITALIC_ATLAS`: the Bold or Italic face baked
  **as a face of its own**, at the Regular style, one layer. Its glyph ids
  are the face's raw ids, so it pairs with `TextShaper::new(ROBOTO_BOLD_ASCII)`
  or `AtlasShaper::new(&atlas, GlyphStyle::Regular)`.
- `ROBOTO_MSYMBOLS_ATLAS`: the merged face as the Regular style, one layer.
- `ROBOTO_MSYMBOLS_STYLED_ATLAS`: the merged face with Bold and Italic as
  style layers: the bake highbay commits today as `roboto-msymbols-48.atlas`,
  for `StyledShaper` and styled `AtlasShaper`s. It needs all three faces.
- `GRENZE_GOTISCH_ATLAS`, `TEXTURINA_ATLAS`: each OFL face as the Regular
  style, one layer, from its default instance.

**`bundled_style_face(style)`** stays available whenever `shaper` is on and
returns `Some` only for a style whose feature is enabled (Regular, Bold,
Italic as above; BoldItalic stays `None`). **`StyledShaper::bundled(regular)`**
keeps its signature and adds only the styles `bundled_style_face` returns;
shaping a missing style is the existing `NoFaceForStyle` error. Both doc
comments name the feature each style needs.

The manifests that describe the faces without containing them
(`TEXT_RANGES`, `MSYMBOLS_ICONS`, `msymbols_codepoint`, `HIGHBAY_ICONS`,
`highbay_codepoint`, `MARKERS`, `OWNED_BLOCKS`, `PRIVATE_USE`) are names and
codepoints, not font data, and stay ungated.

### 4.5 The baker: `libmsdf::bake::Baker`

In `libmsdf-font`, module `bake`, behind its `bake` feature, re-exported by
libmsdf as `libmsdf::bake` behind `cpu-bake`:

```rust
/// One face to bake, and how. Every field is an input to the bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct BakeOptions {
    pub glyph_size: u32,                     // 48 for the bundled atlases
    pub px_range: f64,                       // 6.0
    /// Additional cuts baked as style layers: (style, face file).
    pub styles: Vec<(GlyphStyle, PathBuf)>,
    /// One per licence; at least one covering every baked style.
    pub licences: Vec<LicenceSpec>,
    /// Layout tables and the per-face optional features
    /// (atlas layout tables plan); None bakes no layout tables.
    pub layout: Option<Vec<(GlyphStyle, LayoutOptions)>>,
    /// If set, every input face file must have this sha256, checked before
    /// anything is baked: (file, digest).
    pub expect_sha256: Vec<(PathBuf, [u8; 32])>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LicenceSpec { pub spdx: String, pub source: String, pub styles: Vec<GlyphStyle> }

#[derive(Debug)]
#[non_exhaustive]
pub enum BakeError {
    Io { path: PathBuf, error: std::io::Error },
    FaceHash { path: PathBuf, expected: [u8; 32], found: [u8; 32] },
    Layout(LayoutError),          // includes the equivalence-check Mismatch
    Licence(String),
    Build(String),                // FontAtlasBuilder::build's refusals
}

pub struct Baker;

impl Baker {
    /// Bake `ttf` (the Regular face) with `opts` and return the atlas file
    /// bytes (format 5). Pure Rust: no subprocess, no network.
    pub fn bake_face(ttf: &Path, opts: &BakeOptions) -> Result<Vec<u8>, BakeError>;
    /// Bake one bundled atlas from the face files under `fonts_dir`
    /// (libmsdf's `fonts/`), with its fixed options and recorded hashes.
    pub fn bake_bundled(atlas: BundledAtlas, fonts_dir: &Path) -> Result<Vec<u8>, BakeError>;
}
```

`bake_face` wraps `FontAtlasBuilder` (`new`, `add_shipped_coverage`,
`add_styled_coverage`, `with_layout`, `add_licence`, `build`) and
`FontAtlas::to_bytes`, in the order `examples/bake_atlas.rs` uses today, so
a consumer's build script gets the exact bake libmsdf uses.

**The bundled table** (`libmsdf_font::bundled`, compiled without `bake`, so
libmsdf can name the files and features without the baker):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BundledAtlas {
    RobotoRegular, RobotoBold, RobotoItalic, RobotoMsymbols,
    RobotoMsymbolsStyled, GrenzeGotisch, Texturina,
}
impl BundledAtlas {
    pub const ALL: [BundledAtlas; 7];
    pub fn file_name(self) -> &'static str;      // "roboto-regular-48.atlas", ...
    pub fn features(self) -> &'static [&'static str]; // cargo features that enable it
    pub fn faces(self) -> &'static [BundledFace];      // input files, Regular first
}
pub struct BundledFace { pub file: &'static str, pub sha256: [u8; 32], pub style: GlyphStyle }
```

| Atlas | Faces (under `fonts/`) | Licences |
|---|---|---|
| `roboto-regular-48.atlas` | `Roboto-Regular-ascii.ttf` | `Apache-2.0=Roboto` |
| `roboto-bold-48.atlas` | `Roboto-Bold-ascii.ttf` (baked as Regular) | `Apache-2.0=Roboto` |
| `roboto-italic-48.atlas` | `Roboto-Italic-ascii.ttf` (baked as Regular) | `Apache-2.0=Roboto` |
| `roboto-msymbols-48.atlas` | `Roboto-Regular-ascii-msymbols.ttf` | `Apache-2.0=Roboto`, `Apache-2.0=Material Symbols` |
| `roboto-msymbols-styled-48.atlas` | the merged face; Bold, Italic as style layers | `Apache-2.0=Roboto@regular,bold,italic`, `Apache-2.0=Material Symbols` |
| `grenze-gotisch-48.atlas` | `GrenzeGotisch[wght].ttf` | `OFL-1.1=Grenze Gotisch` |
| `texturina-48.atlas` | `Texturina[opsz,wght].ttf` | `OFL-1.1=Texturina` |

All with `glyph_size` 48, `px_range` 6.0, layout on with **no optional
features**. This matches what the game uses today (section 3.3). A consumer
that wants another feature set, for example Grenze with `tnum`, calls
`Baker::bake_face` from its own build script with its own `LayoutOptions`
(open question 4). Each face's sha256 is in the table: a TTF that differs
from the recorded bytes refuses to bake (`BakeError::FaceHash`), so the
inputs are pinned.

### 4.6 Callers: build script, consumer build scripts, CLI, Python

- **libmsdf's `build.rs`** (section 4.7).
- **A consumer's build script**: `[build-dependencies] libmsdf-font = { git
  = "...", rev = "...", features = ["bake"] }`, then
  `libmsdf_font::bake::Baker::bake_face(path, &opts)` and
  `include_bytes!(concat!(env!("OUT_DIR"), ...))`. It depends on
  `libmsdf-font`, not `libmsdf`, so its build script does not compile wgpu.
  The README gives a ten-line example.
- **CLI**: `font/src/bin/libmsdf-bake.rs`, `required-features = ["bake"]`:

  ```text
  cargo run -p libmsdf-font --features bake --bin libmsdf-bake -- \
      face <font.ttf> <out.atlas> [--size 48] [--px-range 6.0]
           [--style <bold|italic|bolditalic>=<face.ttf>]...
           [--layout] [--feature <tag>[@<style>,...]]...
           [--licence <SPDX>=<source>[@<style>,...]]...
  cargo run -p libmsdf-font --features bake --bin libmsdf-bake -- \
      bundled [--out <dir>] [--check <dir>] [--hashes]
  ```

  `bundled` bakes every `BundledAtlas` into `<dir>` (default
  `out/bundled/`); `--check <dir>` bakes into memory and compares with the
  files in `<dir>`; `--hashes` prints lines in the `fonts/bundled.sha256`
  format. `examples/bake_atlas.rs` stays as a thin wrapper over
  `Baker::bake_face` with its current arguments, so existing commands keep
  working. Its path-less `--style bold` reads `ROBOTO_BOLD_ASCII` and so
  needs `font-apache-roboto-bold`. Without it, it fails with "`--style bold`
  without a path needs the `font-apache-roboto-bold` feature; pass
  `--style bold=<face.ttf>`".
- **Python.** The owner allowed the baker to call Python where that is
  cleaner. It does not need to, and the build script must not. Baking,
  layout extraction (rustybuzz and ttf-parser, atlas layout tables plan)
  and input pinning (sha256) are all in Rust. A build-time Python
  requirement would break every consumer build host that has no `uv`
  (Android CI, a clean Linux box). Python stays where it is today: the
  maintenance scripts that **make** the Roboto TTFs (`fonts/*.py`, run with
  `uv run --with fonttools`, uv 0.10 or later, Python 3.11 or later) and
  `scripts/bake-atlases.py`. The latter gains a `--bundled` mode that runs
  the CLI, and like the face scripts it stops with "this needs `uv` (https://docs.astral.sh/uv/)
  on PATH" when uv is missing. If a later need (for example a provenance
  check that only fontTools can do) makes Python the cleaner path inside
  `Baker`, it is an amendment to this plan, and it never runs from
  `build.rs`.

### 4.7 The build script

`build.rs` keeps its WGSL validation and gains, under
`#[cfg(any(feature = "font-apache-roboto-regular", ...))]` (all six
features; Cargo compiles build scripts with the package's feature cfgs):

```rust
fn bake_bundled() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bundled");
    let fonts = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("fonts");
    for atlas in BundledAtlas::ALL {
        if !atlas.features().iter().all(|f| feature_on(f)) { continue; }   // CARGO_FEATURE_*
        for face in atlas.faces() {
            println!("cargo:rerun-if-changed={}", fonts.join(face.file).display());
        }
        let stamp = stamp_of(atlas, &fonts);   // sha256 of: inputs' bytes, BakeOptions
                                               // Debug text, libmsdf-font version
        if fresh(&out, atlas, &stamp) { continue; }  // <file>.atlas and <file>.stamp match
        let bytes = Baker::bake_bundled(atlas, &fonts)
            .unwrap_or_else(|e| panic!("baking {}: {e}", atlas.file_name()));
        write_atomically(&out.join(atlas.file_name()), &bytes);  // tmp + rename
        write_atomically(&out.join(stamp_name(atlas)), stamp.as_bytes());
    }
}
```

- **Caching.** `OUT_DIR` persists per package, profile and feature set.
  Cargo reruns the build script only when a `rerun-if-changed` path changes
  (the TTFs, `build.rs`, the WGSL files) or the script is rebuilt (which
  happens when `libmsdf-font`, the baker, changes). When it reruns for an
  unrelated reason, such as a shader edit, the stamp check skips every
  atlas whose inputs are unchanged. So a no-op rebuild bakes nothing.
- **Failure.** A bake error panics with the atlas name and the
  `BakeError`, so the build fails loudly. A wrong TTF is
  `FaceHash` with both digests.
- **No font feature.** The `cfg` removes the function and the
  build-dependency is not built: the script is exactly today's.

### 4.8 Determinism and the reference check

Pinned inputs: each TTF's sha256 (checked before baking); the options (in
the bundled table); `msdfgen = "=0.2.1"` and `ttf-parser-018 = "=0.18.1"`
exactly in `libmsdf-font`'s manifest, since msdfgen's float output is the
texels; and the `libmsdf-font` source itself (a path dependency, same
commit). rustybuzz and ttf-parser 0.25 stay caret requirements, because an
exact pin in a library constrains every consumer; a release of either that
changed an extracted table would change the bytes, and the check below
catches that.

`fonts/bundled.sha256` records the reference bake, one line per atlas and
host architecture:

```text
<sha256>  roboto-regular-48.atlas  aarch64-apple-darwin
...
```

Checks:

- `tests/bundled_fonts.rs::bundled_atlases_match_the_reference`, under all
  font features: the sha256 of each atlas constant equals the recorded line
  for the host triple. On a host with no recorded line, it prints the
  digests and passes with the message "no reference bake recorded for
  <host>; record one with `libmsdf-bake bundled --hashes`". That is a gap to
  fill, not a pass to rely on (open question 5).
- `ROBOTO_REGULAR_ATLAS` equals `tests/fixtures/roboto-ascii-48.atlas` byte
  for byte. This is a second, committed reference, independent of the hash
  file.
- `scripts/check-font-features.sh` (step 8) builds the full feature set in
  two separate target directories and compares the `OUT_DIR` atlases byte
  for byte.

### 4.9 What a consumer must set, if anything

- **Nothing**, on stable Rust with a C++ compiler on the build host, which
  is true of libmsdf, the tower defense game and highbay's LLVM profile
  today.
- **A host C++ compiler** whenever a font feature is on (as for `cpu-bake`
  today); macOS or Linux-gnu hosts (section 3.6).
- **On a nightly Cranelift profile**, one line in the workspace manifest:

  ```toml
  [profile.dev.build-override]
  codegen-backend = "llvm"
  ```

  This builds the build scripts and their dependencies, including msdfgen,
  with LLVM while the rest of the profile stays Cranelift. libmsdf cannot set
  it for them (section 3.6). The README and the `Cargo.toml` comment on
  `cpu-bake` say so, and the comment is updated from "enable together with
  the LLVM backend" to this line.
- **Optionally**, a faster bake: `[profile.dev.build-override] opt-level = 2`
  (step 6 records whether this matters).
- **Android and wasm32**: nothing extra. The bake runs on the host. Step 8
  checks `cargo check --target wasm32-unknown-unknown` and
  `--target aarch64-linux-android` with the font features from a macOS host,
  and step 9 runs the game's Android build.

### 4.10 The fallback, and when to switch to it

**Fallback.** The same `Baker`, run explicitly:

```text
cargo run -p libmsdf-font --features bake --bin libmsdf-bake -- bundled --out fonts/atlases --hashes
```

The atlases are committed under `fonts/atlases/<file>.atlas`, the hashes to
`fonts/bundled.sha256`, and the atlas constants become
`include_bytes!("../fonts/atlases/<file>.atlas")`. `build.rs` does not bake
and the `libmsdf-font-bake` build-dependency is removed. Everything else in
this plan is unchanged, including the features, the gates, the reference
check and the AGENTS.md instructions (with the fallback's text).

**Switch criteria.** Step 6 measures these on the reference machine (Apple
Silicon Mac, the stable toolchain the repository uses) and records each
number in `out/fonts/LOG.md`. The primary path stays only if **all** hold:

1. **Split approved.** The owner approves the `libmsdf-font` split (open
   question 1). Without it a build script cannot reach the baker (section
   3.5).
2. **Build time.** With default `build-override` settings, a clean
   `cargo build --features fonts-apache-roboto,fonts-ofl-ledger` takes at
   most 60 s longer than a clean `cargo build`. Of that, the build script's
   own run (seven atlases, equivalence checks included) is at most 30 s. A
   no-op rebuild, and a rebuild after touching a WGSL file, bake 0 atlases
   (counted from `OUT_DIR/bundled/bake.log`, to which the build script
   appends one line per atlas it bakes).
3. **Hosts and targets.** A clean build with all font features succeeds on
   macOS aarch64 and on Linux x86_64-gnu with only a C++ compiler
   installed. `cargo check` with all font features succeeds for
   `wasm32-unknown-unknown` and `aarch64-linux-android` from macOS.
4. **Consumers.** highbay and the tower defense game (desktop and Android)
   build with their features enabled, with no profile changes other than
   section 4.9's `build-override` line.
5. **Determinism.** Two clean builds in different target directories give
   byte-identical atlases, equal to `fonts/bundled.sha256` for the host.

If any criterion fails and cannot be met by a change inside this plan's
design, the implementation switches to the fallback. It records an
addendum in `CHANGES.md` naming the failed criterion and its measured
value. The switch is defined here, so it is not a departure from the plan.
Changing a criterion's numbers is a plan change (Rule 2).

### 4.11 libmsdf's own tests

`cargo test` with no font feature must still cover shaping, baking and
drawing, so Rule 3.4's `cargo test -p libmsdf` stays meaningful and does
not trigger a bake.

- **Unit tests in `src/`** (and in `font/src/`) use a test-only constant,
  `#[cfg(test)] pub(crate) const TEST_FACE: &[u8] =
  include_bytes!(".../fonts/Roboto-Regular-ascii.ttf");`, in place of
  `ROBOTO_REGULAR_ASCII`. `cfg(test)` is set only for the crate's own unit
  tests, never in a consumer's build.
- **Integration tests** read files through `tests/common/faces.rs`
  (`include_bytes!` of the TTFs they use) in place of the crate constants.
  A test binary is not the library.
- **Tests of the gated API** (each constant, the licence records,
  `bundled_style_face`, `StyledShaper::bundled`, each atlas shaping like its
  TTF, the reference check) live in `tests/bundled_fonts.rs`, each under
  `#[cfg(feature = "...")]`. They run under
  `cargo test --features fonts-apache-roboto,fonts-ofl-ledger`, a second
  required test command for every step from step 1 on.

### 4.12 Licence files

Two files at the repository root, built like the game's existing
`FONTS-LICENSE-OFL.txt`: a header saying the crate itself is not under this
licence (it is MIT OR Apache-2.0, README.md) and that the files are compiled
in only when their feature is enabled; one entry per face; a rule line; the
full licence text.

`FONTS-LICENSE-OFL.txt`:

```text
Fonts bundled with libmsdf under the SIL Open Font License 1.1

libmsdf itself is not under this licence: it is MIT OR Apache-2.0 (see
README.md). The font files below, and the MSDF atlases baked from them, are
licensed under the SIL Open Font License, Version 1.1, reproduced in full
after this list. They are compiled into libmsdf only when the cargo feature
named with each one is enabled.

  fonts/GrenzeGotisch[wght].ttf, and the atlas grenze-gotisch-48.atlas
  baked from it
    Feature: font-ofl-grenze-gotisch (or fonts-ofl-ledger)
    Grenze Gotisch
    Copyright 2020 The Grenze Gotisch Project Authors (https://github.com/Omnibus-Type/Grenze-Gotisch)
    Source: https://github.com/google/fonts/tree/main/ofl/grenzegotisch
    No Reserved Font Name.

  fonts/Texturina[opsz,wght].ttf, and the atlas texturina-48.atlas baked
  from it
    Feature: font-ofl-texturina (or fonts-ofl-ledger)
    Texturina
    Copyright 2020 The Texturina Project Authors (https://github.com/Omnibus-Type/Texturina)
    Source: https://github.com/google/fonts/tree/main/ofl/texturina
    No Reserved Font Name.

-----------------------------------------------------------------------------
<the full SIL OFL 1.1 text, as in google/fonts ofl/grenzegotisch/OFL.txt
after its copyright line>
```

`FONTS-LICENSE-APACHE.txt` has the same header for the Apache License 2.0
and entries for each Roboto file and atlas (`font-apache-roboto-regular`,
`-bold`, `-italic`, `-msymbols`; the styled atlas under all three), each
with the copyright line of section 3.2, the source (googlefonts/roboto
v2.138 `roboto-android.zip`; for the merged face also
google/material-design-icons `variablefont/`, Material Symbols Outlined),
and the change notice Apache 2.0 section 4(b) asks for ("subset to
`TEXT_RANGES` and extended by libmsdf's `fonts/*.py` scripts"; the OFL files
are unmodified), followed by the full Apache 2.0 text.

**The old files.** `fonts/LICENSE-Roboto.txt` and
`fonts/LICENSE-MaterialSymbols.txt` are both the bare Apache 2.0 text and
name no file. They are removed. The references to them (two Roboto and one
Material Symbols doc comment in `src/font/mod.rs`, `fonts/msymbols.py`,
`.gitignore`) point at `FONTS-LICENSE-APACHE.txt` instead. Pointer files in
`fonts/` would be a second place to keep in step. The atlas layout tables
plan's mention of the old file (its section 3.5) is a historical finding and
is not edited.

### 4.13 README

A `## Licensing` section replaces `## License`, keeping its text and
`### Contribution`, and adding:

> ### Bundled fonts are optional
>
> libmsdf can bundle font files and the MSDF atlases baked from them. Every
> one is behind a cargo feature that is **off by default**, and the
> feature's name says which licence it brings:
>
> | Feature | Face | Licence | Licence file |
> |---|---|---|---|
> | `font-apache-roboto-regular` | Roboto Regular | Apache-2.0 | FONTS-LICENSE-APACHE.txt |
> | `font-apache-roboto-bold` | Roboto Bold | Apache-2.0 | FONTS-LICENSE-APACHE.txt |
> | `font-apache-roboto-italic` | Roboto Italic | Apache-2.0 | FONTS-LICENSE-APACHE.txt |
> | `font-apache-roboto-msymbols` | Roboto Regular with Material Symbols | Apache-2.0 | FONTS-LICENSE-APACHE.txt |
> | `font-ofl-grenze-gotisch` | Grenze Gotisch | OFL-1.1 | FONTS-LICENSE-OFL.txt |
> | `font-ofl-texturina` | Texturina | OFL-1.1 | FONTS-LICENSE-OFL.txt |
>
> `fonts-apache-roboto` enables the four Roboto features and
> `fonts-ofl-ledger` the two OFL ones.
>
> A font's licence applies to a build only when its feature is enabled.
> With no font feature enabled, the compiled crate contains no font data and
> is licensed purely under MIT OR Apache-2.0. If you enable one and
> distribute the result, distribute the matching licence file with it.
>
> The repository itself contains the font files (under `fonts/`, and Roboto
> atlases under `tests/fixtures/` that the tests use); they stay under their
> own licences there whether or not a feature is enabled.

A `## Bundled fonts` section follows it: the atlases are baked at build
time, what the build host needs (section 4.9), the `build-override` line
for Cranelift users, the consumer build-script example (section 4.6), and
how to verify a bake (`libmsdf-bake bundled --check`, the reference test).
The `## Architectural decisions` table stays last (Rule 4.1).

### 4.14 Instructions for agents (AGENTS.md)

A new section, `## Bundled fonts and atlases`, after Rule 5, in its own
repository-level commit with its own `CHANGES.md` addendum. It is not a
numbered critical rule. Text for the primary path:

> 1. Bundled atlases are **baked, never edited**. They are produced only by
>    `libmsdf::bake::Baker` (in `build.rs`, or by
>    `cargo run -p libmsdf-font --features bake --bin libmsdf-bake`).
>    Never hand-edit an atlas, never copy one in from another repository or
>    an older build, and never substitute one face's atlas for another's.
>    A test or build that needs a different atlas is a reason to stop and
>    ask, not to swap files.
> 2. When you change a TTF under `fonts/`, the bundled table
>    (`font/src/bundled.rs`) or anything in `libmsdf-font` that the bake
>    uses, run
>    `cargo test --features fonts-apache-roboto,fonts-ofl-ledger --test bundled_fonts`.
>    If `bundled_atlases_match_the_reference` fails, the bytes changed: run
>    `cargo run -p libmsdf-font --features bake --bin libmsdf-bake -- bundled --hashes`,
>    explain the change to the owner, and update `fonts/bundled.sha256`
>    only with the owner's agreement, in its own commit.
> 3. A TTF under `fonts/` is replaced only together with its sha256 in the
>    bundled table and its entry in `FONTS-LICENSE-*.txt`.
> 4. Bake output for inspection goes to `out/bundled/` (gitignored), never
>    into `fonts/`.

On the fallback path, item 1 reads "produced only by
`libmsdf-bake bundled --out fonts/atlases --hashes`". It adds: "run it, and
commit `fonts/atlases/` and `fonts/bundled.sha256` together, whenever item
2's conditions apply. Verify with `libmsdf-bake bundled --check fonts/atlases`
before committing, and with the reference test after."

## 5. Versioning (Rule 5)

This plan's changes are not strictly additive: the four `ROBOTO_*`
constants, present in every build today, disappear from a build without
their features, and `bundled_style_face` and `StyledShaper::bundled` return
less without them (Rule 5.4). On its own that is a minor bump.

The atlas layout tables plan already makes the next release 0.4.0. **Both
plans land in that one 0.4.0.** Its step 8 (version, `VERSIONS.md`, tag) is
held, on the owner's instruction ("hold the tag"), until this plan's steps 1
to 9 are done, and is carried out once as this plan's step 10. No external
dependency changes version here, so Rule 5.5 does not apply to libmsdf.
`libmsdf-font` is versioned in lockstep with libmsdf (both 0.4.0, one tag,
one `VERSIONS.md`; open question 2).

The 0.4.0 `VERSIONS.md` entry gains, beside the atlas layout tables plan's
items (its section 4.12, with the Format line):

- **Breaking:** `ROBOTO_REGULAR_ASCII`, `ROBOTO_BOLD_ASCII`,
  `ROBOTO_ITALIC_ASCII` and `ROBOTO_ASCII_MSYMBOLS` exist only with their
  `font-apache-roboto-*` feature and `shaper`. Migration: enable
  `fonts-apache-roboto`, or the single features used.
- **Breaking:** `bundled_style_face` returns `None`, and
  `StyledShaper::bundled` carries no face, for a style whose feature is
  off.
- Changed: the font module is the `libmsdf-font` crate, re-exported as
  `libmsdf::font` with every path unchanged; `cpu-bake` now enables
  `libmsdf-font/bake`.
- Changed (not API): `fonts/LICENSE-Roboto.txt` and
  `fonts/LICENSE-MaterialSymbols.txt` are replaced by
  `FONTS-LICENSE-APACHE.txt`.
- Added: the six `font-*` features and two `fonts-*` groups;
  `GRENZE_GOTISCH_TTF`, `TEXTURINA_TTF`; the seven `*_ATLAS` constants;
  `BUNDLED_ATLAS_GLYPH_SIZE`, `BUNDLED_ATLAS_PX_RANGE`; `libmsdf::bake`
  (`Baker`, `BakeOptions`, `LicenceSpec`, `BakeError`) under `cpu-bake`;
  `libmsdf::font::bundled` (`BundledAtlas`, `BundledFace`); the
  `libmsdf-bake` CLI.

## 6. Steps

Each step is one commit touching one crate, or a repository-level commit
where it says so (Rule 3.1), on the `impl/0.4.0` branch the atlas layout
tables plan uses. Every crate commit passes `cargo fmt -- --check`,
`cargo clippy --all-targets` with no new warnings, `cargo test` and
`cargo test --features fonts-apache-roboto,fonts-ofl-ledger` (from step 6
on, which bakes; before it, the second command builds without atlas
constants). Steps 1 and 2 can start once the atlas layout tables plan's
step 6 (the `shaper` feature) is in. Step 3 starts after its step 7, so the
crate split does not move files under work in progress.

1. **Features, gated TTF constants and test faces.** Add the six font
   features and two groups (as empty features for now); create
   `src/bundled.rs` with the four Roboto TTF constants and the gates of
   section 4.4; make `bundled_style_face` and `StyledShaper::bundled`
   feature-aware; add `TEST_FACE` and `tests/common/faces.rs` and switch the
   tests of section 3.1 to them; give `bake_atlas`'s path-less `--style` the
   error of section 4.6; add `tests/bundled_fonts.rs` with the TTF tests.
   **Done when:** `cargo test` with no font feature passes and runs every
   test that ran before, except those moved into `tests/bundled_fonts.rs`;
   `cargo test --features fonts-apache-roboto` passes and runs those too;
   with `font-apache-roboto-regular` alone, `bundled_style_face(Bold)` is
   `None` and `StyledShaper::bundled` shapes Regular and refuses Bold with
   `NoFaceForStyle`.

2. **Licence files and README (repository-level commit), then the OFL
   faces (crate commit).** First: `FONTS-LICENSE-OFL.txt`,
   `FONTS-LICENSE-APACHE.txt` and the README Licensing section (sections
   4.12, 4.13; the Bundled fonts section comes in step 7), with the
   Material Symbols copyright line read from the upstream variable font's
   `name` ID 0 and quoted in the commit message. Then: `git rm` the two
   `fonts/LICENSE-*.txt`, update the references, add the two OFL TTFs from
   google/fonts at a commit named in the commit message, and
   `GRENZE_GOTISCH_TTF`, `TEXTURINA_TTF` with tests. The licence files land
   first so the font files are never in the repository without them.
   **Done when:** both TTFs' sha256 equal section 3.1's; their `name` ID 0
   lines equal section 3.2's; with `fonts-ofl-ledger`, `TextShaper` over
   each shapes "Treasury 50G" to non-zero glyphs;
   `grep -rn "LICENSE-Roboto\|LICENSE-MaterialSymbols"` finds nothing
   outside `docs/design/` and `CHANGES.md`.

3. **The `libmsdf-font` crate.** Turn the repository into a workspace
   (root manifest: `[workspace] members = ["font"]`), `git mv src/font/*`
   to `font/src/`, move `LINE_BOX_RATIO`, and re-export from libmsdf
   (section 4.1). One commit adding the crate (Rule 3.1's exception), with
   libmsdf's switch to it in the same commit, since neither builds
   without the other.
   **Done when:** `cargo test --workspace` (and with `cpu-bake` and the
   GPU suite) passes unchanged; a new `tests/public_paths.rs` `use`s every
   public `libmsdf::` font path listed in `VERSIONS.md` 0.2.0 and 0.3.0 and
   in the atlas layout tables plan's section 4.12, and compiles;
   `cargo tree -p libmsdf --no-default-features` lists no rustybuzz,
   ttf-parser or msdfgen; `cargo check -p libmsdf-font --no-default-features
   --target wasm32-unknown-unknown` passes.

4. **The baker and the bundled table.** `font/src/bake.rs`
   (`Baker`, `BakeOptions`, `LicenceSpec`, `BakeError`),
   `font/src/bundled.rs` (`BundledAtlas`, `BundledFace` with the recorded
   sha256s), the `libmsdf-bake` CLI, `examples/bake_atlas.rs` as a wrapper,
   `libmsdf::bake` re-exported under `cpu-bake`, the exact msdfgen pins of
   section 4.8.
   **Done when:** `libmsdf-bake bundled --out out/bundled` bakes all seven,
   every equivalence check passing; a second run is byte-identical; the
   Roboto Regular output equals `tests/fixtures/roboto-ascii-48.atlas`; a
   TTF with one byte changed is refused with `FaceHash`; every atlas loads
   as 5:0.1.0 with layout for each style it carries and `LICS` records
   matching the table; the styled atlas has three layers and the same cell
   index (glyph ids and cell coordinates) as highbay's committed
   `roboto-msymbols-48.atlas`; for the OFL faces, every cell in the game's
   v3 atlases has identical texels in the new atlas
   (`FontAtlas::cell_texels`), and every added cell is a layout-closure
   glyph (such as Grenze's `grave.case`). All comparisons are logged with
   their lists under `out/fonts/` (Rule 3.5).

5. **Scripts (repository-level commit).** `scripts/bake-atlases.py` gains
   `--bundled` (runs `libmsdf-bake bundled` and prints its index delta
   against `--reference-dir` if given) and the uv check of section 4.6.
   `fonts/bundled.sha256` is recorded from step 4's bake for
   `aarch64-apple-darwin`, in the same commit.
   **Done when:** `scripts/bake-atlases.py --bundled` reproduces
   `fonts/bundled.sha256`, and the script fails with the uv message when
   `uv` is not on PATH.

6. **Build-time baking, the atlas constants and the switch decision.**
   `build.rs`'s `bake_bundled` (section 4.7), the
   `libmsdf-font-bake` build-dependency and the feature wiring of section
   4.2, and the seven atlas constants with the reference test of section
   4.8. Measure the five switch criteria of section 4.10 and log them under
   `out/fonts/LOG.md`.
   **Done when:** all five criteria hold and are logged, or the fallback
   of section 4.10 is implemented instead and its `CHANGES.md` addendum
   names the failed criterion; either way,
   `cargo test --features fonts-apache-roboto,fonts-ofl-ledger` passes with
   the reference test matching, and a build with no font feature runs no
   bake and builds no `libmsdf-font-bake`
   (`cargo build -vv` shows no `libmsdf-font` build-script dependency).

7. **Agent instructions and README (repository-level commits).** The
   AGENTS.md section of section 4.14 (primary or fallback text, as step 6
   decided), with its own `CHANGES.md` addendum; the README `## Bundled
   fonts` section (section 4.13), and the updated `cpu-bake` comment in
   `Cargo.toml` (its own repository-level commit, Rule 3.1).
   **Done when:** an agent following only the AGENTS.md section can
   verify the atlases (the reference test passes) and knows the one
   command that produces them; the README example builds when pasted into
   a scratch crate's `build.rs` (checked once, logged).

8. **Feature isolation checks (repository-level commit).**
   `scripts/check-font-features.sh` runs:
   - `cargo check` and `cargo check --no-default-features`;
   - for each of the six features and two groups,
     `cargo check --features <f>` and
     `cargo check --no-default-features --features <f>`;
   - `cargo check --target wasm32-unknown-unknown` and
     `--target aarch64-linux-android` with no features and with
     `fonts-apache-roboto,fonts-ofl-ledger` (with and without default
     features);
   - **the byte probe**: `cargo build --release --lib` per configuration,
     then a search of the produced `.rlib` and `.rmeta` for a 64-byte window
     of every bundled TTF and every baked atlas (the first window in each
     file with at least 32 distinct byte values). With no font feature, no
     window may be found. As positive controls, with all font features and
     default features every window must be found, and with
     `--no-default-features` and all font features every atlas window and
     no TTF window;
   - the determinism check of section 4.8 (two target directories).

   **Done when:** the script passes and its output is logged under
   `out/fonts/`. The positive controls find every window; if compiled
   `const` data turns out not to appear in the rlib or rmeta, the step
   stops and the probe target is raised with the owner before anything is
   weakened. A deliberately ungated `include_bytes!` (a local, uncommitted
   edit) makes the no-feature probe fail.

9. **Consumer builds.** As in the 0.3.0 landing: scratch copies of
   libcontrols, highbay, tinytrowel, sigilweaver and the tower defense game
   pointed at the branch, with the features of section 7 and, for the game,
   its `include_bytes!` replaced by the libmsdf constants. Nothing is
   committed in them.
   **Done when:** each builds and its tests pass, libcontrols with no font
   feature; the game's Android build succeeds; the game's ledger capture
   with libmsdf's atlases matches its capture with its own v3 atlases
   (same frame, or differences explained by added closure cells only),
   logged under `out/fonts/`; tinytrowel's and sigilweaver's bundled-face
   use and needed features are listed; the migration lines for 0.4.0 are
   written down.

10. **The joint 0.4.0 release.** This is the atlas layout tables plan's
    step 8, held until now. One commit sets `version = "0.4.0"` for
    libmsdf and `libmsdf-font` and adds the `VERSIONS.md` entry with that
    plan's section 4.12 items (and its Format line) and section 5's items
    here. `git tag v0.4.0` goes on that commit at once (Rule 5.1). Then
    each plan's `Implemented` status amendment and README row follow, in
    the commit that sets them (Rule 4.3).
    **Done when:** the tag `v0.4.0` exists on the commit that changes the
    version; the entry lists every public change named in both plans; the
    README table shows both plans as `Implemented`.

## 7. Consumers

- **libcontrols** uses no bundled face and enables no font feature. Its
  public API carries libmsdf types (`DrawList` in its `Control` trait), so
  when it takes 0.4.0 its own release bumps its minor version (Rule 5.5).
- **highbay** enables `fonts-apache-roboto`. It may later replace its own
  committed `crates/libhbui/assets/roboto-msymbols-48.atlas` with
  `ROBOTO_MSYMBOLS_STYLED_ATLAS`, the same bake at format 5. That is
  highbay's choice, not part of this plan.
- **The tower defense game** enables `fonts-ofl-ledger` and
  `font-apache-roboto-regular`, replaces its four `include_bytes!` with
  `GRENZE_GOTISCH_TTF`, `GRENZE_GOTISCH_ATLAS`, `TEXTURINA_TTF` and
  `TEXTURINA_ATLAS`, and deletes its copies of the two TTFs and two
  atlases. It **keeps its `FONTS-LICENSE-OFL.txt`**, because it still
  distributes the fonts inside its binary, with the paths changed to say
  the files come from libmsdf. Its binary also contains Roboto Regular, so
  it should distribute an Apache 2.0 notice too (libmsdf's
  `FONTS-LICENSE-APACHE.txt` entry for Roboto Regular is suitable). These
  are the game's commits, under its own rules, after 0.4.0 is tagged.
- **tinytrowel, sigilweaver**: the features matching whatever bundled
  constants they use (listed in step 9).

## 8. Open questions

1. **The `libmsdf-font` split (section 4.1).** It is what lets a build
   script, libmsdf's or a consumer's, call the baker. Public paths are
   unchanged, but the repository becomes a workspace and `src/font` moves.
   Approve it? If not, switch criterion 1 fails and the fallback applies:
   `Baker` lives in `libmsdf::bake` and the atlases are committed.
   Recommended: approve.
2. **`libmsdf-font`'s version.** Lockstep with libmsdf (one version, one
   tag, one `VERSIONS.md`), as section 5 proposes, or its own version line
   and entries? Recommended: lockstep while it is `publish = false` and
   has no consumer of its own other than build scripts.
3. **Subset the OFL faces?** Unmodified files (section 4.3) keep provenance
   trivial. Subsetting to `TEXT_RANGES` with a `fonts/ofl.py` in the style
   of `style.py` would save about 350 KB of TTF in a `shaper` build and
   make them Modified Versions under the OFL (allowed: no Reserved Font
   Name). Recommended: unmodified now.
4. **Optional features in the bundled atlases.** Defaults only (section
   4.5), with other choices left to a consumer's own `Baker::bake_face`, or
   bake Grenze with `tnum` for the ledger? Recommended: defaults only, as
   the game uses today.
5. **Reference hashes for other hosts.** `fonts/bundled.sha256` starts with
   `aarch64-apple-darwin` only, and float output across architectures is
   unverified. Record `x86_64-unknown-linux-gnu` too, from a Linux machine,
   before 0.4.0, or accept the "no reference recorded" pass on other hosts?
   Recommended: record it in step 6 when criterion 3 is measured on Linux.
6. **Standalone Bold and Italic atlases.** Ship them as planned (each
   feature brings an atlas), or let Bold and Italic bring only their TTF
   and their layers of the styled atlas? Baking at build time removes the
   repository cost; a consumer pays the bake time and binary size only for
   what it enables. Recommended: ship them.

## Amendment — 2026-10-16 13:34 EDT: owner approval; the OFL faces are subset

The owner's decision, relayed by the coordinating agent: "approve with the
recommendations ... subset for now ascii, east/west europe and common
symbols". The plan is approved, with the six open questions of section 8
answered as below. Its status stays `Draft`: under Rule 1.5, the first
commit that implements it sets `In implementation`.

**What this replaces.** Section 8 has no open questions left. Decision 3
(open question 3) is reversed: the OFL faces are subset, not shipped
unmodified. That replaces:

- the subsetting paragraph of section 4.3, and the two OFL lines of its file
  list;
- the sentence in section 3.1 "The game's copies, which libmsdf's must
  equal byte for byte": those hashes are now the **upstream** hashes,
  recorded for provenance;
- the OFL lines of the bundled table and of "What each atlas is" (sections
  4.4 and 4.5);
- the OFL entries of `FONTS-LICENSE-OFL.txt` (section 4.12);
- step 2's second commit and its **Done when:**.

Everything else stands.

### Answers

1. **The `libmsdf-font` split: approved** (section 4.1). Switch criterion 1
   of section 4.10 holds.
2. **`libmsdf-font` is versioned in lockstep** with libmsdf: one version,
   one tag, one `VERSIONS.md` (section 5).
3. **The OFL faces are subset** to the set below (the rest of this
   amendment).
4. **No `tnum`.** The bundled atlases are baked with the default features
   only (section 4.5). The subset TTFs keep every layout feature (below), so
   a consumer can still bake `tnum` with its own `Baker::bake_face`.
5. **Linux reference hashes.** `fonts/bundled.sha256` gains
   `x86_64-unknown-linux-gnu` lines in step 6, measured on Linux when switch
   criterion 3 is. Step 6's **Done when:** gains: "`fonts/bundled.sha256`
   has lines for `aarch64-apple-darwin` and `x86_64-unknown-linux-gnu`, each
   from a bake on that host". The "no reference recorded" pass of section
   4.8 remains only for other hosts.
6. **Standalone Bold and Italic atlases ship** as section 4.4 says.

### The subset coverage: `EUROPEAN_TEXT_RANGES`

A new constant in `libmsdf-font`, beside `TEXT_RANGES` (which does not
change and stays the Roboto faces' coverage):

```rust
/// The text coverage of the bundled OFL faces (Grenze Gotisch, Texturina):
/// Western and Central/Eastern European Latin plus common symbols.
pub const EUROPEAN_TEXT_RANGES: &[(char, char)] = &[
    ('\u{0020}', '\u{007E}'), // Basic Latin, printable
    ('\u{00A0}', '\u{00FF}'), // Latin-1 Supplement
    ('\u{0100}', '\u{017F}'), // Latin Extended-A
    ('\u{2010}', '\u{2027}'), // General Punctuation: hyphens, figure dash,
                              // en/em dash, horizontal bar, double vertical
                              // line, quotes (single, double, low, reversed),
                              // dagger, double dagger, bullet, triangular
                              // bullet, leaders, ellipsis, hyphenation point
    ('\u{2030}', '\u{203A}'), // per mille, per ten thousand, primes,
                              // reversed primes, caret, single guillemets
    ('\u{20AC}', '\u{20AC}'), // euro sign
    ('\u{2122}', '\u{2122}'), // trade mark sign
];
```

That is 356 codepoints: 95 + 96 + 128 + 24 + 11 + 1 + 1. Dagger (U+2020),
double dagger (U+2021) and bullet (U+2022) are inside U+2010–2027. U+2028
and U+2029 (line and paragraph separators) and U+202A–202F (bidi controls,
narrow no-break space) are deliberately outside the range.

**What each face maps (measured 2026-10-16, fontTools 4.67.0):**

- **Grenze Gotisch: 340 of 356.** Unmapped: U+2012, U+2015, U+2016,
  U+2017, U+201B, U+201F, U+2023, U+2024, U+2025, U+2027, U+2031, U+2034,
  U+2035, U+2036, U+2037, U+2038.
- **Texturina: 338 of 356.** Unmapped: the same sixteen, plus U+017F
  (long s) and U+2011 (non-breaking hyphen).

An unmapped codepoint shapes to the face's glyph 0 and draws its `.notdef`,
as any codepoint outside a face's coverage does today. The range list stays
whole rather than being trimmed per face, so one constant describes both
faces and a later upstream release that adds glyphs fills them in.

**What fires on the set (uharfbuzz 0.56.3, `lnum` and `pnum` on,
every codepoint alone, after `A`, and doubled):** in both faces, only the
`f_f` ligature (and the other `liga` forms), the ten lining figures
(`zero.lf` to `nine.lf`), and the soft hyphen becoming the space glyph.
There is no decomposition, no change in glyph count other than ligatures,
and no GDEF mark glyph produced, and none of the 356 codepoints is a GDEF
mark. So the Latin Extended-A widening stays inside what the atlas layout
tables can carry. fontTools' closure lists combining marks and `locl`
forms, but only through contextual and language-specific rules that do not
fire without a language system; the bake's equivalence check is the
authority on that.

### How the subset is made

A new script, `fonts/ofl_subset.py`, in the style of `fonts/style.py`: run
with `uv run --with fonttools==4.67.0`, Python 3.11 or later; it stops with
a clear message if `uv` is missing.

```text
./fonts/ofl_subset.py <upstream.ttf> <out.ttf> <expected-upstream-sha256>
```

1. It checks the upstream file's sha256 against the expected value and
   refuses a mismatch.
2. It subsets with the fontTools subsetter, the equivalent of:

   ```text
   pyftsubset <upstream.ttf> \
     --unicodes="U+0020-007E,U+00A0-017F,U+2010-2027,U+2030-203A,U+20AC,U+2122" \
     --layout-features='*' --glyph-names --name-IDs='*' --name-languages='*' \
     --notdef-outline --no-hinting --drop-tables+=gasp,DSIG
   ```

   - **Every layout feature is kept** (`'*'`), so `ccmp`, `locl`, `liga`,
     `kern`, `lnum`, `pnum`, `tnum` (Grenze) and the rest stay usable, and
     their glyph closure stays in the file.
   - **Glyph names and the whole `name` table are kept.**
   - **Hinting is dropped**, following section 4.4 of the atlas layout
     tables plan, which carries no virtual-machine data: `--no-hinting`
     removes `prep`, `fpgm`, `cvt ` and per-glyph instructions. `gasp`
     (rasteriser flags) and `DSIG` (a signature the modification would
     invalidate) are dropped too.
   - **The variation tables are kept** (`fvar`, `gvar`, `HVAR`, `STAT`,
     and `avar` for Texturina), so the files remain the variable fonts; the
     atlas is baked from the default instance as before.
   - The `head` timestamp is not recalculated, so the output is
     reproducible. Measured: two runs on Texturina gave byte-identical files
     (sha256 `a2a29082...`).
3. It then sets two `name` records, per OFL good practice for a Modified
   Version, with fixed strings so the bytes stay reproducible:
   - ID 5 (version): the upstream string followed by
     `"; libmsdf subset (U+0020-017F, punctuation)"`;
   - ID 10 (description):
     `"Subset by libmsdf to Latin (U+0020-017F) and common punctuation and symbols; outlines, metrics and layout unchanged."`.
4. **Provenance check.** For every glyph kept, the outline and advance must
   equal the upstream file's, and every kept codepoint must map to the same
   glyph name. Otherwise the script aborts, as `widen.py` and `style.py` do.
5. It prints the output's sha256.

The upstream files are build inputs, not committed. They are fetched to the
gitignored `fonts/upstream/` from google/fonts at a commit named in
`FONTS-LICENSE-OFL.txt`, the way `.gitignore` already treats the Material
Symbols input.

**Measured sizes** (fontTools 4.67.0, before the two `name` edits, which add
a few hundred bytes):

| Face | upstream | subset |
|---|---|---|
| Grenze Gotisch | 193,492 bytes, 1,111 glyphs | 127,732 bytes, 643 glyphs |
| Texturina | 288,320 bytes, 722 glyphs | 204,204 bytes, 454 glyphs |

This replaces section 8's estimate of "about 350 KB" saved: the saving is
about 150 KB for both faces together. Most of what remains is the variation
data.

### Files and licence

- The committed TTFs are the subsets:
  `fonts/GrenzeGotisch[wght]-subset.ttf` and
  `fonts/Texturina[opsz,wght]-subset.ttf`. The suffix keeps them from being
  mistaken for upstream bytes. `GRENZE_GOTISCH_TTF` and `TEXTURINA_TTF`
  include them, with names unchanged.
- `BundledFace` gains `upstream_sha256: Option<[u8; 32]>`, and the bundled
  table records both hashes: the subset's (checked before every bake) and
  the upstream's (from section 3.1, checked by `ofl_subset.py`). The
  upstream values are those in section 3.1: Grenze `701b299d...`,
  Texturina `478a15d7...`.
- **This is an OFL Modified Version, which the OFL allows.** Neither face
  declares a Reserved Font Name, so the family names may stay. Each OFL
  entry in `FONTS-LICENSE-OFL.txt` gains:

  ```text
    Modified Version: subset by libmsdf (fonts/ofl_subset.py) to
    U+0020-007E, U+00A0-017F, U+2010-2027, U+2030-203A, U+20AC, U+2122;
    hinting, gasp and DSIG removed; outlines, metrics, layout tables and
    variation data otherwise unchanged.
    Upstream: <google/fonts commit>, sha256 <upstream sha256>
    This file: sha256 <subset sha256>
  ```

  and its file line names the `-subset.ttf` file. The copyright lines do
  not change.

### Coverage and the baked atlases

- **The atlas covers the same set.** `BakeOptions` gains
  `coverage: Coverage`. `Coverage::Shipped` is today's
  `add_shipped_coverage`: `TEXT_RANGES` plus whatever the face defines in
  the Private Use Area, used by the Roboto rows. `Coverage::Ranges(&[(char,
  char)])` queues exactly those ranges (what the face maps of them) plus the
  shaped closure, and is used by the OFL rows with `EUROPEAN_TEXT_RANGES`.
- **It does not fit in one 320-cell layer.** Grenze maps 340 codepoints and
  Texturina 338; the shaped closure adds the `liga` forms (5), the lining
  figures (10), glyph 0 and, for Grenze, its contextual `.case` accents. So
  each needs about 355 to 370 cells against `atlas_capacity()` = 320
  (8 columns × 40 rows). `ATLAS_ROWS` says growth is "layers, or columns;
  never rows", and `ATLAS_COLS` says "16 columns at the same 40 rows is
  800x2000 - inside the same floor - and holds 640 cells". `BakeOptions`
  therefore gains `columns: u32` (default `ATLAS_COLS`, 8), and
  `FontAtlasBuilder` gains `with_columns(16)`, with capacity per layer
  `columns × ATLAS_ROWS`. The two OFL atlases are baked at 16 columns; the
  Roboto atlases stay at 8, so their bytes and the test fixture do not
  change. No format change is needed: the width is in the header, glyph
  entries hold texel coordinates, and `sdf_render.wgsl` normalises by
  `textureDimensions`.
- **Size.** 16 columns of 50-texel padded cells by 40 rows is 800 × 2000
  texels, so 4,800,000 bytes of 3-channel pixel data per OFL atlas, plus
  about 12 KB of glyph table and the layout tables (measured in step 4).
  That is twice a one-layer Roboto atlas.
- **Merging.** A consumer that stacks atlases into one texture array, as the
  tower defense game does, needs equal dimensions. libmsdf gains
  `FontAtlas::padded_to(&self, width: u32, height: u32) -> Result<FontAtlas, &'static str>`
  (additive). It pads every layer on the right and bottom with empty texels,
  leaving every glyph's texel coordinates, and so its rendering, unchanged.
  It refuses dimensions smaller than the atlas. Step 4's **Done when:**
  gains: "an 8-column Roboto atlas padded to 800 × 2000 draws the same
  `DrawList` frame as unpadded (golden render)".
- **Layout tables over the wider set.** The atlas layout tables plan defines
  the `Cmap` scope (its section 4.6) and the equivalence corpus (its section
  4.9) over `TEXT_RANGES`. For these faces both must be the bake's declared
  coverage: `Cmap` holds every codepoint of `EUROPEAN_TEXT_RANGES` the face
  maps, and the corpus pairs run over those codepoints. That is about
  115,000 pair strings per face instead of 40,401, so switch criterion 2
  measures it. This means `FaceLayout::extract` / `LayoutOptions` take the
  coverage as a parameter. That changes an interface of the atlas layout
  tables plan, which is frozen and whose step 3 is in progress, so under
  Rule 1.7 it is raised with the owner. It is recorded in `CHANGES.md` once
  agreed, before that plan's step 3 fixes the scope. With `TEXT_RANGES` as
  the default, nothing in that plan's Roboto results changes.

### Steps affected

- **Step 2, second commit** (replacing its OFL part): add
  `fonts/ofl_subset.py`, the two `-subset.ttf` files made by it from the
  upstream files of section 3.1, `.gitignore`'s `fonts/upstream/` entry,
  and `GRENZE_GOTISCH_TTF` / `TEXTURINA_TTF`.
  **Done when** (replacing step 2's): the upstream inputs' sha256 equal
  section 3.1's; running `ofl_subset.py` twice gives byte-identical files
  whose sha256 are recorded in the bundled table; its provenance check
  passes; each subset maps exactly the codepoints listed above for its face;
  its `name` ID 0 line equals section 3.2's and IDs 5 and 10 carry the
  modification strings; it has no `prep`, `fpgm`, `cvt `, `gasp` or `DSIG`;
  with `fonts-ofl-ledger`, `TextShaper` over each shapes "Treasury 50G",
  "Łódź", "Œuvre", "Šťastný" and "‹ 25 ‰ ›" to non-zero glyphs, and
  shapes the full set exactly as over the upstream file (glyph names,
  advances and positions); and
  `grep -rn "LICENSE-Roboto\|LICENSE-MaterialSymbols"` finds nothing
  outside `docs/design/` and `CHANGES.md`.
- **Step 4** gains `Coverage`, `columns`, `with_columns`,
  `EUROPEAN_TEXT_RANGES` and `FontAtlas::padded_to`. Its OFL comparison
  with the game's v3 atlases becomes: every glyph in both has identical
  texels (`FontAtlas::cell_texels`; coordinates differ at 16 columns), and
  the cells added are exactly the newly covered codepoints and the layout
  closure, logged with lists.
- **Step 9:** the game's ledger capture is compared as before. The game's
  merge pads the Roboto atlas with `padded_to` before stacking.
- **Section 5** gains, under Added: `EUROPEAN_TEXT_RANGES`, `Coverage`,
  `BakeOptions::{coverage, columns}`, `FontAtlasBuilder::with_columns`,
  `FontAtlas::padded_to`, `BundledFace::upstream_sha256`.

## Amendment — 2026-10-16 13:40 EDT: the coverage parameter and the TEXT, SYMBOLS and MATH groups

The owner's decision, relayed by the coordinating agent: "yes, add the
coverage parameter but symbols and text have different ranges, common
symbols are extended ascii and unicode with common european symbols and
math".

### 1. The coverage parameter: an agreed departure from the atlas layout tables plan

The owner agrees to the departure the 13:34 EDT amendment raised (Rule
1.7). In the atlas layout tables plan, three things were hard-coded to
`TEXT_RANGES` and now take a coverage parameter:

- the layout extraction (`LayoutOptions`, `FaceLayout::extract`);
- the `Cmap` scope (its section 4.6);
- the equivalence corpus (its section 4.9).

The `Cmap` holds every codepoint of the bake's declared coverage that the
face maps, plus the C0/C1 controls the face maps, as before. The corpus's
single codepoints and ordered pairs run over those `Cmap` codepoints. The
interface change is one public field:

```rust
pub struct LayoutOptions {
    pub extra_features: Vec<[u8; 4]>,
    /// The declared text coverage: inclusive codepoint ranges. Defaults to
    /// TEXT_RANGES, so every existing caller and result is unchanged.
    pub coverage: Vec<(char, char)>,
}
```

with `Default` giving `TEXT_RANGES`. `pub(crate) fn cmap_scope()` becomes
`cmap_scope(&[(char, char)])`. The Roboto faces' extracted tables and check
results do not change.

**This changes committed code.** That plan's step 3 (`7c00f63`, bake
extraction, with `cmap_scope()` over `TEXT_RANGES` in
`src/font/layout_bake.rs`) and step 4 (`6d18eab`, `AtlasShaper`) were both
committed before this decision. The change is its own commit, titled
"Atlas layout tables: coverage parameter (departure, see CHANGES.md)":

- It lands after that plan's step 5 (the equivalence check) is committed
  and before this plan's step 4.
- If step 5 is not yet committed when this is picked up, it takes the
  parameter directly.
- Its test: the Roboto Regular tables extracted with the default coverage
  equal those of `7c00f63`; with `coverage = TEXT_RANGES` minus U+2026,
  `Cmap` lacks U+2026 and the check still passes.

**Correction to the 13:34 EDT amendment.** Its sentence "It is recorded in
`CHANGES.md` once agreed, before that plan's step 3 fixes the scope" was out
of date when written: step 3 (`7c00f63`) and step 4 (`6d18eab`) were already
committed. The scope was fixed in committed code and is changed by the
commit above. Its "raised with the owner" is settled by this amendment.

### 2. Coverage groups

Coverage is declared as named groups with different ranges and different
rules for a codepoint the face lacks. This replaces the 13:34 EDT
amendment's `EUROPEAN_TEXT_RANGES` and its `Coverage::Ranges` (both
withdrawn) and its subset codepoint list. `TEXT_RANGES`, the Roboto faces'
coverage, is unchanged, and the Roboto rows keep `Coverage::Shipped`.

In `libmsdf-font`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum CoverageGroupId { Text = 0, Symbols = 1, Math = 2 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingPolicy {
    /// A codepoint the face lacks fails the bake, unless waived for that face.
    Fail,
    /// A codepoint the face lacks is skipped and listed in the bake report.
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverageGroup {
    pub id: CoverageGroupId,
    pub ranges: &'static [(char, char)],
    pub missing: MissingPolicy,
}

pub const COVERAGE_TEXT: CoverageGroup;     // Fail
pub const COVERAGE_SYMBOLS: CoverageGroup;  // Skip
pub const COVERAGE_MATH: CoverageGroup;     // Skip

pub enum Coverage {
    /// TEXT_RANGES plus what the face defines in the Private Use Area:
    /// today's add_shipped_coverage, for the Roboto faces.
    Shipped,
    /// The union of these groups.
    Groups(Vec<CoverageGroup>),
}
```

Groups may overlap: SYMBOLS and MATH share the Latin-1 and General
Punctuation signs that are both, such as `±`, `×`, `°` and `‰`. A codepoint
in TEXT and in another group follows TEXT's rule. `BakeOptions` keeps
`coverage: Coverage` and gains
`waive: Vec<(GlyphStyle, char, &'static str)>`: a TEXT codepoint the face
lacks, with the reason, which turns that one failure into a listed waiver.

**`COVERAGE_TEXT`**: letters with their digits, punctuation and spaces, for
Western and Central/Eastern European languages. 287 codepoints:

```text
U+0020-007E   Basic Latin, all printable (letters, digits, ASCII punctuation
              and signs)
U+00A0        no-break space
U+00AD        soft hyphen
U+00C0-00D6   Latin-1 letters
U+00D8-00F6   Latin-1 letters
U+00F8-00FF   Latin-1 letters
U+0100-017F   Latin Extended-A
```

**`COVERAGE_SYMBOLS`**: common symbols, being the extended-ASCII (Latin-1)
symbols and the common European Unicode ones. 63 codepoints:

```text
U+00A1-00AC   ¡ ¢ £ ¤ ¥ ¦ § ¨ © ª « ¬
U+00AE-00BF   ® ¯ ° ± ² ³ ´ µ ¶ · ¸ ¹ º » ¼ ½ ¾ ¿
U+00D7 U+00F7 × ÷
U+2010-2015   hyphen, non-breaking hyphen, figure dash, en dash, em dash,
              horizontal bar
U+2018-201F   ‘ ’ ‚ ‛ “ ” „ ‟
U+2020-2022   † ‡ •
U+2026        …
U+2030        ‰
U+2032-2033   ′ ″
U+2039-203A   ‹ ›
U+20AC        € euro
U+20B4        ₴ hryvnia (Ukraine)
U+20BA        ₺ lira (Turkey)
U+20BD        ₽ ruble (Russia)
U+20BE        ₾ lari (Georgia)
U+2116        № numero sign (used in Central and Eastern Europe)
U+2122        ™
U+212E        ℮ estimated sign (EU prepackaging)
```

The currencies are the ones in current use in Europe that Latin-1 lacks:
`£`, `¥`, `¢` and the generic `¤` are already in Latin-1, and the obsolete
franc and lira signs are left out. Left out of General Punctuation, as
rarely used and missing from both faces: U+2016, U+2017, U+2023 to U+2025,
U+2027, U+2031 and U+2034 to U+2038.

**`COVERAGE_MATH`**: common mathematical signs. 78 codepoints:

```text
U+00AC U+00B0-00B3 U+00B5 U+00B7 U+00B9 U+00BC-00BE U+00D7 U+00F7
              ¬ ° ± ² ³ µ · ¹ ¼ ½ ¾ × ÷
U+2030 U+2032-2033   ‰ ′ ″
U+2070 U+2074-207F   superscript 0, 4-9, + − = ( ) n
U+2080-208E   subscript 0-9, + − = ( )
U+2153-215E   ⅓ ⅔ ⅕ ⅖ ⅗ ⅘ ⅙ ⅚ ⅛ ⅜ ⅝ ⅞
U+2202        ∂
U+2205-2209   ∅ ∆ ∇ ∈ ∉
U+220F        ∏
U+2211-2213   ∑ − ∓
U+221A U+221E √ ∞
U+2229-222B   ∩ ∪ ∫
U+2248 U+2260 ≈ ≠
U+2264-2265   ≤ ≥
U+2282-2283   ⊂ ⊃
```

The ASCII signs (`+ < = > ^ |`) are in TEXT. **U+2044 FRACTION SLASH is
deliberately excluded.** HarfBuzz, and rustybuzz with it, applies automatic
`numr`/`dnom` fractions to the digits around it. Measured in both faces,
"1⁄2" shapes to `one.numr fraction two.dnom` and "12⁄34" to two numerators,
the slash and two denominators. That is contextual on digit runs, which the
atlas layout tables cannot express, so the bake would fail its equivalence
check. Including it would need an automatic-fraction rule in `AtlasShaper`,
a further format change that is not proposed here. The precomposed
fractions (`½`, `⅛`, ...) are single glyphs and are unaffected.

The bundled OFL rows use
`Coverage::Groups(vec![COVERAGE_TEXT, COVERAGE_SYMBOLS, COVERAGE_MATH])`.
Texturina has the waiver
`(Regular, 'ſ', "long s (U+017F): Texturina has no glyph; archaic")`.

**Measured coverage** (fontTools 4.67.0, upstream files of section 3.1):

| Group | Grenze Gotisch | Texturina |
|---|---|---|
| TEXT (287) | 287, complete | 286: lacks U+017F (waived) |
| SYMBOLS (63) | 57: lacks U+2012, U+2015, U+201B, U+201F, U+20B4, U+20BE | 55: lacks U+2011, U+2012, U+2015, U+201B, U+201F, U+20B4, U+20BE, U+212E |
| MATH (78) | 48: lacks U+207A-207E, U+208A-208E, U+2153-215E, U+2207-2209, U+2213, U+2229, U+222A, U+2282, U+2283 | 50: lacks U+207A-207F, U+208A-208E, U+2153-215A, U+2205, U+2207-2209, U+2213, U+2229, U+222A, U+2282, U+2283 |

Neither face makes any codepoint of the three groups a GDEF mark. Shaping
every mapped codepoint alone, after `A`, doubled and between digits
(uharfbuzz 0.56.3, `lnum` and `pnum`) changes nothing beyond what the 13:34
EDT amendment found: the `liga` forms, the lining figures and the soft
hyphen. For example, "x²+H₂O", "2−5", "№ 5" and "5 €" map glyph for glyph.
The only exception was U+2044, excluded above.

### 3. What the atlas records

- **`Cmap`** carries exactly the codepoints the atlas covers (section 1).
- **A new all-capital libmsdf table, `CVRG`**, per face (`face` = style
  ordinal), records the declared groups and what was missing. It is absent
  for `Coverage::Shipped`.
  - It is a new table rather than new `Head` fields, because `Head`'s
    layout is already in the committed codec (`f15a308`). A table that a
    reader does not know is skipped under the atlas layout tables plan's
    section 4.2, and `Cmap` alone cannot tell "declared but missing" from
    "not declared".
  - `CVRG` is not a registered OpenType tag. It is added to format 5:0.1.0
    before that version is released, so the format version does not move.

```text
0   2  group_count u16
2   2  reserved 0
then group_count groups, ascending by id, each:
0   1  group id      u8: 0 TEXT, 1 SYMBOLS, 2 MATH (a reader keeps an unknown
                     id raw)
1   1  policy        u8: 0 fail, 1 skip
2   2  range_count   u16
4   2  missing_count u16
6   2  reserved 0
8      ranges   [lo u32, hi u32] x range_count, ascending, non-overlapping
       missing  [codepoint u32, kind u8 (0 skipped, 1 waived), 3 reserved]
                x missing_count, ascending
```

`FontAtlas` gains `pub coverage: Vec<FaceCoverage>` (one per face that has
a `CVRG`) and `FontAtlas::coverage(style) -> Option<&FaceCoverage>`, where
`FaceCoverage { style, groups: Vec<GroupCoverage> }` and
`GroupCoverage { id, policy, ranges, skipped: Vec<char>, waived: Vec<char> }`.
Reading checks that every codepoint in `Cmap` lies in a declared range, and
that no missing codepoint is in `Cmap`.

### 4. Missing codepoints and the bake report

- **TEXT** (`MissingPolicy::Fail`): a codepoint the face lacks fails the
  bake with
  `BakeError::MissingText { style, codepoints: Vec<char> }`, listing all of
  them, unless `BakeOptions::waive` names that style and codepoint.
- **SYMBOLS, MATH** (`MissingPolicy::Skip`): a codepoint the face lacks is
  skipped, recorded in `CVRG` and listed in the bake report.
- `Baker::bake_face_report(ttf, opts) -> Result<(Vec<u8>, BakeReport), BakeError>`
  is added. `bake_face` and `bake_bundled` call it and return the bytes.

```rust
pub struct BakeReport { pub faces: Vec<FaceReport> }
pub struct FaceReport { pub style: GlyphStyle, pub groups: Vec<GroupReport> }
pub struct GroupReport {
    pub id: CoverageGroupId,
    pub declared: usize,
    pub carried: usize,
    pub skipped: Vec<char>,
    pub waived: Vec<(char, &'static str)>,
}
```

`BakeReport` implements `Display`: one line per group, then the skipped and
waived codepoints as `U+XXXX`. `build.rs` writes it to
`OUT_DIR/bundled/<atlas>.report.txt`; the `libmsdf-bake` CLI prints it; the
`bundled` subcommand writes it beside each atlas under `out/bundled/`.

### 5. The subset and the cells, recomputed

The subset TTFs keep TEXT ∪ SYMBOLS ∪ MATH. That is 411 codepoints, which
replaces the 13:34 EDT amendment's 356-codepoint set and its `--unicodes`
argument:

```text
--unicodes="U+0020-007E,U+00A0-017F,U+2010-2015,U+2018-2022,U+2026,U+2030,U+2032-2033,U+2039-203A,U+2070,U+2074-208E,U+20AC,U+20B4,U+20BA,U+20BD-20BE,U+2116,U+2122,U+212E,U+2153-215E,U+2202,U+2205-2209,U+220F,U+2211-2213,U+221A,U+221E,U+2229-222B,U+2248,U+2260,U+2264-2265,U+2282-2283"
```

All other subsetting options, the name edits, the provenance check and the
file names stay as the 13:34 EDT amendment says. `ofl_subset.py` takes the
ranges from the same three group definitions, read from a generated list
that a test compares with the Rust constants, so there is one source.
Compared with the 13:34 EDT set, the subset drops U+2016-2017, U+2023-2025,
U+2027, U+2031 and U+2034-2038 (neither face maps any of them). It adds the
MATH signs, the four currencies, `№` and `℮`.

**Measured** with fontTools 4.67.0, using the same options plus U+2044,
which is now excluded; removing it changes the files only slightly, and
step 2 records the final sizes:

| Face | upstream | subset |
|---|---|---|
| Grenze Gotisch | 193,492 bytes, 1,111 glyphs | 132,048 bytes, 659 glyphs |
| Texturina | 288,320 bytes, 722 glyphs | 213,648 bytes, 467 glyphs |

**Cells.** Mapped codepoints without U+2044: Grenze 375, Texturina 374.
Adding the shaped closure that fires (the `liga` forms, ten lining figures,
and Grenze's contextual `.case` accents) and glyph 0 gives roughly 390 to
400 cells per face. That is still more than one 8-column layer (320) and
well inside 16 columns (640), so the 13:34 EDT amendment's 16-column bake,
800 × 2000 texels and about 4.8 MB per OFL atlas, stands. The exact counts
are logged in step 4. The equivalence corpus grows to about 140,000 pair
strings per face, which switch criterion 2 measures.

### Steps affected

- **The coverage-parameter commit** of section 1, before step 4.
- **Step 2**: its 13:34 EDT **Done when:** reads, for "each subset maps
  exactly the codepoints listed above for its face", the codepoints of
  TEXT ∪ SYMBOLS ∪ MATH that the face maps, per the table in section 2
  above. Its shaping strings gain "x² H₂O", "½ ⅛", "2−5 ≤ 7", "№ 5" and
  "5 € ₽".
- **Step 4**: adds `CoverageGroupId`, `MissingPolicy`, `CoverageGroup`,
  the three `COVERAGE_*` constants, `Coverage::Groups`, `BakeOptions::waive`,
  `BakeError::MissingText`, `BakeReport` and `Baker::bake_face_report`, the
  `CVRG` codec and `FontAtlas::coverage`. **Done when** gains:
  - each OFL atlas's `CVRG` and bake report list exactly the missing
    codepoints of the table in section 2;
  - removing Texturina's waiver fails its bake with
    `MissingText { codepoints: ['ſ'] }`;
  - a `CVRG` whose missing codepoint is in `Cmap` is refused on read.
- **Section 5**, Added, gains the same items. `EUROPEAN_TEXT_RANGES` and
  `Coverage::Ranges` are removed from it.

### Owner approval after the highbay review

The owner, relayed by the coordinating agent: "approve libmsdf plan after
quick review of highbay usage but it will adapt, its custom symbols aren't
font sourced". The plan, with this amendment, is approved. Its status stays
`Draft` until its first implementing commit (Rule 1.5).

The coordinating agent's read-only review of highbay found:

- It uses `ROBOTO_REGULAR_ASCII` and `ROBOTO_ASCII_MSYMBOLS` with
  `TextShaper`, so it enables `fonts-apache-roboto` with `shaper` (the
  default), as section 7 already says.
- It uses `MSYMBOLS_ICONS`, `HIGHBAY_ICONS`, `highbay_codepoint` and
  `MARKER_ARROW`. These are codepoint tables for symbols drawn by libmsdf's
  own font scripts, not font bytes. They stay ungated by the font features,
  as section 4.4 says of the manifests and resolvers. `MARKER_ARROW` joins
  that list explicitly.
- It uses `TEXT_RANGES` in a few tests, which is unchanged.

highbay therefore adapts with feature lines in its manifest only. Its
custom symbols are not sourced from the bundled fonts, so the coverage
groups do not affect them.

## Amendment — 2026-10-16 13:42 EDT: the owner confirms `CVRG`

The owner confirmed, relayed by the coordinating agent: "keep CVRG". The
separate all-capital `CVRG` table of the 13:40 EDT amendment, section 3,
stands as written. Coverage groups and missing codepoints are not folded
into `Head`. The amendment's text carried no open caveat on this point;
this note records the confirmation, which was asked for when it was
reported. Nothing else changes.

## Amendment — 2026-10-16 13:55 EDT: implementation started; the document is immutable

The owner approved this plan with its 13:34, 13:40 and 13:42 EDT
amendments, and the coordinating agent extended the implementing agent's
scope to it on the owner's approval (2026-10-16). Implementation starts
with step 1 on the branch `impl/0.4.0`, after the atlas layout tables
plan's steps 1 to 7, and the status is now `In implementation`. From here
the document is immutable (Rule 1.5): departures are recorded in
`CHANGES.md`, and only the owner's explicit instruction changes it.
