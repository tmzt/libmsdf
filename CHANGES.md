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
