# CHANGES

Planned new work, feature plans and architectural changes, appended in order
(AGENTS.md, Rule 0). Entries are never edited.

## 2026-10-15 23:51 EDT — Filled Bézier path plan

New plan `docs/design/20261015_01_FILLED_BEZIER_PATH_PLAN.md` (status Draft):
adds a filled-path primitive, `SdfKind::PathFill`, built with `PathBuilder`
from line, quadratic and cubic segments with any number of contours and holes,
filled by the even-odd rule as a true signed distance and pushed with
`DrawList::push_path_fill`. Requested for Noble's Keep's tool icons (owner:
fills are added to libmsdf as a primitive, never drawn with strokes). No code
changes until the owner approves the plan.

## 2026-10-16 02:03 EDT — Adopt Rule 5: version tags and VERSIONS.md

No plan; a change to the rules (AGENTS.md, `8ae278e`). On the owner's
instruction AGENTS.md now carries libcontrols' critical rules verbatim:
Rules 0 to 4 were already identical, and Rule 5 (versions and tags, with
5.4's 0.x bump policy) is added.

- `VERSIONS.md` is added (`71122a2`) with one retroactive entry, 0.2.0 at
  `2434ec0` (the extraction from matter-stream, which changed the version
  from the stub seed's 0.1.0). The crate stayed at 0.2.0 through `69e6c5c`
  without version changes; the entry lists the public API as of `69e6c5c`
  rather than per commit. No entry is written for the 0.1.0 stub seed.
- Tag `v0.2.0` was created on 2026-10-16 on the old commit `2434ec0`, so the
  version change carries its tag as Rule 5.1 asks. It has not been pushed.
- From here on a version change follows Rule 5.4: the filled Bézier path
  plan's `SdfKind::PathFill` is a variant added to an exhaustive public enum,
  so it is released as 0.3.0.

## 2026-10-16 02:05 EDT — Adopt Rule 5.5: incompatible dependency bumps propagate

No plan; a change to the rules, following the 02:03 addendum. The owner
added Rule 5.5 to libcontrols' AGENTS.md, and libmsdf's Rule 5 was copied
again (`9ec0276`), so Rules 0 to 5 stay byte-identical to libcontrols'.
Under 5.5 a libmsdf release that moves a dependency to a semver-incompatible
version bumps libmsdf's minor version, and every consumer with a public API
that takes such a libmsdf release (for example libcontrols moving from
libmsdf 0.2 to 0.3) bumps its own minor version in turn.

## 2026-10-16 02:36 EDT — Filled paths 0.3.0: how it landed, and departures

Plan: [Filled Bézier paths as an SDF primitive](docs/design/20261015_01_FILLED_BEZIER_PATH_PLAN.md),
frozen since its 02:12 EDT amendment (which lists the open-question answers
in force). Small departures, none changing behaviour or an interface others
depend on:

- **Landing (section 5, "one commit ... on libmsdf main").** As libcontrols
  landed its 0.3.0, every step was one commit on the branch `impl/0.3.0`
  (`0546080`, then `713831b` to `4029003` for steps 1 to 8), each passing
  `cargo fmt -- --check`, `cargo clippy --all-targets` with no new warnings
  and `cargo test`, plus the GPU suite (`cargo test --features gpu-tests`).
  `main` received them as one squash commit, `b375be3`, whose message
  merges the step messages; the version bump and `VERSIONS.md` entry are in
  it, and the tag `v0.3.0` is on it. The `VERSIONS.md` heading names the
  tag but no short commit, since a commit cannot contain its own hash.
- **Before step 1 (`0546080`).** The GPU suite had not compiled since the
  wgpu 30 update (`69e6c5c`: `get_mapped_range` returns a `Result`), and
  `cargo clippy --all-targets` stopped on an `approx_constant` error in a
  drawlist test. Both were fixed in test code only, so steps 6 and 7 could be
  verified and Rule 3.4's clippy check could run.
- **Step 5.** The plan's GPU command run uses
  `CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm`; this stable toolchain rejects that
  variable (an unstable Cargo feature) and already builds with LLVM, so the
  commands ran without it.
- **Step 7, rotation.** The quarter-turn test is byte-identical for an
  asymmetric straight-edged shape, as the plan asks. For a shape with curved
  edges the pre-rotated copy is lowered differently (its y-extrema, where
  quadratics are split, are the original's x-extrema), so the two are equal
  curves made of different segments and f32 rounding differed by 1/255 in
  one channel of one pixel. That case is asserted within 1/255 on at most 16
  channels instead of exactly.
- **Step 8, branches.** Following the decision in force (open question 6),
  `main` was not merged into `main-sigilweaver` or `highbay`.
  `main-sigilweaver` was at `69e6c5c`, the same commit as `main` before
  this work.
- **Step 8, consumers.** Each was built in a scratch copy with its libmsdf
  replaced by the new tree and its lock file updated for libmsdf only.
  Nothing was committed in any consumer, and no pin was bumped.
  - libcontrols (`696e4bb`, libmsdf by git rev with no version requirement,
    so a `[patch]` to 0.3.0 applies): builds, 173 tests pass.
  - sigilweaver (`3cb31d5`): the workspace builds with all targets, and
    1,250 tests pass.
  - tinytrowel (`ab78f0e`): every crate except the two libmorph shells builds,
    and 142 tests pass. `trowel-desktop` and `trowel-android` could not be
    built because libmorph (both the sibling checkout and git `8f19048`)
    calls `DeviceSpec::px_per_mm`, which tinytrowel's `deps/libdevice` does
    not have. That error is in libmorph, which does not depend on libmsdf.
  - No consumer matches `SdfKind` exhaustively: libcontrols uses
    `matches!`, and highbay's tests use `match` with `_` arms. highbay was
    not built, because its libmsdf branch does not take this release.
  - The tower defense game has no Cargo workspace yet and was skipped.
  - Under Rule 5.5, libcontrols bumps to 0.4.0 when it takes libmsdf 0.3.0.
- `cargo check --target wasm32-unknown-unknown` passes.

## 2026-10-16 02:36 EDT — Correction to the 02:36 EDT addendum

Plan: filled Bézier path plan. In the 02:36 EDT addendum, the departure
about `CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm` is labelled "Step 5". It
belongs to the preamble of the plan's section 5 (Steps), which gives the GPU
test command, not to step 5.

## 2026-10-16 12:19 EDT — Atlas layout tables plan

New plan `docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md` (status
Draft): atlas file-major 5 with a split version field (file-major, major,
minor, patch; semver from 0.1.0, parsing gated by `>` comparisons, newer
files refused, v3/v4 still read), a table directory with first-capital
OpenType-derived tables (`Head`, `Cmap`, `Hmtx`, `Gsub`, `Gpos`) and
all-capital libmsdf tables (`LICS`, licence fourcc/version mapped to SPDX,
required for every baked face), a rule that no bytecode or VM-interpreted
table is ever carried, a pure-Rust `AtlasShaper` that matches `TextShaper`
exactly and is proven against rustybuzz on a corpus at bake, and
`rustybuzz`/`ttf-parser` 0.25 made optional behind a default-on `shaper`
feature. Requested by the owner for the tower defense game, whose atlases
would then lay out text without a TTF or a shaping dependency. Proposed as
0.4.0. No code changes until the owner approves the plan and answers its
open questions (notably Grenze Gotisch's contextual accent rules).

## 2026-10-16 12:41 EDT — Atlas layout tables plan: `LICT` amendment

Plan: `docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md` (Draft). On the
owner's decision, an amendment adds `LICT`, an all-capital libmsdf table
holding each licence record's strings (SPDX identifier, source, copyright,
Reserved Font Name) as a decimal length followed by that many printable ASCII
bytes, with no separator, as Itanium C++ mangling encodes identifiers. `LICS`
becomes fixed-size (fourcc, version, styles mask, `lict_index`). Non-ASCII
strings, and strings starting with a digit, are refused at bake. The
amendment replaces section 4.8's `LICS` layout and adds codec tests to step 2.
Still no code changes until the owner approves the plan.

## 2026-10-16 12:56 EDT — Atlas layout tables plan: `LICT` length bytes

Plan: `docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md` (Draft). On the
owner's correction, an amendment changes `LICT`'s string encoding from a
decimal count to one length byte (0 to 255) followed by that many printable
ASCII bytes. The decimal-count rules of the 12:41 EDT amendment (leading
zeros, digit count, the 4096 maximum, no leading digit in text) are
withdrawn; strings over 255 bytes are refused at bake (the longest measured
is 98). Truncation, non-ASCII text and `LICS`/`LICT` mismatches are still
refused. Still no code changes until the owner approves the plan.

## 2026-10-16 13:02 EDT — Atlas layout tables plan: open questions decided

Plan: `docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md` (Draft). An
amendment records the decisions on all seven open questions, made by the
coordinating agent at the owner's request:

- `Gsub` kind 3 (context single) is accepted, so Grenze Gotisch bakes.
- Kerning is stored in integer font units.
- Optional features are chosen per face and recorded in that face's `Head`.
- `Head` now carries ascender, descender, line gap, cap height and x-height.
- The real v3 fixture is kept under `tests/fixtures/legacy/`.
- From format major 1, a reader accepts a newer minor and refuses a newer
  major.
- Rule 5 is to gain the VERSIONS.md Format line and the minor-bump rule for
  format changes; that AGENTS.md change is proposed but not written yet.

It also records the owner's question "does file-major need to be last for back
compat?" and its answer: no, it must come first, so that legacy
little-endian v3/v4 versions read as file-major 3/4 and old readers refuse
`05 00 01 00` (u32 65541) cleanly. The plan has no open questions left. Still
no code changes until the owner approves the plan.

## 2026-10-16 13:03 EDT — Atlas layout tables plan: format version reads as Cargo semver

Plan: `docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md` (Draft). The
owner confirmed that the format version follows semver as Cargo reads it.

- **From 1.0:** major is breaking, minor is additive (only tables an older
  reader skips), and patch is no format change.
- **In 0.x the meanings move one place left:** a newer minor is refused, and
  a newer patch is accepted with any unknown tables skipped.
- **0.0.x** stays refused.

The amendment refines decision 6 of the 13:02 EDT amendment and the earlier
"patch never changes parsing". It replaces step 1's test-table rows for split
versions: a 0.1.0 reader accepts 0.1.1 and skips its unknown table, and
refuses 0.2.0.

## 2026-10-16 13:28 EDT — Bundled fonts plan (Draft); the 0.4.0 tag is held

Plan: `docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md` (Draft, new).
On the owner's direction, every bundled face becomes an opt-in cargo
feature, off by default, whose name carries its licence:
`font-apache-roboto-{regular,bold,italic,msymbols}` (group
`fonts-apache-roboto`) and `font-ofl-{grenze-gotisch,texturina}` (group
`fonts-ofl-ledger`), the two OFL faces moving in from the tower defense
game. Each feature brings its face's format-5 atlas, baked from the
committed TTF by `build.rs` into `OUT_DIR` through a public
`libmsdf::bake::Baker::bake_face`. That entry point lives in a proposed
wgpu-free `libmsdf-font` crate split out of `src/font`, so a build script
can depend on it. The TTF bytes are compiled in only with `shaper`. A
fallback (an explicit `libmsdf-bake` command and committed atlases) has
exact switch criteria, and AGENTS.md gains instructions for agents never to
hand-edit or substitute an atlas. `FONTS-LICENSE-OFL.txt` and
`FONTS-LICENSE-APACHE.txt` go at the root, replacing `fonts/LICENSE-*.txt`,
and the README states that with no font feature the crate is purely
MIT OR Apache-2.0.

**Held tag.** On the owner's instruction ("hold the tag"), step 8 of
`docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md` (version 0.4.0,
`VERSIONS.md`, tag `v0.4.0`) is not carried out when that plan's other
steps are done. It is performed once, as step 10 of the bundled fonts plan,
so that both plans land in one 0.4.0. This records a departure from that
frozen plan's step order (Rule 1.7); its text is unchanged. No code changes
until the owner approves the bundled fonts plan.

## 2026-10-16 13:35 EDT — Bundled fonts plan: approved; OFL faces subset

Plan: `docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md` (Draft;
approved, not yet in implementation). An amendment records the owner's
approval with the recommendations:

- the `libmsdf-font` split is approved;
- `libmsdf-font` is versioned in lockstep with libmsdf;
- no `tnum` in the bundled atlases;
- Linux reference hashes are recorded in step 6;
- standalone Bold and Italic atlases ship.

One recommendation is reversed: Grenze Gotisch and Texturina are subset to
`EUROPEAN_TEXT_RANGES`, which is U+0020–007E, U+00A0–017F, U+2010–2027,
U+2030–203A, U+20AC and U+2122 (356 codepoints; Grenze maps 340, Texturina
338). The new `fonts/ofl_subset.py` does the subsetting with fontTools
4.67.0, keeping every layout feature, glyph names, the `name` table and the
variation data, and dropping hinting, `gasp` and `DSIG`. It is reproducible
and checks provenance against the upstream sha256. The result is an OFL
Modified Version, recorded in `FONTS-LICENSE-OFL.txt` and in `name` IDs 5
and 10. The two OFL atlases need about 360 cells, more than a 320-cell
layer, so they are baked at 16 columns (800 × 2000 texels, about 4.8 MB
each) through new `BakeOptions::{coverage, columns}`, and
`FontAtlas::padded_to` lets consumers stack them with 8-column atlases.

**Pending the owner (Rule 1.7):** the atlas layout tables plan (In
implementation) defines its `Cmap` scope and equivalence corpus over
`TEXT_RANGES`. Baking the subset faces needs both over the bake's declared
coverage, which means `LayoutOptions` / `FaceLayout::extract` take a
coverage parameter. That is an interface departure from that frozen plan.
It must be agreed with the owner, and recorded here, before that plan's
step 3 fixes the scope. No code changes for the bundled fonts plan yet.

## 2026-10-16 13:41 EDT — Atlas layout tables: coverage parameter (departure); bundled fonts coverage groups

Plans: `docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md` (In
implementation) and `docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md`
(Draft; approved).

**Departure from the atlas layout tables plan (Rule 1.7), agreed by the
owner:** "yes, add the coverage parameter". The layout extraction, the
`Cmap` scope and the equivalence corpus take the declared coverage instead
of the hard-coded `TEXT_RANGES`, through a new
`LayoutOptions::coverage: Vec<(char, char)>` that defaults to `TEXT_RANGES`,
so the Roboto results do not change. This changes committed code: that
plan's step 3 (`7c00f63`) and step 4 (`6d18eab`). The change lands as its
own commit after that plan's step 5 and before the bundled fonts plan's
step 4. The format also gains `CVRG`, an all-capital per-face table
recording the declared coverage groups and the missing codepoints. It is
added within 5:0.1.0, before that version is released. The bundled fonts
plan's 13:34 EDT amendment said this would be settled "before that plan's
step 3 fixes the scope"; that was already out of date, and its 13:4x EDT
amendment corrects it.

**Bundled fonts plan:** coverage is split into groups with different
ranges and rules:

- `COVERAGE_TEXT` (287 codepoints: Basic Latin, the Latin-1 letters, NBSP,
  soft hyphen, Latin Extended-A). A missing codepoint fails the bake unless
  waived per face; Texturina's U+017F is waived.
- `COVERAGE_SYMBOLS` (63: the Latin-1 symbols, General Punctuation dashes,
  quotes, daggers, bullet, ellipsis, per mille, primes and guillemets,
  € ₴ ₺ ₽ ₾, № ™ ℮). Missing codepoints are skipped and reported.
- `COVERAGE_MATH` (78: common operators and relations, superscript and
  subscript digits and signs, vulgar fractions). Missing codepoints are
  skipped and reported.

U+2044 FRACTION SLASH is excluded, because rustybuzz's automatic
`numr`/`dnom` fractions cannot be carried by the tables. Measured:

- Grenze Gotisch: TEXT 287/287, SYMBOLS 57/63, MATH 48/78.
- Texturina: TEXT 286/287, SYMBOLS 55/63, MATH 50/78.

The OFL subsets keep the 411-codepoint union (Grenze about 132 KB,
Texturina about 214 KB). Each atlas carries roughly 390 to 400 cells, so
the 16-column bake stands.

**Approval:** the owner approved the plan after the coordinating agent's
read-only highbay review ("it will adapt, its custom symbols aren't font
sourced"). highbay needs only feature lines; its icon and marker codepoint
tables stay ungated. No code changes for the bundled fonts plan yet.

## 2026-10-16 13:42 EDT — Bundled fonts plan: `CVRG` confirmed; correction

Plan: `docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md` (Draft;
approved). A note-amendment records the owner's confirmation ("keep CVRG")
of the separate `CVRG` coverage table, rather than new `Head` fields.

**Correction:** the previous addendum ("Atlas layout tables: coverage
parameter (departure); bundled fonts coverage groups") refers to the
plan's "13:4x EDT amendment". It is the **13:40 EDT** amendment.

## 2026-10-16 13:50 EDT — Atlas layout tables: steps 1 to 7 landed on impl/0.4.0, and departures

Plan: [Atlas-carried layout tables and a shaper without rustybuzz](docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md)
(In implementation). Steps 1 to 7 are committed on `impl/0.4.0`:
`ba766d8` (1), `f15a308` (2), `7c00f63` (3), `6d18eab` (4), `aab4ff2`
(5), `d4faeb9` (6), `720cd24` (the agreed coverage parameter, 13:41 EDT
addendum), `46e3c74` (7), with the repository-level script commits
`6393a23`, `d59ea40` and `fdf18c5`. Step 8 (the release) is held, as the
13:28 EDT addendum records; it becomes step 10 of the bundled fonts plan.
Small departures, none changing behaviour others depend on beyond what the
plan already changes:

- **Step 1, `FontAtlas::file_version`.** The version a file was read at
  needs storage, so `FontAtlas` gains a third public field,
  `read_version: AtlasFileVersion`, beside `layout` and `licences`. It is
  public because the tower defense game builds `FontAtlas` with a struct
  literal; a private field would make that impossible rather than a
  one-line change.
- **Step 1, messages.** `from_bytes` returns `&'static str`, so refusals
  name the rule but not the numbers ("newer than this build reads (a newer
  minor at major 0)" rather than "5:0.2.0 is newer than 5:0.1").
- **Step 2, additions.** `FontAtlas::try_to_bytes` (the writer refuses
  layout or licence data that breaks a table rule; `to_bytes` panics on
  it), `FaceLayout::validate`, `AtlasLayout::{from_faces, faces}` (the
  plan's `faces` field is private, so it needs a constructor),
  `LicenceKind`, `licence_by_spdx`, `licence_by_key` and
  `LicenceTextError`'s public type. All additive.
- **Step 3, per-face options.** Decision 3 of the 13:02 amendment (options
  per face) is `FontAtlasBuilder::with_layout(opts)` for the regular face
  plus `with_style_layout(style, opts)` for another style; also
  `bakes_layout` and `face_data`. Glyph 0 is queued as the placeholder
  cell when layout is baked, since `AtlasShaper` can output it.
- **Step 3, extraction details** the plan leaves implicit, all checked by
  the equivalence check: rustybuzz 0.20.1's full default feature list
  (including `ltra`, `ltrm`, `rand`, `abvm`, `blwm`, `curs`, `dist`);
  the first feature per tag in the language system, as HarfBuzz does;
  contextual rules for one glyph that substitute differently, or a rule
  that matches without substituting ahead of one that does, are refused
  (rule order would matter); faces with AAT tables, a legacy `kern` table
  or a FeatureVariations record live at the default instance are refused.
- **Step 3, test faces.** The unsupported-lookup face, and step 5's
  reordered-lookups face, are made by `scripts/make-layout-test-face.py`
  from the bundled Roboto subset and committed under
  `tests/fixtures/layout/`. Reordering two of Roboto's own lookups changes
  nothing (they commute), so step 5's reorder test uses the second face.
- **Step 4.** `AtlasShaper::from_face` and `face` are added, and
  `is_default_ignorable` is public. `ShapedGlyph` and `ShapedRun` derive
  `PartialEq` and `Eq` (additive) so runs compare whole.
- **Step 5.** The "strings with no letter at all" are a constant,
  `LAYOUT_CHECK_NO_LETTERS`. After the coverage parameter, a fixed corpus
  string with a character the face maps outside the declared coverage is
  skipped, since that case is outside `AtlasShaper`'s contract.
- **Step 6.** `GlyphProjection` stays outside `shaper`: `MsdfCompute`, which
  the plan keeps available, takes one. Its producer `glyph_projection` is
  gated. The test compiled without default features (`tests/atlas_only.rs`)
  landed with step 7, because it loads step 7's fixture.
- **Step 7, consumers.** libcontrols (`464111e`, scratch copy with a
  `[patch]` to this branch, nothing committed) builds and its 306 tests
  pass. The tower defense game was not built: on the owner's instruction
  its workspace is not touched. Its two committed v3 atlases were loaded
  read-only through `FontAtlas::from_bytes` and round-trip through format
  5 unchanged. tinytrowel and sigilweaver were not built, for disk space
  (under 1 GB free); they are left to the bundled fonts plan's step 9.
- **Evidence.** `out/layout/log.md`: release-build bakes with
  `--layout` of the bundled Roboto faces and of the game's Grenze Gotisch
  and Texturina (read from its assets), each passing the equivalence check:
  Roboto Regular 46,695 strings, Bold 48,469, Italic 48,447,
  Grenze Gotisch 153,766, Texturina 48,291; the slowest single-face check
  took 1.14 s (Grenze), under the 10 s budget.

## 2026-10-16 14:50 EDT — Bundled fonts: steps 1 to 6, the switch to the fallback, and departures

Plan: [Bundled fonts as optional, licence-named features, baked at build time](docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md)
(In implementation since `817d24a`). Commits on `impl/0.4.0`: `817d24a` (1),
`2164d0c` and `c716b49` (2), `50422a7` (3), `508f50d` and `84bc0b8` (4),
`053e8b3` (5), `b42ac2c` (6), with the script commit `a707cc1`.

**Switch to the fallback (section 4.10).** Criterion 2 failed: the build
script would run the baker at the default build-override opt-level 0, and
the baker built at opt-level 0 takes 5 min 28 s (user time) for the seven
atlases, Grenze Gotisch alone about 220 s, against the 30 s budget for the
build script's run. Nothing inside the plan's design brings that under 30 s
at opt-level 0 (the equivalence check over Grenze's ~150,000-string corpus
dominates). So the atlases are committed under `fonts/atlases/`, made by
`libmsdf-bake bundled --out fonts/atlases --hashes`, and included from
there; `build.rs` does not bake and there is no build-time baker
dependency. A consumer needs no C++ compiler to use the bundled atlases.
Criteria 3 and 5 could not be measured on Linux from this machine, and the
`x86_64-unknown-linux-gnu` lines the 13:34 EDT amendment asks for in
`fonts/bundled.sha256` are not recorded: **open, for the owner** (a Linux
bake with `libmsdf-bake bundled --hashes`). Evidence: `out/fonts/LOG.md`.

Small departures (Rule 1.7):

- **Step 3, where the TTF constants live.** `bundled_style_face` and
  `StyledShaper::bundled` live in `libmsdf-font` and must see the TTF
  constants, which a dependency cannot read from libmsdf. So the six TTF
  constants are in `libmsdf-font`'s `faces` module behind font features
  of the same names, which libmsdf's `font-*` features forward; libmsdf's
  `src/bundled.rs` re-exports them (and holds the atlas constants), and
  every path the plan names (`libmsdf::ROBOTO_REGULAR_ASCII`,
  `libmsdf::font::ROBOTO_REGULAR_ASCII`, ...) resolves. `ttf-parser` stays an
  optional libmsdf dependency under `shaper`, used only by the integration
  tests, so `cargo tree -p libmsdf --no-default-features` lists none.
- **Step 3.** The `AtlasShaper` DrawList test moved to
  `tests/atlas_shaper_draws.rs`, since `DrawList` stays in libmsdf. Unit
  tests in both crates use `cfg(test)` face constants (`TEST_FACE` and,
  in `libmsdf-font`, its Bold, Italic and msymbols siblings).
- **Step 4, one crate per commit.** Split into a `libmsdf-font` commit
  (`508f50d`) and a libmsdf commit (`84bc0b8`).
- **Step 4, additions.** `BakeOptions::new`, `Baker::{bundled_options,
  bake_bundled_report}`, `BundledAtlas::{licences, waivers, is_ofl}`,
  `bundled::hex_of`, `bake::sha256`, `FontAtlasBuilder::add_styled_face`
  (registers a style face without queueing coverage, for coverage groups),
  and `FontAtlasBuilder::columns`. All additive.
- **Step 4, `CVRG` on read.** "Every codepoint in `Cmap` lies in a
  declared range" exempts the C0/C1 control codepoints, which the `Cmap`
  carries by the atlas layout tables plan's section 4.6 whatever the
  coverage.
- **13:40 EDT amendment, MATH count.** Its MATH list has 77 codepoints, not
  78 (the 411-codepoint union is consistent with 77); the carried MATH
  counts are therefore Grenze 47 and Texturina 49, with exactly the missing
  codepoints the amendment lists.
- **Step 2.** The upstream OFL files come from google/fonts at
  `98931f43a8dc` (Grenze Gotisch) and `f4a8ff8c122a` (Texturina), named in
  `FONTS-LICENSE-OFL.txt`; their sha256 equal section 3.1's. The check that
  each subset shapes the full set exactly as its upstream file was a one-off
  test (the upstream files are not committed), logged in `out/fonts/LOG.md`.
- **Step 4, the game's v3 atlases.** Matched by glyph name, every old cell
  present in the new atlas has identical texels; five old cells are Private
  Use Area glyphs the subset drops (Grenze `ijacute`, `IJacute.ss01`;
  Texturina `uniE0FF`, `uniEFFD`, `uniF000`), which the game's text never
  reaches.

## 2026-10-16 14:50 EDT — AGENTS.md: bundled fonts and atlases

Plan: [Bundled fonts as optional, licence-named features, baked at build time](docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md),
step 7. AGENTS.md gains the section `## Bundled fonts and atlases` (after
Rule 5, not a numbered critical rule) in its fallback form, since step 6
switched to the fallback: bundled atlases are produced only by
`libmsdf-bake bundled --out fonts/atlases --hashes` and never edited,
copied in or substituted; changing a face, the bundled table or the baker
means re-baking, verifying (`--check fonts/atlases`, the `bundled_fonts`
test) and committing `fonts/atlases/` with `fonts/bundled.sha256`, the
latter only with the owner's agreement when the bytes change.

## 2026-10-16 14:55 EDT — 0.4.0 released (local tag); both plans stay In implementation

Plans: [Atlas-carried layout tables and a shaper without rustybuzz](docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md)
and [Bundled fonts as optional, licence-named features, baked at build time](docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md).
The joint release step (the first plan's step 8, the second's step 10) is
`b48d066`: libmsdf and libmsdf-font at 0.4.0, the VERSIONS.md entry with
its Format line, and the tag `v0.4.0` on that commit (local; not pushed,
on the owner's instruction). Step 9 of the bundled fonts plan built
libcontrols (306 tests pass, no font feature) and checked the tower defense
game's atlases read-only.

**Neither plan is marked Implemented**, because some **Done when:** items
could not be met here, and Rule 1.3 sets `Implemented` only when every one
holds. Open, for the owner:

- atlas layout tables step 7, and bundled fonts step 9: the tower defense
  game was not built (the owner's instruction not to touch it); tinytrowel,
  sigilweaver and highbay were not built (disk under 1 GB free); the game's
  Android build and ledger-capture comparison were not run;
- bundled fonts step 6 (13:34 EDT answer 5) and switch criteria 3 and 5:
  no Linux host, so `fonts/bundled.sha256` has no
  `x86_64-unknown-linux-gnu` lines.

When these are done, the `Implemented` amendments and README rows follow
(Rule 4.3). The branch `impl/0.4.0` is fast-forwarded onto `main` (no
squash, so every step commit and the tag stay on `main`).

## 2026-10-21 22:12 EDT — Plan: stateful components that render into DrawLists

New plan, docs/design/20261021_01_COMPONENTS_PLAN.md (Draft, 3377508), on the
owner's instruction, from Bubble Pop's session.
- **The trait:** a design-neutral `Component` in a new `component`
  module.
  - `update` reports whether the component wants another frame.
  - `render` returns a `DrawList` of its own.
  - `render_into` appends to a caller's list, without allocating.
  - An optional `PaneMark` lets a `PaneGate`-backed `Gated` wrapper
    skip an unchanged component.
- **Placing children:** a `RenderCx` carries only libmsdf's own things
  (rectangle, scale, time, atlas, shaper), and `render_at` places a
  child's ink.
- **Why not libcontrols:** it is not used, so applications are not tied to
  its theme, router or modal stack.
- **Release:** strictly additive, so it ships as 0.4.1.

## 2026-10-21 22:24 EDT — Plan: a minimal Render trait; the components plan superseded

New plan, docs/design/20261021_02_RENDER_TRAIT_PLAN.md (Draft, a1b9ca9), on the
owner's decision, superseding docs/design/20261021_01_COMPONENTS_PLAN.md
before any of it was built.
- **The trait:** `Render`, with `render_into(&self, out)` to append a
  drawing and `render_mut(&mut self, out)` for renderers that settle
  state as they draw (by default `render_into`). It has no context and no
  clip.
- **For `DrawList`:** it is implemented by appending, at top level only.
- **Release:** strictly additive, so 0.4.1.
- **Not in libmsdf:** components, hit testing, bounds, overlays, gating
  wrappers and atlases by role.

## 2026-10-21 22:41 EDT — 0.4.1 released: the Render trait

The Render trait plan (docs/design/20261021_02_RENDER_TRAIT_PLAN.md),
approved by the owner and implemented.
- **The trait:** `libmsdf::Render`, with `render_into` and `render_mut`
  (4b11d81), and a README paragraph (69622c0).
- **The release:** 0.4.1, tagged `v0.4.1` (876121e), with `libmsdf-font`
  following in lockstep. The plan is Implemented.

## 2026-10-21 23:14 EDT — Plan: several atlases in one draw list

New plan, docs/design/20261021_03_ATLAS_SET_PLAN.md (Draft, 2bfd3bc), on the
owner's instruction.
- **The set:** `AtlasSet` holds several `FontAtlas`es under small ids, as
  one texture array (padded to the largest atlas) and one merged glyph
  table. The renderer uploads it once and keeps it.
- **The scope:** `push_atlas(id)` and `push_atlas_end()` work like
  `push_clip`, tagging every instance. It is a scope because `FontAtlas`
  has no id, and adding one would break the API. A debug check catches text
  shaped against a different atlas than the scope's.
- **Lowering:** `lower_with(&set)` moves each text instance's glyph
  indices up by its atlas's first table entry. The shader and the instance
  format are unchanged.
- **Release:** strictly additive, so 0.4.2.

## 2026-10-21 23:16 EDT — Atlas set plan: handles replace the scope

Plan: docs/design/20261021_03_ATLAS_SET_PLAN.md (Draft), amended in
e63d7ca on the owner's agreement.
- **Handles:** `AtlasSet::atlas(id)` returns an `AtlasRef` (the id and the
  atlas). New `push_shaped_text_with` and `push_icon_with` take it and
  tag the instance. The existing calls are unchanged and tag the default.
- **Dropped:** the `push_atlas` scope and the debug-only address check.
- **Ids name their set,** and `lower_with` refuses another set's ids, in
  release builds too.
- **Release:** still strictly additive, 0.4.2.

## 2026-10-21 23:26 EDT — Atlas set plan: no panics in release, standard sizes

Plan: docs/design/20261021_03_ATLAS_SET_PLAN.md (Draft), amended in a4d8bd0 on
the owner's agreement.
- **Foreign ids:** `lower_with` drops text with a foreign id and counts it
  in a warning, with a `debug_assert!` in debug builds, like the rest of
  lowering. `AtlasSet::atlas` returns `Option`.
- **Sizes:** atlases come in standard sizes (400 x 2000, or 800 x 2000 for
  16 columns), so padding to the widest stands. The per-layer-size question
  is closed.

## 2026-10-21 23:37 EDT — 0.4.2 released: several atlases in one draw list

The atlas set plan (docs/design/20261021_03_ATLAS_SET_PLAN.md), approved by
the owner with the shaping helper, and implemented.
- **The set:** `AtlasSet`, `AtlasId`, `AtlasRef` with `shaper(style)`
  (95fa12d).
- **The list:** `push_shaped_text_with`, `push_icon_with` and `lower_with`
  (12951be).
- **The renderer:** `new_with_atlas_set`, `upload_atlas_set` and
  `render_draw_list_in` (780d217), with a README paragraph (c66c812).
- **The release:** 0.4.2, tagged `v0.4.2` (345db37), with `libmsdf-font`
  following in lockstep. The plan is Implemented.
- **Departure (step 3, section 5):** the plan said `upload_atlas_set`
  panics, as `upload_msdf_atlas` does, when the texture is too small for
  the set. `upload_msdf_atlas` does not panic: it logs an error and
  uploads what fits. `upload_atlas_set` does the same, which keeps to the
  plan's own rule of no panics in release.

## 2026-10-23 02:29 EDT — Plan: a terminal face and a symbol page, from Noto

New plan, docs/design/20261023_01_TERMINAL_FONTS_PLAN.md (Draft), on the
owner's instruction relayed by another session; not yet reviewed by the
owner, and nothing is implemented.
- **The faces:** Noto Sans Mono Regular and Bold, one two-layer 16-column
  atlas. A symbol page merged from Noto Sans Mono, Noto Sans Symbols 2,
  Noto Sans Symbols, Noto Sans Math and Noto Emoji onto the 0.6 em cell.
- **Measured:** the seven requested blocks come to 1,057 codepoints, more
  than one 640-cell page. The page holds four of them in full plus curated
  sets: 605 cells. 166 accented capitals of Noto Sans Mono pass the cell
  window at the default cap placement, so the placement becomes a bake
  option. Box drawing is remapped onto the 1.3 em line box to join.
- **Release:** breaking, so 0.5.0 (`BundledAtlas` gains variants;
  `BakeOptions` gains `cap`).

## 2026-10-23 03:28 EDT — 0.5.0 released: a terminal face and a symbol page, from Noto

The terminal fonts plan (docs/design/20261023_01_TERMINAL_FONTS_PLAN.md),
approved by the owner with the answers in its 02:32 EDT amendment, and
implemented: steps 1 to 7 in 2a38b6f, 2c90aed, c721474, 54d945f, 8212c19,
c351b91 and c3cf511 (tag `v0.5.0`), with the departures below in 40a7e9f,
f75671b and 7348020.

Departures (Rule 1.7), each needed to make the plan work:
- **Rule 2 centres the advance box (section 4.4; 40a7e9f).** Centring each
  glyph's ink moved every Braille pattern differently: a one-column
  pattern such as ⠸ drew 3 px off the column ⠿ puts the same dots in.
  `terminal_symbols.py` centres the source's advance box instead and
  nudges the ink inside the margins only where it would cross one.
- **Two shader changes (section 4.2 said none; f75671b).** The render
  checks of step 6 found that box drawing did not join:
  - the glyph sample is clamped to the cell's last texel centre, since the
    last half texel blended in the zeroed padding and cut a black row
    under box drawing at some line positions;
  - a text run's glyphs are composited as one shape (the largest coverage
    per pixel), since glyph-over-glyph compositing beaded every cell join
    (1.48 of the stroke's ink at 12 px x1; 1.09 now).

  The golden scene differs in 4 of 320,000 pixels, by at most 25 levels,
  and passes its tolerance. VERSIONS.md marks the compositing change
  Breaking (rendering).
- **Face hashes (section 4.1).** The plan said `fonts/bundled.sha256` gains
  the three faces. That file records atlases. The faces' sha256s are in
  `BundledFace` (font/src/bundled.rs), `rebuild-fonts.sh` and
  `FONTS-LICENSE-OFL.txt`, as for every other face. The two new atlases
  have their lines in `bundled.sha256`.
- **`ofl_subset.py --instance` round-trips the instance** through the file
  format. Otherwise its provenance check compared the subset's stored
  integer coordinates with the instance's floats and failed on Bold.
- **The render checks' details (section 5).**
  - Coverage is read from the frame's alpha where a glyph drew.
  - Stroke continuity is the ink summed across the stroke's width, along
    stretches between crossings that span the cell and line boundaries.
  - The rounded corners are sampled from where their arcs end.
  - The Braille check runs at 24 physical px and up, since below that the
    dots of a column touch.
  - The A-ring check places the ring's top within 1 px.
- **`check-font-features.sh` (7348020).** Its byte probe failed at 73ea080
  already: the probes share a target directory, and the all-features
  build's rlibs matched the no-default-features probe's globs. It now
  removes libmsdf's artifacts before each probe. The whole script passes.

Corrections and findings:
- **Section 3.3's 166 glyphs is 65.** The measurement also swept the
  upstream face's box, block and geometric glyphs, which the text atlas
  does not carry. 65 TEXT glyphs, the accented capitals and ĥ ĺ, pass the
  default window, by up to 83 units; none pass at the terminal placement.
- **For the owner: the existing OFL atlases clip too.** The new report line
  shows that at the default placement Texturina has 54 glyphs, by up to 53
  units, and Grenze Gotisch 2, by 47, whose ink passes the cell top. Their
  bytes are unchanged here. A `CapPlacement` for them would fix it with a
  re-bake, which changes committed atlases and needs the owner's
  agreement. The Roboto atlases use the shipped coverage, which the report
  does not cover.
- **Measured.**
  - Noto Sans Mono: 407 and 406 cells (Regular, Bold), 9,637,288 bytes.
  - The symbol page: 607 cells, 4,827,764 bytes.
  - Box-drawing joins at most 1.09 of mid-cell, no seam at 12 to 24 px and
    scale 1 to 3.
  - Evidence: out/terminal/LOG.md.
- The Bold subset's name ID 2 still reads "Regular", from the variable
  face's default instance. libmsdf reads the style from the bundled table,
  not the name.

## 2026-10-23 12:57 EDT — Planned: a per-atlas cell box, and atlas format 5:0.2.0

Plan: `docs/design/20261023_02_PER_ATLAS_CELL_BOX_PLAN.md` (Draft).

The owner asked whether the clipping found in Texturina and Grenze Gotisch
(docs/FONT_TEXTURINA.md, docs/FONT_GRENZE_GOTISCH.md) needs a larger glyph
box for every atlas or a per-atlas setting with a format version bump. The
plan proposes the per-atlas setting: a `CellBox` (how far cells reach above
and below the 1.3 em line box) chosen at bake time, recorded in a `CELL`
table written only when used, at format 5:0.2.0 so that libmsdf 0.5 and
earlier refuse such an atlas; text drawn from the cell box; Texturina and
Grenze Gotisch re-baked whole with their baselines unchanged. Breaking, so
libmsdf 0.6.0 when implemented (libcontrols 0.7.0 after it, Rule 5.5).
Nothing is implemented until the owner approves it.

Also recorded: AGENTS.md gained Rule 3.6 (findings documents under `docs/`
are append-only) and, in Rule 3.5, evidence directories named
`YYYYMMDD_NN_name/`, on the owner's instruction.

## 2026-10-23 14:13 EDT — Per-atlas cell box plan, steps 1-4: departures and findings

Plan: `docs/design/20261023_02_PER_ATLAS_CELL_BOX_PLAN.md` (In
implementation). Steps 1 to 4 are committed (07f642c to 6997d85); step 5,
the 0.6.0 release, waits for the owner's approval. Small departures needed
to make the plan work (Rule 1.7):

- **Step 1, section 4.2: the `px_per_em_frac` check runs only with a
  `CELL` table.** Without one, the plan also refused a set whose
  `px_per_em_frac` is not `1 / 1.3`. That would refuse atlases built in
  memory or by other tools with other values (the crate's own tests build
  some), for no drawing benefit: such an atlas draws exactly as before.
- **Step 1: new public items the plan did not name.**
  `ATLAS_FORMAT_VERSION_LINE_BOX` (5:0.1.0, the version a line-box atlas is
  written at), `CELL_TABLE_TAG` and `CapPlacement::top_frac_in` (the cap
  as a fraction of a cell with a given box). All additive; they will be
  listed in VERSIONS.md.
- **Step 2: the line-box path computes exactly as before.** `top_frac_in`,
  `CapPlacement::window` and the draw list's cell height take the old
  expressions for `CellBox::LINE_BOX` rather than the general formula,
  whose floating-point result can differ in the last place; that keeps all
  nine atlases byte-identical and the bake reports unchanged.
- **Step 3: `lower_stream_with_cell_box`.** `lower_stream` takes no atlas,
  so it cannot read a box; it keeps the line box, and the new function takes
  a `CellBox`. Additive.
- **Step 4: ranges in ascending order.** `CVRG` refuses ranges that are not
  ascending, so the new TERMINAL and TERMINAL_EMOJI codepoints are inserted
  in order rather than appended; COVERAGE_TERMINAL_EMOJI's "extendable by
  appending" note was wrong and is corrected. The terminal symbol face
  sorts its codepoints anyway, so its bytes are the same either way.
- **Step 4, section 5: the GPU check measures against `H`.** The renderer
  puts every edge a fixed fraction of a pixel from the outline (its
  half-texel sampling and the MSDF threshold), so the check compares each
  tall glyph's top and bottom with the outline after removing the offset
  measured on `H`, and counts a row as inked at quarter coverage (an
  acute's tip never reaches half at 12 px). Run against libmsdf 0.5's
  atlases, the check fails at every size, `ĥ ĺ` by up to 8.8 px at 56 px.
- **A process slip.** Step 1's round-trip test (f403dc1) was committed
  without rustfmt; 4978313 formats it.

Findings: all nine atlas pages stay 400 or 800 by 2000 texels, within a
2048 limit; the symbol page grows from 607 to 616 of its 640 cells. Claude
Code's starburst spinner is Dingbats (`· ✢ ✳ ✶ ✻ ✽`), not Braille, and was
already covered.

## 2026-10-23 14:51 EDT — 0.6.0 released: the per-atlas cell box plan

Plan: `docs/design/20261023_02_PER_ATLAS_CELL_BOX_PLAN.md`, now
Implemented. Released on the owner's approval ("the release looks good,
let's release it") as 0.6.0, `4de319c`, tag `v0.6.0`. The departures are
the addendum above (37e0e7e); nothing else departed. For
libcontrols, this is a semver-incompatible move: its next release is 0.7.0
(Rule 5.5).
