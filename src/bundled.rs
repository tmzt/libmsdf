//! **The bundled fonts**: every face libmsdf can compile in, each behind its
//! own cargo feature, off by default, whose name carries the face's licence
//! (`docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md`, section 4.4).
//!
//! | Feature | Constant | Licence |
//! |---|---|---|
//! | `font-apache-roboto-regular` | [`ROBOTO_REGULAR_ASCII`] | Apache-2.0 |
//! | `font-apache-roboto-bold` | [`ROBOTO_BOLD_ASCII`] | Apache-2.0 |
//! | `font-apache-roboto-italic` | [`ROBOTO_ITALIC_ASCII`] | Apache-2.0 |
//! | `font-apache-roboto-msymbols` | [`ROBOTO_ASCII_MSYMBOLS`] | Apache-2.0 |
//!
//! A TTF constant also needs the `shaper` feature (on by default), since
//! nothing else in libmsdf reads a TTF. With no font feature enabled the
//! compiled crate contains no font data and is licensed purely under
//! MIT OR Apache-2.0 (README.md, "Licensing").
//!
//! The manifests that describe the faces without containing them
//! (`TEXT_RANGES`, `MSYMBOLS_ICONS`, `HIGHBAY_ICONS`, `MARKERS`,
//! `MARKER_ARROW`, ...) are codepoints, not font data, and stay ungated in
//! [`crate::font`].

/// Bundled Roboto Regular, subset to [`TEXT_RANGES`](crate::font::TEXT_RANGES) — printable ASCII plus
/// Latin-1 Supplement — with both [`OWNED_BLOCKS`](crate::font::OWNED_BLOCKS) added.
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
/// face covers is [`TEXT_RANGES`](crate::font::TEXT_RANGES), which is checked against the bytes.)
///
/// # Glyph 0 is a BOX, and that is the whole missing-character story
///
/// A codepoint outside [`TEXT_RANGES`](crate::font::TEXT_RANGES) shapes to glyph 0. That used to be an
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
/// Nothing about this is an icon fallback. See [`msymbols_codepoint`](crate::font::msymbols_codepoint).
#[cfg(all(feature = "font-apache-roboto-regular", feature = "shaper"))]
pub const ROBOTO_REGULAR_ASCII: &[u8] = include_bytes!("../fonts/Roboto-Regular-ascii.ttf");

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
/// check U+E668  chevron_left U+E5CB             chevron_right U+E5CC
/// expand_less U+E5CE              expand_more U+E5CF
/// ```
///
/// The last five are the four chevrons and the tick, added 2026-09-09 because
/// components in this workspace were drawing them as `<SdfLine>` strokes
/// beside icons that came from the face, or leaving them out: `checkbox.tsx`
/// composes the tick from two lines, `props_sheet.tsx` composes two chevrons
/// from two more, and `genius_canvas.tsx` omits its chevron entirely. Each
/// says in its own words that the manifest carries no such name, which was
/// true and is the one reason a manifest gets widened rather than a shape
/// substituted. See [`MSYMBOLS_ICONS`](crate::font::MSYMBOLS_ICONS) for why the vertical chevrons are
/// `expand_less`/`expand_more` rather than a `chevron_*` pair, and why `check`
/// is `U+E668`.
///
/// The merge is `fonts/msymbols.py`, which is additive and verifies every
/// already-merged icon against the upstream build before appending a new one.
///
/// This face also carries both [`OWNED_BLOCKS`](crate::font::OWNED_BLOCKS), as the plain one does — see
/// [`ROBOTO_REGULAR_ASCII`]. So one of the two icon sets, [`HIGHBAY_ICONS`](crate::font::HIGHBAY_ICONS),
/// is in the *plain* face as well: a glyph this repo drew belongs to whichever
/// face is loaded, and only the borrowed half is what makes this one merged.
///
/// Material Symbols is © Google, licensed Apache-2.0 — see
/// `fonts/LICENSE-MaterialSymbols.txt`.
#[cfg(all(feature = "font-apache-roboto-msymbols", feature = "shaper"))]
pub const ROBOTO_ASCII_MSYMBOLS: &[u8] =
    include_bytes!("../fonts/Roboto-Regular-ascii-msymbols.ttf");

/// **Roboto Bold**, subset to [`TEXT_RANGES`](crate::font::TEXT_RANGES) - the same coverage the regular
/// face declares, in the heavier cut, so `**bold**` resolves to outlines a type
/// designer drew rather than to a synthesized weight.
///
/// Baked into [`GlyphSet::BoldText`](crate::font::GlyphSet::BoldText) and addressed as
/// [`GlyphStyle::Bold`](crate::font::GlyphStyle::Bold). Made by `fonts/style.py` from the SAME upstream drop
/// as the regular face (Roboto v2.138, `roboto-android.zip`), which that script
/// checks outline-for-outline rather than asserting - a bold cut from a
/// different build passes every test in this repo and simply looks wrong beside
/// the prose.
///
/// # What it does NOT carry, and why neither is an omission
///
/// * **No Private Use Area.** Markers and both icon vocabularies are
///   style-invariant geometry; see [`GlyphSet::style`](crate::font::GlyphSet::style).
/// * **No outline on glyph 0.** The placeholder is style-invariant too -
///   [`GlyphStyle::styled_glyph_id`](crate::font::GlyphStyle::styled_glyph_id) addresses it as glyph 0 in every style,
///   so a styled `.notdef` would be a cell nothing can reach.
///
/// 19,628 bytes; 13,084 gzipped. Roboto is (c) The Roboto Project Authors,
/// licensed Apache-2.0 - see `fonts/LICENSE-Roboto.txt`.
#[cfg(all(feature = "font-apache-roboto-bold", feature = "shaper"))]
pub const ROBOTO_BOLD_ASCII: &[u8] = include_bytes!("../fonts/Roboto-Bold-ascii.ttf");

/// **Roboto Italic**, subset to [`TEXT_RANGES`](crate::font::TEXT_RANGES) - everything
/// [`ROBOTO_BOLD_ASCII`] says, in the slanted cut, for `*italic*`.
///
/// It is here because it is the same MECHANISM and not merely more bytes: one
/// `fonts/style.py` run, one [`GlyphStyle`](crate::font::GlyphStyle) variant, one pair of sets, and the
/// bake path cannot tell the two apart. A true italic is also the reason to do
/// this at all rather than shear the regular face - Roboto's italic `a` is a
/// different letterform, not a slanted one, and a shear would have been a
/// renderer feature that looks like typography from a distance.
///
/// 21,520 bytes; 14,833 gzipped.
#[cfg(all(feature = "font-apache-roboto-italic", feature = "shaper"))]
pub const ROBOTO_ITALIC_ASCII: &[u8] = include_bytes!("../fonts/Roboto-Italic-ascii.ttf");
