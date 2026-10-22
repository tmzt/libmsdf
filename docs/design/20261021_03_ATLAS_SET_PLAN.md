---
title: Several atlases in one draw list, cached in the renderer
date: 2026-10-21
status: Implemented
covers: >
  An `AtlasSet` that holds several `FontAtlas`es under small ids and lays
  them out as one texture array and one glyph table; a scoped
  `push_atlas(id)` / `push_atlas_end()` on `DrawList`, in the style of
  `push_clip` and `push_rotate`, that tags each instance with the atlas its
  glyphs come from; lowering a list against a set; and a renderer that
  uploads a set once and draws any list in it. Strictly additive: lists and
  renderers that never use a set behave exactly as today. Released as 0.4.2.
sources:
  - >
    Owner (Tim), 2026-10-21, from Bubble Pop's session: "does it make more
    sense for a drawlist to support switching atlas and caching them in the
    renderer?", then "shouldn't we use a Set or something else, like
    push/pop matrix?", then "yes, write the plan in libmsdf".
  - src/drawlist/mod.rs (`DrawList::push_shaped_text`, `push_icon`, the
    clip and rotate stacks, `append_from`, `lower`, the char packing
    `(table_idx << 16) | delta`), src/gpu/mod.rs (`new_with_msdf_layers`,
    `upload_msdf_atlas`, `upload_glyph_table`, `render_draw_list`,
    `MAX_GLYPH_TABLE_ENTRIES` = 4096), src/gpu/sdf_render.wgsl (the glyph
    table's `g1.w` layer, `atlas_dim` from `textureDimensions`),
    font/src/atlas.rs (`FontAtlas`, its public fields, `glyph_table_u32s`,
    `to_rgba_bytes`, `MAX_ATLAS_LAYERS` = 256), font/src/glyph_table.rs
    (`GlyphEntry::to_gpu_u32s`).
related:
  - docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md (layers in the
    glyph table, which this builds on)
  - docs/design/20261021_02_RENDER_TRAIT_PLAN.md (a `Render` drawing inside
    a caller's atlas scope inherits it, as it inherits a clip)
  - Bubble Pop (~/src/bubble), which today keeps two renderers and two
    lists (Fredoka text, Roboto with Material Symbols icons) and so splits
    a piece that uses both into two `Render` structs. Moving it onto a set
    is that repository's own change, not part of this plan.
---

# Several atlases in one draw list, cached in the renderer

## 1. Goal

A `DrawList` can draw from only one atlas. Each glyph is packed as an index
into the renderer's single glyph table, and the renderer holds one atlas
texture. An application with two faces keeps two renderers and two lists,
draws each list in its own pass, and cannot interleave the faces. A label
from one face can never sit under an icon from the other unless the lists
are drawn in that order. Every piece that uses both faces is split in two.

This plan lets one list draw from several atlases and lets the renderer
keep all of them uploaded:
- **Renderer side, a set:** `AtlasSet` holds the atlases under small ids. It
  lays them out as one texture array (each atlas's layers, one after
  another) and one glyph table (each atlas's entries, one after another).
  The renderer uploads the set once and keeps it.
- **List side, a scope:** `push_atlas(id)` and `push_atlas_end()` work like
  `push_clip` and `push_rotate`. The stack is resolved at push time and
  each instance records the atlas id on top of it.
- **Lowering:** a list is lowered against the set. Each text instance's
  glyph indices are offset by its atlas's first entry in the merged table,
  so the shader and the instance format do not change.

## 2. Why a scope, and not a tag taken from the atlas argument

`push_shaped_text` and `push_icon` already take `&FontAtlas`, so the first
idea was to record the atlas from that argument and need no scope. It does
not work without a breaking change:
- **The argument has no identity.** `FontAtlas` has no id, and all its
  fields are public, so adding one is a field added to an exhaustive public
  struct: a breaking change under Rule 5.4, and a 0.5.0.
- **The alternatives are worse.** Hashing the atlas on every push costs
  time per glyph run. Comparing addresses breaks silently when a caller
  clones an atlas.

So the scope carries the id, and the atlas argument still supplies the
glyph indices and metrics, as today. The two could disagree, for example
`push_atlas(B)` followed by `push_shaped_text(.., &a, ..)`. That is caught
in debug builds (section 4.3).

## 3. The set

In `src/drawlist/atlas_set.rs`, public as `libmsdf::drawlist::AtlasSet`
and re-exported at the root as `libmsdf::{AtlasSet, AtlasId}`:

```rust
use crate::font::atlas::FontAtlas;

/// An atlas's place in an [`AtlasSet`], in registration order. The first
/// atlas registered is `AtlasId(0)`, the id every list starts with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct AtlasId(pub u16);

/// Several atlases laid out as one texture array and one glyph table.
#[derive(Default)]
pub struct AtlasSet {
    atlases: Vec<FontAtlas>,
    /// Per atlas: its first entry in the merged glyph table, and its first
    /// layer in the merged texture array.
    table_base: Vec<u32>,
    layer_base: Vec<u32>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AtlasSetError {
    /// The set would hold more layers than `MAX_ATLAS_LAYERS` (256).
    TooManyLayers { have: u32, adding: u32 },
    /// The merged glyph table would pass 65,536 entries, the most a draw
    /// list's 16-bit glyph index can address.
    TooManyGlyphs { have: u32, adding: u32 },
    /// The set already holds 65,536 atlases.
    Full,
}

impl AtlasSet {
    pub fn new() -> Self;
    /// Takes `atlas` into the set and returns its id.
    pub fn register(&mut self, atlas: FontAtlas) -> Result<AtlasId, AtlasSetError>;
    /// The atlas under `id`, for shaping and for the `push_*` calls.
    /// Panics on an id this set did not hand out.
    pub fn atlas(&self, id: AtlasId) -> &FontAtlas;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    /// The texture array's size: the widest and tallest atlas, and every
    /// atlas's layers added up.
    pub fn extent(&self) -> [u32; 2];
    pub fn layers(&self) -> u32;
    /// Every atlas's first glyph-table entry, indexed by id.
    pub fn table_bases(&self) -> &[u32];
    /// The merged glyph table, 8 u32s per entry (as `glyph_table_u32s`),
    /// each entry's layer (`g1.w`) moved up by its atlas's first layer.
    pub fn glyph_table_u32s(&self) -> Vec<u32>;
    /// The merged texels as RGBA8, layer after layer, each atlas's layers
    /// placed at the top left of an `extent()`-sized layer, with the rest
    /// zero.
    pub fn to_rgba_bytes(&self) -> Vec<u8>;
}
```

**Why padding is safe.** The shader takes a cell's position in texels from
the glyph table and divides by `textureDimensions` (`atlas_dim`). A smaller
atlas placed at a layer's top left keeps every cell's texel position, and
the larger divisor gives the right UV. The zeroed margin is never sampled,
because the shader checks cell bounds before it samples. The cost is
memory: the padded area of the smaller atlases' layers.

**Channels.** `to_rgba_bytes` expands each atlas with that atlas's own
`FontAtlas::to_rgba_bytes`, so atlases with different channel counts can
share a set.

## 4. The draw list

### 4.1 The scope

```rust
impl DrawList {
    /// Draw the glyphs pushed until the matching `push_atlas_end` from
    /// atlas `id` of the set the list is lowered with. Nests like
    /// `push_clip`. Outside any scope, instances are `AtlasId(0)`.
    pub fn push_atlas(&mut self, id: AtlasId);
    pub fn push_atlas_end(&mut self);
    /// The atlas instance `index` was pushed under.
    pub fn instance_atlas(&self, index: usize) -> AtlasId;
}
```

- **Storage:** a stack `active_atlas: Vec<AtlasId>`, and a parallel
  `instance_atlas: Vec<AtlasId>` filled in `DrawList::push` with the top of
  the stack, or `AtlasId(0)` when it is empty, beside the clip, transform,
  elevation and effects vectors. `clear` clears both.
- **Every instance gets an id,** not only text. The id is read only for
  `MsdfText`. Recording it for everything keeps the parallel vectors the
  same length, which `lower` already asserts for the other four.
- **Open scopes:** `lower` and `lower_with` debug-assert that no atlas
  scope is open, with the same wording as the clip and rotate assertions.
  `append_from` asserts the destination has none open, as it does for
  clips. It copies the source's ids unchanged, so a kept list keeps its
  atlases. The two lists must be lowered against the same set.
- **`translate_from`** moves positions only, so ids are unchanged.

### 4.2 Lowering

```rust
impl DrawList {
    /// `lower`, against a set: each `MsdfText` instance's glyph indices are
    /// moved up by its atlas's first entry in the set's merged table. The
    /// instances, params and draw order are what `lower` gives.
    pub fn lower_with(&self, set: &AtlasSet) -> SdfFrame;
}
```

- **The char buffer:** `lower_with` copies `chars` and, for each
  `MsdfText` instance, adds `set.table_bases()[id]` to the high 16 bits of
  each char in `char_start..char_start + char_count`. The low 16 bits (the
  advance delta) are untouched. The buffer is shared between instances
  only by position, so each char is rewritten exactly once.
- **Overflow:** `register` refuses a set past 65,536 entries, so a rewritten
  index never passes 16 bits.
- **An id the set does not hold** panics, naming the id and the set's
  length. It is a programming error, like an unclosed scope.
- **`lower` itself is unchanged.** It debug-asserts that every instance is
  `AtlasId(0)`: a list that used `push_atlas` must be lowered with
  `lower_with`, and lowering it without the set would draw from the wrong
  cells.

### 4.3 The agreement check (debug builds only)

Under `cfg(debug_assertions)`, the list also keeps
`instance_atlas_addr: Vec<usize>`: the address of the `&FontAtlas` given to
the `push_shaped_text` that made each `MsdfText` instance, and 0 for every
other instance. `lower_with` asserts, for each text instance, that
`std::ptr::eq(set.atlas(id), addr)`. The message names the instance, the
scope's id and both addresses, and says the glyphs were shaped against an
atlas other than the scope's (often a clone, or a missing `push_atlas`). In
release builds the field does not exist and the scope is trusted.

## 5. The renderer

```rust
impl GpuSdfRenderer {
    /// A renderer whose MSDF texture array and glyph table are sized for
    /// `set`: `set.extent()` by `set.layers()`, and a glyph table of
    /// `max(4096, entries)`.
    pub fn new_with_atlas_set(device, surface_format, set: &AtlasSet) -> Self;
    /// Upload every atlas in `set` and the merged glyph table. Call once,
    /// and again only when the set changes. Panics, as
    /// `upload_msdf_atlas` does, if the texture was made for a smaller set.
    pub fn upload_atlas_set(&self, queue: &wgpu::Queue, set: &AtlasSet);
    /// `render_draw_list`, lowering with `list.lower_with(set)`.
    #[allow(clippy::too_many_arguments)]
    pub fn render_draw_list_in(
        &self, device, queue, target, width, height, scale,
        list: &DrawList, set: &AtlasSet, time_ms: f32,
    );
}
```

The atlases are cached in the GPU texture and glyph table that
`upload_atlas_set` fills. Each frame uploads only the chars and params, as
`render_draw_list` does today. The shader does not change: it already reads
each glyph's layer from the table (`g1.w`).

## 6. Tests

In `tests/atlas_set.rs`:
- **`register`:** ids are handed out in order. `extent` is the widest and
  tallest atlas, and `layers` the sum. `table_bases` and the layer bases
  are the running sums. A set past 256 layers is refused with
  `TooManyLayers`, and one past 65,536 entries with `TooManyGlyphs`.
- **The merged table:** each atlas's entries appear in order, unchanged
  except `g1.w`, which is moved up by that atlas's first layer.
- **The merged texels:** each layer is `extent` sized, and each atlas's
  texels sit at its layer's top left with zeros elsewhere.
- **The scope:** instances pushed outside any scope are `AtlasId(0)`.
  Nested scopes tag by the innermost. `push_atlas_end` restores the outer
  id. `append_from` keeps the source's ids, and panics with an atlas scope
  open in the destination. `lower` panics in debug with an open scope, or
  with a non-zero id.
- **`lower_with`:** for a list with text from atlas 0 and atlas 1, atlas 0's
  chars are unchanged and atlas 1's high 16 bits are moved up by its table
  base, with the low 16 bits untouched. Draws and params equal `lower`'s.
  A list with only atlas 0 gives exactly `lower`'s frame.
- **The agreement check** (debug only): shaping against a clone of the
  scope's atlas panics in `lower_with`.

On the GPU, skipped where there is no adapter, as in tests/atlas_layers.rs:
- **One list, two atlases:** a line from the bundled Roboto atlas, then an
  icon from the bundled Roboto with Material Symbols atlas, then a second
  line from Roboto, drawn by one renderer with `render_draw_list_in`. Each
  piece's pixels equal the same piece drawn alone by a renderer holding only
  its own atlas.

## 7. Release

Strictly additive. These are new types (`AtlasSet`, `AtlasId`,
`AtlasSetError`) and new methods on `DrawList` and `GpuSdfRenderer`, and
nothing existing changes for a list that never calls `push_atlas`. So
under Rule 5.4 it releases as **0.4.2**, tagged `v0.4.2`, with its
`VERSIONS.md` entry and `libmsdf-font` in lockstep.

## 8. Steps

1. **The set:** `src/drawlist/atlas_set.rs`, the re-exports, and section
   6's set tests.
   **Done when:** the tests pass, with fmt and clippy clean.
2. **The scope and lowering:** `push_atlas`, `push_atlas_end`,
   `instance_atlas`, `lower_with`, the debug agreement check, the
   assertions in `lower` and `append_from`, and section 6's list tests.
   **Done when:** they pass, the whole suite still passes, and fmt and
   clippy are clean.
3. **The renderer:** `new_with_atlas_set`, `upload_atlas_set`,
   `render_draw_list_in`, the GPU test, and a README paragraph with a short
   example: register two atlases, scope an icon, render.
   **Done when:** the GPU test passes on this machine's adapter, and fmt
   and clippy are clean.
4. **Release 0.4.2** (Rule 5).
   **Done when:** the version, its `VERSIONS.md` entry and the tag are in.

## 9. Open questions

1. **A 0.5.0 instead:** give `FontAtlas` an id, `#[non_exhaustive]` or a
   private field, so lists tag glyphs from the atlas argument and need no
   scope (section 2). It is breaking for every crate that builds a
   `FontAtlas` with a struct literal.
2. **Per-atlas sizes:** pad every layer to the largest atlas (this plan),
   or let the shader read a per-layer size from a small table. The second
   saves memory when the sizes differ widely, but it changes the shader.
3. **`AtlasShaper` borrowing from the set:** an application that shapes
   with `AtlasShaper::new(&atlas, style)` would shape with
   `AtlasShaper::new(set.atlas(id), style)`. Is a helper on the set worth it?

## Amendment — 2026-10-21 23:16 EDT: handles replace the scope

On the owner's question ("couldnt it just return a handle/id?") and
agreement, instances are tagged from a handle passed to the push calls, not
from a scope. Where this amendment and the text above disagree, this
amendment wins.

**Replaced:** section 2 entirely; section 4.1 (`push_atlas`,
`push_atlas_end`, the `active_atlas` stack and its open-scope
assertions); section 4.3 (the debug-only address check); `AtlasId` and
`AtlasSet::atlas` in section 3; the scope and agreement-check tests in
section 6; open question 1. Everything else stands. That covers the
per-instance id vector filled in `DrawList::push`, `lower_with(&set)`,
`lower` asserting every id is the default, `append_from` copying ids,
`translate_from`, and the renderer of section 5.

**Ids name their set.**

```rust
/// An atlas's place in an [`AtlasSet`]: which set, and which atlas in it.
/// `AtlasId::default()` (`set: 0, index: 0`) is what instances pushed
/// with a plain `&FontAtlas` record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct AtlasId {
    /// The set's number, from a global `AtomicU32` counter starting at 1
    /// when the set is created (`AtlasSet::new`); 0 is no set.
    pub set: u32,
    /// The atlas's place in the set, in registration order.
    pub index: u16,
}

/// A handle to an atlas in a set: its id and the atlas. It derefs to
/// `FontAtlas`, so shaping (`AtlasShaper::new(&*handle, style)`) and
/// metrics work through it.
#[derive(Clone, Copy)]
pub struct AtlasRef<'a> {
    pub id: AtlasId,
    pub atlas: &'a FontAtlas,
}
impl std::ops::Deref for AtlasRef<'_> { type Target = FontAtlas; /* self.atlas */ }

impl AtlasSet {
    /// As before: takes `atlas` into the set and returns its id.
    pub fn register(&mut self, atlas: FontAtlas) -> Result<AtlasId, AtlasSetError>;
    /// The handle for `id`. Panics on an id of another set, or one this
    /// set did not hand out.
    pub fn atlas(&self, id: AtlasId) -> AtlasRef<'_>;
    /// This set's number, as its ids carry it.
    pub fn number(&self) -> u32;
}
```

**The push calls.**

```rust
impl DrawList {
    /// `push_shaped_text` from an atlas in a set: the same drawing, with
    /// the handle's id recorded on the instance.
    pub fn push_shaped_text_with(&mut self, run: &ShapedRun, atlas: AtlasRef<'_>,
        pos: [f32; 2], font_size: f32, px_range: f32, color: [f32; 4]) -> f32;
    /// `push_icon` likewise.
    pub fn push_icon_with(&mut self, run: &ShapedRun, atlas: AtlasRef<'_>,
        rect_pos: [f32; 2], rect_size: [f32; 2], em: f32, px_range: f32,
        color: [f32; 4]) -> Option<[f32; 2]>;
    /// The atlas instance `index` was pushed from.
    pub fn instance_atlas(&self, index: usize) -> AtlasId;
}
```

- **The existing calls are unchanged.** `push_shaped_text` and `push_icon`
  keep taking `&FontAtlas` and record `AtlasId::default()`. Every
  non-text instance records the default too.
- **The ids are recorded by the push calls.** The existing bodies move into
  private functions that take the id, and the public calls pass the
  default or the handle's id. `DrawList::push`, the one place instances
  enter the list, takes the id from a private field set for the length of
  that call, so no public signature changes.
- **`lower_with(&set)` checks the set,** in release builds too, since it
  is one compare per text instance. It panics on any `MsdfText` instance
  whose id is neither the default nor one of `set.number()`'s, naming the
  instance and both set numbers. Instances with the default id use the
  set's atlas 0, as section 4.2 had for unscoped instances.
- **A list that never uses a set** is lowered by `lower` exactly as today.

**Tests** (replacing section 6's scope and agreement tests):
- `push_shaped_text_with` and `push_icon_with` record the handle's id.
  The plain calls and every non-text instance record the default.
  `append_from` keeps the source's ids.
- Each set gets its own number, and `set.atlas(id)` of another set's id
  panics.
- `lower_with` panics on text from another set's handle. A list of plain
  calls lowered with a set holding the same atlas at index 0 gives exactly
  `lower`'s frame.
- The GPU test uses the two handles in place of scopes.

**Steps:** step 2 becomes "the handle calls and lowering:
`push_shaped_text_with`, `push_icon_with`, `instance_atlas`,
`lower_with`, the assertion in `lower`, and the tests above". Its Done
when is unchanged. Step 3's README example uses handles in place of a
scope.

**Release:** still strictly additive (new types and methods, nothing
existing changed), so 0.4.2.

**Open questions:** question 1 (a 0.5.0 with an id on `FontAtlas`) is
withdrawn, since handles tag glyphs without a breaking change. Questions 2
and 3 stand.

## Amendment — 2026-10-21 23:26 EDT: no panics in release; standard atlas sizes

On the owner's agreement, after two questions: "does lower_with normally
panic?" and "the atlas is based on sorting, what's the issue with the atlas
size?". Where this amendment and the text above disagree, this amendment
wins.

**1. `lower_with` never panics in release.** Lowering in libmsdf does not
panic in release builds. `lower` uses `debug_assert!` for programmer
errors (unclosed scopes, vectors out of step), drops filled paths it cannot
fit with a `log::warn!` count, and the shader skips a glyph whose layer is
missing rather than draw the wrong one. `lower_with` follows the same
pattern. This replaces section 4.2's "An id the set does not hold panics",
the previous amendment's "`lower_with(&set)` checks the set, in release
builds too … It panics on any `MsdfText` instance whose id is neither the
default nor one of `set.number()`'s", and its "Panics on an id of another
set" on `AtlasSet::atlas`:
- **A text instance with a foreign id** (another set's number, or an index
  this set did not hand out):
  - debug builds: a `debug_assert!` naming the instance, its id and the
    set's number;
  - release builds: the instance is left out of the frame and draws
    nothing. The count goes to one `log::warn!` per lowering, as
    `paths_dropped` does. Every other instance lowers as it would have.
- **`AtlasSet::atlas(id) -> Option<AtlasRef<'_>>`:** `None` for an id of
  another set or one this set did not hand out.
- **Tests,** replacing "`lower_with` panics on text from another set's
  handle" and "`set.atlas(id)` of another set's id panics":
  - debug builds: lowering text from another set's handle panics
    (`#[should_panic]`, `#[cfg(debug_assertions)]`);
  - release builds (`#[cfg(not(debug_assertions))]`): that instance is
    missing from the frame, and every other draw, the chars of the other
    instances and the params equal the frame without it;
  - `set.atlas` of another set's id, or of an index past the set's end, is
    `None`.

**2. Atlas sizes: padding to the widest is the design.** The baker places
uniform cells on a fixed grid, so atlases come in standard sizes. The
bundled 48 px atlases are 400 x 2000 (8 columns: Roboto Regular, Bold,
Italic, Roboto with Material Symbols, plain and styled) or 800 x 2000 (16
columns: Grenze Gotisch, Texturina), and Bubble Pop's Fredoka SemiBold 48
is 400 x 2000. A set of same-width atlases needs no padding. A set mixing 8-
and 16-column atlases pads each 400-wide layer to 800, about 3.2 MB of RGBA8
per padded layer. Section 3's padding stands as written. Open question 2
(a per-layer size read by the shader) is closed: it would change the
shader to save memory only in that mixed case.

Open question 3 stands.

## Amendment — 2026-10-21 23:29 EDT: approved, with the shaping helper; implementation started

The owner approved this plan with its amendments ("approve the atlas set
plan and push 0.4.2, add the helper"). Open question 3 is answered: add the
helper. It goes on the handle, since a handle is what a caller holds:

```rust
impl AtlasRef<'_> {
    /// `AtlasShaper::new(self.atlas, style)`: a shaper over this atlas's
    /// layout tables, for runs to push with `push_shaped_text_with`.
    pub fn shaper(&self, style: GlyphStyle) -> Result<AtlasShaper, NoLayout>;
}
```

`AtlasShaper` owns a copy of its face's tables and borrows nothing, so the
shaper outlives the handle. It is tested with the set: a shaper from a
handle shapes a run exactly as `AtlasShaper::new` over the same atlas
does. It is added in step 1 with the set.

Implementation starts with step 1 in the commit that adds this amendment:
the status is In implementation, and the document is now immutable. It
changes only on the owner's explicit instruction.

## Amendment — 2026-10-21 23:37 EDT: implemented

Every step is done and its Done when holds; the status is Implemented.

- Step 1, the set: 95fa12d (`AtlasSet`, `AtlasId`, `AtlasRef` with
  `shaper`, `AtlasSetError`, the re-exports, six tests).
- Step 2, the handle calls and lowering: 12951be (`push_shaped_text_with`,
  `push_icon_with`, `instance_atlas`, `lower_with`, the assertion in
  `lower`; twelve tests in debug, eleven in release with the release-only
  drop test; the full suite passes).
- Step 3, the renderer: 780d217 (`new_with_atlas_set`, `upload_atlas_set`,
  `render_draw_list_in`; the GPU test passes on this machine's adapter,
  one renderer drawing both atlases pixel for pixel as two renderers do)
  and c66c812 (the README paragraph).
- Step 4, the release: 345db37, `libmsdf` and `libmsdf-font` at 0.4.2 in
  lockstep, the `VERSIONS.md` entry, and the tag `v0.4.2`.

One departure from section 5 is recorded in CHANGES.md: `upload_atlas_set`
logs a set too large for the renderer as an error and leaves it out, as
`upload_msdf_atlas` does, rather than panicking.
