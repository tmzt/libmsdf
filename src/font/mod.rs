//! Font shaping, MSDF atlas generation, and glyph table management.
//!
//! Extracted from matter-stream's `matterstream-font` (same author), with
//! atlas management (dynamic append + eviction) and glyph-outline extraction
//! (for the compute-shader MSDF path) added for Highbay.
//!
//! - **`shaper`** — rustybuzz (HarfBuzz port) text shaping
//! - **`atlas`** — Multi-Channel Signed Distance Field (MSDF) atlas
//!   (loading is wasm-clean; CPU baking via msdfgen is native-only)
//! - **`glyph_table`** — GPU-uploadable glyph metrics table
//! - **`packer`** — shelf bin packer for atlas cells
//! - **`manager`** — dynamic atlas region allocation + eviction hooks
//! - **`outline`** — glyph outline → colored edge list (compute MSDF input)

pub mod atlas;
pub mod glyph_table;
pub mod manager;
pub mod outline;
pub mod packer;
pub mod shaper;

pub use atlas::{
    ATLAS_COLS, ATLAS_HEADER_SIZE, ATLAS_MAGIC, ATLAS_ROWS, ATLAS_VERSION, CAP_TOP_FRAC,
    CELL_EM_RATIO, FALLBACK_BASELINE_FRAC, FALLBACK_CAP_HEIGHT_EM, FALLBACK_MAX_INK_DESCENT_EM,
    FontAtlas, FontAtlasBuilder,
    GlyphProjection, SetMetrics, atlas_capacity, cap_height, glyph_projection,
};
pub use glyph_table::GlyphEntry;
pub use manager::{AtlasManager, AtlasRegion};
pub use outline::{Edge, EdgeKind, GlyphOutline, extract_outline};
pub use packer::ShelfPacker;
pub use shaper::{ShapedGlyph, ShapedRun, TextShaper};

/// **The TEXT coverage of the bundled faces, and there is exactly one of it.**
///
/// Inclusive codepoint ranges. Every one of them is queued into
/// [`GlyphSet::Text`], so the order they are WRITTEN in here is no longer the
/// order the atlas packs them: cells go in `(set, glyph id)` order, and this
/// list can be reordered, or a range widened, without renumbering a cell that
/// a range below it already owns. (It used to be load-bearing, and
/// [`FontAtlasBuilder::add_shipped_coverage`] carried a `debug_assert` about
/// its first element to keep it that way.)
///
/// * `U+0020..=U+007E` printable ASCII.
/// * `U+00A0..=U+00FF` **Latin-1 Supplement**, the widening: it is what a
///   European name needs (`José`, `Müller`, `Ångström`), it is contiguous, and
///   it stops nowhere near [`PRIVATE_USE`].
///
/// What is NOT here still has an answer, and it is a visible one: a codepoint
/// outside these ranges shapes to glyph 0, which the bundled faces draw as a
/// hollow box (see [`ROBOTO_REGULAR_ASCII`]). There is no scrubbing step
/// between arbitrary text and this list, and no panic if text steps outside it.
pub const TEXT_RANGES: &[(char, char)] = &[('\u{0020}', '\u{007E}'), ('\u{00A0}', '\u{00FF}')];

/// **The carveout our own glyphs live in**, and the reason [`TEXT_RANGES`] can
/// grow without a codepoint audit.
///
/// The Unicode Basic Multilingual Plane's Private Use Area. Every glyph this
/// repo owns rather than borrows — the [`MSYMBOLS_ICONS`] set at Material
/// Symbols' own codepoints, and the [`HIGHBAY_ICONS_BLOCK`] and [`MARKERS`]
/// blocks we draw ourselves — sits inside it, and Unicode guarantees no
/// standard character ever will. So the two halves of a bundled face cannot
/// collide by construction rather than by review:
/// [`FontAtlasBuilder::add_shipped_coverage`] queues this range as *whatever
/// the face defines here*, never as a list of names, and a `debug_assert` in
/// [`msymbols_codepoint`]'s test pins the icons inside it.
///
/// Widening [`TEXT_RANGES`] toward it is the one thing that could break that,
/// which is why they are stated together, one screen apart.
pub const PRIVATE_USE: (char, char) = ('\u{E000}', '\u{F8FF}');

/// **The top 256 codepoints of [`PRIVATE_USE`] are OURS**, and the boundary is
/// one comparison: `cp >= U+F800`.
///
/// Everything above this line is drawn in `fonts/` by a script in this repo;
/// everything below is borrowed at some vendor's codepoints (the nine
/// [`MSYMBOLS_ICONS`] top out at `U+F0D3`, more than two thousand codepoints
/// clear). `owned_blocks_are_the_top_of_the_carveout` checks it rather than
/// leaving it to memory.
///
/// The 256 split in two, because *edge marker* and *UI icon* are different
/// kinds of thing and the namespace should say so rather than a comment:
///
/// * [`MARKERS`], the last sixteen — geometry the RENDERER reaches for.
/// * [`HIGHBAY_ICONS_BLOCK`], the 240 below them — names an APP asks for.
///
/// The atlas says so too: they are [`GlyphSet::Markers`] and
/// [`GlyphSet::OwnedIcons`], two sets rather than one, which is what keeps a
/// growing icon vocabulary from moving the arrowhead's cell.
pub const OWNED_BLOCKS: (char, char) = ('\u{F800}', '\u{F8FF}');

/// **The block our own UI ICONS are allocated from** — names this repo owns,
/// as opposed to the Material Symbols names it borrows.
///
/// `U+F800..=U+F8EF`: the bottom 240 of [`OWNED_BLOCKS`], directly below
/// [`MARKERS`]. Codepoints are handed out *upward* from `U+F800` for the same
/// reason markers run upward — a name added above every existing one is a pure
/// append to the block, so no icon that has already shipped is renumbered
/// (`fonts/icon.py` records what that renumbering cost when the codepoints
/// were still `ICON_BASE + index`). What keeps the icon's atlas CELL where it
/// was is [`GlyphSet::OwnedIcons`] plus `icon.py` appending glyph ids, since
/// cells are laid out in `(set, glyph id)` order — but the two rules point the
/// same way, and allocating downward would mean renumbering a shipped
/// codepoint to no purpose.
///
/// It is 15x the size of [`MARKERS`] because the two grow at completely
/// different rates: the marker set is an arrowhead and whatever cardinality
/// adornments a diagram needs, and it is done in single digits, whereas an
/// application's icon vocabulary is the one that actually accumulates. Sized
/// once, generously, so the block never has to move — moving it downward later
/// is the one change that would renumber cells that already shipped.
///
/// What is IN it is [`HIGHBAY_ICONS`], which is a separate statement: this is
/// the address space, that is the manifest.
pub const HIGHBAY_ICONS_BLOCK: (char, char) = ('\u{F800}', '\u{F8EF}');

/// **The block inside [`PRIVATE_USE`] we DRAW, rather than borrow** — edge
/// markers for diagram arcs: the arrowhead, and whatever cardinality
/// adornments follow it.
///
/// Sixteen slots at the very TOP of the Private Use Area, allocated *upward*
/// from [`MARKER_ARROW`]. Both halves of that matter:
///
/// * **At the top**, because Material Symbols' codepoints are the vendor's and
///   run far below here (the bundled nine top out at `U+F0D3`), so a borrowed
///   icon and a drawn marker can never land on the same codepoint — checked by
///   `icons_sort_below_the_marker_block`, not by remembering.
/// * **Upward**, so a marker added above every existing one never renumbers a
///   marker that has already shipped. Its atlas CELL is held still by a
///   different rule — [`GlyphSet::Markers`] is its own set and `fonts/marker.py`
///   appends glyph ids, so a new marker sorts last within the set and moves
///   nothing before it.
///
/// [`HIGHBAY_ICONS_BLOCK`] sits immediately below, so the two owned blocks are
/// contiguous and a marker still sorts last of everything in the face. Their
/// CELLS are the other way round on purpose: [`GlyphSet::Markers`] is baked
/// before [`GlyphSet::OwnedIcons`], because the icon vocabulary is the one that
/// accumulates and a set that grows must come after one that does not.
///
/// The outlines themselves are authored in `fonts/marker.py`, which is where
/// the geometry is decided; `marker_contract_holds` asserts what Rust relies on
/// against the shipped bytes.
pub const MARKERS: (char, char) = ('\u{F8F0}', '\u{F8FF}');

/// **The arrowhead**: a filled triangle, tip forward at `+x`, half a
/// [`MARKERS`] block's worth of room above it for what comes next.
///
/// Drawn through [`crate::DrawList::push_marker`], which anchors it by its
/// advance-width point and rotates it to a curve's tangent. It is not an icon
/// and is deliberately not in [`MSYMBOLS_ICONS`]: an icon is a *name a
/// developer types* and a missing one is a missing asset to report
/// ([`msymbols_codepoint`]), whereas this is geometry the renderer reaches for
/// itself.
pub const MARKER_ARROW: char = '\u{F8F0}';

/// **Which vocabulary a glyph was baked from — and, in this declaration order,
/// where its cell goes.**
///
/// # What this replaces
///
/// A glyph's atlas cell used to be decided by WHEN it was queued.
/// [`FontAtlasBuilder::add_shipped_coverage`] called `add_ascii`, then
/// `add_shaped_ascii`, then scanned the Private Use Area, then the rest of
/// [`TEXT_RANGES`], and the packer took them in exactly that order. So the
/// layout was ALREADY grouped by vocabulary — by accident of call sequence.
/// Nothing declared it, nothing preserved it, and swapping two of those calls
/// silently renumbered every cell from the first one on. The atlas also
/// carried a `HashMap<u16, usize>` rebuilt on every load, which existed for no
/// other reason than that the entries had no order worth binary-searching.
///
/// This is the order, stated. The builder records the set at the moment a
/// glyph is QUEUED — the only moment it is known, because a glyph id cannot be
/// asked afterwards which vocabulary asked for it — and cells are placed in
/// `(set, glyph id)` order.
///
/// # Why the set id and not the codepoint
///
/// The codepoint space is sparse and it is not ours: borrowed icons sit at
/// Material's scattered `U+E0xx`..`U+F0xx`, ours at `U+F800`, text is
/// elsewhere again. Sorting by codepoint would interleave a vendor's
/// allocation decisions with our own, and a lookup could not use the result
/// anyway — the key a cell is fetched by is a GLYPH ID, which is what the
/// shaper hands back. Glyph ids are not ours either: re-merge Roboto with
/// Material Symbols differently and every id moves (in the shipped merged face
/// the borrowed icons are glyphs 111..=119 and 231..=234, two merge waves,
/// with 107 Latin-1 glyphs sitting between them). The SET is the part of the
/// order that is ours; the glyph id orders within it, and it does so
/// append-only because every script in `fonts/` appends.
///
/// # The order runs most-fixed to most-fluid
///
/// Growth in the LAST set is a pure append: no earlier set's cells move. So
/// the sets are declared in ascending order of how much they can still change,
/// which is what makes `a_later_set_never_moves_an_earlier_sets_cells` a
/// property of the design rather than a coincidence of today's contents.
///
/// It also buys a property across the two bundled faces: the borrowed half is
/// LAST, so [`ROBOTO_REGULAR_ASCII`] and [`ROBOTO_ASCII_MSYMBOLS`] bake to the
/// same 211 cells in the same places, and the merged face simply appends its
/// 13 borrowed ones. Before this the merged bake shifted Latin-1 and the
/// placeholder by 13 cells relative to the plain one, because a vendor's
/// `U+E0xx` sorts below our `U+F8xx` in a codepoint scan.
///
/// # Markers and icons are TWO sets, not one
///
/// [`OWNED_BLOCKS`] already argues that *edge marker* and *UI icon* are
/// different KINDS of thing and that "the namespace should say so rather than
/// a comment". This is that namespace, so it says so. The distinction earns
/// its place here rather than only documenting one: [`MARKERS`] is geometry
/// the RENDERER reaches for and mod.rs sizes it at single digits, done;
/// [`HIGHBAY_ICONS_BLOCK`] is names an APP asks for and is "the one that
/// actually accumulates". Two sets, growing set later, means the icon
/// vocabulary can grow for years without ever moving the arrowhead's cell.
/// One set would have put them back on a shared numbering where a new icon
/// shifts a marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum GlyphSet {
    /// Glyph 0, the placeholder box ([`ROBOTO_REGULAR_ASCII`]). One glyph, no
    /// codepoint maps to it, and there will never be a second — the most fixed
    /// thing in the atlas, so it is first and its cell is cell 0.
    Placeholder = 0,
    /// [`TEXT_RANGES`] resolved through the face's `cmap`. Declared coverage,
    /// and declared closed.
    Text = 1,
    /// What the SHAPER emits for text beyond what `cmap` names: ligatures and
    /// GSUB forms ([`FontAtlasBuilder::add_shaped_ascii`]). A function of the
    /// face's layout tables rather than of a range we wrote down, which is why
    /// it is a set of its own and sits after the ranges it supplements.
    ShapedText = 2,
    /// The [`MARKERS`] block: geometry the renderer reaches for.
    Markers = 3,
    /// The [`HIGHBAY_ICONS_BLOCK`]: names an app asks for, drawn by this repo.
    OwnedIcons = 4,
    /// The vendor half of [`PRIVATE_USE`] — [`MSYMBOLS_ICONS`] at Material's
    /// own codepoints, present only in the merged face. Last, because it is
    /// the one set whose glyph ids someone else allocates.
    BorrowedIcons = 5,
}

impl GlyphSet {
    /// The variant whose discriminant is `n`.
    ///
    /// Written as an exhaustive match rather than a transmute so that adding a
    /// variant is a COMPILE ERROR here, not a silently unreachable arm - the
    /// same reason [`CellKey`] can claim to be lossless.
    pub const fn from_ordinal(n: u8) -> Self {
        match n {
            0 => GlyphSet::Placeholder,
            1 => GlyphSet::Text,
            2 => GlyphSet::ShapedText,
            3 => GlyphSet::Markers,
            4 => GlyphSet::OwnedIcons,
            5 => GlyphSet::BorrowedIcons,
            _ => panic!("no GlyphSet has this discriminant - a CellKey was built from raw bits"),
        }
    }
}

/// **A cell's sort position, as one integer**: the set above the glyph id.
///
/// `(set, glyph id)` is the order [`GlyphSet`] declares, and this is that pair
/// JOINED rather than compared field by field - the set in the high bits, the
/// glyph id in the low sixteen. Ascending numeric order is therefore exactly
/// the declared cell order, so the sort is one `u32` compare and the ordering
/// cannot drift from the join: they are the same number.
///
/// **Lossless, and provably so.** A glyph id is a `u16`, and [`GlyphSet`] has
/// six variants with explicit discriminants - three bits. Nineteen bits into
/// thirty-two, with [`CellKey::set`] and [`CellKey::glyph_id`] recovering both
/// exactly; `a_key_round_trips_every_set_and_glyph_id` pins it at the extremes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct CellKey(u32);

impl CellKey {
    /// Bits reserved for the glyph id. The set occupies everything above.
    const GLYPH_BITS: u32 = 16;

    /// Join a set and a glyph id into the position their cell takes.
    pub const fn new(set: GlyphSet, glyph_id: u16) -> Self {
        Self(((set as u32) << Self::GLYPH_BITS) | glyph_id as u32)
    }

    /// The set half.
    pub const fn set(self) -> GlyphSet {
        GlyphSet::from_ordinal((self.0 >> Self::GLYPH_BITS) as u8)
    }

    /// The glyph id half - what [`crate::FontAtlas::get_glyph`] is called with.
    pub const fn glyph_id(self) -> u16 {
        self.0 as u16
    }

    /// The joined bits. For a test that wants to show the order IS the number.
    pub const fn bits(self) -> u32 {
        self.0
    }
}


impl GlyphSet {
    /// Every set, in cell order. Iterating this is how a caller walks the
    /// atlas by vocabulary without hard-coding the list a second time.
    pub const ALL: &'static [GlyphSet] = &[
        GlyphSet::Placeholder,
        GlyphSet::Text,
        GlyphSet::ShapedText,
        GlyphSet::Markers,
        GlyphSet::OwnedIcons,
        GlyphSet::BorrowedIcons,
    ];
}

/// Bundled Roboto Regular, subset to [`TEXT_RANGES`] — printable ASCII plus
/// Latin-1 Supplement — with both [`OWNED_BLOCKS`] added.
///
/// The markers and our own icons are here as well as in
/// [`ROBOTO_ASCII_MSYMBOLS`] on purpose: they are glyphs this repo DREW, so
/// they belong to whichever face is loaded, and the two bundled faces still
/// differ only by the borrowed Material Symbols half. They are authored in
/// `fonts/marker.py` and `fonts/icon.py`.
///
/// Roboto is © The Roboto Project Authors, licensed Apache-2.0 — see
/// `fonts/LICENSE-Roboto.txt`. Used as the deterministic test fixture and
/// as the default face for the M3 demo theme.
///
/// (The file is still named `-ascii` and so is this constant; the bake widened
/// past ASCII in the Latin-1 wave and the names have not caught up. What the
/// face covers is [`TEXT_RANGES`], which is checked against the bytes.)
///
/// # Glyph 0 is a BOX, and that is the whole missing-character story
///
/// A codepoint outside [`TEXT_RANGES`] shapes to glyph 0. That used to be an
/// outline-less glyph with no atlas cell, so it drew NOTHING and still
/// advanced — an invisible gap, and the reason a debug build used to abort on
/// one. Both halves of that were wrong once arbitrary runtime data started
/// reaching the text path: a signed-in user named `José` either crashed the
/// pane or silently lost a letter, depending on which build you had.
///
/// So glyph 0 now carries **Roboto's own `.notdef` frame with the X taken
/// out** — a hollow box, U+25A1 WHITE SQUARE's form at the face's own stroke
/// weight (54 units), cap height and advance. Three things follow, and they
/// are the point:
///
/// * **It is at glyph 0, not at a codepoint.** The shaper already routes every
///   uncovered codepoint there, so nothing has to look the placeholder up and
///   no lookup can drift from the bake. Mapping a codepoint to glyph 0 is not
///   even expressible — `cmap` treats that as "unmapped".
/// * **It is a box, not Roboto's crossed box.** The X is unreadable at UI
///   sizes: at 14px the two diagonals merge with the frame into a dark blob
///   that reads as a filled square and pulls the eye harder than the real text
///   around it. The empty frame stays legible as a frame all the way down.
/// * **It is not `U+FFFD`.** Roboto carries one, but the replacement character
///   means *these bytes did not decode*, which is a different failure from
///   *this face has no glyph* — and it is drawn as a heavy filled diamond.
///
/// Nothing about this is an icon fallback. See [`msymbols_codepoint`].
pub const ROBOTO_REGULAR_ASCII: &[u8] = include_bytes!("../../fonts/Roboto-Regular-ascii.ttf");

/// [`ROBOTO_REGULAR_ASCII`] with a Material Symbols icon subset merged in at
/// the icons' own Private Use Area codepoints.
///
/// **One face, so an icon is a glyph on the text path** — same cmap, same
/// shaper, same atlas bake. A caller resolves an icon by shaping its
/// codepoint, exactly as it resolves a letter; there is no second font to keep
/// in step and no parallel "icon renderer".
///
/// The text half is byte-for-byte Roboto's, layout tables included: dropping
/// `GPOS`/`GSUB` in the merge would have taken kerning and the fi/fl/ffi/ffl
/// ligatures with it and shifted every text advance in every frame. The icon
/// half is the variable Material Symbols Outlined face instanced at its
/// default location (FILL 0, GRAD 0, opsz 24, wght 400), subset to the names
/// below, and scaled from 960 to Roboto's 2048 upem.
///
/// Declared coverage, and deliberately small — a name outside this list has no
/// glyph, which is a missing-asset error for the caller to report rather than
/// substitute:
///
/// ```text
/// menu U+E5D2   more_vert U+E5D4   send U+E163   chat U+E0C9   person U+F0D3
/// home U+E9B2   search U+EF7A      library_books U+E02F        settings U+E8B8
/// code U+E86F   edit U+F097        undo U+E166   redo U+E15A
/// ```
///
/// The merge is `fonts/msymbols.py`, which is additive and verifies every
/// already-merged icon against the upstream build before appending a new one.
///
/// This face also carries both [`OWNED_BLOCKS`], as the plain one does — see
/// [`ROBOTO_REGULAR_ASCII`]. So one of the two icon sets, [`HIGHBAY_ICONS`],
/// is in the *plain* face as well: a glyph this repo drew belongs to whichever
/// face is loaded, and only the borrowed half is what makes this one merged.
///
/// Material Symbols is © Google, licensed Apache-2.0 — see
/// `fonts/LICENSE-MaterialSymbols.txt`.
pub const ROBOTO_ASCII_MSYMBOLS: &[u8] =
    include_bytes!("../../fonts/Roboto-Regular-ascii-msymbols.ttf");

/// The BORROWED icon half of [`ROBOTO_ASCII_MSYMBOLS`]: every declared
/// Material Symbols name, and the codepoint Material draws it at.
///
/// The icons this repo drew itself are [`HIGHBAY_ICONS`], a separate manifest
/// behind a separate resolver — see [`highbay_codepoint`] for why the two are
/// never merged.
///
/// **This is the coverage manifest for the borrowed set, and there is exactly
/// one of it.** It lives beside the bytes because it is a fact about the bake,
/// not about any
/// renderer: a second copy in a consumer is a copy that can disagree with the
/// font about which names exist. A caller resolves a name here and shapes the
/// codepoint; a name that is absent has no glyph, and that is a missing-asset
/// error for the caller to report rather than substitute.
///
/// Sorted by name so [`msymbols_codepoint`] can binary-search it, and so the
/// list reads as a list.
///
/// **The codepoint is the one Material's own `.codepoints` manifest declares**,
/// not the lowest or the highest alias the face happens to answer to: several
/// names carry legacy Material Icons aliases as well, and `edit` answers to
/// five, of which `U+F097` is the published one. Taking the declared value is
/// what keeps this a statement about a catalogue rather than about whichever
/// alias a scan picked.
pub const MSYMBOLS_ICONS: &[(&str, char)] = &[
    ("chat", '\u{E0C9}'),
    ("code", '\u{E86F}'),
    ("edit", '\u{F097}'),
    ("home", '\u{E9B2}'),
    ("library_books", '\u{E02F}'),
    ("menu", '\u{E5D2}'),
    ("more_vert", '\u{E5D4}'),
    ("person", '\u{F0D3}'),
    ("redo", '\u{E15A}'),
    ("search", '\u{EF7A}'),
    ("send", '\u{E163}'),
    ("settings", '\u{E8B8}'),
    ("undo", '\u{E166}'),
];

/// The codepoint [`ROBOTO_ASCII_MSYMBOLS`] draws `name` at, or `None` when the
/// bake declares no coverage for it.
///
/// `None` is the whole point of the return type: it is the missing-asset
/// answer, and a caller that turns it into a fallback shape has defeated the
/// reason this is fallible.
///
/// # A missing ICON is not a missing glyph, and the two paths never meet
///
/// Text arrives as CHARACTERS. It goes straight to the shaper, and whatever
/// the face cannot draw comes back as glyph 0 — the box
/// ([`ROBOTO_REGULAR_ASCII`]). That is right for text, because the text is the
/// user's: a name, a chat line, a synced cell. A box says "this character did
/// not survive" and leaves the rest of the string readable.
///
/// An icon arrives as a NAME, and a name is resolved HERE, before anything is
/// shaped. An unknown name never becomes a codepoint, so it never reaches the
/// shaper and cannot pick up the box on the way through. The caller gets
/// `None` and reports it (`DrawFinding::MissingIcon`; Rule 28) — because an
/// icon name is a DEVELOPER's, and a box drawn where `sned` was typed is a
/// substitution that makes the typo look like a rendering quirk instead of the
/// missing asset it is.
///
/// The same reasoning covers the second gate: a name that IS in this manifest
/// but that the live face does not carry. A caller asks
/// [`TextShaper::covers`] first and reports the miss, rather than shaping the
/// codepoint and getting a box for an asset gap. `covers` answers from the
/// `cmap`, so glyph 0 having gained an outline did not change its answer for a
/// single codepoint.
pub fn msymbols_codepoint(name: &str) -> Option<char> {
    MSYMBOLS_ICONS
        .binary_search_by_key(&name, |&(n, _)| n)
        .ok()
        .map(|i| MSYMBOLS_ICONS[i].1)
}

/// **The icons this repo DREW, and the codepoints it drew them at** — the
/// manifest for [`HIGHBAY_ICONS_BLOCK`], exactly as [`MSYMBOLS_ICONS`] is the
/// manifest for the borrowed set.
///
/// ```text
/// graph U+F800   props U+F801   table U+F802   screen U+F803
/// ```
///
/// These exist because the vocabulary Material publishes does not contain
/// them. `table` has no close Material match, `props` — a property sheet's
/// label/value rows — has none at all, and `screen` (one screen of the app
/// being built) has only near-misses whose NAMES mean other things:
/// `crop_square` means crop-to-square, `check_box_outline_blank` means an
/// unticked checkbox, `rectangle` means a rectangle. So the alternatives were
/// to pick a Material name that means something else and let the codebase
/// learn a lie, or to draw our own and say so. This is saying so: they carry no Material
/// name, they sit two thousand codepoints clear of Material's, and
/// [`msymbols_codepoint`] answers `None` for every one of them.
///
/// The outlines are authored in `fonts/icon.py`, which is where the geometry
/// is decided and where `table` and `props` record which proportions they take
/// from `highbay_ui`'s since-retired `draw_table_glyph`/`draw_props_glyph`.
///
/// Sorted by name so [`highbay_codepoint`] can binary-search it. The
/// codepoints no longer sort the same way — `screen` was added after `table`
/// and therefore ABOVE it, because the block is allocated upward and an
/// alphabetically-placed codepoint would have renumbered a glyph that had
/// already shipped.
pub const HIGHBAY_ICONS: &[(&str, char)] = &[
    ("graph", '\u{F800}'),
    ("props", '\u{F801}'),
    ("screen", '\u{F803}'),
    ("table", '\u{F802}'),
];

/// The codepoint the bundled faces draw the repo's own icon `name` at, or
/// `None` when there is no such icon.
///
/// # This is a SECOND resolver on purpose, and it must stay one
///
/// It would be a two-line change to fold [`MSYMBOLS_ICONS`] and
/// [`HIGHBAY_ICONS`] into one table behind one `icon_codepoint(name)`, and it
/// would cost the only thing this separation buys: at the call site, whether a
/// name is a vocabulary **Material publishes** or one **we drew**. That
/// distinction is the whole reason these glyphs exist rather than a
/// near-enough Material icon wearing the wrong name, and a merged resolver
/// erases it at exactly the moment someone would need it — when they go
/// looking for `table` in Material's catalogue and find a different mark.
///
/// A fallback between the two would be worse than a merge. `msymbols_codepoint`
/// returning `None` means *Material has no such icon*, which is a fact about a
/// published catalogue; falling through to our block would turn a name we
/// happened to draw into an answer to a question about theirs, and drawing
/// theirs for a name of ours would be the same error mirrored. Both directions
/// are asserted against in `a_name_never_crosses_between_the_two_vocabularies`.
///
/// Everything else here matches [`msymbols_codepoint`], including the part
/// that matters most: `None` is the missing-asset answer, the caller reports it
/// (`DrawFinding::MissingIcon`; Rule 28), and a caller that turns it into a
/// fallback shape has defeated the reason this is fallible.
pub fn highbay_codepoint(name: &str) -> Option<char> {
    HIGHBAY_ICONS
        .binary_search_by_key(&name, |&(n, _)| n)
        .ok()
        .map(|i| HIGHBAY_ICONS[i].1)
}

#[cfg(test)]
mod cell_key_tests {
    use super::{CellKey, GlyphSet};

    /// **The join is lossless at the extremes**, which is what lets the sort be
    /// one integer compare instead of a tuple compare.
    #[test]
    fn a_key_round_trips_every_set_and_glyph_id() {
        let sets = [
            GlyphSet::Placeholder,
            GlyphSet::Text,
            GlyphSet::ShapedText,
            GlyphSet::Markers,
            GlyphSet::OwnedIcons,
            GlyphSet::BorrowedIcons,
        ];
        for set in sets {
            for glyph_id in [0u16, 1, 255, 256, 32767, 32768, u16::MAX] {
                let key = CellKey::new(set, glyph_id);
                assert_eq!(key.set(), set, "set lost for glyph {glyph_id}");
                assert_eq!(key.glyph_id(), glyph_id, "glyph id lost for {set:?}");
            }
        }
    }

    /// **Ascending numeric order IS `(set, glyph id)` order.** This is the whole
    /// claim the join rests on: if it failed, cells would be laid out in an
    /// order the sets do not describe, silently.
    #[test]
    fn numeric_order_is_the_declared_order() {
        let sets = [
            GlyphSet::Placeholder,
            GlyphSet::Text,
            GlyphSet::ShapedText,
            GlyphSet::Markers,
            GlyphSet::OwnedIcons,
            GlyphSet::BorrowedIcons,
        ];
        let mut keys: Vec<CellKey> = Vec::new();
        for set in sets {
            for glyph_id in [0u16, 7, u16::MAX] {
                keys.push(CellKey::new(set, glyph_id));
            }
        }
        let mut by_bits = keys.clone();
        by_bits.sort_unstable_by_key(|k| k.bits());
        let mut by_pair = keys.clone();
        by_pair.sort_unstable_by_key(|k| (k.set(), k.glyph_id()));
        assert_eq!(by_bits, by_pair, "the packed order and the pair order differ");
    }

    /// A glyph id can never reach into the set's bits - the guard that makes
    /// "lossless" a property rather than an observation about small inputs.
    #[test]
    fn the_widest_glyph_id_cannot_reach_the_set_bits() {
        let low = CellKey::new(GlyphSet::Placeholder, u16::MAX);
        let high = CellKey::new(GlyphSet::Text, 0);
        assert!(low < high, "a maximal glyph id in one set outranked the next set");
    }
}
