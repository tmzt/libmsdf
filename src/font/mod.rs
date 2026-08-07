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

pub use atlas::{FontAtlas, FontAtlasBuilder, GlyphProjection, glyph_projection};
pub use glyph_table::GlyphEntry;
pub use manager::{AtlasManager, AtlasRegion};
pub use outline::{Edge, EdgeKind, GlyphOutline, extract_outline};
pub use packer::ShelfPacker;
pub use shaper::{ShapedGlyph, ShapedRun, TextShaper};

/// **The TEXT coverage of the bundled faces, and there is exactly one of it.**
///
/// Inclusive codepoint ranges, in the order [`FontAtlasBuilder::add_shipped_coverage`]
/// queues them — which is also the order the atlas packs them, so this list is
/// append-only (see that method for why the ordering is load-bearing).
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
/// repo owns rather than borrows — today that is the [`MSYMBOLS_ICONS`] set,
/// at Material Symbols' own codepoints — sits inside it, and Unicode
/// guarantees no standard character ever will. So the two halves of a bundled
/// face cannot collide by construction rather than by review:
/// [`FontAtlasBuilder::add_shipped_coverage`] queues this range as *whatever
/// the face defines here*, never as a list of names, and a `debug_assert` in
/// [`msymbols_codepoint`]'s test pins the icons inside it.
///
/// Widening [`TEXT_RANGES`] toward it is the one thing that could break that,
/// which is why they are stated together, one screen apart.
pub const PRIVATE_USE: (char, char) = ('\u{E000}', '\u{F8FF}');

/// Bundled Roboto Regular, subset to [`TEXT_RANGES`] — printable ASCII plus
/// Latin-1 Supplement.
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
/// ```
///
/// Material Symbols is © Google, licensed Apache-2.0 — see
/// `fonts/LICENSE-MaterialSymbols.txt`.
pub const ROBOTO_ASCII_MSYMBOLS: &[u8] =
    include_bytes!("../../fonts/Roboto-Regular-ascii-msymbols.ttf");

/// The icon half of [`ROBOTO_ASCII_MSYMBOLS`]: every declared name, and the
/// codepoint it is drawn at.
///
/// **This is the coverage manifest, and there is exactly one of it.** It lives
/// beside the bytes because it is a fact about the bake, not about any
/// renderer: a second copy in a consumer is a copy that can disagree with the
/// font about which names exist. A caller resolves a name here and shapes the
/// codepoint; a name that is absent has no glyph, and that is a missing-asset
/// error for the caller to report rather than substitute.
///
/// Sorted by name so [`msymbols_codepoint`] can binary-search it, and so the
/// list reads as a list.
pub const MSYMBOLS_ICONS: &[(&str, char)] = &[
    ("chat", '\u{E0C9}'),
    ("home", '\u{E9B2}'),
    ("library_books", '\u{E02F}'),
    ("menu", '\u{E5D2}'),
    ("more_vert", '\u{E5D4}'),
    ("person", '\u{F0D3}'),
    ("search", '\u{EF7A}'),
    ("send", '\u{E163}'),
    ("settings", '\u{E8B8}'),
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
