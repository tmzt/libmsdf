---
title: A scoped append, for kept lists placed under open scopes
date: 2026-10-28
status: Draft
covers: >
  One additive `DrawList` method, `append_scoped(&mut self, other:
  &DrawList, offset: [f32; 2])`, that splices a separately recorded list
  into this one as if its calls were made now, moved by `offset`, inside
  whatever clip, effects and rotate/translate scopes are open: each
  appended instance's clip is the open clip intersected with its own
  (moved by `offset` and mapped through the open transform), its transform
  the open transform composed with its own, its effects its own or else the
  open ones, with the char, path-segment and transform banks rebased as
  `append_from` rebases them. Covers the exact rules for nested clips,
  effects and rotations, the cost, the tests, and the libmsdf patch
  release. `append_from` and everything that uses it are unchanged.
sources:
  - >
    Owner (Tim), 2026-10-28, through flankr's session: "write the libmsdf
    append_scoped plan". That is only to write this plan as a Draft; it is
    not an approval to build it, and the status stays `Draft` until the
    owner approves it.
  - >
    libcontrols' docs/design/20261028_08_SCROLLBAR_PLAN.md (In
    implementation; read only here): decision 9 (a scoped append, "append
    `other` as if each of its instances were pushed now, under the open
    scopes"), step 6 (placement: inside the pane's clip and translate,
    `push_translate(vp.x, vp.y + top)` then the scoped append), step 7,
    open question 1, and its 23:13 EDT amendment (steps 6-7 wait for this
    plan).
  - >
    libcontrols src/router.rs at its working tree of 2026-10-28
    (`draw_subtree`, ~:690-712, and `render_at`, ~:597): a pane opens
    `push_clip` in screen pixels (moved by `-shift`, because clips are not
    moved by an open translate) and then `push_translate(-off)` for its
    children.
  - >
    src/drawlist/mod.rs at 30350a4: `DrawList` and its side channels
    (:465-525), `push` (:575), `active_clip` (:595), `translate_from`
    (:655), `append_from` and its `debug_assert!` and naming note
    (:692-789), `translate_transforms_from` (:789), `begin_effects` /
    `end_effects` (:831-838), `push_rotate` / `push_translate` and their
    ends (:910-987), `push_clip` / `push_clip_end` (:1081-1094),
    `SdfKind::translate` (:182), `RotationTransform` (:354) and
    `rotate_rect` (:429), the `lower` scope asserts (~:1536), and the
    splice tests (`a_spliced_pane_lowers_to_what_drawing_it_in_place_would_have`,
    `a_spliced_rotation_still_names_its_own_matrix`,
    `an_untagged_instance_is_not_rebased_into_a_transform`,
    `append_from_rebases_seg_start`, `frame_digest`).
  - >
    src/core/sdf.rs (`ClipRect`, `UNBOUNDED`, `intersect` :189);
    src/gpu/sdf_render.wgsl (the clip test against the screen pixel,
    :385; the ombré reads the instance's own `pos`/`size`, :697);
    src/drawlist/render.rs (`Render for DrawList` uses `append_from`);
    src/pane.rs (`PaneGate::draw` splices with `append_from`).
related:
  - ~/src/libcontrols/docs/design/20261028_08_SCROLLBAR_PLAN.md (the first
    user: steps 6-7, sections drawn from a per-id cache)
  - docs/design/20261028_01_CODING_AGENT_GLYPHS_PLAN.md (Draft, also
    proposed as 0.6.1; the two release in whichever order they are built,
    section 5)
  - docs/design/20261021_02_RENDER_TRAIT_PLAN.md (`Render for DrawList`,
    which keeps `append_from`)
---

# A scoped append, for kept lists placed under open scopes

## 1. Goal

libcontrols' scrollbar plan draws a scroll pane's content as sections. The
caller's `render_section(id)` records each section into a `DrawList` of its
own, in section coordinates (top-left `[0, 0]`). The pane keeps that list
and, every frame the section is in range, splices it into the frame's list:
- inside the pane's open clip (the viewport);
- inside the pane's open translate (the scroll offset), and any translate
  around the whole router (a drawer's sheet);
- at the section's top;
- without recording the section again.

`DrawList::append_from` cannot do this. It copies each instance's own
recorded clip, effects and transform, and it `debug_assert!`s that the
destination has no scope open, because those instances would escape it.
The tags are private, so libcontrols cannot re-tag them itself.

After this plan, `DrawList::append_scoped(&other, offset)` places a kept
list under the open scopes, in one pass over its instances.

Out of scope:
- any change to `append_from`, `Render for DrawList` or `PaneGate`
  (section 3.10);
- scale transforms (there is still no `push_scale`);
- a change to how `push_clip` treats an open transform.

## 2. What exists

- **Side channels.** Every instance has, parallel to `instances`: a
  1-based transform id (`instance_transforms`, `0` = none), an
  `Elevation`, a `DrawEffects` snapshot, a resolved `ClipRect`
  (`UNBOUNDED` when nothing clipped) and an `AtlasId`. `push` snapshots
  the tops of the open stacks into them.
- **Banks.** `chars` (text runs name `char_start`), `path_segments` (paths
  name `seg_start`) and `transforms` (named by the 1-based id). Each is
  private.
- **Clips are screen pixels.** `push_clip` intersects its rect with the open
  clip and is **not** moved by an open rotate or translate. The shader
  tests the clip against the output pixel. That is why libcontrols' router
  pushes a pane's clip at `vp - shift`.
- **Effects replace.** `begin_effects` pushes its value as is. An instance
  takes the top, so an inner scope replaces the outer one.
  `DrawEffects::default()` (blur 0, ombré 0) is what an instance gets with
  no scope open. The ombré is computed over the instance's own rect, so
  effects carry no coordinates.
- **Transforms compose.** `push_rotate` and `push_translate` push
  `compose(open, new)` onto the bank. Every entry is orthogonal (rotation
  plus translation), and quarter turns and translates compose exactly.
- **`translate_from(start, dx, dy)`** moves an instance range: `position`,
  the absolute points inside `SdfKind` (`BezierStroke`'s `c1`, `c2`,
  `end`; `Line`'s `end`) through `SdfKind::translate` (an exhaustive
  match), a path's own segments, bounded clips, and transforms (a moved
  copy `t' = t + d - M d` per distinct id, repointed).
- **`append_from(other)`** concatenates the three banks, rebases
  `char_start`, `seg_start` and non-zero transform ids by the lengths this
  list had, and copies every side channel unchanged.

## 3. Design

### 3.1 Name and signature

```rust
impl DrawList {
    /// Splice `other` onto the end of this list as if its calls were made
    /// now, moved by `offset`, inside the clip, effects and rotate/translate
    /// scopes open on this list.
    pub fn append_scoped(&mut self, other: &DrawList, offset: [f32; 2]);
}
```

- **`append_scoped`** is the name libcontrols' plan uses (decision 9). Like
  `append_from` it collides with no std method, so the drawing gates
  (`libhbui/tests/drawing_allowlist.rs` and its two siblings) can match
  `.append_scoped(` outright. Those gates need it added as a needle (open
  question 2).
- `_scoped` says what differs from `append_from`: the open scopes apply.
  `append_scoped_from` was considered. It is longer, and it still does not
  contain `append_from`, so it would need its own needle anyway.
- **`offset`** is in this list's current coordinates, the same space a
  `push_*` call made now would use. libcontrols' plan spelled the call
  without it (decision 9). Section 3.8 says why it is here, and that
  libcontrols can pass `[0.0, 0.0]`.

### 3.2 The rule

`append_scoped(other, d)` gives the same list as replaying `other`'s
recorded calls now, with every coordinate moved by `d`, under one rule that
differs from `push_clip`: **`other`'s own clips are mapped through the open
transform** (section 3.5). With no transform open, the result is exactly
the list replaying would give.

For each instance `i` of `other`, in order, with `O` the open clip
(`active_clip()`), `T` the open transform (`active_transform` top, or none)
and `E` the open effects (`active_effects` top, or none):

| Channel | Appended value |
|---|---|
| geometry | `other`'s, moved by `d` (section 3.3) |
| transform id | none open and `i` untagged: `0`; `T` open and `i` untagged: `T`'s id; `i` tagged `k`: `base + k` (section 3.4) |
| clip | `i` unbounded: `O`; `i` bounded `C`: `O.intersect(map_T(C moved by d))` (section 3.5) |
| effects | `i`'s own unless it is `DrawEffects::default()`; then `E` if open, else the default (section 3.6) |
| elevation, atlas, color, anim | copied |
| `char_start` / `seg_start` | plus this list's `chars` / `path_segments` length before the call |

### 3.3 Banks and geometry

1. Record `char_base = chars.len()`, `seg_base = path_segments.len()`,
   `transform_base = transforms.len()`.
2. Extend `chars` with `other.chars` unchanged. Glyph entries are atlas
   indices and advance deltas, not positions; `translate_from` does not
   touch them either.
3. Extend `path_segments` with `other.path_segments`, each moved by `d`
   with `GpuPathSegment::translate`. Every segment belongs to exactly one
   of `other`'s paths, so each moves once. Segments no instance names (none
   today) move too, which is harmless.
4. For each instance: copy it; add `d` to `position`; call
   `kind.translate(d)` (the exhaustive match, so a new kind with absolute
   points has to answer here as well); rebase `char_start` / `seg_start`.
   A path's bound is its `position`/`size`, so it moves with `position`.
   Text runs, terminal lines, markers and icons are `MsdfText` and move by
   `position`.
5. When `d == [0.0, 0.0]` the additions are skipped, so the copy is
   bit-identical to `append_from`'s (`-0.0 + 0.0` would otherwise turn a
   `-0.0` into `0.0`).

### 3.4 Transforms and rotations

`other`'s bank is mapped entry by entry, in order, so a tagged instance's
new id is `transform_base + k` exactly as in `append_from`. Every entry is
mapped, used or not, as `append_from` copies every entry. For `other`'s
entry `S = (M, t)`:
1. **Moved by `d`:** `S' = (M, t + d - M d)`, the same adjustment
   `translate_transforms_from` makes. With the geometry moved by `d`, `S'`
   draws `S`'s picture moved by exactly `d`: `M (v + d) + t + d - M d =
   S(v) + d`. Skipped when `d` is zero.
2. **Under the open transform:** `compose(T, S')` when `T` is open (`T`
   applied after `S'`, the order `push_rotate` composes in), else `S'`.

An untagged instance takes `T`'s id when one is open. No bank entry is
added for it.

The result per instance on screen is `T(S(v) + d)`: the recorded rotation,
then the placement, then the open scopes. Replaying `other` under `T` would
give the same picture. Replaying computes `compose(T, about(R, pivot + d))`
per scope, while this composes once per entry; the two are bit-equal for
quarter turns and translates with exactly representable sums, and within
rounding for `Radians` (section 4, test 6).

`other`'s own rotations nest inside the open ones. A rotation open in the
destination therefore rotates the whole appended list about its pivot,
which is what a rotated pane holding sections means.

### 3.5 Clips, and how nested clips intersect the open one

`other`'s resolved clip per instance is already the intersection of every
clip `other` had open around it (`push_clip` resolves at push time). So a
clip nested inside `other` needs nothing beyond the one intersection here:
`O ∩ map_T(C1 ∩ C2 + d) = O ∩ map_T(C1 + d) ∩ map_T(C2 + d)` for the
translate-only and quarter-turn maps.

- **Moved by `d`:** `min + d`, `max + d`. Translation commutes with the
  `max`/`min` of `intersect` exactly (rounding is monotone), so `(C1 ∩ C2)
  + d` equals `(C1 + d) ∩ (C2 + d)` bit for bit.
- **Mapped through `T`:** `map_T(R)` is the axis-aligned box of `T` applied
  to `R`'s four corners. It is `rotate_rect`'s computation, moved into a
  private `RotationTransform::map_rect(ClipRect) -> ClipRect` that
  `rotate_rect` then calls. It is exact for translates and quarter turns,
  whose image of a box is a box. Under a `Radians` rotation it is the
  bounding box, so ink of `other` outside its own rotated clip but inside
  that box shows. A scissor in screen pixels cannot do better.
- **Intersected with `O`:** `O.intersect(mapped)`. `ClipRect::intersect`
  is associative bit for bit, empty results included: per axis, `min` is
  always the largest min whatever the order, and an empty axis always ends
  at `max == min`. So the order in which `other`'s clips and the open one
  meet does not matter.
- **Unbounded:** an instance `other` did not clip takes `O` (which is
  `UNBOUNDED` when nothing is open). The `UNBOUNDED` sentinel is never
  moved or mapped.
- **Disjoint:** a clip of `other` that misses `O` gives an empty clip, as a
  nested `push_clip` does. The instance stays in the list and draws no
  pixels, as a row scrolled out of view does today.

**Why clips follow the open transform here and not in `push_clip`.** A
`push_clip` call is made by code that knows the screen and passes screen
pixels; libcontrols' router does this with its `shift`. A kept list was
recorded without knowing where it would be placed, in its own coordinates,
so its clips can only be correct on screen if they move with its ink. This
is what libcontrols' decision 9 asks for ("moved by the open translation
when bounded"); mapping through the whole open transform extends it to a
rotated pane. Changing `push_clip` to do the same would change every
existing caller, and is not proposed.

### 3.6 Effects

- An instance whose recorded effects are not `DrawEffects::default()` keeps
  them: `other` opened an effects scope around it, and an inner scope
  replaces an outer one.
- An instance recorded with the default (no effects scope open in `other`)
  takes `E`, the open effects, or the default when none is open.
- The one case this cannot tell apart: `other` explicitly opened
  `begin_effects(DrawEffects::default())` inside an `E`. Replaying would
  give the default; this gives `E`. Telling them apart needs one more side
  channel (a bit per instance: "an effects scope was open"). The plan does
  not add it (open question 3).
- Effects carry no coordinates, so `d` and `T` do not touch them.

### 3.7 Preconditions

- `other` has no scope open: `debug_assert!` that its `active_clips`,
  `active_effects` and `active_transform` are empty. Its open scopes have
  not been resolved, and `lower` would assert on them anyway.
- The destination may have any scopes open. That is the point.
- `append_scoped` does not open, close or change any scope.
  `pending_atlas` is untouched.
- `other` is borrowed, so it cannot be `self`.

### 3.8 Why an `offset` as well as `push_translate`

A caller can place `other` either way, with the same pixels:
1. `push_translate(dx, dy)`, `append_scoped(&other, [0.0, 0.0])`,
   `push_translate_end()`: libcontrols' step 6 placement. The ink moves on
   the GPU through one more bank entry per placement; the clips move
   through `map_T`.
2. `append_scoped(&other, [dx, dy])`: the ink and clips move on the CPU.
   No bank entry is added, and no transform is composed per placement.
   Untagged ink stays untagged when no transform is open. For a pane of
   sections that is one transform per section per frame fewer.

`offset` costs one addition per point, which the copy already pays for in
cache traffic. libcontrols may use either form; the second suits it
(section 3.9).

### 3.9 Placing sections (for libcontrols' step 6; not built here)

Inside the pane's `push_clip` and `push_translate(-off)`, for a section at
`top` (content coordinates):

```rust
list.append_scoped(&kept, [vp.x, vp.y + top]);
```

The ink lands at `[vp.x, vp.y + top - off]` plus any router origin, through
the open translate. The section's own clips land in the same place,
through `map_T`, and are intersected with the viewport. This is step 6's
last **Done when:** ("an instance pushed by a source at `[0, 0]` lands at
`[vp.x, vp.y + top - offset]` and carries the pane's clip").

libcontrols' plan is frozen. Its decision 9 spells the call without
`offset`, and step 6 places with `push_translate`. Passing `[0.0, 0.0]`
after its `push_translate` matches the plan as written; passing the top as
the offset is a small departure for libcontrols' `CHANGES.md`. Either
needs this plan's signature, which adds an argument to the spelling in
decision 9 (open question 1).

### 3.10 What does not change

- `append_from`: same body, same assertion, same results. Composing panes
  at top level stays its job.
- `Render for DrawList` keeps `append_from`, so `render_into` still asserts
  at top level. A scoped `Render` would be a separate decision.
- `PaneGate::draw` keeps `append_from`.
- The wire format, the shader and `lower` are unchanged: the appended
  instances carry ordinary tags.

### 3.11 Cost

O(instances + chars + segments + transforms) of `other`, in one pass each,
with each destination vector reserved once. Per instance: a copy, two to
eight additions (the points in its kind), one clip map (four corner
transforms and eight compares, or two additions when `T` is translate-only
or absent) and one intersection. Per bank entry of `other`: one adjustment
and one compose. No allocation besides the vectors' growth, no hashing and
no search: `translate_transforms_from`'s per-id lookup is not needed,
because every entry is mapped in order.

For libcontrols' 20 000-line case, a 1000 px window shows about 10 sections
of 10 lines each, so a frame appends roughly the instances of the visible
lines. Re-recording them would shape every line again.

## 4. Tests

In `src/drawlist/mod.rs`'s tests, beside the splice tests. "Equal" means
`instances`, `chars`, `path_segments`, every side channel and the lowered
frame (`frame_digest`, and `draws`/`param_bank`/`char_buffer`) are equal,
unless a test says otherwise. Coordinates and offsets are integers, so
every sum is exact.

1. **`append_scoped_at_top_level_is_append_from`.** `every_existing_kind()`
   plus a clip, an effects scope and a quarter rotation: `append_scoped(&x,
   [0.0, 0.0])` into an empty list with nothing open equals `append_from`.
2. **`an_appended_list_is_clipped_by_the_open_clip`.** Destination
   `push_clip([10, 10], [50, 50])`; `other` has an unclipped box at
   `[0, 0]` and one at `[100, 100]`. Both take the clip `[10, 10]-[60, 60]`.
   The lowered draws carry it. After `push_clip_end`, a box pushed is
   unbounded again.
3. **`an_appended_list_moves_by_its_offset`.** Offset `[30, -5]`: a box,
   a `Line` (both ends), a `BezierStroke` (all four points), an `Outline`,
   a path (bound and every segment) and a text run each move by exactly
   `[30, -5]`; equal to `append_from` followed by `translate_from(start,
   30, -5)` on the appended range.
4. **`a_nested_clip_intersects_the_open_one`.** `other` pushes
   `[0, 0]-[40, 40]`, inside it `[20, 20]-[80, 80]` (resolved
   `[20, 20]-[40, 40]`); destination clip `[25, 0]-[100, 100]`, offset
   `[5, 5]`. The inner instance's clip is `[25, 25]-[45, 45]`, the outer
   one's `[25, 5]-[45, 45]`. A disjoint pair gives an empty clip
   (`max == min`), not an inverted one.
5. **`a_kept_lists_clips_follow_the_open_translate`.** Destination
   `push_clip([0, 0], [100, 100])` (the viewport) then
   `push_translate(0, -30)`; `other` has a clip `[0, 40]-[100, 60]` around
   a box. The appended clip is `[0, 10]-[100, 30]`, and the box's transform
   is the open translate's id. A second case under
   `push_rotate(Quarter(1), p)`: the clip is `rotate_rect` of the moved
   clip about `p`, bit-exact.
6. **`rotations_nest_inside_the_open_one`.** Destination
   `push_rotate(Quarter(1), [50, 50])`; `other` has `push_rotate(Quarter(2),
   [10, 10])` around a box, plus an unrotated box. With offset `[20, 0]`:
   the rotated box's entry equals `compose(open, moved(S))` bit for bit and
   is at `transform_base + 1`; the unrotated box carries the open id; the
   bank grew by exactly one entry. A `Radians(0.3)` variant compares the
   lowered matrices with a tolerance of 1e-5.
7. **`effects_are_inherited_only_where_the_list_had_none`.** Destination
   `begin_effects(blur 2)`; `other` has a plain box and a box inside
   `begin_effects(ombré 0.5)`. The first gets blur 2, the second keeps
   ombré 0.5 with blur 0. With no destination scope, both keep their own.
8. **`text_and_paths_rebase_into_a_non_empty_list`** (`shaper` feature,
   with `text_fixture`). The destination already holds a text run and a
   path; `other` holds "ok" and a triangle. Their `char_start` / `seg_start`
   are offset by the destination's lengths, and the lowered char buffer and
   segment rows are the ones `other` lowered alone (segments moved by the
   offset).
9. **`a_scoped_append_lowers_to_what_recording_in_place_would_have`**
   (`shaper`). The equivalence test. A destination opens
   `push_clip([0, 0], [300, 200])` and `begin_effects(blur 1)`; then either
   `append_scoped(&pane, [40, 60])` where `pane` is `pane_one` plus a
   `Line`, a `BezierStroke`, a path and a quarter rotation, or the same
   calls made directly with every coordinate plus `[40, 60]` (clips
   included). The two lists are equal. No transform is open, so the rule of
   3.2 is exactly replaying. One allowance: a text run's `position` is
   derived from the pen with metrics that are not dyadic (`font_size` times
   the line-box ratio), so `(pen + d) - k` and `(pen - k) + d` may differ in
   the last bit; text positions are compared within 1e-4 px, everything
   else exactly.
10. **`under_an_open_translate_it_matches_a_router_that_shifts_its_clips`.**
   As test 9 inside `push_translate(0, -120)`, against a direct recording
   whose `push_clip` rects are moved by `[0, -120]` (libcontrols' `shift`).
   Equal.
11. **`appending_a_list_with_an_open_scope_asserts`** (`#[should_panic]`,
   debug builds): `other` with an unclosed `push_clip`.
12. **The existing splice tests are unchanged and pass**, showing
   `append_from` is untouched.

## 5. Versioning (Rule 5)

**Additive: a patch release.** One public method is added to `DrawList`. No
type, signature, field, variant or existing behaviour changes, and
`append_from`'s assertion stays.

- **The number follows release order.** The coding-agent glyphs plan (also
  a Draft, with the owner's answers) also proposes 0.6.1. Whichever of the
  two is released first is 0.6.1, and the other 0.6.2. This plan is
  written as "the next patch after 0.6.0". libcontrols' scrollbar plan
  names 0.6.1 for this call; it reads as "the release that has
  `append_scoped`".
- `libmsdf` and `libmsdf-font` stay in lockstep, as at 0.6.0: both crates'
  `version` move to the release's number and one tag covers both, though
  only `libmsdf` changes here.
- The `VERSIONS.md` entry lists `DrawList::append_scoped` as **Added**.
- **Pins move together.** libcontrols and flankr (the companion app, which
  pins the rev libcontrols pins so the graph holds one libmsdf) pin
  libmsdf by git rev, at `4de319c` today. Two revs are two crates to cargo,
  and a `DrawList` from one is not the other's type: a list built by
  libcontrols could not be passed to flankr's renderer. Both move to the
  release's rev in the same change; any other crate in the graph that pins
  libmsdf (the companion's comment names voicexn) moves with them.
  libcontrols' move is semver-compatible, so Rule 5.5 does not make its
  release breaking.

## 6. Steps

1. **The method.**
   - `RotationTransform::map_rect` (section 3.5), and `rotate_rect` calling
     it.
   - `DrawList::append_scoped` (sections 3.2-3.7), with a doc comment that
     states the rule of 3.2, the clip mapping and why it differs from
     `push_clip`, the effects rule and its one ambiguity, and the gate
     note on the name (as `append_from`'s does).
   - A sentence on `append_from`'s doc pointing to `append_scoped` for
     placing a kept list under open scopes, and the same on
     `src/drawlist/render.rs`'s note.

   **Done when:**
   - `cargo fmt -p libmsdf -- --check` and `cargo clippy -p libmsdf
     --all-targets` (no new warnings) are clean;
   - `cargo test -p libmsdf` passes, with the existing `rotate_rect` and
     splice tests unchanged.
2. **The tests.** Section 4's tests 1-11.

   **Done when:** `cargo test -p libmsdf` and `cargo test -p libmsdf
   --no-default-features` pass, the `shaper`-gated tests running in the
   first; each of tests 1-11 fails if its rule is removed from the method
   (checked once by hand, by breaking the method locally, and noted in the
   commit message).
3. **Release.**
   - Both crates' `version` at the next patch (section 5), the
     `VERSIONS.md` entry, the README row moved to the new status.
   - Tag `v<version>` on the version commit.
   - A `CHANGES.md` landing addendum, and this plan's amendment marking it
     Implemented with the commits of each step.
   - The release waits for the owner's approval.

   **Done when:**
   - fmt, clippy (no new warnings), the full test suite with `gpu-tests`
     and `scripts/check-font-features.sh` pass at the tagged commit;
   - the tag is on the version commit;
   - libcontrols can build its step 6 against the rev (the pin move itself
     is libcontrols' and flankr's commit, not this plan's).

## 7. Open questions

1. **The signature.** This plan adds `offset: [f32; 2]` to the call
   libcontrols' decision 9 spelled as `append_scoped(&mut self, other:
   &DrawList)`. libcontrols' frozen plan then calls it with `[0.0, 0.0]`
   (or with the section's top, recorded as a departure in its
   `CHANGES.md`). Keep the offset, or drop it and place only with
   `push_translate`?
2. **The drawing gates.** `.append_scoped(` should be a needle in
   `libhbui/tests/drawing_allowlist.rs` and its two siblings, as
   `.append_from(` is. Those are not in this repository. Who adds it?
3. **Explicit default effects.** A list that explicitly opens
   `begin_effects(DrawEffects::default())` to cancel an outer blur loses
   that when appended under a blur (section 3.6). No caller does this
   today. Accept it, or add a per-instance "effects scope open" bit so the
   append is exact?
4. **Clips through a `Radians` rotation** are their bounding box (section
   3.5), so a kept list's own clip is looser under an open free rotation
   than the ink's rotated rect. Nothing rotates a pane by a free angle
   today. Acceptable?
5. **`Render for DrawList` and `PaneGate`** stay on `append_from` (section
   3.10). Should either gain a scoped form later, as a separate plan?

## Amendment — 2026-10-28 23:39 EDT: approved; built as 0.6.1

The owner (2026-10-28), before the plan was written: "approve and build
when it's written". The plan is approved as written. Its open questions
keep the body's choices: the `offset` argument stays (libcontrols records
the extra argument as a departure when it calls it); the explicit-default
effects edge case and the loose clip under a free rotation are accepted;
the drawing-gate needles in libhbui are outside this repository and are
not changed here; `Render for DrawList` and `PaneGate` stay on
`append_from`. It ships before the coding-agent glyphs plan (a Draft), so
by release order it is **0.6.1** (libmsdf and libmsdf-font in lockstep),
and the glyphs plan takes the next patch. The status stays `Draft` until
the first implementing commit (Rule 1.5).
