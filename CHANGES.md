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
