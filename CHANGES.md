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
