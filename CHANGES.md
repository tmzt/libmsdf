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
