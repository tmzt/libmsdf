//! MSDF atlas — packed glyph bitmaps + metrics.
//!
//! Loading/serialization (`FontAtlas`) is target-independent and wasm-clean.
//! CPU *baking* (`FontAtlasBuilder::build`) uses `msdfgen`, which is C++ FFI
//! and therefore native-only; on wasm32 atlases are either loaded from baked
//! bytes (`FontAtlas::from_bytes`) or generated at runtime by the WGSL
//! compute path (`gpu::MsdfCompute`).
//!
//! The atlas packs glyphs into a single texture using shelf-based bin
//! packing. Each glyph is rendered as a 3-channel (RGB) MSDF bitmap.

use crate::font::{CellKey, GlyphSet, GlyphStyle};
use crate::font::glyph_table::GlyphEntry;
#[cfg(all(feature = "cpu-bake", not(target_arch = "wasm32")))]
use crate::font::packer::ShelfPacker;

/// **The first four bytes of a baked atlas.** `hbfa` - highbay font atlas.
///
/// See [`FontAtlas::from_bytes`] for why a magic exists at all: v1 of this
/// format started at `width`, so without one an old file misparses into a
/// plausible-looking atlas instead of failing.
pub const ATLAS_MAGIC: &[u8; 4] = b"hbfa";

/// **The version a SINGLE-LAYER atlas is written at**, and one of the two this
/// build reads - see [`ATLAS_VERSION_LAYERED`] for the other.
///
/// * v1 had neither magic nor a set header.
/// * v2 added [`ATLAS_MAGIC`] and the [`SetMetrics`] table at 32 bytes a row.
/// * v3 added [`SetMetrics::ink_descent_em`], which makes a row 36 bytes.
/// * v4 added the [`AtlasLayer`] table and multi-layer pixel data. **A
///   single-layer atlas is still written at v3**, which is the whole reason
///   the three committed artifacts did not have to be re-baked: layers cost a
///   version only when a file actually has more than one.
///
/// **v3 is a bump rather than a spare-byte fill because there were no spare
/// bytes**: a v2 row was 2 + 2 + 7x4 = exactly 32, so the new field can only
/// widen the stride. A v2 file read at 36 would take the second set's header
/// four bytes late, walk every later row further off, and land the glyph table
/// at an offset that is wrong by `4 * set_count` - which is not a clean failure
/// but a plausible-looking atlas. That is the exact substitution the magic and
/// the version exist to prevent, so the version moves and every atlas is
/// re-baked.
pub const ATLAS_VERSION: u32 = 3;

/// **The version a MULTI-LAYER atlas is written at.** Written only when
/// [`FontAtlas::layers`] holds more than one; read alongside
/// [`ATLAS_VERSION`].
///
/// # Why layers cost a version at all, when the ENTRY did not
///
/// [`GlyphEntry`]'s layer index went into two bytes that were already pad, so
/// nothing about an entry needed a version. The pixel data is the reason: a v4
/// file carries `width * height * channels * layers` texels and a v3 reader
/// computes `width * height * channels`. Its length check
/// (`data.len() < pixel_start + expected`) would PASS on the longer buffer,
/// take the first layer, and load an atlas that draws every layer-0 glyph
/// correctly and every other glyph from whatever cell coordinate landed on -
/// a plausible-looking atlas, which is precisely what
/// [`FontAtlas::from_bytes`]' magic and version exist to convert into a stop.
///
/// So the version is directed by CONTENT rather than by build: a file gets the
/// version it needs. One layer is a v3 file an older build reads correctly;
/// two or more is a v4 file an older build refuses.
pub const ATLAS_VERSION_LAYERED: u32 = 4;

/// **The most layers an atlas may declare**: WebGPU guarantees
/// `maxTextureArrayLayers` of 256 on every adapter, and this repo pins no
/// limits, so 256 is what a bake may assume without asking the device.
///
/// For scale, `(point size, style)` over the M3 type scale's 14 declared sizes
/// and four styles is 56 layers - so the guarantee is not the binding
/// constraint. Texture MEMORY is, because every layer of an array shares the
/// array's dimensions ([`AtlasLayer`]).
pub const MAX_ATLAS_LAYERS: usize = 256;

/// **What one texture-array layer HOLDS**: a point size and a style.
///
/// # Why a layer is the pair, and not one axis or the other
///
/// The pinned grid ([`ATLAS_ROWS`]) is a per-LAYER budget now, so "what shares
/// a texture with what" became a design decision instead of a capacity
/// accident. It has two halves that pull opposite ways:
///
/// * **Point size is what must AGREE inside a layer.** A run mixing prose with
///   an icon is the common case, and it costs nothing exactly when the icon's
///   cells and the text's cells are in ONE layer - which is what the merged
///   face ([`crate::font::ROBOTO_ASCII_MSYMBOLS`]) has always bought and what
///   this keeps. So the merged vocabulary inside a layer is *text plus Private
///   Use Area, at that size*.
/// * **Style is what must DIFFER between layers.** A second face is 205 more
///   cells and a layer holds 320, so two styles cannot share one; and they
///   need not, because a run does not usually alternate weight per glyph.
///
/// The Private Use Area is deliberately NOT duplicated per style: an icon has
/// no weight, the outlines would be identical, and 13 borrowed cells per style
/// would buy a distinction no caller can make. A bold run containing an icon
/// takes one layer change, which is the ordinary cost of any layer change and
/// far rarer than text beside an icon.
///
/// # `size_px` is the bake's cell size, which is the point size it is FOR
///
/// [`FontAtlasBuilder::glyph_size`] - 48 for everything this repo ships. MSDF
/// upscales cleanly and degrades on the way DOWN
/// ([`FontAtlas::min_antialiased_font_size`]), so a per-size bake is an
/// investment at the SMALL end; the number here is what a caller asks for when
/// it wants the layer baked for its size.
///
/// # Every layer of one array shares the array's DIMENSIONS
///
/// That is a wgpu fact, not a choice here, and it is the real cost of the
/// per-size axis: a 16px layer inside a 400x2000 array occupies its own
/// 400x2000 of texture memory however few texels its cells cover. The grid is
/// per-layer - each entry carries its own `atlas_x`/`atlas_y`, so nothing in
/// the shader assumes a common cell size - which means a small-cell layer can
/// pack far more cells into the same rectangle, but it cannot make the
/// rectangle smaller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AtlasLayer {
    /// The cell size this layer was baked at, in texels - the point size the
    /// layer is for.
    pub size_px: u16,
    /// The cut its text cells are in. [`GlyphStyle::Regular`] also owns the
    /// markers and both icon sets, which are style-invariant.
    pub style: GlyphStyle,
}

impl AtlasLayer {
    /// Packed size in a v4 atlas file: 2 + 1 + 1 reserved.
    pub const PACKED_SIZE: usize = 4;

    pub const fn new(size_px: u16, style: GlyphStyle) -> Self {
        Self { size_px, style }
    }

    fn to_bytes(self) -> [u8; Self::PACKED_SIZE] {
        let mut buf = [0u8; Self::PACKED_SIZE];
        buf[0..2].copy_from_slice(&self.size_px.to_le_bytes());
        buf[2] = self.style as u8;
        buf
    }

    fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < Self::PACKED_SIZE {
            return Err("atlas layer table entry too short");
        }
        // Checked rather than punned, for `SetMetrics::from_bytes`' reason: a
        // file is untrusted input, and a style this build has never heard of
        // must be a clean refusal instead of a transmute.
        let Some(style) = GlyphStyle::try_from_ordinal(data[2]) else {
            return Err(
                "atlas names a glyph style this build does not have - it was baked by a newer \
                 libmsdf; update, or re-bake with this one",
            );
        };
        Ok(Self { size_px: u16::from_le_bytes([data[0], data[1]]), style })
    }
}

impl core::fmt::Display for AtlasLayer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}px {:?}", self.size_px, self.style)
    }
}

/// **A layer this atlas does not have** - what [`FontAtlas::layer_index`]
/// answers with instead of a substitute.
///
/// Layer 0 is always *a* layer, so falling back to it would always "work" and
/// would always be wrong: 48px regular cells where the caller asked for 16px
/// bold is upright text where the document says emphasis, at a size nobody
/// asked for. That is the same wrong-render-that-looks-right
/// [`StyledGlyphError`] exists to prevent, one axis over, and it is refused
/// the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerNotBaked(pub AtlasLayer);

impl core::fmt::Display for LayerNotBaked {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "this atlas has no {} layer - bake one, or ask for a layer it has; layer 0 is not a \
             substitute for a size or a style the caller named",
            self.0
        )
    }
}

/// Fixed part of the header, before the [`SetMetrics`] table: magic, version,
/// width, height, glyph count, channels, set count.
pub const ATLAS_HEADER_SIZE: usize = 28;

/// **What one [`GlyphSet`]'s cells were baked FROM**, in units that do not
/// mention the bake size.
///
/// # Why the atlas has to say this, rather than the reader working it out
///
/// A consumer needs the baseline to put a rule under a run, and the face's own
/// underline metrics to know how far under. Until this header existed, none of
/// that was in the file: `libhbui` recovered the baseline from
/// `atlas.glyphs.first()` - whichever cell happened to sort first - and got
/// away with it only for as long as every cell agreed. When the table gained
/// an order (by glyph id), "first" became glyph 0 instead of the space glyph,
/// the two disagreed by `0.75 - 0.6969` of a cell, and every underline in the
/// app moved most of a pixel. Nothing failed; the frames just changed.
///
/// So the fix is not a better cell to read: it is that a per-cell field cannot
/// answer a per-FACE question, and the file now carries the answer.
///
/// # Every field is a fraction, and that is the point
///
/// Nothing here is in cell pixels, so the same header serves a consumer
/// drawing at 12px and one drawing at 48px, and a re-bake at a different
/// `glyph_size` does not change a single number in it.
///
/// Sign convention is the FACE's, not the screen's: `underline_pos_em`,
/// `descent_em` and `ink_descent_em` are negative BELOW the baseline, exactly
/// as `post` and `hhea` state them, so a reader comparing against a font tool
/// sees the same signs. There is NO field here with the screen's sign - a
/// consumer that wants a downward depth calls
/// [`SetMetrics::max_ink_descent_em`], which is a method precisely so that the
/// flip is visible at the call site rather than sitting in a struct where two
/// neighbouring "descent" fields would disagree about which way is down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetMetrics {
    /// Which set these describe.
    pub set: GlyphSet,
    /// Cells this set contributed to the bake. Not an index range - the table
    /// is re-sorted by glyph id on load ([`FontAtlas::from_bytes`]), so the
    /// set's cells are not contiguous in it. It is a COUNT, and the loader
    /// checks the counts sum to the number of entries.
    pub glyph_count: u16,
    /// Fraction of the cell, from its top, at which the baseline sits.
    /// Consumer: placing anything relative to a run's baseline - today the
    /// underline in `libhbui`'s `underline_rect`.
    pub baseline_frac: f32,
    /// One em as a fraction of the cell ([`CELL_EM_RATIO`] inverted).
    /// Consumer: converting the em-denominated fields below into pixels. A
    /// cell drawn to a line box of `size * LINE_BOX_RATIO` puts one em at
    /// `px_per_em_frac * LINE_BOX_RATIO * size` - which is why "one em is the
    /// font size" holds, and this is where a consumer can check it rather
    /// than assume it.
    pub px_per_em_frac: f32,
    /// `hhea` ascender, in em (positive above the baseline).
    /// Consumer: line height. `LINE_BOX_RATIO` is a constant today, and
    /// `ascent - descent + line_gap` is the face's own answer for it.
    pub ascent_em: f32,
    /// `hhea` descender, in em (NEGATIVE below the baseline). What the face
    /// DECLARES, which is a different quantity from `ink_descent_em` below -
    /// see there.
    pub descent_em: f32,
    /// **The deepest ink any glyph OF THIS SET actually reaches**, in em,
    /// NEGATIVE below the baseline: the minimum `yMin` over the set's glyph
    /// bounding boxes, from the `glyf` outlines the cells were baked from.
    ///
    /// # Why this is not `descent_em`
    ///
    /// `descent_em` is a DECLARATION and is not a bound on ink in either
    /// direction. Measured on the bundled Roboto subset (upem 2048, `hhea`
    /// descender -500):
    ///
    /// * [`GlyphSet::Text`] reaches -495 (`U+00A7 SECTION SIGN`) - 5 units
    ///   SHALLOWER than declared, so a rule placed at the declared descender is
    ///   low by a quarter-pixel at 48px for no reason.
    /// * [`GlyphSet::Markers`] reaches -512 (`U+F8F0`) - 12 units DEEPER than
    ///   declared, so the declaration would have been an under-report and a
    ///   rule trusting it would be crossed.
    ///
    /// One face, one `hhea` row, two sets that disagree with it in OPPOSITE
    /// directions. That is the whole argument for measuring: a face-wide
    /// declaration cannot answer a per-set question, and it is not conservative
    /// enough to be used as a bound instead.
    ///
    /// # Per-SET, and that is the point of it
    ///
    /// The face-wide minimum is -512, from a marker glyph that never appears in
    /// a text run. A consumer ruling a line under text wants the TEXT set's
    /// -495, and gets it here without knowing anything about which glyphs the
    /// run contained.
    ///
    /// Zero for a set whose glyphs have no outlines at all (whitespace only),
    /// and POSITIVE for a set whose ink never reaches the baseline
    /// ([`GlyphSet::OwnedIcons`] bottoms out at +0.1875 em, above it) - both
    /// are the measurement, not a sentinel. See
    /// [`SetMetrics::max_ink_descent_em`] for the consumer-facing form.
    pub ink_descent_em: f32,
    /// `hhea` line gap, in em. Part of the line-height sum above; carried with
    /// the other two because a line height computed from two of the three
    /// would be wrong for any face that uses it.
    pub line_gap_em: f32,
    /// `post` underlinePosition, in em: the top of the underline relative to
    /// the baseline, NEGATIVE below it. Consumer: `underline_rect`, which
    /// deliberately draws a roomier rule than this at body sizes and can now
    /// say so against the face's actual number instead of a remembered one.
    pub underline_pos_em: f32,
    /// `post` underlineThickness, in em.
    pub underline_thickness_em: f32,
}

impl SetMetrics {
    /// Packed size in the atlas file: 2 + 2 + 8x4. It was 32 in v2 and had no
    /// slack, which is why `ink_descent_em` cost a version - see
    /// [`ATLAS_VERSION`].
    pub const PACKED_SIZE: usize = 36;

    /// **How far below the baseline the set's deepest ink reaches**, in em, as
    /// a DOWNWARD depth: positive is below the baseline, the screen's sign and
    /// not the face's.
    ///
    /// A method rather than a field so that the one flip in this type happens
    /// somewhere a reader can see it. Negative for a set whose ink never
    /// reaches the baseline, so a caller adding it to a baseline should clamp
    /// at zero unless it means to draw above one.
    pub fn max_ink_descent_em(&self) -> f32 {
        -self.ink_descent_em
    }

    fn to_bytes(self) -> [u8; Self::PACKED_SIZE] {
        let mut buf = [0u8; Self::PACKED_SIZE];
        buf[0..2].copy_from_slice(&(self.set as u16).to_le_bytes());
        buf[2..4].copy_from_slice(&self.glyph_count.to_le_bytes());
        buf[4..8].copy_from_slice(&self.baseline_frac.to_le_bytes());
        buf[8..12].copy_from_slice(&self.px_per_em_frac.to_le_bytes());
        buf[12..16].copy_from_slice(&self.ascent_em.to_le_bytes());
        buf[16..20].copy_from_slice(&self.descent_em.to_le_bytes());
        buf[20..24].copy_from_slice(&self.line_gap_em.to_le_bytes());
        buf[24..28].copy_from_slice(&self.underline_pos_em.to_le_bytes());
        buf[28..32].copy_from_slice(&self.underline_thickness_em.to_le_bytes());
        buf[32..36].copy_from_slice(&self.ink_descent_em.to_le_bytes());
        buf
    }

    fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < Self::PACKED_SIZE {
            return Err("atlas set header entry too short");
        }
        let f = |o: usize| f32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        let ordinal = u16::from_le_bytes([data[0], data[1]]);
        // Checked rather than punned: `GlyphSet::from_ordinal` PANICS on an
        // unknown discriminant, and a file is untrusted input. An atlas baked
        // by a newer libmsdf with a set this build has never heard of must be
        // a clean refusal here.
        //
        // **This is the extension point the styled sets used.** Growing
        // `GlyphSet` past `BorrowedIcons` cost no format version precisely
        // because an old reader lands here and stops with a message that names
        // the fix, instead of misreading a bold cell as a text one.
        let Some(set) = u8::try_from(ordinal).ok().and_then(GlyphSet::try_from_ordinal) else {
            return Err(
                "atlas names a glyph set this build does not have - it was baked by a newer \
                 libmsdf; update, or re-bake with this one",
            );
        };
        Ok(Self {
            set,
            glyph_count: u16::from_le_bytes([data[2], data[3]]),
            baseline_frac: f(4),
            px_per_em_frac: f(8),
            ascent_em: f(12),
            descent_em: f(16),
            line_gap_em: f(20),
            underline_pos_em: f(24),
            underline_thickness_em: f(28),
            ink_descent_em: f(32),
        })
    }
}

/// **Why a styled lookup came back empty** - and the distinction is the
/// reason [`FontAtlas::styled_glyph`] is fallible in the first place.
///
/// Three failures that a single `None` would have flattened into one, each
/// with a different owner:
///
/// * [`StyledGlyphError::StyleNotBaked`] - an ASSET gap. The atlas was baked
///   without that face, so no glyph in that style exists here. The caller
///   reports it (Rule 28's shape); it must not draw the regular glyph, because
///   upright text where the document says emphasis is a wrong render that
///   looks like a correct one.
/// * [`StyledGlyphError::NoCell`] - the style is here and this glyph of it is
///   not. A codepoint outside [`crate::font::TEXT_RANGES`], or a bake that
///   queued less than the face covers.
/// * [`StyledGlyphError::NotARawGlyphId`] - the caller passed an id that is
///   already an address ([`GlyphStyle::styled_glyph_id`]), or a face wider
///   than [`crate::font::MAX_RAW_GLYPH_ID`]. A programming error rather than
///   an asset one, and it is refused instead of being prefixed twice into
///   whatever cell that lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyledGlyphError {
    /// This atlas carries no cells in that style at all.
    StyleNotBaked(GlyphStyle),
    /// The style is baked; that glyph of it is not.
    NoCell(GlyphStyle, u16),
    /// Not a raw glyph id: too wide to prefix, or already prefixed.
    NotARawGlyphId(u16),
}

impl core::fmt::Display for StyledGlyphError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::StyleNotBaked(style) => write!(
                f,
                "this atlas was baked without {style:?} - re-bake it with that face, or report \
                 the gap; drawing the regular glyph instead is a wrong render that looks right"
            ),
            Self::NoCell(style, gid) => {
                write!(f, "{style:?} is baked here, but glyph {gid} of it has no cell")
            }
            Self::NotARawGlyphId(gid) => write!(
                f,
                "{gid} is not a raw glyph id - it is above MAX_RAW_GLYPH_ID, or it is already a \
                 styled address and would be prefixed twice"
            ),
        }
    }
}

/// MSDF atlas containing packed glyph bitmaps and their metrics.
#[derive(Debug, Clone)]
pub struct FontAtlas {
    /// Atlas texture width in pixels
    pub width: u32,
    /// Atlas texture height in pixels
    pub height: u32,
    /// Number of channels (3 = MSDF, 4 = MTSDF)
    pub channels: u32,
    /// Raw pixel data: `layers.len() * width * height * channels` bytes,
    /// LAYER-major and row-major within a layer - layer `n` starts at
    /// [`FontAtlas::layer_offset`].
    pub pixel_data: Vec<u8>,
    /// **Per-glyph entries, ordered by glyph id** — see
    /// [`FontAtlas::get_glyph`] for why that specific order, and
    /// [`crate::font::GlyphSet`] for why it is not the order the CELLS are in.
    pub glyphs: Vec<GlyphEntry>,
    /// **Per-set metrics, one entry per [`GlyphSet`] that has cells here** -
    /// see [`SetMetrics`] for what they are and why the file has to carry
    /// them. Ordered by set; read it through [`FontAtlas::set_metrics`].
    ///
    /// EMPTY for an atlas built at runtime ([`FontAtlas::empty`] plus
    /// appends), which has no face to measure. A BAKED atlas always has one:
    /// [`FontAtlas::from_bytes`] refuses a file without it.
    pub sets: Vec<SetMetrics>,
    /// **What each texture-array layer holds**, indexed by layer - so
    /// `layers[e.layer]` is the `(point size, style)` of the cell `e` names.
    ///
    /// NEVER empty: an atlas has at least one layer, and a file written before
    /// layers existed declares exactly the one it always had. See
    /// [`AtlasLayer`] for what a layer is and [`FontAtlas::layer_index`] for
    /// the lookup that goes the other way.
    pub layers: Vec<AtlasLayer>,
}

impl FontAtlas {
    /// The metrics of one glyph set, or `None` for a set this atlas has no
    /// cells from - and for every set of a runtime-populated atlas.
    pub fn set_metrics(&self, set: GlyphSet) -> Option<&SetMetrics> {
        self.sets.iter().find(|m| m.set == set)
    }

    /// **Which layer holds `(point size, style)`**, or a refusal.
    ///
    /// The one call a caller makes to find out whether an atlas can serve the
    /// size and cut it wants, and it never substitutes - see [`LayerNotBaked`]
    /// for why layer 0 is not an acceptable answer to a question about layer
    /// 3.
    ///
    /// A caller that already has a [`GlyphEntry`] does not need this: the
    /// entry carries its own [`GlyphEntry::layer`], which is what the GPU
    /// table uploads. This is for the question asked BEFORE a lookup.
    pub fn layer_index(&self, layer: AtlasLayer) -> Result<u16, LayerNotBaked> {
        self.layers
            .iter()
            .position(|&l| l == layer)
            .map(|i| i as u16)
            .ok_or(LayerNotBaked(layer))
    }

    /// How many texture-array layers this atlas needs. At least 1.
    pub fn layer_count(&self) -> u32 {
        self.layers.len().max(1) as u32
    }

    /// Byte offset of one layer's pixels inside [`FontAtlas::pixel_data`].
    ///
    /// The ONE place layer-major addressing is written down. Everything that
    /// reads a texel - [`FontAtlas::cell_texels`], the RGBA expansion, the
    /// per-layer upload - goes through it, so a layer stride cannot be
    /// computed two ways that disagree.
    pub fn layer_offset(&self, layer: u16) -> usize {
        (layer as usize) * (self.width as usize) * (self.height as usize) * (self.channels as usize)
    }

    /// **One cell's texels**, rows top-down, `atlas_w * channels` bytes a row -
    /// from the layer the entry names.
    ///
    /// Published rather than left to each caller because the layer is a second
    /// term in an address that used to have one, and a reader that forgets it
    /// gets plausible bytes from the wrong layer rather than an error. It is
    /// also what the bake tests compare, so the comparison and the shader agree
    /// about where a cell is by construction.
    pub fn cell_texels(&self, glyph_id: u16) -> Option<Vec<u8>> {
        let e = self.get_glyph(glyph_id)?;
        let base = self.layer_offset(e.layer);
        let row_len = (e.atlas_w as usize) * (self.channels as usize);
        let mut out = Vec::with_capacity(row_len * e.atlas_h as usize);
        for dy in 0..e.atlas_h as u32 {
            let row = (e.atlas_y as u32 + dy) * self.width + e.atlas_x as u32;
            let start = base + (row * self.channels) as usize;
            out.extend_from_slice(self.pixel_data.get(start..start + row_len)?);
        }
        Some(out)
    }

    /// **The baseline of the atlas's TEXT cells**, as a fraction of the cell
    /// from its top, or `None` if this atlas carries no text.
    ///
    /// The published form of the question `libhbui` asks every time it rules a
    /// line under a run. Answered from the set header rather than from a cell,
    /// so it cannot depend on which cell sorts first.
    pub fn text_baseline_frac(&self) -> Option<f32> {
        self.set_metrics(GlyphSet::Text).map(|m| m.baseline_frac)
    }

    /// **How far below the baseline the TEXT cells' deepest ink reaches**, in
    /// em as a downward depth, or `None` if this atlas carries no text.
    ///
    /// The published form of the second question `libhbui` asks when it rules
    /// a line: the first is where the baseline is, this is what has to be
    /// cleared. Answered from [`GlyphSet::Text`] for the same reason the
    /// baseline is - the run being ruled is text, so the marker and icon sets'
    /// geometry is not its business, and the face's own `hhea` descender is a
    /// declaration rather than a measurement
    /// ([`SetMetrics::ink_descent_em`] has the numbers).
    pub fn text_max_ink_descent_em(&self) -> Option<f32> {
        self.set_metrics(GlyphSet::Text).map(SetMetrics::max_ink_descent_em)
    }
    /// Look up a glyph entry by glyph ID.
    ///
    /// # Two orders, and this is the one the LOOKUP uses
    ///
    /// [`FontAtlas::glyphs`] is sorted by glyph id, so this is a binary search
    /// over at most [`atlas_capacity`] entries and the atlas carries no lookup
    /// table at all. It used to carry a `HashMap<u16, usize>` that was never
    /// serialized and was rebuilt on every load — an accelerator that existed
    /// because the entries had no order worth searching. They have one now.
    ///
    /// The order the CELLS are packed in is a different thing:
    /// `(set, glyph id)`, declared by [`crate::font::GlyphSet`], and it lives
    /// in each entry's `atlas_x`/`atlas_y`. Cell order is what has to stay put
    /// across a re-bake, because it decides the artifact's bytes and the
    /// texture coordinate every frame samples. Table order is derived on load
    /// and matters to nothing outside one loaded atlas: the GPU table
    /// ([`FontAtlas::glyph_table_u32s`]) and the index packed into a draw list
    /// both come from the same instance.
    pub fn get_glyph(&self, glyph_id: u16) -> Option<&GlyphEntry> {
        self.glyph_table_index(glyph_id).map(|idx| &self.glyphs[idx])
    }

    /// **Does this atlas have cells in `style`?** Asked BEFORE a run is shaped,
    /// by a caller for whom regular glyphs are the wrong answer.
    ///
    /// # A runtime atlas answers from its CELLS, because it has nothing else
    ///
    /// [`FontAtlas::empty`] carries no set header at all, so there is no row to
    /// read. Two rules there, and the second is what keeps the compute-MSDF
    /// path from being locked out of emphasis:
    ///
    /// * [`GlyphStyle::Regular`] is always carried. Everything that populates
    ///   such an atlas starts from a regular face, and an atlas with no cells
    ///   yet should answer "no cell" rather than "no style".
    /// * Any other style is carried once a cell ADDRESSED in it has been
    ///   appended ([`FontAtlas::insert_entry`]). A consumer generating bold
    ///   glyphs at runtime - `gpu::MsdfCompute` over the bold face, filed under
    ///   [`GlyphStyle::styled_glyph_id`] - is carrying bold, and the entries are
    ///   the only evidence there is. Until it does, a bold lookup refuses
    ///   rather than handing back the upright glyph that happens to sit at that
    ///   raw id.
    ///
    /// That second rule matters more than it looks: a style's baked cells and
    /// the layer they sit in are ~300 KiB gzipped (measured: bold +301,840,
    /// italic +342,707, on the merged 48px face) against a 13 KiB face, so
    /// generating styled cells on demand is a serious option for a browser -
    /// `gpu::MsdfCompute::generate_into_texture_layer` into that style's layer
    /// - and it needs no different address than a baked one.
    pub fn carries_style(&self, style: GlyphStyle) -> bool {
        if !self.sets.is_empty() {
            return self.set_metrics(style.text_set()).is_some();
        }
        style == GlyphStyle::Regular
            || self
                .glyphs
                .iter()
                .any(|e| GlyphStyle::split_glyph_id(e.glyph_id).0 == style)
    }

    /// **A glyph in a STYLE**: this raw glyph id, from that style's face, or a
    /// refusal that says which half is missing.
    ///
    /// The one call a styled run makes, and the reason it returns a `Result`
    /// rather than an `Option` is the whole point of the type: an atlas with no
    /// bold cells must not answer a bold question with a regular glyph, and a
    /// caller reading `None` would have no way to tell "this atlas has no bold"
    /// from "bold has no such glyph" - the first is an asset gap to report, the
    /// second is a codepoint outside coverage, and they are fixed in different
    /// places.
    ///
    /// `raw_glyph_id` is the id the STYLE's own face shapes to - what
    /// [`crate::font::StyledShaper`] hands back before prefixing, or
    /// `ttf_parser`'s `glyph_index` on that face. Passing an already-prefixed
    /// id is refused as [`StyledGlyphError::NotARawGlyphId`] rather than
    /// silently double-prefixed.
    ///
    /// Exactly equivalent to [`FontAtlas::get_glyph`] for
    /// [`GlyphStyle::Regular`], by construction - the prefix is zero - and
    /// `regular_styled_lookup_is_the_plain_lookup` holds the two against each
    /// other over every cell of the shipped atlas.
    pub fn styled_glyph(
        &self,
        style: GlyphStyle,
        raw_glyph_id: u16,
    ) -> Result<&GlyphEntry, StyledGlyphError> {
        if !self.carries_style(style) {
            return Err(StyledGlyphError::StyleNotBaked(style));
        }
        let Some(addr) = style.styled_glyph_id(raw_glyph_id) else {
            return Err(StyledGlyphError::NotARawGlyphId(raw_glyph_id));
        };
        self.get_glyph(addr)
            .ok_or(StyledGlyphError::NoCell(style, raw_glyph_id))
    }

    /// Every style this atlas has text cells for, in cell order. For a
    /// consumer deciding what it can offer, and for a report that says what a
    /// committed artifact actually contains.
    pub fn styles(&self) -> Vec<GlyphStyle> {
        GlyphStyle::ALL
            .iter()
            .copied()
            .filter(|&s| self.carries_style(s))
            .collect()
    }

    /// Index of a glyph in the GPU glyph table (position in `glyphs`).
    pub fn glyph_table_index(&self, glyph_id: u16) -> Option<usize> {
        self.glyphs
            .binary_search_by_key(&glyph_id, |e| e.glyph_id)
            .ok()
    }

    /// The full GPU glyph table: 8 u32 (2 × vec4<u32>) per entry, in table
    /// order — upload via `GpuSdfRenderer::upload_glyph_table`.
    pub fn glyph_table_u32s(&self) -> Vec<u32> {
        let mut out = Vec::with_capacity(self.glyphs.len() * 8);
        for e in &self.glyphs {
            out.extend_from_slice(&e.to_gpu_u32s());
        }
        out
    }

    /// Expand the pixel data to RGBA8 (alpha = 255) for texture upload -
    /// **every layer**, in layer order, which is the order
    /// `GpuSdfRenderer::upload_msdf_atlas` writes them.
    ///
    /// The texel count is taken from `pixel_data` rather than from
    /// `width * height` so that it follows the layer count without this
    /// method having to multiply by it - one less place a layer stride is
    /// written down.
    pub fn to_rgba_bytes(&self) -> Vec<u8> {
        let texels = self.pixel_data.len() / (self.channels.max(1) as usize);
        let mut rgba = Vec::with_capacity(texels * 4);
        match self.channels {
            3 => {
                for i in 0..texels {
                    let s = i * 3;
                    rgba.push(self.pixel_data.get(s).copied().unwrap_or(0));
                    rgba.push(self.pixel_data.get(s + 1).copied().unwrap_or(0));
                    rgba.push(self.pixel_data.get(s + 2).copied().unwrap_or(0));
                    rgba.push(255);
                }
            }
            4 => rgba.extend_from_slice(&self.pixel_data),
            _ => {
                for i in 0..texels {
                    let v = self.pixel_data.get(i).copied().unwrap_or(0);
                    rgba.extend_from_slice(&[v, v, v, 255]);
                }
            }
        }
        rgba
    }

    /// Serialize the atlas to bytes for embedding / baking to disk.
    ///
    /// Format ([`ATLAS_MAGIC`]), at [`ATLAS_VERSION`] for one layer:
    /// ```text
    /// [4b magic "hbfa"][4b version]                      -- 8
    /// [4b width][4b height][4b num_glyphs][4b channels]  -- 16
    /// [4b set_count]                                     -- 4   = 28 byte header
    /// [SetMetrics * set_count]                           -- 36 bytes each
    /// [GlyphEntry * num_glyphs]                          -- 32 bytes each
    /// [pixel_data]                                       -- width*height*channels bytes
    /// ```
    ///
    /// ...and at [`ATLAS_VERSION_LAYERED`] for more than one, which inserts a
    /// layer count and the [`AtlasLayer`] table between the fixed header and
    /// the sets, and multiplies the pixel data by the layer count:
    /// ```text
    /// [ ...the 28 byte header, version 4... ]
    /// [4b layer_count]                                   -- 4   = 32 byte header
    /// [AtlasLayer * layer_count]                         -- 4 bytes each
    /// [SetMetrics * set_count]                           -- 36 bytes each
    /// [GlyphEntry * num_glyphs]                          -- 32 bytes each
    /// [pixel_data]                                       -- w*h*channels*layers bytes
    /// ```
    ///
    /// **The version follows the CONTENT.** A single-layer atlas is written at
    /// v3, byte for byte what it was before layers existed - which is why the
    /// three committed artifacts still pass `bake_atlas --check` without being
    /// re-baked. [`ATLAS_VERSION_LAYERED`] says why the multi-layer case
    /// cannot share that version.
    pub fn to_bytes(&self) -> Vec<u8> {
        let layered = self.layers.len() > 1;
        let layer_table_size = if layered {
            4 + self.layers.len() * AtlasLayer::PACKED_SIZE
        } else {
            0
        };
        let sets_size = self.sets.len() * SetMetrics::PACKED_SIZE;
        let entries_size = self.glyphs.len() * GlyphEntry::PACKED_SIZE;
        let total =
            ATLAS_HEADER_SIZE + layer_table_size + sets_size + entries_size + self.pixel_data.len();

        let mut buf = Vec::with_capacity(total);
        buf.extend_from_slice(ATLAS_MAGIC);
        buf.extend_from_slice(
            &if layered { ATLAS_VERSION_LAYERED } else { ATLAS_VERSION }.to_le_bytes(),
        );
        buf.extend_from_slice(&self.width.to_le_bytes());
        buf.extend_from_slice(&self.height.to_le_bytes());
        buf.extend_from_slice(&(self.glyphs.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.channels.to_le_bytes());
        buf.extend_from_slice(&(self.sets.len() as u32).to_le_bytes());

        if layered {
            buf.extend_from_slice(&(self.layers.len() as u32).to_le_bytes());
            for layer in &self.layers {
                buf.extend_from_slice(&layer.to_bytes());
            }
        }
        for set in &self.sets {
            buf.extend_from_slice(&set.to_bytes());
        }
        for entry in &self.glyphs {
            buf.extend_from_slice(&entry.to_bytes());
        }

        buf.extend_from_slice(&self.pixel_data);
        buf
    }

    /// Deserialize from bytes.
    ///
    /// # The first eight bytes are a REFUSAL, not decoration
    ///
    /// Version 1 of this format began at `width`, with no magic and no
    /// version. Reading a v1 file with this code would take `width`'s low half
    /// as the magic, disagree, and stop - which is the entire reason the magic
    /// goes FIRST. Without it, a v1 file's `width` would be read as the magic's
    /// place, every later field would be off by the header's growth, and the
    /// atlas would load: wrong dimensions, garbage metrics, glyphs sampling
    /// whatever was at those coordinates. A stale artifact that still parses is
    /// the failure this repo has already paid for twice (see `bake_atlas`'s
    /// `--check`), so the break here is deliberate and loud, and the message
    /// says which command fixes it.
    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < ATLAS_HEADER_SIZE {
            return Err("atlas data too short");
        }
        if &data[0..4] != ATLAS_MAGIC {
            return Err(
                "not an 'hbfa' atlas - this is a headerless pre-v2 file, and reading it as v2 \
                 would silently misparse rather than fail. Re-bake it: cargo run -p libmsdf \
                 --features cpu-bake --example bake_atlas -- <font.ttf> <out.atlas> 48 6.0",
            );
        }
        let version = u32::from_le_bytes(data[4..8].try_into().unwrap());
        if version != ATLAS_VERSION && version != ATLAS_VERSION_LAYERED {
            return Err(
                "atlas version is not one this build reads - re-bake it with this libmsdf's \
                 bake_atlas example",
            );
        }

        let width = u32::from_le_bytes(data[8..12].try_into().unwrap());
        let height = u32::from_le_bytes(data[12..16].try_into().unwrap());
        let num_glyphs = u32::from_le_bytes(data[16..20].try_into().unwrap()) as usize;
        let channels = u32::from_le_bytes(data[20..24].try_into().unwrap());
        let set_count = u32::from_le_bytes(data[24..28].try_into().unwrap()) as usize;

        // **The layer table, present only at v4.** A v3 file is single-layer
        // by definition; its one layer is read off the cells below rather than
        // guessed at, so the two paths converge on the same `layers` vector
        // and nothing downstream has to ask which version it came from.
        let (layer_count, layers_end) = if version == ATLAS_VERSION_LAYERED {
            if data.len() < ATLAS_HEADER_SIZE + 4 {
                return Err("atlas data too short for the layer count");
            }
            let n = u32::from_le_bytes(
                data[ATLAS_HEADER_SIZE..ATLAS_HEADER_SIZE + 4].try_into().unwrap(),
            ) as usize;
            if n == 0 {
                return Err("atlas declares zero layers - every atlas has at least one");
            }
            if n > MAX_ATLAS_LAYERS {
                return Err(
                    "atlas declares more layers than maxTextureArrayLayers guarantees (256)",
                );
            }
            (n, ATLAS_HEADER_SIZE + 4 + n * AtlasLayer::PACKED_SIZE)
        } else {
            (1, ATLAS_HEADER_SIZE)
        };
        if data.len() < layers_end {
            return Err("atlas data too short for the layer table");
        }
        let mut layers = Vec::with_capacity(layer_count);
        if version == ATLAS_VERSION_LAYERED {
            for i in 0..layer_count {
                let offset = ATLAS_HEADER_SIZE + 4 + i * AtlasLayer::PACKED_SIZE;
                layers.push(AtlasLayer::from_bytes(
                    &data[offset..offset + AtlasLayer::PACKED_SIZE],
                )?);
            }
        }

        let sets_end = layers_end + set_count * SetMetrics::PACKED_SIZE;
        if data.len() < sets_end {
            return Err("atlas data too short for the set header");
        }
        let mut sets = Vec::with_capacity(set_count);
        for i in 0..set_count {
            let offset = layers_end + i * SetMetrics::PACKED_SIZE;
            sets.push(SetMetrics::from_bytes(
                &data[offset..offset + SetMetrics::PACKED_SIZE],
            )?);
        }
        // The counts are a fact about the same bake as the entries, so they are
        // checked against it rather than believed. A file where they disagree
        // was assembled by something other than `build`.
        if !sets.is_empty()
            && sets.iter().map(|s| s.glyph_count as usize).sum::<usize>() != num_glyphs
        {
            return Err("atlas set counts do not sum to its glyph count");
        }

        let entries_start = sets_end;
        let entries_end = entries_start + num_glyphs * GlyphEntry::PACKED_SIZE;
        if data.len() < entries_end {
            return Err("atlas data too short for glyph entries");
        }

        let mut glyphs = Vec::with_capacity(num_glyphs);
        for i in 0..num_glyphs {
            let offset = entries_start + i * GlyphEntry::PACKED_SIZE;
            glyphs.push(GlyphEntry::from_bytes(
                &data[offset..offset + GlyphEntry::PACKED_SIZE],
            )?);
        }
        // **Sorted here, not trusted from the file.** [`FontAtlas::get_glyph`]
        // binary-searches this, and a stale artifact baked before the order
        // existed would otherwise answer lookups with the wrong glyph rather
        // than failing — the worst available outcome. Sorting an already-sorted
        // few hundred entries costs a scan, against the HashMap build (an
        // allocation and a rehash per load, wasm included) this replaces.
        //
        // Nothing is lost by re-ordering a loaded atlas: every entry carries
        // its own cell coordinates, so the file's entry order is not a fact
        // about the texture.
        glyphs.sort_unstable_by_key(|e| e.glyph_id);

        // **Every entry has to name a layer the file declares.** Checked, not
        // trusted, for the reason the set counts are: an entry pointing past
        // the table would sample a layer that does not exist, and a GPU array
        // index out of range is a clamp on some drivers - which draws the
        // wrong glyph rather than failing.
        // No `debug_assert` here, deliberately: this is a fact about UNTRUSTED
        // FILE INPUT, not a programming invariant, so it must be the same
        // clean refusal in a debug build as in a release one.
        if glyphs.iter().any(|e| e.layer as usize >= layer_count) {
            return Err(
                "an atlas entry names a layer the file does not declare - at v3 that means \
                 non-zero bytes in what was pad, so the file was not written by any libmsdf",
            );
        }
        // A v3 file's single layer, read off the cells: they all agree about
        // the cell size (one bake, one `glyph_size`) and the STYLE of a cell
        // is in its address, so nothing here is a guess. `Regular` for an
        // atlas with no glyphs at all, which has no style to be wrong about.
        if layers.is_empty() {
            let size_px = glyphs.first().map_or(0, |e| e.atlas_h);
            let style = glyphs
                .first()
                .map_or(GlyphStyle::Regular, |e| GlyphStyle::split_glyph_id(e.glyph_id).0);
            layers.push(AtlasLayer::new(size_px, style));
        }

        let pixel_start = entries_end;
        let expected_pixels = (width as usize) * (height as usize) * (channels as usize)
            * layer_count;
        if data.len() < pixel_start + expected_pixels {
            return Err("atlas data too short for pixel data");
        }

        let pixel_data = data[pixel_start..pixel_start + expected_pixels].to_vec();

        Ok(Self {
            width,
            height,
            channels,
            pixel_data,
            glyphs,
            sets,
            layers,
        })
    }

    /// Build an atlas shell (no pixel data yet) for dynamic population via
    /// the atlas manager + compute-MSDF path. ONE layer - see
    /// [`FontAtlas::empty_layered`] for more.
    pub fn empty(width: u32, height: u32, channels: u32) -> Self {
        Self::empty_layered(width, height, channels, &[AtlasLayer::new(0, GlyphStyle::Regular)])
    }

    /// [`FontAtlas::empty`] with a declared layer table - the runtime shell
    /// for a consumer generating cells into more than one layer
    /// (`gpu::MsdfCompute` over a style face, filed under
    /// [`GlyphStyle::styled_glyph_id`] with the layer set on the entry).
    ///
    /// A layer of `size_px: 0` means "not baked at a declared size", which is
    /// what a runtime atlas is: its cells are generated at whatever size was
    /// asked for. [`FontAtlas::layer_index`] will not match such a layer
    /// against a real size, and that is correct - a caller asking for the 16px
    /// layer of an atlas that has none should be refused.
    ///
    /// **A single-layer atlas serialized and reloaded comes back with its
    /// size MEASURED**, not with the zero: a v3 file carries no layer table,
    /// so [`FontAtlas::from_bytes`] reads the size off the cells. That is a
    /// gain rather than a round-trip defect - the reloaded atlas says what its
    /// cells actually are - but it does mean `empty` plus appends is not a
    /// fixed point through the file, and `a_runtime_shell_gains_its_measured
    /// _size_through_the_file` pins the behaviour so it cannot drift into
    /// being one silently.
    pub fn empty_layered(
        width: u32,
        height: u32,
        channels: u32,
        layers: &[AtlasLayer],
    ) -> Self {
        let layers: Vec<AtlasLayer> = if layers.is_empty() {
            vec![AtlasLayer::new(0, GlyphStyle::Regular)]
        } else {
            layers.to_vec()
        };
        Self {
            width,
            height,
            channels,
            pixel_data: vec![
                0;
                (width as usize) * (height as usize) * (channels as usize) * layers.len()
            ],
            glyphs: Vec::new(),
            layers,
            // No face was measured, so there is nothing to say. A consumer
            // that needs a baseline here gets `FALLBACK_BASELINE_FRAC` and
            // knows it is a fallback, rather than a plausible-looking number
            // it cannot tell apart from a measured one.
            sets: Vec::new(),
        }
    }

    /// The (square) glyph cell size in atlas texels, or `None` for an atlas
    /// with no glyphs. Every cell in an atlas is baked at one size.
    pub fn cell_px(&self) -> Option<f32> {
        self.glyphs.first().map(|g| g.atlas_h as f32)
    }

    /// The smallest font size (logical px) this atlas can still antialias, for
    /// a bake of `px_range` texels — the **documented minimum** of the MSDF
    /// text path.
    ///
    /// A glyph cell is drawn scaled to the line box (`font_size ×
    /// LINE_BOX_RATIO`), so the baked field is minified by
    /// `cell_px / line_box_h` and the usable screen-space distance range is
    /// `px_range × line_box_h / cell_px` pixels (see
    /// [`crate::drawlist::screen_px_range`]). Antialiasing needs at least one
    /// pixel of range, which bottoms out at
    ///
    /// ```text
    /// font_size >= cell_px / (LINE_BOX_RATIO * px_range)
    /// ```
    ///
    /// — 6.15px for the shipped 48px/6.0 Roboto atlas. Below it the shader
    /// clamps the ramp to one pixel but the field itself has nothing left to
    /// give, so glyph edges harden and sub-pixel features (a 't' crossbar, an
    /// 'e' bar) start dropping out. Bake a finer atlas (smaller cells at the
    /// same `px_range`, or a larger `px_range`) rather than shipping text
    /// below this size.
    pub fn min_antialiased_font_size(&self, px_range: f32) -> f32 {
        let cell = self.cell_px().unwrap_or(0.0);
        if px_range <= 0.0 || cell <= 0.0 {
            return 0.0;
        }
        cell / (crate::drawlist::LINE_BOX_RATIO * px_range)
    }

    /// **Whether a run at `font_size` still has ink in it** — at or above HALF
    /// of [`FontAtlas::min_antialiased_font_size`], where the baked field still
    /// covers half a pixel of alpha ramp.
    ///
    /// This is the exact question
    /// [`DrawList::push_shaped_text`](crate::DrawList::push_shaped_text)
    /// debug-asserts on, and that assertion is written in terms of this method
    /// so there is ONE expression rather than two that must agree. A caller
    /// that scales its type off a region — a device preview, a zooming pane —
    /// can therefore ask *before* composing whether the surface it is about to
    /// draw on can carry its own type, instead of finding out one run at a time
    /// after every size has already collapsed.
    ///
    /// Between this bound and the full [`FontAtlas::min_antialiased_font_size`]
    /// antialiasing degrades but the glyph is still there, which is why a
    /// zooming view is expected to pass through that range. Below THIS bound
    /// there is nothing left to draw.
    pub fn antialiases_at(&self, font_size: f32, px_range: f32) -> bool {
        // The 1e-3 slack keeps a size computed as exactly the bound (through a
        // scale factor, so with rounding) on the passing side of it.
        font_size + 1e-3 >= 0.5 * self.min_antialiased_font_size(px_range)
    }

    /// Register a glyph entry (dynamic append path). Replaces any existing
    /// entry for the same glyph id and returns the table index.
    ///
    /// The entry is inserted at its glyph id's place, not appended, because
    /// [`FontAtlas::get_glyph`] binary-searches the table. Table indices after
    /// the insertion point therefore shift — which is why an index is a value
    /// to use now, never one to cache across a later insert. (Its CELL does not
    /// move: this path allocates cells through [`crate::font::AtlasManager`],
    /// which is a separate allocator over the texture.)
    pub fn insert_entry(&mut self, entry: GlyphEntry) -> usize {
        match self
            .glyphs
            .binary_search_by_key(&entry.glyph_id, |e| e.glyph_id)
        {
            Ok(idx) => {
                self.glyphs[idx] = entry;
                idx
            }
            Err(idx) => {
                self.glyphs.insert(idx, entry);
                idx
            }
        }
    }
}

// ── Uniform em-square projection (shared by CPU bake + compute MSDF) ────

/// Projection mapping shape (font-unit) coordinates into a glyph's atlas
/// cell: `cell_px = (shape + translate) * scale`, y-up. The same numbers
/// drive CPU msdfgen baking and the WGSL compute generator so their outputs
/// are comparable texel-for-texel.
#[derive(Debug, Clone, Copy)]
pub struct GlyphProjection {
    /// Uniform scale: font units → cell pixels.
    pub scale: f64,
    /// Translation in font units (applied before scaling).
    pub tx: f64,
    pub ty: f64,
    /// Atlas row of the baseline, measured from top of cell (pixels).
    pub baseline_row: f32,
    /// Left ink margin inside the cell (pixels).
    pub x_margin: f32,
    /// Atlas pixels per em.
    pub px_per_em: f32,
    /// Standard horizontal advance, normalized to em.
    pub advance_x: f32,
}

/// Compute the uniform em-square projection for one glyph.
///
/// Matches matter-stream's `FontAtlasBuilder::build` layout math: all glyphs
/// share `scale = gs / (upem * 1.3)`; cap height (from 'A', resolved via
/// cmap — upstream hardcoded GID 36) is aligned to 15% from the cell top;
/// ink is centered horizontally. Returns None for glyphs without outlines
/// (whitespace) — callers fall back to default cell metrics.
pub fn glyph_projection(
    face: &ttf_parser::Face,
    glyph_id: u16,
    glyph_size: u32,
) -> Option<GlyphProjection> {
    let gs = glyph_size as f64;
    let upem = face.units_per_em() as f64;
    let em_scale = gs / (upem * CELL_EM_RATIO);

    let gid = ttf_parser::GlyphId(glyph_id);
    let advance_x = face.glyph_hor_advance(gid).unwrap_or(0) as f32 / upem as f32;

    let cap_y_max = cap_height(face);

    let bbox = face.glyph_bounding_box(gid)?;

    // Horizontal: center the ink in the cell.
    let ink_w = (bbox.x_max - bbox.x_min) as f64 * em_scale;
    let target_px_x = (gs - ink_w) * 0.5;
    let g_tx = target_px_x / em_scale - bbox.x_min as f64;

    // Vertical: align the font's cap height to 15% from the cell top so all
    // digits and caps share a level top boundary.
    let target_top_px = gs * CAP_TOP_FRAC;
    let g_ty = (gs - target_top_px) / em_scale - cap_y_max;

    Some(GlyphProjection {
        scale: em_scale,
        tx: g_tx,
        ty: g_ty,
        baseline_row: (gs - em_scale * g_ty) as f32,
        x_margin: (em_scale * g_tx) as f32,
        px_per_em: (em_scale * upem) as f32,
        advance_x,
    })
}

/// **Where the cap band starts**, as a fraction of the cell from its top.
///
/// [`glyph_projection`] aligns every face's cap height to this line so that
/// digits and capitals share a level top boundary, and the baseline falls out
/// of it: `baseline_frac = CAP_TOP_FRAC + cap_height / (upem * LINE_BOX_RATIO)`.
/// Named because three places need the same number and one of them
/// ([`FALLBACK_BASELINE_FRAC`]) is not near the other two.
///
/// It is NOT [`crate::drawlist::X_MARGIN_FRAC`], which is also 0.15 and is the
/// horizontal ink margin - two different measurements that happen to share a
/// value, kept apart so that changing one does not silently change the other.
pub const CAP_TOP_FRAC: f64 = 0.15;

/// The cap height assumed for a face whose `'A'` has no bounding box, in em.
/// Only [`glyph_projection`] and [`FALLBACK_BASELINE_FRAC`] use it.
pub const FALLBACK_CAP_HEIGHT_EM: f64 = 0.7;

/// **Em per atlas cell**: a cell is 1.3 em tall, which is why one em of drawn
/// text is exactly the font size.
///
/// The same ratio as [`crate::drawlist::LINE_BOX_RATIO`] and it must stay that
/// way - `a_cell_is_the_line_box` asserts it - but it is written here as its
/// own `f64` rather than converted from that `f32`. `LINE_BOX_RATIO as f64` is
/// 1.2999999523162842, not 1.3, and this number multiplies the projection every
/// baked texel is generated through: taking the lossy route would re-bake every
/// cell in every atlas to buy nothing.
pub const CELL_EM_RATIO: f64 = 1.3;

/// **The baseline of an atlas that carries no [`SetMetrics`] at all** — a
/// runtime-populated [`FontAtlas::empty`], never a baked one.
///
/// It is [`glyph_projection`]'s own answer for a face it cannot measure
/// (`CAP_TOP_FRAC + FALLBACK_CAP_HEIGHT_EM / LINE_BOX_RATIO`), rather than a
/// separate number chosen here, so the two fallbacks cannot drift apart. A
/// baked atlas can no longer reach it: [`FontAtlas::from_bytes`] refuses a file
/// with no set header rather than substituting a default, which is exactly the
/// substitution that used to hide a wrong baseline.
pub const FALLBACK_BASELINE_FRAC: f32 =
    (CAP_TOP_FRAC + FALLBACK_CAP_HEIGHT_EM / CELL_EM_RATIO) as f32;

/// **The ink depth assumed for an atlas that carries no [`SetMetrics`]** - the
/// same runtime-populated [`FontAtlas::empty`] that gets
/// [`FALLBACK_BASELINE_FRAC`], and never a baked one.
///
/// A downward depth in em, matching [`SetMetrics::max_ink_descent_em`], and
/// deliberately the deepest ink in EITHER bundled face rather than its Text
/// set's: 512/2048, the `U+F8F0` marker. A fallback is used when nothing was
/// measured, so it should be a bound on what this repo bakes rather than a
/// typical value - erring here puts a rule slightly low, and erring the other
/// way puts it through a descender.
pub const FALLBACK_MAX_INK_DESCENT_EM: f32 = 0.25;

/// Default cell metrics for glyphs without outlines (whitespace):
/// (baseline_row, x_margin, px_per_em).
///
/// # The baseline here is the FACE's, not a round number
///
/// It used to be `gs * 0.75`, which was not any face's baseline: every glyph
/// WITH an outline is baked at `CAP_TOP_FRAC * gs + cap_height * em_scale`
/// (33.45 of 48 for Roboto, a fraction of 0.6969), so the two whitespace cells
/// in the shipped atlas were the only cells in it that disagreed with the ink
/// beside them by most of a pixel at body sizes.
///
/// That disagreement was invisible - a whitespace cell is empty, so where its
/// baseline sits changes no texel - right up until something read a baseline
/// off "a cell" rather than off a face, and got whichever cell sorted first.
/// The cells all agree now, so that read cannot go wrong again, and
/// [`FontAtlas::set_metrics`] answers it properly on top of that.
pub fn default_cell_metrics(face: &ttf_parser::Face, glyph_size: u32) -> (f32, f32, f32) {
    let gs = glyph_size as f64;
    let upem = face.units_per_em() as f64;
    let em_scale = gs / (upem * CELL_EM_RATIO);
    (
        (gs * CAP_TOP_FRAC + em_scale * cap_height(face)) as f32,
        // The same 0.15 as [`crate::drawlist::X_MARGIN_FRAC`], written out
        // because that constant is an `f32` and `0.15f32 as f64` is
        // 0.1500000059604645 - enough to land on a different `f32` here.
        (gs * 0.15) as f32,
        (em_scale * upem) as f32,
    )
}

/// The face's cap height in font units - `'A'`'s ink top, or
/// [`FALLBACK_CAP_HEIGHT_EM`] of the em square for a face that does not draw
/// one. The one place the cap band is measured, for
/// [`glyph_projection`] and [`default_cell_metrics`] alike.
pub fn cap_height(face: &ttf_parser::Face) -> f64 {
    face.glyph_index('A')
        .and_then(|a| face.glyph_bounding_box(a))
        .map(|b| b.y_max as f64)
        .unwrap_or(face.units_per_em() as f64 * FALLBACK_CAP_HEIGHT_EM)
}

// ── The pinned atlas grid ───────────────────────────────────────────────

/// Columns of glyph cells. Fixed, for predictable shelf alignment — and
/// because a texture whose WIDTH changed would move every glyph's `u` in
/// `sdf_render.wgsl`, exactly as a changing height moves every `v`.
///
/// # This is the binding constraint on ONE LAYER's capacity, and it is a COST
/// rather than a limit
///
/// [`ATLAS_ROWS`] argues that 40 rows is the largest pin that fits the weakest
/// target, which is true at 8 columns and reads as though the grid were full.
/// It is not: 8 x 50px is 400 texels of a 2048 floor, so the shipped texture
/// uses 19% of the area the weakest target guarantees. **16 columns at the same
/// 40 rows is 800x2000 - inside the same floor - and holds 640 cells.**
///
/// That used to be the only way emphasis could be baked: a [`GlyphStyle`] is
/// 205 cells (191 declared codepoints plus 14 shaped forms), 96 were free, and
/// nothing about ROWS could make room. **[`AtlasLayer`] retired that
/// argument.** A style is a second LAYER, with its own 320 cells, so widening
/// this buys nothing emphasis needs and still costs the one-time re-bake in
/// which every glyph's `u` moves and every review fixture is re-blessed once.
///
/// What would still want columns is a wider vocabulary AT ONE SIZE AND CUT -
/// a symbol set that outgrew 320 cells of regular text plus icons. That has
/// not happened; the shipped layer is 224 of 320.
pub const ATLAS_COLS: u32 = 8;

/// **Rows of glyph cells the atlas is baked to, whether or not they are used**
/// — so the texture's dimensions are a CONSTANT and a re-bake cannot move a
/// glyph that did not change.
///
/// # Why the height is pinned rather than fitted
///
/// The shader samples a cell at `(atlas_gy + acy + 0.5) / atlas_dim.y`
/// (`sdf_render.wgsl`). Fitting the texture to its contents makes that
/// denominator a function of the glyph COUNT, so adding one glyph anywhere
/// perturbs the sample coordinate of *every* glyph in *every* frame — an
/// unaccountable last-ulp diff across the whole fixture suite, in panes the
/// change never touched. Reserving the rows up front costs some blank texels
/// and buys the property [`FontAtlasBuilder::add_shipped_coverage`] wants: a
/// re-bake is a strict superset of the last one, and **a rendered frame that
/// moves is a real finding**.
///
/// (Cells still shift within the grid if a glyph sorts BEFORE an existing one
/// — see [`crate::font::GlyphSet`], which declares the order so that a glyph
/// added to the last set cannot. Pinning the height fixes the global
/// denominator; the set order fixes the local numbering. Both are needed and
/// they are different things.)
///
/// # Why 40, and what the budget is
///
/// 40 rows x [`ATLAS_COLS`] = **320 cells**, **per [`AtlasLayer`]**. The
/// figures below describe the layer the shipped coverage is in; a style or a
/// second point size is a second layer with its own 320, not a claim on these. At the shipped 48px cell that is
/// `8 * 50 = 400` x `40 * 50 = 2000` px, inside the **2048** floor that
/// `wgpu::Limits::downlevel_defaults()` and `downlevel_webgl2_defaults()` set
/// for `max_texture_dimension_2d` (WebGPU's own default is 8192 and desktop /
/// Android Vulkan is typically 4096+; this repo pins no limits, so 2048 is the
/// conservative assumption). 40 rows is therefore the LARGEST pin that fits the
/// weakest target — which is the point of paying the one-time cost: there is no
/// second churn available under that ceiling.
///
/// What is queued today (`add_shipped_coverage`, merged face, 2026-08-16):
///
/// ```text
///   95  printable ASCII                        U+0020..U+007E
///   96  Latin-1 Supplement                     U+00A0..U+00FF
///   14  the shaped-ASCII superset beyond cmap  (ligatures, GSUB forms)
///   18  SYMBOLS: 13 borrowed + 4 drawn + 1 marker
///    1  glyph 0, the placeholder box
///  ---
///  224  of 320   (96 free; the plain face is 211, being 4 borrowed short)
/// ```
///
/// The text side is closed — those ranges are declared in
/// [`crate::font::TEXT_RANGES`] and are not going to grow again. Only the
/// SYMBOL side grows, and Tim's estimate (2026-08-16) is **~16 symbol glyphs
/// total** across icons and markers; we are at 18. So the 96 free cells are
/// roughly five times the entire intended symbol budget, and a wave that needs
/// to raise this number should first ask why the symbol set quintupled.
///
/// Raising it past 40 rows is not a packing decision — at 48px cells it puts
/// the texture over 2048 and becomes a question about which GPUs we support.
/// **And a texture ARRAY does not relax it**: every layer of an array shares
/// the array's dimensions, so 2050px is over the floor whether one layer needs
/// it or all of them do. Growth is layers, or columns; never rows.
pub const ATLAS_ROWS: u32 = 40;

/// Glyph cells one baked atlas layer holds: [`ATLAS_ROWS`] x [`ATLAS_COLS`].
/// [`FontAtlasBuilder::build`] fails rather than silently growing past it, and
/// it checks PER LAYER - see [`AtlasLayer`] for what shares one.
pub const fn atlas_capacity() -> usize {
    (ATLAS_ROWS * ATLAS_COLS) as usize
}

// ── Builder (CPU baking — native-only) ──────────────────────────────────

/// Builder for constructing MSDF font atlases.
///
/// Queueing works on every target; [`FontAtlasBuilder::build`] (the msdfgen
/// CPU bake) is native-only.
pub struct FontAtlasBuilder {
    font_data: Vec<u8>,
    glyph_size: u32,
    px_range: f64,
    /// **Queued glyphs, each with the vocabulary it was queued FROM.**
    ///
    /// The set has to be recorded here because here is the only place it
    /// exists: the caller knows it is adding the marker block or the shaped
    /// superset at the moment it asks, and a glyph id carries no trace of it
    /// afterwards. Recovering it later would mean guessing from the id, which
    /// is exactly the emergent ordering [`crate::font::GlyphSet`] replaces.
    ///
    /// This is a QUEUE, not the layout: [`FontAtlasBuilder::cell_order`] sorts
    /// it.
    queued_glyphs: Vec<CellKey>,
    /// **The face each STYLE's cells are baked from**, beyond the regular one
    /// in `font_data`.
    ///
    /// A style is a second FACE, so its glyphs cannot be baked from the face
    /// the builder was constructed with - and the bake has to be able to get
    /// back to the right one, per cell, long after the queue was filled. The
    /// styled glyph id says which ([`GlyphStyle::split_glyph_id`]), so this is
    /// the table that turns that answer into bytes.
    ///
    /// Empty for every bake that exists today, which is what makes the styled
    /// path unable to disturb them.
    style_faces: Vec<(GlyphStyle, Vec<u8>)>,
}

impl FontAtlasBuilder {
    /// Create a builder for a given font.
    ///
    /// `glyph_size` is the MSDF bitmap size per glyph (e.g., 32 or 48).
    /// `px_range` is the distance field range in pixels (typically 4.0-8.0).
    pub fn new(font_data: Vec<u8>, glyph_size: u32, px_range: f64) -> Self {
        Self {
            font_data,
            glyph_size,
            px_range,
            queued_glyphs: Vec::new(),
            style_faces: Vec::new(),
        }
    }

    /// The face a style's cells are baked from: the builder's own for
    /// [`GlyphStyle::Regular`], a registered one otherwise.
    fn style_face(&self, style: GlyphStyle) -> Option<&[u8]> {
        if style == GlyphStyle::Regular {
            return Some(&self.font_data);
        }
        self.style_faces
            .iter()
            .find(|(s, _)| *s == style)
            .map(|(_, data)| data.as_slice())
    }

    /// MSDF bitmap size per glyph cell.
    pub fn glyph_size(&self) -> u32 {
        self.glyph_size
    }

    /// Distance field range in pixels.
    pub fn px_range(&self) -> f64 {
        self.px_range
    }

    /// Queue a single glyph ID for atlas generation, as part of `set`.
    ///
    /// **A glyph queued twice keeps the LOWEST set that claimed it**, not the
    /// first one to ask. One glyph gets one cell, so a rule is needed either
    /// way; taking the minimum makes it independent of the order the queue
    /// calls came in, which is the whole point of recording a set. It is also
    /// the meaning the sets want: an `f` reached by both
    /// [`FontAtlasBuilder::add_ascii`] and
    /// [`FontAtlasBuilder::add_shaped_ascii`] is TEXT, and
    /// [`GlyphSet::ShapedText`] means "beyond the declared ranges" rather than
    /// "everything shaping emits".
    pub fn add_glyph(&mut self, set: GlyphSet, glyph_id: u16) {
        let key = CellKey::new(set, glyph_id);
        match self.queued_glyphs.iter_mut().find(|k| k.glyph_id() == glyph_id) {
            Some(queued) => *queued = (*queued).min(key),
            None => self.queued_glyphs.push(key),
        }
    }

    /// Queue all glyphs for a codepoint range (resolves via cmap) into `set`.
    ///
    /// **Through the face `set` belongs to**, which for a styled set is the
    /// one registered by [`FontAtlasBuilder::add_styled_coverage`] and not the
    /// builder's own. A codepoint is resolved by the cmap of the face that will
    /// bake it, because that is the only face whose glyph ids mean anything.
    pub fn add_codepoint_range(&mut self, set: GlyphSet, start: char, end: char) {
        let style = set.style();
        let Some(face_data) = self.style_face(style) else {
            debug_assert!(
                false,
                "{set:?} was queued with no {style:?} face registered - \
                 `add_styled_coverage` registers one, and without it this queues nothing"
            );
            return;
        };
        let face = match ttf_parser::Face::parse(face_data, 0) {
            Ok(f) => f,
            Err(_) => return,
        };

        let mut gids = Vec::new();
        for cp in (start as u32)..=(end as u32) {
            if let Some(ch) = char::from_u32(cp) {
                // The ADDRESS, not the face's own id: two faces number their
                // glyphs independently and one atlas cannot hold both
                // numberings. A face too wide to prefix contributes nothing
                // rather than aliasing onto another style's cell.
                if let Some(gid) = face.glyph_index(ch).and_then(|g| style.styled_glyph_id(g.0)) {
                    gids.push(gid);
                }
            }
        }
        for gid in gids {
            self.add_glyph(set, gid);
        }
    }

    /// Queue common Latin + digit + punctuation glyphs, as [`GlyphSet::Text`].
    ///
    /// cmap-only: this is the glyph a codepoint maps to *in isolation*. Text is
    /// not laid out in isolation — see [`FontAtlasBuilder::add_shaped_ascii`].
    pub fn add_ascii(&mut self) {
        self.add_codepoint_range(GlyphSet::Text, ' ', '~');
    }

    /// **The order cells are placed in**: every queued glyph as
    /// `(set, glyph id)`, sorted — [`crate::font::GlyphSet`] first, glyph id
    /// within it.
    ///
    /// [`FontAtlasBuilder::build`] lays the atlas out in exactly this sequence,
    /// so entry *n* of this is the glyph in cell *n*: row `n / ATLAS_COLS`,
    /// column `n % ATLAS_COLS`. It is available without the `cpu-bake` feature
    /// on purpose — the layout is a fact about the QUEUE, so a caller (or a
    /// test) can ask where a glyph will land, or check where a shipped atlas
    /// put it, without running msdfgen.
    pub fn cell_order(&self) -> Vec<CellKey> {
        let mut order = self.queued_glyphs.clone();
        // ONE integer compare. The key packs the set above the glyph id, so
        // ascending numeric order IS `(set, glyph id)` order - the join and the
        // sort are the same fact rather than two that must agree.
        order.sort_unstable();
        order
    }

    /// Queue every glyph the SHAPER can emit for printable ASCII, as
    /// [`GlyphSet::ShapedText`] — a superset of [`FontAtlasBuilder::add_ascii`]'s
    /// cmap lookups, of which only the glyphs BEYOND that set land here (a
    /// glyph keeps the lowest set that claims it, see
    /// [`FontAtlasBuilder::add_glyph`]).
    ///
    /// Two ways shaping escapes the cmap set:
    /// * **GSUB substitutions on a single codepoint** — some subset fonts remap
    ///   digits (`lnum`/`pnum`) to glyphs the cmap never names;
    /// * **ligatures**, which only appear when the right characters are
    ///   *adjacent*: Roboto's `liga` folds `fi`/`fl`/`ffi`/`ffl` into single
    ///   glyphs. Shaping the alphabet one character at a time never produces
    ///   them, and a glyph with no atlas cell falls back to table index 0 and
    ///   draws blank — "fifty" renders as "fty", silently.
    ///
    /// So this shapes every ordered PAIR of printable ASCII (plus `ff`-led
    /// triples, for the three-glyph ligatures), interleaved into one string per
    /// leading character to keep it to ~100 shaping calls.
    pub fn add_shaped_ascii(&mut self) {
        self.add_shaped_ascii_of(GlyphStyle::Regular);
    }

    /// [`FontAtlasBuilder::add_shaped_ascii`] for one style's face, into that
    /// style's shaped set ([`GlyphStyle::shaped_set`]).
    ///
    /// The probe strings are the same; the face is not, and neither are the
    /// glyph ids that come back. See `shaped_set`'s doc for why a second face
    /// cannot borrow the first one's answer.
    pub fn add_shaped_ascii_of(&mut self, style: GlyphStyle) {
        let Some(face_data) = self.style_face(style) else {
            debug_assert!(false, "no {style:?} face registered to shape with");
            return;
        };
        let Ok(shaper) = crate::font::shaper::TextShaper::new(face_data.to_vec()) else {
            return;
        };
        let set = style.shaped_set();
        let printable: Vec<char> = (0x20u8..=0x7e).map(|b| b as char).collect();

        let mut probes: Vec<String> = Vec::with_capacity(printable.len() + 2);
        probes.push(printable.iter().collect());
        // `x y0 x y1 x y2 …` carries every (x, yn) AND (yn, x) pair.
        for &x in &printable {
            let mut s = String::with_capacity(printable.len() * 2);
            for &y in &printable {
                s.push(x);
                s.push(y);
            }
            probes.push(s);
        }
        // `ff y0 ff y1 …` carries every ffi/ffl-shaped triple.
        let mut triples = String::new();
        for &y in &printable {
            triples.push_str("ff");
            triples.push(y);
        }
        probes.push(triples);

        let mut queued: Vec<u16> = Vec::new();
        for probe in probes {
            for g in shaper.shape(&probe).glyphs {
                // Raw glyph 0 is skipped rather than queued: the placeholder is
                // style-invariant and is addressed as cell 0 in every style
                // ([`GlyphStyle::styled_glyph_id`]), so queueing it here would
                // ask `GlyphSet::Placeholder`'s one cell to belong to a styled
                // set as well. (Full coverage means it never comes up for the
                // bundled faces; a narrower face is where it would.)
                if g.glyph_id == 0 {
                    continue;
                }
                if let Some(addr) = style.styled_glyph_id(g.glyph_id) {
                    queued.push(addr);
                }
            }
        }
        for addr in queued {
            self.add_glyph(set, addr);
        }
    }

    /// **Queue exactly what a bundled face ships**, so the bake and
    /// [`crate::font::TEXT_RANGES`] cannot disagree about coverage.
    ///
    /// Stated as ranges the face is asked about, never as a list of glyphs.
    /// [`crate::font::PRIVATE_USE`] is asked for as its three declared BLOCKS,
    /// so the merged face's borrowed icons, our own icons and both faces'
    /// [`crate::font::MARKERS`] come along without this ever learning an icon
    /// name or a marker name, and a face that defines nothing in a block queues
    /// nothing for it. **Adding a glyph is therefore an edit to the FONT, not
    /// to this method** - which is the whole reason coverage has one home.
    ///
    /// # The order used to be this method, and now it is not
    ///
    /// Cells were packed in QUEUE order, so the sequence of calls below was the
    /// atlas layout: ASCII, then the shaped superset, then the Private Use Area
    /// in codepoint order, then the rest of the text ranges, then glyph 0.
    /// Reordering two lines renumbered every cell after the first of them, and
    /// nothing said so. Latin-1 sat after the Private Use Area for no reason
    /// except that ASCII and the icons had been baked first.
    ///
    /// The layout is now [`FontAtlasBuilder::cell_order`]: `(set, glyph id)`,
    /// with the sets declared by [`crate::font::GlyphSet`]. Two consequences
    /// worth naming, because they were the two live defects:
    ///
    /// * **A new PUA glyph is a true append.** Scanning that range by codepoint
    ///   used to put a new glyph *before* Latin-1 and, for a BORROWED icon, in
    ///   the middle of its own block - a Material icon sits at Material's
    ///   codepoint, so `code` (`U+E86F`) lands between `more_vert` and
    ///   `settings` whatever we would prefer. Every script in `fonts/` appends
    ///   glyph ids, so ordering by glyph id inside a set makes any additive
    ///   font edit append here too, borrowed icons included.
    /// * **The call sequence below is no longer load-bearing.** These lines can
    ///   be reordered, and [`crate::font::TEXT_RANGES`] can be reordered or
    ///   widened, without moving a cell. The `debug_assert` that used to pin
    ///   `TEXT_RANGES[0]` to the range `add_ascii` covers is gone with the
    ///   coupling it guarded.
    ///
    /// A re-bake is therefore a strict superset of the last one whenever the
    /// addition is to the last set that has anything in it, and a rendered
    /// frame that moves is a real finding rather than repacking noise.
    ///
    /// The atlas's DIMENSIONS are not affected by any of this: they are pinned
    /// ([`ATLAS_ROWS`]), so the divisor in `sdf_render.wgsl`'s
    /// `(atlas_gx + acx + 0.5) / atlas_dim.x` is a constant either way.
    pub fn add_shipped_coverage(&mut self) {
        // The declared TEXT coverage, every range of it, through the cmap.
        for &(lo, hi) in crate::font::TEXT_RANGES {
            self.add_codepoint_range(GlyphSet::Text, lo, hi);
        }
        // ...unioned with what the SHAPER emits for ASCII, which is a superset.
        // Glyphs already claimed above stay Text (`add_glyph` keeps the lowest
        // set), so this contributes exactly the ligatures and GSUB forms.
        self.add_shaped_ascii();

        // The Private Use Area, as the three blocks that tile it: the vendor's
        // half, then ours. Still ranges, never names.
        let (pua_lo, pua_hi) = crate::font::PRIVATE_USE;
        let (own_lo, own_hi) = crate::font::OWNED_BLOCKS;
        let (icons_lo, icons_hi) = crate::font::HIGHBAY_ICONS_BLOCK;
        let (markers_lo, markers_hi) = crate::font::MARKERS;
        debug_assert!(
            own_hi == pua_hi
                && icons_lo == own_lo
                && markers_hi == own_hi
                && icons_hi as u32 + 1 == markers_lo as u32,
            "the owned blocks no longer tile the top of the carveout, so the three \
             scans below leave a hole in it - a codepoint in the gap would be \
             defined by the face and never queued, which draws as a tofu box \
             (`owned_blocks_are_the_top_of_the_carveout` states the same shape)"
        );
        let borrowed_hi = char::from_u32(own_lo as u32 - 1).expect("U+F7FF is a scalar value");
        self.add_codepoint_range(GlyphSet::BorrowedIcons, pua_lo, borrowed_hi);
        self.add_codepoint_range(GlyphSet::Markers, markers_lo, markers_hi);
        self.add_codepoint_range(GlyphSet::OwnedIcons, icons_lo, icons_hi);

        // **The placeholder.** Queued explicitly because no codepoint maps to
        // it: `cmap` cannot name glyph 0, so no `add_codepoint_range` will ever
        // reach it, and without a cell every uncovered character is back to
        // drawing nothing. Its own set, and the first one: one glyph, no way to
        // gain a second, so it is the one cell that can never be pushed along.
        //
        // It is also the one cell every STYLE shares
        // ([`GlyphStyle::styled_glyph_id`]), which is why the styled queues
        // below never add a second.
        self.add_glyph(GlyphSet::Placeholder, 0);
    }

    /// **Queue a second FACE as a style** - the declared text coverage, and
    /// what that face's own shaper emits beyond it.
    ///
    /// This is the whole bake side of emphasis: `add_styled_coverage(Bold,
    /// ROBOTO_BOLD_ASCII.to_vec())` beside [`add_shipped_coverage`], and the
    /// atlas comes out with bold cells that a run addresses through
    /// [`GlyphStyle::styled_glyph_id`].
    ///
    /// [`add_shipped_coverage`]: FontAtlasBuilder::add_shipped_coverage
    ///
    /// # It queues TEXT and nothing else
    ///
    /// No Private Use Area scan, in deliberate contrast to
    /// [`add_shipped_coverage`]. Markers, our own icons and the borrowed
    /// Material Symbols are style-INVARIANT (see [`GlyphSet::style`]): a bold
    /// arrowhead is not a thing, and a second copy of `send` in a heavier
    /// weight would be 13 cells spent on a distinction no caller can make. A
    /// style face is prose, and prose is [`crate::font::TEXT_RANGES`].
    ///
    /// # Every cell it queues sorts AFTER every cell that already existed
    ///
    /// [`GlyphStyle::text_set`] and [`GlyphStyle::shaped_set`] are sets 6..=11,
    /// declared after the unstyled ones, and cells are laid out in
    /// `(set, glyph id)` order. So this is a pure append: an atlas re-baked with
    /// a style is a strict superset of the same atlas without one, cell for
    /// cell, and a frame that moves after adding bold is a real finding rather
    /// than repacking noise. `a_styled_bake_appends` holds it against a bake.
    ///
    /// # It can refuse, and there are three ways
    ///
    /// The face does not parse; the style already has one (registering a second
    /// would silently pick a winner for cells that are already queued); or the
    /// style is [`GlyphStyle::Regular`], which is the builder's own face and
    /// [`add_shipped_coverage`]'s job.
    pub fn add_styled_coverage(
        &mut self,
        style: GlyphStyle,
        font_data: Vec<u8>,
    ) -> Result<(), String> {
        if style == GlyphStyle::Regular {
            return Err(
                "GlyphStyle::Regular is the face this builder was constructed with - \
                 add_shipped_coverage queues it"
                    .into(),
            );
        }
        if self.style_face(style).is_some() {
            return Err(format!("{style:?} already has a face registered on this builder"));
        }
        ttf_parser::Face::parse(&font_data, 0).map_err(|e| format!("{style:?} face: {e}"))?;
        self.style_faces.push((style, font_data));

        for &(lo, hi) in crate::font::TEXT_RANGES {
            self.add_codepoint_range(style.text_set(), lo, hi);
        }
        // ...and the superset ITS layout tables emit. A glyph already claimed
        // by the text set stays there (`add_glyph` keeps the lowest set), so
        // this contributes exactly that face's ligatures and GSUB forms.
        self.add_shaped_ascii_of(style);
        Ok(())
    }

    /// The styles this builder will bake cells for, in cell order.
    pub fn styles(&self) -> Vec<GlyphStyle> {
        let mut styles: Vec<GlyphStyle> = self
            .queued_glyphs
            .iter()
            .map(|k| k.set().style())
            .collect();
        styles.sort_unstable();
        styles.dedup();
        styles
    }
}

/// One baked glyph cell: RGB MSDF pixels (`gs × gs × 3`, top-down rows) and
/// its glyph-table entry with `atlas_x`/`atlas_y` left at 0 — the caller
/// (atlas builder or dynamic-append manager) fills those in after packing.
#[cfg(all(feature = "cpu-bake", not(target_arch = "wasm32")))]
pub struct BakedGlyphCell {
    pub rgb: Vec<u8>,
    pub entry: GlyphEntry,
}

#[cfg(all(feature = "cpu-bake", not(target_arch = "wasm32")))]
impl FontAtlasBuilder {
    /// Bake a single glyph cell via CPU msdfgen (the reference
    /// implementation; the compute-shader path is `gpu::MsdfCompute`).
    pub fn bake_cell(&self, glyph_id: u16) -> Result<BakedGlyphCell, String> {
        self.bake_cell_of(&self.font_data, glyph_id, glyph_id)
    }

    /// [`FontAtlasBuilder::bake_cell`] from an explicit face, with the cell's
    /// ADDRESS given separately from the glyph id that face knows.
    ///
    /// The two differ for a styled cell and only there: the outline and every
    /// metric come from `raw_glyph_id` in `face_data`, and the entry is filed
    /// under `address` ([`GlyphStyle::styled_glyph_id`]) so that two faces'
    /// independent numberings cannot collide in one table.
    fn bake_cell_of(
        &self,
        face_data: &[u8],
        raw_glyph_id: u16,
        address: u16,
    ) -> Result<BakedGlyphCell, String> {
        use msdfgen::{Bitmap, FillRule, FontExt, Framing, MsdfGeneratorConfig, Rgb};

        let glyph_id = raw_glyph_id;
        // ttf-parser 0.25 for metrics; 0.18 for msdfgen's FontExt.
        let face25 = ttf_parser::Face::parse(face_data, 0)
            .map_err(|e| format!("font parse error: {e}"))?;
        let face18 = ttf_parser_018::Face::parse(face_data, 0)
            .map_err(|e| format!("font parse error (v18): {e}"))?;

        let gs = self.glyph_size;
        let channels = 3u32;
        let (baseline_def, x_margin_def, px_per_em) = default_cell_metrics(&face25, gs);

        let gid18 = ttf_parser_018::GlyphId(glyph_id);
        let projection = glyph_projection(&face25, glyph_id, gs);
        let has_outline = face18.glyph_shape(gid18).is_some() && projection.is_some();

        // Default to fully OUTSIDE the field (0 → median < 0.5 → renders
        // nothing). Outline glyphs overwrite every texel below; outline-less
        // glyphs (space) must stay empty, not render as a solid white cell.
        // (matter-stream seeded 255/all-inside here, which drew whitespace as
        // filled boxes once every shaped glyph — spaces included — is sampled.)
        let mut rgb = vec![0u8; (gs * gs * channels) as usize];
        let mut entry = GlyphEntry {
            glyph_id: address,
            atlas_x: 0,
            atlas_y: 0,
            atlas_w: gs as u16,
            atlas_h: gs as u16,
            // Filled in by `build` once the cell is placed - this method bakes
            // a cell, and which layer it lands in is the layout's business.
            layer: 0,
            advance_x: {
                let upem = face25.units_per_em() as f32;
                face25
                    .glyph_hor_advance(ttf_parser::GlyphId(glyph_id))
                    .unwrap_or(0) as f32
                    / upem
            },
            baseline_row: baseline_def,
            px_per_em,
            x_margin: x_margin_def,
        };

        if has_outline {
            let proj = projection.unwrap();
            // CRITICAL (unit of `Framing::range`): msdfgen evaluates distances
            // in SHAPE (font) space — `Projection::unproject` maps the sample
            // back before the distance finder runs — and `range` is in those
            // same shape units; the projection scale never enters. Its output
            // texel is `distance/range + 0.5`.
            //
            // So to make the field span `px_range` ATLAS TEXELS, the range has
            // to be *expressed* in shape units: `px_range / proj.scale`
            // (`proj.scale` is atlas px per font unit). Passing `px_range`
            // straight through — as this did before — asks for a range of 4
            // FONT UNITS, i.e. 4·scale ≈ 0.07 atlas px at 48px/2048upem: the
            // field saturates within a fraction of a texel and the atlas
            // degenerates into a 1-bit mask, which no amount of shader
            // antialiasing can recover (sub-pixel features simply drop).
            let range_shape = self.px_range / proj.scale;
            let g_framing = Framing {
                range: range_shape,
                projection: msdfgen::Projection::new(
                    msdfgen::Vector2::new(proj.scale, proj.scale),
                    msdfgen::Vector2::new(proj.tx, proj.ty),
                ),
            };

            let mut bitmap: Bitmap<Rgb<f32>> = Bitmap::new(gs, gs);
            if let Some(mut shape) = face18.glyph_shape(gid18) {
                shape.edge_coloring_simple(3.0, 0);
                shape.generate_msdf(&mut bitmap, &g_framing, MsdfGeneratorConfig::default());
                shape.correct_sign(&mut bitmap, &g_framing, FillRule::default());
            }

            for y in 0..gs {
                for x in 0..gs {
                    let pixel = bitmap.pixel(x, y);
                    // msdfgen bitmaps are y-up; atlas cells are top-down.
                    let idx = (((gs - 1 - y) * gs + x) * channels) as usize;
                    rgb[idx] = msdf_to_u8(pixel.r);
                    rgb[idx + 1] = msdf_to_u8(pixel.g);
                    rgb[idx + 2] = msdf_to_u8(pixel.b);
                }
            }

            entry.baseline_row = proj.baseline_row;
            entry.x_margin = proj.x_margin;
            entry.px_per_em = proj.px_per_em;
        }

        Ok(BakedGlyphCell { rgb, entry })
    }

    /// Build the MSDF atlas from all queued glyphs (CPU msdfgen bake).
    ///
    /// Cells are placed in [`FontAtlasBuilder::cell_order`] - the declared
    /// `(set, glyph id)` order - and the finished table is then sorted by glyph
    /// id, which is the key it is looked up by ([`FontAtlas::get_glyph`]). The
    /// two orders are deliberately different and neither is the queue's.
    pub fn build(&self) -> Result<FontAtlas, String> {
        let face25 = ttf_parser::Face::parse(&self.font_data, 0)
            .map_err(|e| format!("font parse error: {e}"))?;

        // **One parsed face per style**, so the per-cell work below can ask
        // which face a cell came from without re-parsing, and so a queue that
        // names a style with no registered face stops HERE with a message
        // rather than at an `expect` in the middle of a bake.
        let mut styled: Vec<(GlyphStyle, ttf_parser::Face)> = Vec::new();
        for (style, data) in &self.style_faces {
            let face = ttf_parser::Face::parse(data, 0)
                .map_err(|e| format!("{style:?} face parse error: {e}"))?;
            styled.push((*style, face));
        }
        for style in self.styles() {
            if self.style_face(style).is_none() {
                return Err(format!(
                    "{style:?} cells are queued but no {style:?} face is registered - \
                     add_styled_coverage does both, and add_glyph on a styled set does neither"
                ));
            }
        }
        let face_of = |style: GlyphStyle| -> &ttf_parser::Face {
            if style == GlyphStyle::Regular {
                &face25
            } else {
                styled
                    .iter()
                    .find(|(s, _)| *s == style)
                    .map(|(_, f)| f)
                    .expect("checked above: every queued style has a registered face")
            }
        };

        let gs = self.glyph_size;
        let padded = gs + 2; // 1px padding on each side
        let channels = 3u32;

        let atlas_w = ATLAS_COLS * padded;

        struct Placed {
            cell: BakedGlyphCell,
            layer: u16,
            atlas_x: u32,
            atlas_y: u32,
        }

        let order = self.cell_order();

        // **The layer table, from the queue.** One layer per
        // `(glyph_size, style)`, and a builder bakes at one size, so this is
        // the styles present - in `GlyphStyle` order, which puts
        // [`GlyphStyle::Regular`] at layer 0 whenever it is present at all.
        //
        // That is the regression bar in one line: an unstyled bake has exactly
        // one layer, its packer is the packer it always was, and its pixel
        // data is the same length at the same offsets. Adding a style adds a
        // LAYER instead of competing for the 320 cells of this one, which is
        // what unblocks a styled bake without moving a single existing `u`.
        let layers: Vec<AtlasLayer> = {
            let mut ls: Vec<AtlasLayer> = order
                .iter()
                .map(|k| AtlasLayer::new(self.glyph_size as u16, k.set().style()))
                .collect();
            ls.sort_unstable();
            ls.dedup();
            ls
        };
        if layers.len() > MAX_ATLAS_LAYERS {
            return Err(format!(
                "{} layers queued, but maxTextureArrayLayers guarantees only {MAX_ATLAS_LAYERS}",
                layers.len()
            ));
        }
        let layer_of = |key: &CellKey| -> u16 {
            let want = AtlasLayer::new(self.glyph_size as u16, key.set().style());
            layers
                .iter()
                .position(|&l| l == want)
                .expect("every queued cell's layer is in the table built from those cells")
                as u16
        };

        // One packer PER LAYER: a layer is its own texture rectangle with its
        // own grid, so cell 0 of every layer is at the same coordinates and a
        // layer's numbering cannot be perturbed by another layer's contents.
        let mut packers: Vec<ShelfPacker> =
            layers.iter().map(|_| ShelfPacker::new(atlas_w)).collect();
        let mut placed = Vec::with_capacity(self.queued_glyphs.len());
        for key in &order {
            let address = key.glyph_id();
            // The SET says which face; the address says which glyph OF it. For
            // every cell that exists today the two are the identity - regular
            // sets, prefix 0 - which is what makes this the same bake it was.
            let style = key.set().style();
            let raw = GlyphStyle::split_glyph_id(address).1;
            let face_data = self
                .style_face(style)
                .expect("checked above: every queued style has a registered face");
            let cell = self.bake_cell_of(face_data, raw, address)?;
            let layer = layer_of(key);
            let (x, y) = packers[layer as usize].pack(padded, padded);
            placed.push(Placed { cell, layer, atlas_x: x + 1, atlas_y: y + 1 });
        }

        // **The set header, measured off the cells that were just baked** -
        // not copied from a face-wide number computed alongside them.
        //
        // Today every set answers the same, because one face is baked and
        // [`glyph_projection`] derives the baseline from that face's cap
        // height. That is a fact about this bake, not a property of the
        // format: a set fed from a second face would land here with its own
        // baseline and the header would say so without any further change.
        // Reading it off the cells is what makes that true.
        let sets = {
            // **Per SET, and therefore per FACE.** `SetMetrics`' doc already
            // said this was the shape it wanted - "a set fed from a second face
            // would land here with its own baseline and the header would say so
            // without any further change" - and a styled bake is that second
            // face. Bold's ascender, underline and ink depth are its own, and
            // the header carries them per set with no format change at all.
            // **The deepest ink is measured from the OUTLINES, per set** -
            // `SetMetrics::ink_descent_em` has the numbers and the argument.
            // Taken from the same `glyf` bounding box `glyph_projection` lays
            // the cell out from, so it describes the ink that was actually
            // baked. A glyph with no outline (whitespace) contributes nothing:
            // 0.0 is not below the baseline, so `min` passes over it, and a
            // set that is ALL whitespace reports 0.0, which is true of it.
            let ink_y_min_em = |key: &CellKey| {
                let face = face_of(key.set().style());
                let upem = face.units_per_em() as f32;
                let raw = GlyphStyle::split_glyph_id(key.glyph_id()).1;
                face.glyph_bounding_box(ttf_parser::GlyphId(raw))
                    .map_or(0.0, |bb| bb.y_min as f32 / upem)
            };
            let mut sets: Vec<SetMetrics> = Vec::new();
            for (key, p) in order.iter().zip(placed.iter()) {
                let frac = p.cell.entry.baseline_row / gs as f32;
                match sets.iter_mut().find(|m| m.set == key.set()) {
                    Some(m) => {
                        debug_assert_eq!(
                            m.baseline_frac, frac,
                            "glyph {} disagrees with its own set's baseline - every cell of a set \
                             is projected from one face, so this is a bake bug, not a metric",
                            key.glyph_id()
                        );
                        m.glyph_count += 1;
                        m.ink_descent_em = m.ink_descent_em.min(ink_y_min_em(key));
                    }
                    None => sets.push(SetMetrics {
                        set: key.set(),
                        glyph_count: 1,
                        baseline_frac: frac,
                        px_per_em_frac: p.cell.entry.px_per_em / gs as f32,
                        ink_descent_em: ink_y_min_em(key),
                        ..self.face_em_metrics(face_of(key.set().style()))
                    }),
                }
            }
            sets
        };

        // **The pin is per LAYER now**, which is the whole point of the layer
        // axis: 320 cells stopped being the atlas's budget and became one
        // layer's.
        for (i, layer) in layers.iter().enumerate() {
            let n = placed.iter().filter(|p| p.layer as usize == i).count();
            if n > atlas_capacity() {
                return Err(format!(
                    "layer {i} ({layer}) has {n} cells, but a layer is pinned at {} \
                     ({ATLAS_ROWS} rows x {ATLAS_COLS} columns) - read ATLAS_COLS and then \
                     ATLAS_ROWS before raising either; columns are the axis with headroom and \
                     rows are the one without. A second STYLE is a second layer and does not \
                     spend this budget; a wider vocabulary at one size does",
                    atlas_capacity()
                ));
            }
        }
        // **Pinned, not fitted** — see [`ATLAS_ROWS`]. `used_height` is still the
        // floor for a degenerate bake with a huge cell, which cannot happen at
        // the shipped 48px but is not worth being wrong about. Taken over every
        // layer, because an array's layers share the array's dimensions.
        let atlas_h = packers
            .iter()
            .map(|p| p.used_height())
            .max()
            .unwrap_or(0)
            .max(ATLAS_ROWS * padded)
            .max(1);
        let layer_texels = (atlas_w as usize) * (atlas_h as usize) * (channels as usize);
        let mut pixel_data = vec![0u8; layer_texels * layers.len()];
        let mut glyphs = Vec::with_capacity(placed.len());

        for p in &mut placed {
            let layer_base = (p.layer as usize) * layer_texels;
            for row in 0..gs {
                for col in 0..gs {
                    let src_idx = ((row * gs + col) * channels) as usize;
                    let dst_idx = layer_base
                        + (((p.atlas_y + row) * atlas_w + p.atlas_x + col) * channels) as usize;
                    if src_idx + 2 < p.cell.rgb.len() && dst_idx + 2 < pixel_data.len() {
                        pixel_data[dst_idx] = p.cell.rgb[src_idx];
                        pixel_data[dst_idx + 1] = p.cell.rgb[src_idx + 1];
                        pixel_data[dst_idx + 2] = p.cell.rgb[src_idx + 2];
                    }
                }
            }
            let mut entry = p.cell.entry;
            entry.atlas_x = p.atlas_x as u16;
            entry.atlas_y = p.atlas_y as u16;
            entry.layer = p.layer;
            glyphs.push(entry);
        }
        // The table is the LOOKUP order; the cells above were the layout. Each
        // entry carries the coordinates it was placed at, so re-ordering here
        // moves nothing in the texture.
        glyphs.sort_unstable_by_key(|e| e.glyph_id);

        Ok(FontAtlas {
            width: atlas_w,
            height: atlas_h,
            channels,
            pixel_data,
            glyphs,
            sets,
            layers,
        })
    }

    /// The face-wide half of [`SetMetrics`] - everything that comes from the
    /// font's own tables rather than from the cells: `hhea` line metrics and
    /// `post` underline metrics, converted to em.
    ///
    /// Returned as a `SetMetrics` with the per-set fields left at zero, for
    /// the caller to fill with `..`; there is no second place that decides
    /// what a missing `post` table means.
    fn face_em_metrics(&self, face: &ttf_parser::Face) -> SetMetrics {
        let upem = face.units_per_em() as f32;
        // A face with no `post` table states no underline. Zero is the honest
        // reading of "the face does not say", and a consumer choosing its own
        // rule (which `libhbui` does at body sizes) is unaffected either way.
        let underline = face.underline_metrics();
        SetMetrics {
            set: GlyphSet::Placeholder,
            glyph_count: 0,
            baseline_frac: 0.0,
            px_per_em_frac: 0.0,
            ascent_em: face.ascender() as f32 / upem,
            descent_em: face.descender() as f32 / upem,
            line_gap_em: face.line_gap() as f32 / upem,
            underline_pos_em: underline.map_or(0.0, |m| m.position as f32 / upem),
            underline_thickness_em: underline.map_or(0.0, |m| m.thickness as f32 / upem),
            // Per-set, measured off the cells by `build`; zero here so that
            // `..em` cannot quietly supply a face-wide answer to a per-set
            // question.
            ink_descent_em: 0.0,
        }
    }
}

/// Quantize one msdfgen output channel to u8.
///
/// msdfgen already writes `distance / range + 0.5` (see `DistancePixelConversion`
/// in msdfgen's `core/msdfgen.cpp`), and [`FontAtlasBuilder::bake_cell`] passes a
/// `range` expressed in shape units such that it equals the atlas-texel
/// `px_range` — so the value arriving here is ALREADY the unit-normalized field
/// the shader expects: 0.5 on the outline, 1.0 at `px_range/2` texels inside,
/// 0.0 at `px_range/2` texels outside. All that is left is the clamp + scale.
/// msdfgen with TrueType: positive = inside → inside maps > 0.5 (light).
#[cfg(all(feature = "cpu-bake", not(target_arch = "wasm32")))]
pub(crate) fn msdf_to_u8(unit_value: f32) -> u8 {
    (unit_value.clamp(0.0, 1.0) * 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roboto() -> Vec<u8> {
        crate::font::ROBOTO_REGULAR_ASCII.to_vec()
    }

    /// **The cell IS the line box**, and two constants say so independently.
    ///
    /// [`CELL_EM_RATIO`] scales every baked projection; `LINE_BOX_RATIO` scales
    /// the box that projection is drawn into. They are the same fact, written
    /// twice because one has to be `f64` to bake with and the other is `f32`
    /// arithmetic in the hot path. If they ever disagree, one em of baked type
    /// stops being one em of drawn type - and every metric in [`SetMetrics`],
    /// which is stated in em, quietly means something else.
    #[test]
    fn a_cell_is_the_line_box() {
        assert_eq!(CELL_EM_RATIO as f32, crate::drawlist::LINE_BOX_RATIO);
    }

    /// **A v1 atlas is REFUSED, and the message says what to run.**
    ///
    /// v1 began at `width` with no magic, so these bytes are exactly what a
    /// pre-v2 artifact looks like: plausible dimensions, a sane glyph count,
    /// real pixel data. Read as v2 they would parse - `width` would be taken
    /// for the magic, and every field after it read from the wrong offset - so
    /// the atlas would LOAD and draw from garbage coordinates. That is the
    /// failure mode the magic exists to convert into a stop.
    #[test]
    fn a_v1_atlas_is_refused_rather_than_misparsed() {
        let mut v1 = Vec::new();
        v1.extend_from_slice(&400u32.to_le_bytes()); // width
        v1.extend_from_slice(&2000u32.to_le_bytes()); // height
        v1.extend_from_slice(&1u32.to_le_bytes()); // num_glyphs
        v1.extend_from_slice(&3u32.to_le_bytes()); // channels
        v1.extend_from_slice(&GlyphEntry {
            glyph_id: 0,
            atlas_x: 1,
            atlas_y: 1,
            atlas_w: 48,
            atlas_h: 48,
            layer: 0,
            advance_x: 0.5,
            baseline_row: 33.45,
            px_per_em: 36.923,
            x_margin: 7.2,
        }
        .to_bytes());
        v1.resize(v1.len() + 400 * 2000 * 3, 0);

        let err = FontAtlas::from_bytes(&v1).expect_err("a v1 atlas must not load");
        assert!(err.contains("hbfa"), "the message must name the magic: {err}");
        assert!(err.contains("bake_atlas"), "the message must name the fix: {err}");
    }

    /// A file whose version is neither of ours stops too, for the same reason.
    ///
    /// Written against [`ATLAS_VERSION_LAYERED`] rather than
    /// [`ATLAS_VERSION`] because "one past what this build writes" is now two
    /// numbers, and the one that must be refused is the one past the HIGHER.
    #[test]
    fn a_future_version_is_refused() {
        let mut buf = Vec::new();
        buf.extend_from_slice(ATLAS_MAGIC);
        buf.extend_from_slice(&(ATLAS_VERSION_LAYERED + 1).to_le_bytes());
        buf.resize(ATLAS_HEADER_SIZE, 0);
        let err = FontAtlas::from_bytes(&buf).expect_err("a v5 atlas must not load");
        assert!(err.contains("version"), "{err}");
    }

    /// **Both versions this build reads are read, and nothing between them is
    /// invented.** The pair is stated here so that adding a third is a change
    /// to a test rather than a silent widening.
    #[test]
    fn the_two_versions_this_build_reads() {
        assert_eq!((ATLAS_VERSION, ATLAS_VERSION_LAYERED), (3, 4));
        for bad in [0u32, 1, 2, 5, u32::MAX] {
            let mut buf = Vec::new();
            buf.extend_from_slice(ATLAS_MAGIC);
            buf.extend_from_slice(&bad.to_le_bytes());
            buf.resize(ATLAS_HEADER_SIZE, 0);
            assert!(
                FontAtlas::from_bytes(&buf).is_err(),
                "version {bad} must not load"
            );
        }
    }

    /// The set header survives a round trip, every field of it.
    #[test]
    fn the_set_header_round_trips() {
        let mut atlas = FontAtlas::empty(4, 4, 3);
        atlas.glyphs.push(GlyphEntry {
            glyph_id: 7,
            atlas_x: 0,
            atlas_y: 0,
            atlas_w: 4,
            atlas_h: 4,
            layer: 0,
            advance_x: 0.5,
            baseline_row: 2.5,
            px_per_em: 3.0,
            x_margin: 0.6,
        });
        atlas.sets = vec![SetMetrics {
            set: GlyphSet::Text,
            glyph_count: 1,
            baseline_frac: 0.696875,
            px_per_em_frac: 0.769231,
            ascent_em: 0.92773,
            descent_em: -0.24414,
            line_gap_em: 0.0,
            underline_pos_em: -0.07324,
            underline_thickness_em: 0.04883,
            ink_descent_em: -0.241699,
        }];

        let round = FontAtlas::from_bytes(&atlas.to_bytes()).expect("round trips");
        assert_eq!(round.sets, atlas.sets);
        assert_eq!(round.set_metrics(GlyphSet::Text), atlas.sets.first());
        assert_eq!(round.set_metrics(GlyphSet::Markers), None);
        assert_eq!(round.text_baseline_frac(), Some(0.696875));
        // Stored with the FACE's sign, read out with the SCREEN's. Both
        // directions are asserted here because the flip is the one place this
        // type's convention is not uniform.
        assert_eq!(round.sets[0].ink_descent_em, -0.241699);
        assert_eq!(round.text_max_ink_descent_em(), Some(0.241699));
    }

    /// A header whose counts do not add up to the entries it ships with was
    /// not written by `build`, and is refused rather than half-believed.
    #[test]
    fn set_counts_must_sum_to_the_glyph_count() {
        let mut atlas = FontAtlas::empty(4, 4, 3);
        atlas.sets = vec![SetMetrics {
            set: GlyphSet::Text,
            glyph_count: 9,
            baseline_frac: 0.7,
            px_per_em_frac: 0.77,
            ascent_em: 0.9,
            descent_em: -0.2,
            line_gap_em: 0.0,
            underline_pos_em: -0.07,
            underline_thickness_em: 0.05,
            ink_descent_em: -0.24,
        }];
        let err = FontAtlas::from_bytes(&atlas.to_bytes()).expect_err("9 != 0 entries");
        assert!(err.contains("sum"), "{err}");
    }

    /// **An atlas with no header says so**, instead of answering 0.75.
    ///
    /// The runtime-populated path has no face to measure. What matters is that
    /// a consumer can TELL - `None`, not a number indistinguishable from a
    /// measured one. Returning 0.75 here is precisely how a wrong baseline
    /// shipped.
    #[test]
    fn a_runtime_atlas_declares_no_baseline() {
        let atlas = FontAtlas::empty(64, 64, 3);
        assert_eq!(atlas.text_baseline_frac(), None);
        assert!(atlas.sets.is_empty());
    }

    #[cfg(all(feature = "cpu-bake", not(target_arch = "wasm32")))]
    #[test]
    fn build_ascii_atlas() {
        let mut builder = FontAtlasBuilder::new(roboto(), 32, 4.0);
        builder.add_ascii();
        let atlas = builder.build().expect("atlas build failed");

        assert!(atlas.width > 0);
        assert!(atlas.height > 0);
        assert_eq!(atlas.channels, 3);
        assert!(!atlas.glyphs.is_empty());
        assert!(!atlas.pixel_data.is_empty());

        // Verify some glyphs have non-zero MSDF data
        let has_content = atlas.pixel_data.iter().any(|&b| b != 0);
        assert!(has_content, "atlas should have non-zero pixel data");
    }

    #[cfg(all(feature = "cpu-bake", not(target_arch = "wasm32")))]
    #[test]
    fn atlas_roundtrip() {
        let mut builder = FontAtlasBuilder::new(roboto(), 32, 4.0);
        builder.add_codepoint_range(GlyphSet::Text, 'A', 'Z');
        let atlas = builder.build().expect("build failed");
        let bytes = atlas.to_bytes();
        let parsed = FontAtlas::from_bytes(&bytes).expect("parse failed");

        assert_eq!(atlas.width, parsed.width);
        assert_eq!(atlas.height, parsed.height);
        assert_eq!(atlas.glyphs.len(), parsed.glyphs.len());
        assert_eq!(atlas.pixel_data.len(), parsed.pixel_data.len());
        // Entries survive byte-for-byte.
        for (a, b) in atlas.glyphs.iter().zip(parsed.glyphs.iter()) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn projection_is_deterministic_and_baseline_sane() {
        let data = roboto();
        let face = ttf_parser::Face::parse(&data, 0).unwrap();
        let gid = face.glyph_index('A').unwrap().0;
        let p1 = glyph_projection(&face, gid, 48).expect("A has an outline");
        let p2 = glyph_projection(&face, gid, 48).expect("A has an outline");
        assert_eq!(p1.baseline_row.to_bits(), p2.baseline_row.to_bits());
        // Baseline must sit inside the cell, below the 15% cap-height line.
        assert!(p1.baseline_row > 48.0 * 0.15 && p1.baseline_row < 48.0);
        assert!(p1.advance_x > 0.0 && p1.advance_x < 2.0);
    }

    #[test]
    fn empty_atlas_insert_entry() {
        let mut atlas = FontAtlas::empty(64, 64, 3);
        let e = GlyphEntry {
            glyph_id: 7, atlas_x: 1, atlas_y: 1, atlas_w: 32, atlas_h: 32, layer: 0,
            advance_x: 0.5, baseline_row: 24.0, px_per_em: 24.6, x_margin: 4.8,
        };
        let idx = atlas.insert_entry(e);
        assert_eq!(idx, 0);
        assert_eq!(atlas.glyph_table_index(7), Some(0));
        // Replacement keeps the index.
        let idx2 = atlas.insert_entry(GlyphEntry { advance_x: 0.6, ..e });
        assert_eq!(idx2, 0);
        assert_eq!(atlas.get_glyph(7).unwrap().advance_x, 0.6);
    }

    /// The dynamic-append path keeps the table searchable however the caller
    /// happens to order its inserts — a glyph arriving out of order lands at
    /// its own place rather than at the end, so `get_glyph`'s binary search
    /// stays correct.
    #[test]
    fn insert_entry_keeps_the_table_searchable_out_of_order() {
        let mut atlas = FontAtlas::empty(64, 64, 3);
        let entry = |glyph_id: u16| GlyphEntry {
            glyph_id, atlas_x: 1, atlas_y: 1, atlas_w: 32, atlas_h: 32, layer: 0,
            advance_x: glyph_id as f32 / 100.0, baseline_row: 24.0,
            px_per_em: 24.6, x_margin: 4.8,
        };
        for gid in [90u16, 7, 300, 0, 41] {
            atlas.insert_entry(entry(gid));
        }
        let ids: Vec<u16> = atlas.glyphs.iter().map(|e| e.glyph_id).collect();
        assert_eq!(ids, vec![0, 7, 41, 90, 300], "the table is not glyph-id ordered");
        for gid in [0u16, 7, 41, 90, 300] {
            assert_eq!(atlas.get_glyph(gid).map(|e| e.glyph_id), Some(gid));
        }
        // ...and a glyph that was never inserted is still absent, rather than
        // matching the neighbour a bad search would land on.
        for gid in [1u16, 42, 89, 299, 301] {
            assert!(atlas.get_glyph(gid).is_none(), "glyph {gid} was never inserted");
        }
    }

    /// **A glyph belongs to the lowest set that claimed it, whatever order the
    /// queue calls came in** — so the layout does not depend on the sequence
    /// of `add_*` calls, which is the failure the sets exist to remove.
    #[test]
    fn the_queue_call_order_no_longer_decides_anything() {
        let gid = |ch: char| {
            ttf_parser::Face::parse(&crate::font::ROBOTO_REGULAR_ASCII, 0)
                .unwrap()
                .glyph_index(ch)
                .unwrap()
                .0
        };

        let mut forwards = FontAtlasBuilder::new(roboto(), 48, 6.0);
        forwards.add_codepoint_range(GlyphSet::Text, 'a', 'z');
        forwards.add_glyph(GlyphSet::ShapedText, gid('a'));
        forwards.add_codepoint_range(GlyphSet::OwnedIcons, '\u{F800}', '\u{F8EF}');
        forwards.add_glyph(GlyphSet::Placeholder, 0);

        let mut backwards = FontAtlasBuilder::new(roboto(), 48, 6.0);
        backwards.add_glyph(GlyphSet::Placeholder, 0);
        backwards.add_codepoint_range(GlyphSet::OwnedIcons, '\u{F800}', '\u{F8EF}');
        backwards.add_glyph(GlyphSet::ShapedText, gid('a'));
        backwards.add_codepoint_range(GlyphSet::Text, 'a', 'z');

        assert_eq!(forwards.cell_order(), backwards.cell_order());
        // Vacuity: the two really did queue in opposite orders, so the equality
        // above is the SORT agreeing and not the two builders being identical.
        assert_ne!(forwards.queued_glyphs, backwards.queued_glyphs);
        // And `a` is Text in both, not ShapedText: the lowest set wins.
        let order = forwards.cell_order();
        assert!(order.contains(&CellKey::new(GlyphSet::Text, gid('a'))));
        assert!(!order.iter().any(|k| k.glyph_id() == gid('a') && k.set() == GlyphSet::ShapedText));
    }

    /// **The payoff, on the queue: growth in a LATER set leaves an earlier
    /// set's cells exactly where they were.**
    ///
    /// Over the real shipped coverage, and stated as cell INDEX because that is
    /// what `build` turns into `atlas_x`/`atlas_y` — see
    /// `a_new_icon_leaves_every_earlier_cell_untouched` for the same property
    /// demonstrated on baked pixels.
    ///
    /// The added glyph is `number_of_glyphs`, i.e. the id the next glyph would
    /// get: `fonts/icon.py`, `marker.py` and `msymbols.py` all extend the face
    /// with `getGlyphOrder() + added`, so an appended glyph is exactly what a
    /// font edit produces.
    #[test]
    fn a_later_set_never_moves_an_earlier_sets_cells() {
        for face_bytes in [crate::font::ROBOTO_REGULAR_ASCII, crate::font::ROBOTO_ASCII_MSYMBOLS] {
            let next_gid = ttf_parser::Face::parse(face_bytes, 0).unwrap().number_of_glyphs();

            let mut before = FontAtlasBuilder::new(face_bytes.to_vec(), 48, 6.0);
            before.add_shipped_coverage();
            let before = before.cell_order();

            let mut after = FontAtlasBuilder::new(face_bytes.to_vec(), 48, 6.0);
            after.add_shipped_coverage();
            after.add_glyph(GlyphSet::OwnedIcons, next_gid);
            let after = after.cell_order();

            assert_eq!(after.len(), before.len() + 1, "the new glyph was swallowed");
            let new_cell = after
                .iter()
                .position(|k| k.set() == GlyphSet::OwnedIcons && k.glyph_id() == next_gid)
                .expect("the new icon is somewhere");

            // Every cell BEFORE the new one is the same glyph in the same cell.
            assert_eq!(&after[..new_cell], &before[..new_cell]);
            // Every set EARLIER than the new glyph's is entirely inside that
            // untouched prefix — the property, rather than an accident of how
            // many cells happened to precede it.
            for (cell, key) in before.iter().enumerate() {
                if key.set() < GlyphSet::OwnedIcons {
                    assert!(cell < new_cell, "an earlier set's cell moved");
                }
            }
            // Vacuity: cells DO move when the addition is not to the last set.
            // The merged face carries borrowed icons after ours, and they shift
            // by exactly one; the plain face has none, so there is nothing
            // after the new cell and nothing to shift.
            let later: Vec<_> = before[new_cell..].to_vec();
            assert_eq!(after[new_cell + 1..], later[..], "a later set shifted by more than one");
            // Everything after the new cell belongs to a LATER set: it sorted
            // into its own set rather than onto the end of the queue.
            for key in &after[new_cell + 1..] {
                let set = key.set();
                assert!(set > GlyphSet::OwnedIcons, "a cell of set {set:?} sorted after an icon");
            }
            if face_bytes == crate::font::ROBOTO_ASCII_MSYMBOLS {
                assert!(!later.is_empty(), "the merged face should have borrowed cells after ours");
                assert_ne!(after[new_cell..], before[new_cell..], "nothing moved at all");
                // The sharp one: the merged face has borrowed cells AFTER ours,
                // so a new icon must land in the middle of the atlas. Landing
                // last would mean cells still follow the queue rather than the
                // declared order.
                assert!(
                    new_cell + 1 < after.len(),
                    "the new icon landed at the END of the atlas rather than at the end \
                     of its SET - that is queue order, which is what the sets replaced"
                );
            }
        }
    }

    /// **The same payoff, demonstrated on BAKED PIXELS.** The queue test above
    /// compares cell indices; this bakes two atlases and compares the texture.
    ///
    /// Reads as the event it stands for: a face gains one icon, and everything
    /// baked before it — the text, the marker — is byte-for-byte where it was.
    #[cfg(all(feature = "cpu-bake", not(target_arch = "wasm32")))]
    #[test]
    fn a_new_icon_leaves_every_earlier_cell_untouched() {
        use std::collections::BTreeMap;

        let gid = |ch: char| {
            ttf_parser::Face::parse(&crate::font::ROBOTO_REGULAR_ASCII, 0)
                .unwrap()
                .glyph_index(ch)
                .map(|g| g.0)
                .unwrap_or_else(|| panic!("the shipped face draws U+{:04X}", ch as u32))
        };

        /// Every glyph's cell: where it sits, and the texels in it.
        fn cells(atlas: &FontAtlas) -> BTreeMap<u16, ((u16, u16), Vec<u8>)> {
            atlas
                .glyphs
                .iter()
                .map(|e| {
                    let mut texels = Vec::new();
                    for row in 0..e.atlas_h as u32 {
                        let start = (((e.atlas_y as u32 + row) * atlas.width + e.atlas_x as u32)
                            * atlas.channels) as usize;
                        let len = (e.atlas_w as u32 * atlas.channels) as usize;
                        texels.extend_from_slice(&atlas.pixel_data[start..start + len]);
                    }
                    (e.glyph_id, ((e.atlas_x, e.atlas_y), texels))
                })
                .collect()
        }

        let bake = |icons: &[char]| {
            let mut b = FontAtlasBuilder::new(roboto(), 32, 4.0);
            b.add_glyph(GlyphSet::Placeholder, 0);
            b.add_codepoint_range(GlyphSet::Text, 'A', 'E');
            b.add_glyph(GlyphSet::Markers, gid(crate::font::MARKER_ARROW));
            for &ch in icons {
                b.add_glyph(GlyphSet::OwnedIcons, gid(ch));
            }
            b.build().expect("bake")
        };

        // `graph` and `props` ship; then the face gains `table`.
        let before = bake(&['\u{F800}', '\u{F801}']);
        let after = bake(&['\u{F800}', '\u{F801}', '\u{F802}']);

        let (b, a) = (cells(&before), cells(&after));
        assert_eq!(b.len() + 1, a.len(), "the second bake should have one more cell");
        for (glyph_id, cell) in &b {
            assert_eq!(
                a.get(glyph_id),
                Some(cell),
                "glyph {glyph_id}'s cell moved or changed when an icon was added \
                 after it - the atlas is not append-only across sets any more"
            );
        }
        // Vacuity, both halves. The new icon really is a new cell...
        let new_cell = a[&gid('\u{F802}')].0;
        assert!(!b.values().any(|(xy, _)| *xy == new_cell), "the new icon reused a cell");
        // ...and this comparison can SEE a cell move: adding to an earlier set
        // (one more letter, in Text) shifts the marker and the icons.
        let mut widened = FontAtlasBuilder::new(roboto(), 32, 4.0);
        widened.add_glyph(GlyphSet::Placeholder, 0);
        widened.add_codepoint_range(GlyphSet::Text, 'A', 'F');
        widened.add_glyph(GlyphSet::Markers, gid(crate::font::MARKER_ARROW));
        for ch in ['\u{F800}', '\u{F801}'] {
            widened.add_glyph(GlyphSet::OwnedIcons, gid(ch));
        }
        let widened = cells(&widened.build().expect("bake"));
        let arrow = gid(crate::font::MARKER_ARROW);
        assert_ne!(
            widened[&arrow].0, b[&arrow].0,
            "adding a letter should push the marker's cell along - if it does not, \
             this test cannot see movement and the assertions above prove nothing"
        );
    }
}
