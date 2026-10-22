---
title: A minimal Render trait
date: 2026-10-21
status: In implementation
covers: >
  One small public trait, `Render`, for anything that draws itself into a
  `DrawList`: `render_into(&self, out)` appends its drawing, and
  `render_mut(&mut self, out)` does the same for a renderer that settles
  state as it draws, by default `render_into`. No context, no clipping, no
  components: those stay with applications and UI libraries. An
  implementation for `DrawList` itself (a kept list renders by appending),
  the tests, and the strictly additive 0.4.1 release.
sources:
  - >
    Owner (Tim), 2026-10-21, from Bubble Pop's session: "is there a render
    trait in libmsdf that has a method returning a renderlist?"; then, after
    weighing a full component trait (docs/design/20261021_01_COMPONENTS_PLAN.md):
    "I mean does a complete component trait belong in libmsdf?" and "just the
    minimal Render trait in libmsdf with render_into and render_mut unless
    into implies ctx or clipped". It does not: `_into` names the target list,
    as `append_from` and `translate_from` name theirs.
  - src/drawlist/mod.rs (`DrawList::append_from` and its notes on naming for
    the drawing gates and on open scopes), src/pane.rs (`PaneGate`).
related:
  - docs/design/20261021_01_COMPONENTS_PLAN.md (superseded by this plan)
  - libcontrols (`DrawControl::draw(&self, list, ctx)`), which could
    implement this trait for its controls; and Bubble Pop, a first consumer.
    Neither is part of this plan.
---

# A minimal Render trait

## 1. Goal

libmsdf has no shared name for "a thing that draws itself into a
`DrawList`". Applications and UI libraries each invent one (libcontrols'
`DrawControl::draw`, Bubble Pop's screen functions, a pane's build closure),
so code that only needs to ask something to draw cannot be written once.
This plan adds the smallest such contract and nothing else: no context, no
clipping, no input, no layout, no lifecycle. Those belong to whoever hosts
the drawing.

## 2. The trait

In `src/drawlist/render.rs`, a submodule of `drawlist`, public as
`libmsdf::drawlist::Render` and re-exported at the root as
`libmsdf::Render`:

```rust
use super::DrawList;

/// Something that draws itself into a [`DrawList`].
///
/// `render_into` appends this thing's drawing to `out` and never clears it.
/// The name says where the drawing goes, nothing more: there is no context
/// and no clip. A renderer that needs fonts, atlases, a rectangle or a time
/// holds them, or is handed them by its own methods; a caller clips with
/// [`DrawList::push_clip`] around the call (see "Open scopes").
///
/// `render_mut` is for a renderer that settles state as it draws (a measured
/// layout, a scroll bound written back). It is `render_into` unless
/// overridden. **After `render_mut`, a `render_into` from the same state
/// draws the same thing**: hosts with only `&self` access (a read-only
/// pass, a gate's audit) rely on it.
///
/// # Open scopes
///
/// What a renderer draws with `push_*` calls inherits any clip, effect or
/// rotation the caller has open. Ink appended with
/// [`DrawList::append_from`] (a kept list, as [`DrawList`]'s own
/// implementation does) carries its own scopes and must be rendered at top
/// level: `append_from` asserts the destination has none open.
pub trait Render {
    fn render_into(&self, out: &mut DrawList);

    fn render_mut(&mut self, out: &mut DrawList) {
        self.render_into(out)
    }
}

/// A kept list renders by appending itself (`append_from`'s rules apply:
/// at top level only).
impl Render for DrawList {
    fn render_into(&self, out: &mut DrawList) {
        out.append_from(self)
    }
}
```

**Names.** `render_into` and `render_mut` follow the API's habit of naming
methods so a source-level check can find them: `_into` like
[`DrawList::append_from`]'s `_from`, and neither collides with a common std
method. Downstream drawing gates that match on spelling (libhbui's
composition-boundary checks) will need the two names added to watch them;
that is their change, not this plan's.

**Not added:** a method returning a `DrawList`. A caller wanting one writes
`let mut l = DrawList::new(); r.render_into(&mut l);`, which keeps the
trait at two methods and every allocation visible.

## 3. Tests

In `tests/render.rs`:
- `render_into` appends without clearing: a list with ink before the call
  keeps it, followed by the renderer's.
- `render_mut`'s default equals `render_into` (instances and chars).
- A renderer that overrides `render_mut` to settle a field (a counter of
  measured widths), then draws from it: `render_into` after `render_mut`
  equals what `render_mut` drew.
- `DrawList`'s implementation equals `append_from`, chars and path segments
  included; and rendering a kept list inside an open `push_clip` panics with
  `append_from`'s assertion (`#[should_panic]`).
- A renderer drawing with `push_*` calls inside a caller's `push_clip` has
  its instances clipped to it.

## 4. Release

Strictly additive (a new trait and an implementation of it for an existing
type), so under Rule 5.4 it releases as **0.4.1**, tagged `v0.4.1`, with
its `VERSIONS.md` entry.

## 5. Steps

1. **The trait:** `src/drawlist/render.rs`, the re-export, the `DrawList`
   implementation, the tests of section 3, and a README paragraph with a
   three-line example.
   **Done when:** the tests pass, with fmt and clippy clean.
2. **Release 0.4.1** (Rule 5).
   **Done when:** the version, its `VERSIONS.md` entry and the tag are in.

## 6. Open questions

None.

## Amendment — 2026-10-21 22:40 EDT: approved; implementation started

The owner approved this plan ("approve libmsdf plan and push 0.4.1").
Implementation starts with step 1 in the commit that adds this amendment:
the status is In implementation, and the document is now immutable. It
changes only on the owner's explicit instruction.
