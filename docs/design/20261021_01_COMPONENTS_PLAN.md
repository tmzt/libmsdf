---
title: Stateful components that render into DrawLists
date: 2026-10-21
status: Draft
covers: >
  A small, design-neutral component trait in libmsdf: a stateful component
  that updates over time and renders into a `DrawList` of its own (a method
  that returns one, and an allocation-free form that appends to a caller's
  list), with an optional `PaneMark` and key so a wrapper can skip unchanged
  components through the existing `PaneGate`, and helpers to place a
  component's ink in its parent. No theme, no input routing, no layout
  engine: those stay with the application or with libcontrols. Also
  covered: why this is not libcontrols' `DrawControl`, the tests, an
  example, and the 0.4.1 release.
sources:
  - >
    Owner (Tim), 2026-10-21, from Bubble Pop's session: "is there a render
    trait in libmsdf that has a method returning a renderlist?" (there is
    not: libmsdf's public traits are `Rasterizer` and `PaneKey`, and only
    `PaneGate::draw` composes lists), then: "it makes sense in libmsdf and
    needs a plan in that repo, libcontrols has its own but we dont want to
    pull that in because it pushes all UIs to the same design, however that
    would allow for stateful components that output drawlists".
  - src/drawlist/mod.rs (`DrawList`, `append_from`, `translate_from`,
    `push_translate`, `push_clip`), src/pane.rs (`PaneMark`, `PaneGate`,
    `PaneKey`).
  - libcontrols ccb783d, src/traits.rs and src/control.rs (`DrawControl`,
    `DrawContext`, `HostedControl`), for contrast.
related:
  - Bubble Pop (~/src/bubble), its screens' hand-drawn pieces (the
    name-entry keyboard, the store's cards, the bowl), a first consumer.
  - libcontrols (~/src/libcontrols), which may later implement its controls
    on this trait (not part of this plan).
---

# Stateful components that render into DrawLists

## 1. Goal

Applications built on libmsdf draw their own UI pieces: Bubble Pop's
name-entry keyboard, the store's cards and the bowl are each a few hundred
lines of hand-placed draw calls (its own shader's instances, and libmsdf
`DrawList` text and panels) inside one screen function, with their state (a
pressed key, a picked-up item, an animation) kept wherever the screen keeps
it. There is no shared shape for "a thing with state that draws
itself", so nothing can be reused, gated or tested on its own.

libcontrols has such a shape (`DrawControl` + `HostedControl`), but its
`DrawContext` carries a `ControlTheme`, router `NodeFlags` and `NodeState`,
and a modal `LayerView`. Taking it means taking its theme, router and modal
stack, which pushes every UI toward libcontrols' design. The owner does not
want that for games like Bubble Pop.

This plan adds the missing piece to libmsdf itself: a component trait that
knows only what libmsdf knows (a `DrawList`, a font atlas and shaper, a
rectangle, a scale, time) and nothing about looks or input.

## 2. Not in scope

- **Themes, colours, typography choices.** A component holds its own style,
  as its own fields.
- **Input.** Hit testing, focus, routing and gestures stay with the
  application (or libcontrols). A component may expose methods of its own
  for them; the trait does not.
- **Layout.** The parent decides each child's rectangle. No flex, no
  constraints solver.
- **A retained tree.** Components are owned by the application, in its own
  structs; the trait has no children list.

## 3. The interface

A new public module, `libmsdf::component` (`src/component.rs`), re-exported
at the crate root as `libmsdf::{Component, RenderCx, Gated}`.

```rust
use crate::drawlist::DrawList;
use crate::font::{FontAtlas, TextShaper};
use crate::pane::{PaneGate, PaneKey, PaneMark};

/// What a component is given each frame: where it is, how big a logical
/// unit is, the time, and the text tools. Nothing about looks.
#[derive(Clone, Copy)]
pub struct RenderCx<'a> {
    /// The component's rectangle in the list's coordinates: top-left, size.
    pub pos: [f32; 2],
    pub size: [f32; 2],
    /// Pixels per logical unit (the application's UI scale).
    pub scale: f32,
    /// Seconds, the application's clock (for animation in render).
    pub time: f32,
    pub atlas: Option<&'a FontAtlas>,
    pub shaper: Option<&'a TextShaper>,
    /// The MSDF pixel range for text.
    pub px_range: f32,
}

impl<'a> RenderCx<'a> {
    /// The same context for a child at `pos`, `size` (absolute).
    pub fn child(&self, pos: [f32; 2], size: [f32; 2]) -> RenderCx<'a>;
}

/// A stateful piece of UI that renders into a `DrawList`.
pub trait Component {
    /// Advance the component's own state by `dt` seconds (animations,
    /// timers). Returns true if it wants another frame soon, so an
    /// idle-until-work loop can sleep when every component returns false.
    fn update(&mut self, _dt: f32) -> bool {
        false
    }

    /// Render into `out`, appending (never clearing). The required method:
    /// allocation-free composition. `&mut self` because rendering may
    /// settle state (a measured layout, a scroll bound), as panes do.
    fn render_into(&mut self, cx: &RenderCx, out: &mut DrawList);

    /// Render into a list of its own and return it.
    fn render(&mut self, cx: &RenderCx) -> DrawList {
        let mut list = DrawList::new();
        self.render_into(cx, &mut list);
        list
    }

    /// The mark its writes move, if it keeps one: with [`Gated`], an
    /// unmarked component with an unchanged key is not rendered again.
    fn mark(&self) -> Option<&PaneMark> {
        None
    }
}

/// A component whose ink is kept and spliced while nothing changed: a
/// `PaneGate` keyed by the component's `RenderCx` rectangle and scale plus
/// a key the component supplies. A component with no mark is always
/// rendered (the gate never skips what cannot tell it changed).
pub struct Gated<C, K: PaneKey = ()> {
    pub inner: C,
    gate: PaneGate<(GateRect, K)>,
}

impl<C: Component, K: PaneKey + Clone> Gated<C, K> {
    pub fn new(name: &'static str, inner: C) -> Self;
    /// Render through the gate with this frame's `key`.
    pub fn render_into_keyed(&mut self, cx: &RenderCx, key: K, out: &mut DrawList);
    pub fn rebuilds(&self) -> usize;
    pub fn skips(&self) -> usize;
}

impl<C: Component> Component for Gated<C, ()> { /* render_into_keyed with () */ }

/// The rectangle and scale a gate compares (bit-exact f32s).
#[derive(Clone, Copy, PartialEq)]
struct GateRect { pos: [u32; 2], size: [u32; 2], scale: u32 }
```

**Placing a child.** A parent renders a child at the child's own rectangle
by passing `cx.child(pos, size)`; the child draws in absolute coordinates,
as every `DrawList` call already does. For a child that draws in local
coordinates (its top-left at 0, 0), one helper:

```rust
/// Render `c` with its rectangle at the origin and move its ink to `pos`.
pub fn render_at(c: &mut impl Component, cx: &RenderCx, pos: [f32; 2], out: &mut DrawList);
```

implemented with `DrawList::translate_from(start, dx, dy)` over what the
child appended.

## 4. Why this shape

- **`render` returns a `DrawList`** (the owner's question) and
  **`render_into` appends to one.** Returning an owned list allocates per
  component per frame; appending does not. The required method is the cheap
  one, and the returning one is a default built on it, so a component
  writes one method and callers choose.
- **`&mut self`.** Components are stateful; settling state during render is
  the pattern `PaneGate` already documents.
- **`update` returns whether it wants a frame**, for an idle-until-work
  frame loop such as the one `src/pane.rs`'s module doc describes.
- **Gating reuses `PaneGate`**, so its audit mode (`with_audit`) checks a
  skipped component against a fresh render in tests.
- **No context beyond libmsdf's own types**, so a game can implement it
  without adopting a theme, and libcontrols could later implement
  `DrawControl` in terms of it (or the reverse) without either requiring
  the other.

## 5. Tests and example

In `tests/component.rs`:
- a component's `render` equals `render_into` on an empty list
  (instances and chars), and `render_into` appends without clearing;
- a stateful counter component renders its count, and its state persists
  across frames; `update` returns true while it animates, false at rest;
- `Gated`: a marked-and-unchanged component with the same key and
  rectangle is skipped (rebuilds stays 1 over 10 frames, skips 9); marking
  it, changing the key, or moving the rectangle rebuilds; with the gate in
  audit mode, a skipped frame equals a fresh render; a component with no
  mark is rebuilt every frame;
- `render_at` places a local-coordinate child's ink at `pos` (its first
  instance moves by exactly `pos`), and leaves earlier ink in `out` alone.

An example, `examples/components.rs`: a small screen of three components
(a label, a toggle with an eased knob, a gated card) rendered to a PNG with
the CPU rasterizer, each frame through `Component`.

## 6. Release

Strictly additive (a new module, a new trait and types; nothing changed),
so under Rule 5.4 it releases as **0.4.1**, tagged `v0.4.1`, with its
`VERSIONS.md` entry.

## 7. Steps

1. **The module:** `src/component.rs` with `RenderCx`, `Component`,
   `Gated`, `render_at`, re-exported at the root, with section 5's tests.
   **Done when:** the tests pass, with fmt and clippy clean.
2. **The example** of section 5, and a README section, "Components", with a
   short example.
   **Done when:** the example renders its PNG to `out/components/`.
3. **Release 0.4.1** (Rule 5).
   **Done when:** the version, `VERSIONS.md` entry and tag are in, and Bubble
   Pop builds against it.

## 8. Open questions

1. Should `RenderCx` carry a clip rectangle, so a parent can clip a child
   without pushing a clip itself?
2. Should `Component` have an optional `hit(&self, p) -> bool`, the one
   piece of input most components need, or does every bit of input stay
   out?
3. Bubble Pop as the first consumer: port its name-entry keyboard and the
   store's cards to components as a follow-up in that repo?
