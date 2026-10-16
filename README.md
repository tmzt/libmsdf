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

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

## Architectural decisions

| Document | Date | Status | Decision |
|---|---|---|---|
| [Filled Bézier paths as an SDF primitive](docs/design/20261015_01_FILLED_BEZIER_PATH_PLAN.md) | 2026-10-15 | Implemented | Adds `SdfKind::PathFill`: closed line/quadratic/cubic paths filled by an exact even-odd signed distance, segments in the aux param bank; every step done, released as 0.3.0 (`v0.3.0`). |
| [Atlas-carried layout tables and a shaper without rustybuzz](docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md) | 2026-10-16 | In implementation | Atlas format 5 (split, version-gated field) with tagged `Head`/`Cmap`/`Hmtx`/`Gsub`/`Gpos`, `LICS` and `LICT` tables, no VM data, and a pure-Rust `AtlasShaper` proven equal to rustybuzz at bake; approved by the owner, in implementation on `impl/0.4.0`. |
| [Bundled fonts as optional, licence-named features, baked at build time](docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md) | 2026-10-16 | Draft | Every bundled face (four Roboto, Apache-2.0; Grenze Gotisch and Texturina, OFL-1.1) becomes an off-by-default feature named with its licence (`font-apache-*`, `font-ofl-*`), its atlas baked in `build.rs` by a public `libmsdf::bake::Baker` (fallback: committed atlases), so with no font feature the crate is purely MIT OR Apache-2.0; OFL faces subset to Latin U+0020–017F plus common punctuation and symbols (16-column atlases); lands in the joint 0.4.0 whose tag is held; approved by the owner, Draft until its first implementing commit (Rule 1.5). |
