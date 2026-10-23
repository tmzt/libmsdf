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

#[cfg(feature = "shaper")]
use crate::CellKey;
use crate::glyph_table::GlyphEntry;
#[cfg(all(feature = "bake", not(target_arch = "wasm32")))]
use crate::packer::ShelfPacker;
use crate::{GlyphSet, GlyphStyle};

/// **The first four bytes of a baked atlas.** `hbfa` - highbay font atlas.
///
/// See [`FontAtlas::from_bytes`] for why a magic exists at all: v1 of this
/// format started at `width`, so without one an old file misparses into a
/// plausible-looking atlas instead of failing.
pub const ATLAS_MAGIC: &[u8; 4] = b"hbfa";

/// **The LEGACY version a single-layer atlas was written at** until 0.4.0.
/// Still read; no longer written - every file is now written at
/// [`ATLAS_FORMAT_VERSION`] (file-major 5).
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

/// **The LEGACY version a multi-layer atlas was written at** until 0.4.0.
/// Still read alongside [`ATLAS_VERSION`]; no longer written.
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
///   face ([`crate::ROBOTO_ASCII_MSYMBOLS`]) has always bought and what
///   this keeps. So the merged vocabulary inside a layer is *text plus Private
///   Use Area, at that size*.
/// * **Style is what must DIFFER between layers.** A second face is 215 more
///   cells and a layer holds 320, so two styles cannot share one; and they
///   need not, because a run does not usually alternate weight per glyph.
///
/// The Private Use Area is deliberately NOT duplicated per style: an icon has
/// no weight, the outlines would be identical, and 18 borrowed cells per style
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
        Ok(Self {
            size_px: u16::from_le_bytes([data[0], data[1]]),
            style,
        })
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

/// Fixed part of a LEGACY (v3 and v4) header, before the [`SetMetrics`]
/// table: magic, version, width, height, glyph count, channels, set count.
/// A file-major 5 header is [`ATLAS_HEADER_SIZE_V5`].
pub const ATLAS_HEADER_SIZE: usize = 28;

// ── The split version field (file-major 5) ──────────────────────────────

/// **File-major of the generation this build writes.** Bytes 4..8 of a file
/// of this generation are `[file_major, major, minor, patch]`; a legacy v3 or
/// v4 file's little-endian u32 reads as file-major 3 or 4 with zeros after
/// it, which is how one reader tells them apart (plan section 4.1).
pub const ATLAS_FILE_MAJOR: u8 = 5;

/// The newest `(major, minor, patch)` of [`ATLAS_FILE_MAJOR`] this build
/// reads, and the one it writes.
pub const ATLAS_FORMAT_VERSION: AtlasFormatVersion = AtlasFormatVersion {
    file_major: ATLAS_FILE_MAJOR,
    major: 0,
    minor: 2,
    patch: 0,
};

/// **The version an atlas without a `CELL` table is written at**: 5:0.1.0,
/// so that every atlas whose cells are the line box stays byte-identical to
/// what libmsdf 0.5 wrote, and stays readable by it. Only an atlas with a
/// [`CellBox`] other than [`CellBox::LINE_BOX`] is written at
/// [`ATLAS_FORMAT_VERSION`], which a 0.1 reader refuses rather than drawing
/// its cells onto a 1.3 em box (per-atlas cell box plan, section 4.2).
pub const ATLAS_FORMAT_VERSION_LINE_BOX: AtlasFormatVersion = AtlasFormatVersion {
    file_major: ATLAS_FILE_MAJOR,
    major: 0,
    minor: 1,
    patch: 0,
};

/// The tag of the table that records an atlas's [`CellBox`].
pub const CELL_TABLE_TAG: [u8; 4] = *b"CELL";

/// Fixed part of a file-major 5 header: the legacy 28 bytes plus the layer
/// count and the table count.
pub const ATLAS_HEADER_SIZE_V5: usize = 36;

/// The most tables a file-major 5 atlas may carry.
pub const MAX_ATLAS_TABLES: usize = 64;

/// The most table bytes (bodies, unpadded) a file-major 5 atlas may carry.
pub const MAX_ATLAS_TABLE_BYTES: usize = 16 << 20;

/// The `face` byte of a table that belongs to the whole atlas rather than to
/// one style's face.
pub const ATLAS_TABLE_FACE_ALL: u8 = 0xFF;

/// Bytes of one table directory record.
const TABLE_RECORD_SIZE: usize = 16;

/// All-capital tags that are registered OpenType tables. A table of ours is
/// never named one of these, so a raw OpenType table cannot pass as ours.
const OPENTYPE_UPPER_TAGS: &[&[u8; 4]] = &[
    b"BASE", b"CBDT", b"CBLC", b"CFF ", b"CFF2", b"COLR", b"CPAL", b"DSIG", b"EBDT", b"EBLC",
    b"EBSC", b"GDEF", b"GPOS", b"GSUB", b"HVAR", b"JSTF", b"MATH", b"MERG", b"MVAR", b"STAT",
    b"SVG ", b"VARC", b"VORG", b"VVAR",
];

/// **A file-major 5 format version**: `file_major` then semver
/// `major.minor.patch`, read the way Cargo reads semver (plan section 4.1 and
/// its 13:02 and 13:03 EDT amendments).
///
/// * From 1.0: a newer major is refused, a newer minor is accepted (it may
///   only add tables an older reader skips), patch never changes the format.
/// * Before 1.0 the meanings move one place left: a newer minor is refused,
///   a newer patch is accepted with any unknown tables skipped.
/// * 0.0.x is never written and is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AtlasFormatVersion {
    /// The format generation (5 for this one).
    pub file_major: u8,
    /// Semver major, starting at 0.
    pub major: u8,
    /// Semver minor.
    pub minor: u8,
    /// Semver patch.
    pub patch: u8,
}

impl AtlasFormatVersion {
    /// Whether a reader whose newest known version is `self` reads a file at
    /// `file`, of the same file-major.
    pub(crate) fn reads(self, file: AtlasFormatVersion) -> Result<(), &'static str> {
        if file.file_major != self.file_major {
            return Err("atlas file-major is not one this build reads");
        }
        if file.major == 0 {
            if (file.major, file.minor) < (0, 1) {
                return Err("atlas format version 0.0.x was never written by libmsdf");
            }
            if (file.major, file.minor) > (self.major, self.minor) {
                return Err(
                    "atlas format version is newer than this build reads (a newer minor at \
                     major 0); update libmsdf",
                );
            }
            return Ok(());
        }
        if file.major > self.major {
            return Err(
                "atlas format version has a newer major than this build reads; update libmsdf",
            );
        }
        Ok(())
    }
}

impl core::fmt::Display for AtlasFormatVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{}:{}.{}.{}",
            self.file_major, self.major, self.minor, self.patch
        )
    }
}

/// **The version gate**: whether a field added after `since` is present in a
/// file at `v` (both `(major, minor)` within one file-major). Every field
/// added after 0.1 is read behind one of these.
#[allow(dead_code)]
pub(crate) const fn gate(v: (u8, u8), since: (u8, u8)) -> bool {
    v.0 > since.0 || (v.0 == since.0 && v.1 > since.1)
}

/// **What the version field of a file says**, before anything else is
/// parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtlasFileVersion {
    /// A pre-split file: [`ATLAS_VERSION`] (3) or [`ATLAS_VERSION_LAYERED`]
    /// (4).
    Legacy(u32),
    /// File-major 5 and later.
    Split(AtlasFormatVersion),
}

impl AtlasFileVersion {
    /// Reads bytes 0..8 (magic and version) and decides whether this build
    /// reads the file. Refuses a bad magic, a legacy version other than 3 or
    /// 4 or with non-zero high bytes, an unknown file-major, a 0.0.x version
    /// and a version newer than [`ATLAS_FORMAT_VERSION`].
    pub fn of(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 8 {
            return Err("atlas data too short");
        }
        if &data[0..4] != ATLAS_MAGIC {
            return Err(
                "not an 'hbfa' atlas - this is a headerless pre-v2 file, and reading it as v2 \
                 would silently misparse rather than fail. Re-bake it: cargo run -p libmsdf \
                 --features cpu-bake --example bake_atlas -- <font.ttf> <out.atlas> 48 6.0",
            );
        }
        let v = [data[4], data[5], data[6], data[7]];
        match v[0] {
            3 | 4 => {
                if v[1..] != [0, 0, 0] {
                    return Err(
                        "legacy atlas version with non-zero high bytes; not written by any libmsdf",
                    );
                }
                Ok(Self::Legacy(v[0] as u32))
            }
            1 | 2 => Err(
                "atlas version is not one this build reads - re-bake it with this libmsdf's \
                 bake_atlas example",
            ),
            ATLAS_FILE_MAJOR => {
                let file = AtlasFormatVersion {
                    file_major: v[0],
                    major: v[1],
                    minor: v[2],
                    patch: v[3],
                };
                ATLAS_FORMAT_VERSION.reads(file)?;
                Ok(Self::Split(file))
            }
            _ => Err(
                "atlas file-major is not one this build reads; it was written by a newer \
                 libmsdf, or is not an atlas",
            ),
        }
    }
}

/// One record of a file-major 5 table directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TableRecord {
    tag: [u8; 4],
    face: u8,
    offset: u32,
    length: u32,
}

/// A table body to write: tag, face byte and unpadded bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TableBody {
    pub(crate) tag: [u8; 4],
    pub(crate) face: u8,
    pub(crate) body: Vec<u8>,
}

/// **Whether `tag` may name a table** (plan section 4.2): first-capital
/// (`[A-Z][a-z0-9]*` padded with spaces) or all-capital (`[A-Z0-9]*` padded
/// with spaces), at least one character, and not a registered OpenType
/// all-capital tag.
fn check_tag(tag: &[u8; 4]) -> Result<(), &'static str> {
    let len = tag.iter().position(|&b| b == b' ').unwrap_or(4);
    if len == 0 || tag[len..].iter().any(|&b| b != b' ') {
        return Err("atlas table tag is malformed (empty, or a space before a character)");
    }
    let body = &tag[..len];
    let first_capital = body[0].is_ascii_uppercase()
        && body[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    let all_capital = body
        .iter()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
    if !first_capital && !all_capital {
        return Err(
            "atlas table tag is neither first-capital nor all-capital (OpenType's lowercase \
             tables such as cmap, fpgm, prep and cvt are never carried)",
        );
    }
    if OPENTYPE_UPPER_TAGS.contains(&tag) {
        return Err("atlas table tag is a registered OpenType table tag, which is never carried");
    }
    Ok(())
}

const fn align4(n: usize) -> usize {
    (n + 3) & !3
}

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
        let Some(set) = u8::try_from(ordinal)
            .ok()
            .and_then(GlyphSet::try_from_ordinal)
        else {
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
///   not. A codepoint outside [`crate::TEXT_RANGES`], or a bake that
///   queued less than the face covers.
/// * [`StyledGlyphError::NotARawGlyphId`] - the caller passed an id that is
///   already an address ([`GlyphStyle::styled_glyph_id`]), or a face wider
///   than [`crate::MAX_RAW_GLYPH_ID`]. A programming error rather than
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
                write!(
                    f,
                    "{style:?} is baked here, but glyph {gid} of it has no cell"
                )
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
    /// [`crate::GlyphSet`] for why it is not the order the CELLS are in.
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
    /// Layout tables, if the atlas was baked with them
    /// ([`crate::AtlasShaper`] reads them).
    pub layout: Option<crate::AtlasLayout>,
    /// One record per licence of a baked face (empty for a runtime atlas and
    /// for a legacy file).
    pub licences: Vec<crate::LicenceRecord>,
    /// **The version this atlas was read at** - [`ATLAS_FORMAT_VERSION`] for
    /// one built in memory. [`FontAtlas::to_bytes`] always writes
    /// [`ATLAS_FORMAT_VERSION`] whatever this says.
    pub read_version: AtlasFileVersion,
    /// The declared coverage groups of each face baked with them (`CVRG`;
    /// empty for `Coverage::Shipped` bakes, runtime atlases and legacy
    /// files). Read it through [`FontAtlas::coverage`].
    pub coverage: Vec<crate::FaceCoverage>,
    /// Where the cells sit relative to the line box (`CELL`;
    /// [`CellBox::LINE_BOX`] for an atlas without the table). Read it
    /// through [`FontAtlas::cell_box`].
    pub cell_box: CellBox,
}

impl FontAtlas {
    /// **Where this atlas's cells sit relative to the line box**: the box it
    /// was baked with, or [`CellBox::LINE_BOX`] for an atlas that carries no
    /// `CELL` table (every atlas before format 5:0.2.0). Drawing reads it to
    /// size a run's box from the cell rather than the line box.
    pub fn cell_box(&self) -> CellBox {
        self.cell_box
    }

    /// The version this atlas was read at ([`ATLAS_FORMAT_VERSION`] for one
    /// built in memory).
    pub fn file_version(&self) -> AtlasFileVersion {
        self.read_version
    }

    /// Every licence record of this atlas.
    pub fn licences(&self) -> &[crate::LicenceRecord] {
        &self.licences
    }

    /// The declared coverage groups of one style's face, if it has a `CVRG`.
    pub fn coverage(&self, style: GlyphStyle) -> Option<&crate::FaceCoverage> {
        self.coverage.iter().find(|c| c.style == style)
    }

    /// **This atlas on a larger texture**: every layer padded on the right
    /// and bottom with empty texels to `width` x `height`. Every glyph keeps
    /// its texel coordinates, so it draws exactly as before; this is what a
    /// consumer stacking atlases of different widths into one texture array
    /// needs (bundled fonts plan, 13:34 EDT amendment). Refuses dimensions
    /// smaller than the atlas.
    pub fn padded_to(&self, width: u32, height: u32) -> Result<FontAtlas, &'static str> {
        if width < self.width || height < self.height {
            return Err("an atlas can only be padded to dimensions at least its own");
        }
        let ch = self.channels as usize;
        let (ow, oh) = (self.width as usize, self.height as usize);
        let (nw, nh) = (width as usize, height as usize);
        let layers = self.layer_count() as usize;
        let mut pixel_data = vec![0u8; nw * nh * ch * layers];
        for l in 0..layers {
            for row in 0..oh {
                let src = (l * oh + row) * ow * ch;
                let dst = (l * nh + row) * nw * ch;
                if let Some(s) = self.pixel_data.get(src..src + ow * ch) {
                    pixel_data[dst..dst + ow * ch].copy_from_slice(s);
                }
            }
        }
        Ok(FontAtlas {
            width,
            height,
            pixel_data,
            ..self.clone()
        })
    }

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
        self.set_metrics(GlyphSet::Text)
            .map(SetMetrics::max_ink_descent_em)
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
    /// `(set, glyph id)`, declared by [`crate::GlyphSet`], and it lives
    /// in each entry's `atlas_x`/`atlas_y`. Cell order is what has to stay put
    /// across a re-bake, because it decides the artifact's bytes and the
    /// texture coordinate every frame samples. Table order is derived on load
    /// and matters to nothing outside one loaded atlas: the GPU table
    /// ([`FontAtlas::glyph_table_u32s`]) and the index packed into a draw list
    /// both come from the same instance.
    pub fn get_glyph(&self, glyph_id: u16) -> Option<&GlyphEntry> {
        self.glyph_table_index(glyph_id)
            .map(|idx| &self.glyphs[idx])
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
    /// [`crate::StyledShaper`] hands back before prefixing, or
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

    /// Serialize the atlas to bytes for embedding / baking to disk, at
    /// [`ATLAS_FORMAT_VERSION`] (file-major 5), whatever version it was read
    /// at:
    ///
    /// ```text
    /// offset  size
    /// 0       4    magic "hbfa"
    /// 4       4    version [5, 0, 1, 0], or [5, 0, 2, 0] with a CELL table
    /// 8       4    width          12  4  height
    /// 16      4    num_glyphs     20  4  channels
    /// 24      4    set_count      28  4  layer_count (1..=256)
    /// 32      4    table_count (0..=64)
    /// 36           [AtlasLayer   x layer_count]   4 bytes each
    ///              [SetMetrics   x set_count]    36 bytes each
    ///              [GlyphEntry   x num_glyphs]   32 bytes each
    ///              [TableRecord  x table_count]  16 bytes each
    ///              zero padding to a multiple of 4
    ///              table bodies, in directory order, each 4-aligned and
    ///              zero-padded
    ///              pixel data: width * height * channels * layer_count bytes
    /// ```
    ///
    /// A TableRecord is `tag [u8; 4]`, `face u8` (a [`GlyphStyle`] ordinal, or
    /// [`ATLAS_TABLE_FACE_ALL`]), `flags u8 = 0`, `reserved u16 = 0`,
    /// `offset u32`, `length u32` (unpadded). Records are sorted by
    /// `(face, tag)`.
    ///
    /// **Every file is written at one version.** Until 0.4.0 a single-layer
    /// atlas was written at v3 and a layered one at v4; both are still read
    /// ([`FontAtlas::from_bytes`]), and neither is written any more.
    ///
    /// # Panics
    ///
    /// When [`FontAtlas::layout`] or [`FontAtlas::licences`] break a table
    /// rule ([`FontAtlas::try_to_bytes`] says which). An atlas from
    /// [`FontAtlasBuilder::build`] or [`FontAtlas::from_bytes`] never does.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.try_to_bytes()
            .unwrap_or_else(|e| panic!("this atlas cannot be serialized: {e}"))
    }

    /// [`FontAtlas::to_bytes`], refusing layout or licence data that breaks
    /// a table rule instead of panicking.
    pub fn try_to_bytes(&self) -> Result<Vec<u8>, &'static str> {
        Ok(self.write_v5(&self.table_bodies()?))
    }

    /// The tables this atlas carries, sorted by `(face, tag)`.
    pub(crate) fn table_bodies(&self) -> Result<Vec<TableBody>, &'static str> {
        let mut tables: Vec<TableBody> = Vec::new();
        tables.extend(crate::layout::encode_tables(self.layout.as_ref())?);
        tables.extend(crate::licence::encode_tables(&self.licences)?);
        tables.extend(crate::coverage::encode_tables(&self.coverage)?);
        if !self.cell_box.is_line_box() {
            tables.push(TableBody {
                tag: CELL_TABLE_TAG,
                face: ATLAS_TABLE_FACE_ALL,
                body: self.cell_box.to_body(),
            });
        }
        tables.sort_by(|a, b| (a.face, a.tag).cmp(&(b.face, b.tag)));
        if tables.len() > MAX_ATLAS_TABLES {
            return Err("atlas would carry more than 64 tables");
        }
        Ok(tables)
    }

    /// The file-major 5 writer, over an explicit list of table bodies.
    pub(crate) fn write_v5(&self, tables: &[TableBody]) -> Vec<u8> {
        let layers: Vec<AtlasLayer> = if self.layers.is_empty() {
            vec![AtlasLayer::new(0, GlyphStyle::Regular)]
        } else {
            self.layers.clone()
        };
        let v = if self.cell_box.is_line_box() {
            ATLAS_FORMAT_VERSION_LINE_BOX
        } else {
            ATLAS_FORMAT_VERSION
        };
        let mut buf = Vec::new();
        buf.extend_from_slice(ATLAS_MAGIC);
        buf.extend_from_slice(&[v.file_major, v.major, v.minor, v.patch]);
        buf.extend_from_slice(&self.width.to_le_bytes());
        buf.extend_from_slice(&self.height.to_le_bytes());
        buf.extend_from_slice(&(self.glyphs.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.channels.to_le_bytes());
        buf.extend_from_slice(&(self.sets.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(layers.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(tables.len() as u32).to_le_bytes());
        for layer in &layers {
            buf.extend_from_slice(&layer.to_bytes());
        }
        for set in &self.sets {
            buf.extend_from_slice(&set.to_bytes());
        }
        for entry in &self.glyphs {
            buf.extend_from_slice(&entry.to_bytes());
        }
        let dir_end = buf.len() + tables.len() * TABLE_RECORD_SIZE;
        let mut offset = align4(dir_end);
        for t in tables {
            buf.extend_from_slice(&t.tag);
            buf.push(t.face);
            buf.push(0);
            buf.extend_from_slice(&0u16.to_le_bytes());
            buf.extend_from_slice(&(offset as u32).to_le_bytes());
            buf.extend_from_slice(&(t.body.len() as u32).to_le_bytes());
            offset = align4(offset + t.body.len());
        }
        buf.resize(align4(buf.len()), 0);
        for t in tables {
            buf.extend_from_slice(&t.body);
            buf.resize(align4(buf.len()), 0);
        }
        buf.extend_from_slice(&self.pixel_data);
        buf
    }

    /// Deserialize from bytes: a legacy v3 or v4 file, or file-major 5 up to
    /// [`ATLAS_FORMAT_VERSION`] ([`AtlasFileVersion::of`] says which, and
    /// refuses everything else).
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
    ///
    /// # Tables
    ///
    /// A file-major 5 directory is checked before any body is read: sorted
    /// unique `(face, tag)` records, bodies back to back at 4-byte alignment,
    /// zero padding, at most [`MAX_ATLAS_TABLES`] tables and
    /// [`MAX_ATLAS_TABLE_BYTES`] bytes, well-formed tags that are not OpenType
    /// tables. A table whose tag this build does not know is skipped; a known
    /// one that fails to parse refuses the file.
    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        let version = AtlasFileVersion::of(data)?;
        let header_size = match version {
            AtlasFileVersion::Legacy(_) => ATLAS_HEADER_SIZE,
            AtlasFileVersion::Split(_) => ATLAS_HEADER_SIZE_V5,
        };
        if data.len() < header_size {
            return Err("atlas data too short");
        }
        let u32_at = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        let width = u32_at(8);
        let height = u32_at(12);
        let num_glyphs = u32_at(16) as usize;
        let channels = u32_at(20);
        let set_count = u32_at(24) as usize;

        // **The layer table**: absent at v3 (one layer, measured off the
        // cells below), after a count at v4, after the v5 header at v5.
        let (layer_count, layers_start, table_count) = match version {
            AtlasFileVersion::Legacy(ATLAS_VERSION) => (1, ATLAS_HEADER_SIZE, 0),
            AtlasFileVersion::Legacy(_) => {
                if data.len() < ATLAS_HEADER_SIZE + 4 {
                    return Err("atlas data too short for the layer count");
                }
                (u32_at(ATLAS_HEADER_SIZE) as usize, ATLAS_HEADER_SIZE + 4, 0)
            }
            AtlasFileVersion::Split(_) => (
                u32_at(28) as usize,
                ATLAS_HEADER_SIZE_V5,
                u32_at(32) as usize,
            ),
        };
        let has_layer_table = version != AtlasFileVersion::Legacy(ATLAS_VERSION);
        if has_layer_table {
            if layer_count == 0 {
                return Err("atlas declares zero layers - every atlas has at least one");
            }
            if layer_count > MAX_ATLAS_LAYERS {
                return Err(
                    "atlas declares more layers than maxTextureArrayLayers guarantees (256)",
                );
            }
        }
        if table_count > MAX_ATLAS_TABLES {
            return Err("atlas declares more than 64 tables");
        }
        let layers_end = if has_layer_table {
            layers_start + layer_count * AtlasLayer::PACKED_SIZE
        } else {
            layers_start
        };
        if data.len() < layers_end {
            return Err("atlas data too short for the layer table");
        }
        let mut layers = Vec::with_capacity(layer_count);
        if has_layer_table {
            for i in 0..layer_count {
                let offset = layers_start + i * AtlasLayer::PACKED_SIZE;
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
            let style = glyphs.first().map_or(GlyphStyle::Regular, |e| {
                GlyphStyle::split_glyph_id(e.glyph_id).0
            });
            layers.push(AtlasLayer::new(size_px, style));
        }

        // **The table directory and bodies** (file-major 5 only).
        let (pixel_start, records) = if table_count > 0 || header_size == ATLAS_HEADER_SIZE_V5 {
            Self::read_directory(data, entries_end, table_count)?
        } else {
            (entries_end, Vec::new())
        };
        let (layout, licences, coverage) = Self::read_tables(data, &records)?;
        let cell_box = Self::read_cell_box(data, &records, version, &sets)?;

        let expected_pixels =
            (width as usize) * (height as usize) * (channels as usize) * layer_count;
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
            layout,
            licences,
            read_version: version,
            coverage,
            cell_box,
        })
    }

    /// The `CELL` table's box, or [`CellBox::LINE_BOX`] without one. Refused:
    /// the table in a file older than 5:0.2.0 (which no libmsdf writes), a
    /// malformed body, and a set whose `px_per_em_frac` is not `1 /
    /// cell_em`, which would draw its cells at the wrong size.
    fn read_cell_box(
        data: &[u8],
        records: &[TableRecord],
        version: AtlasFileVersion,
        sets: &[SetMetrics],
    ) -> Result<CellBox, &'static str> {
        let Some(rec) = records
            .iter()
            .find(|r| r.tag == CELL_TABLE_TAG && r.face == ATLAS_TABLE_FACE_ALL)
        else {
            return Ok(CellBox::LINE_BOX);
        };
        match version {
            AtlasFileVersion::Split(v) if (v.major, v.minor) >= (0, 2) => {}
            _ => return Err("atlas carries a CELL table at a version before 5:0.2.0"),
        }
        let start = rec.offset as usize;
        let cell = CellBox::from_body(&data[start..start + rec.length as usize])?;
        let frac = (1.0 / cell.cell_em()) as f32;
        if sets.iter().any(|m| (m.px_per_em_frac - frac).abs() > 1e-5) {
            return Err("atlas set's px_per_em_frac disagrees with its CELL table");
        }
        Ok(cell)
    }

    /// Read and check a file-major 5 table directory that starts at
    /// `dir_start`, returning where the pixel data starts and the records.
    fn read_directory(
        data: &[u8],
        dir_start: usize,
        table_count: usize,
    ) -> Result<(usize, Vec<TableRecord>), &'static str> {
        let dir_end = dir_start + table_count * TABLE_RECORD_SIZE;
        if data.len() < dir_end {
            return Err("atlas data too short for the table directory");
        }
        let mut records: Vec<TableRecord> = Vec::with_capacity(table_count);
        for i in 0..table_count {
            let r = &data[dir_start + i * TABLE_RECORD_SIZE..][..TABLE_RECORD_SIZE];
            let tag: [u8; 4] = r[0..4].try_into().unwrap();
            check_tag(&tag)?;
            let face = r[4];
            if face != ATLAS_TABLE_FACE_ALL && GlyphStyle::try_from_ordinal(face).is_none() {
                return Err("atlas table names a face that is neither a style nor the atlas");
            }
            if r[5] != 0 {
                return Err("atlas table record has non-zero flags (reserved)");
            }
            if r[6] != 0 || r[7] != 0 {
                return Err("atlas table record has non-zero reserved bytes");
            }
            let rec = TableRecord {
                tag,
                face,
                offset: u32::from_le_bytes(r[8..12].try_into().unwrap()),
                length: u32::from_le_bytes(r[12..16].try_into().unwrap()),
            };
            if let Some(prev) = records.last()
                && (prev.face, prev.tag) >= (rec.face, rec.tag)
            {
                return Err("atlas table records are not sorted by (face, tag), or one repeats");
            }
            records.push(rec);
        }
        let total: usize = records.iter().map(|r| r.length as usize).sum();
        if total > MAX_ATLAS_TABLE_BYTES {
            return Err("atlas tables exceed 16 MiB");
        }
        // Bodies back to back, each at the 4-aligned end of the one before;
        // every byte between them is padding and must be zero.
        let mut cursor = dir_end;
        for rec in &records {
            let expected = align4(cursor);
            if rec.offset as usize != expected {
                return Err(
                    "atlas table body is not where the directory rule puts it (misaligned, \
                     overlapping or out of order)",
                );
            }
            if data.len() < expected || data[cursor..expected].iter().any(|&b| b != 0) {
                return Err("atlas table padding is not zero");
            }
            cursor = expected + rec.length as usize;
            if data.len() < cursor {
                return Err("atlas data too short for a table body");
            }
        }
        let pixel_start = align4(cursor);
        if data.len() < pixel_start || data[cursor..pixel_start].iter().any(|&b| b != 0) {
            return Err("atlas table padding is not zero");
        }
        Ok((pixel_start, records))
    }

    /// Parse the tables this build knows; skip the rest (their bounds were
    /// checked by [`FontAtlas::read_directory`]).
    #[allow(clippy::type_complexity)]
    fn read_tables(
        data: &[u8],
        records: &[TableRecord],
    ) -> Result<
        (
            Option<crate::AtlasLayout>,
            Vec<crate::LicenceRecord>,
            Vec<crate::FaceCoverage>,
        ),
        &'static str,
    > {
        let bodies: Vec<(&[u8; 4], u8, &[u8])> = records
            .iter()
            .map(|r| {
                let start = r.offset as usize;
                (&r.tag, r.face, &data[start..start + r.length as usize])
            })
            .collect();
        let layout = crate::layout::decode_tables(&bodies)?;
        let licences = crate::licence::decode_tables(&bodies)?;
        let coverage = crate::coverage::decode_tables(&bodies, layout.as_ref())?;
        Ok((layout, licences, coverage))
    }

    /// Build an atlas shell (no pixel data yet) for dynamic population via
    /// the atlas manager + compute-MSDF path. ONE layer - see
    /// [`FontAtlas::empty_layered`] for more.
    pub fn empty(width: u32, height: u32, channels: u32) -> Self {
        Self::empty_layered(
            width,
            height,
            channels,
            &[AtlasLayer::new(0, GlyphStyle::Regular)],
        )
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
    pub fn empty_layered(width: u32, height: u32, channels: u32, layers: &[AtlasLayer]) -> Self {
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
                (width as usize)
                    * (height as usize)
                    * (channels as usize)
                    * layers.len()
            ],
            glyphs: Vec::new(),
            layers,
            // No face was measured, so there is nothing to say. A consumer
            // that needs a baseline here gets `FALLBACK_BASELINE_FRAC` and
            // knows it is a fallback, rather than a plausible-looking number
            // it cannot tell apart from a measured one.
            sets: Vec::new(),
            layout: None,
            licences: Vec::new(),
            read_version: AtlasFileVersion::Split(ATLAS_FORMAT_VERSION),
            coverage: Vec::new(),
            cell_box: CellBox::LINE_BOX,
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
        cell / (crate::LINE_BOX_RATIO * px_range)
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
    /// move: this path allocates cells through [`crate::AtlasManager`],
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
#[cfg(feature = "shaper")]
pub fn glyph_projection(
    face: &ttf_parser::Face,
    glyph_id: u16,
    glyph_size: u32,
) -> Option<GlyphProjection> {
    glyph_projection_with(face, glyph_id, glyph_size, CapPlacement::DEFAULT)
}

/// **Where a bake puts a face's cap height in the cell**
/// (`docs/design/20261023_01_TERMINAL_FONTS_PLAN.md`, section 4.2).
///
/// [`CapPlacement::DEFAULT`] is what every bake did before the placement
/// was an option, and gives the same bytes. A face whose accents rise higher
/// than the default window allows moves its cap height down the cell
/// instead: the shader reads each glyph's `baseline_row` from the glyph table
/// and [`SetMetrics::baseline_frac`] records each set's, so a placement
/// changes no format and no shader.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapPlacement {
    /// The cap height's distance from the cell top, as a fraction of the
    /// cell. [`CAP_TOP_FRAC`] by default.
    pub top_frac: f64,
    /// The cap height in em, or `None` to measure the face's own `A`
    /// ([`cap_height`]). Given explicitly for a face with no `A` that must
    /// share another face's baseline, such as the terminal symbol page.
    pub height_em: Option<f64>,
}

impl CapPlacement {
    /// The placement of every bake before this option: the cap height at
    /// [`CAP_TOP_FRAC`], measured from the face's `A`.
    pub const DEFAULT: Self = Self {
        top_frac: CAP_TOP_FRAC,
        height_em: None,
    };

    /// The terminal faces' placement: the cap height at 0.24 of the cell,
    /// which fits Noto Sans Mono's accented capitals and descenders in the
    /// 1.3 em window (terminal fonts plan, section 4.2).
    pub const TERMINAL_TOP_FRAC: f64 = 0.24;

    /// This placement's cap height for `face`, in font units.
    #[cfg(feature = "shaper")]
    pub fn cap_units(&self, face: &ttf_parser::Face) -> f64 {
        match self.height_em {
            Some(em) => em * face.units_per_em() as f64,
            None => cap_height(face),
        }
    }

    /// The cell window for `face` in font units, `(bottom, top)`: the y
    /// range a cell holds. Ink outside it is clipped by the cell.
    #[cfg(feature = "shaper")]
    pub fn window(&self, face: &ttf_parser::Face) -> (f64, f64) {
        let cell = face.units_per_em() as f64 * CELL_EM_RATIO;
        let top = self.cap_units(face) + self.top_frac * cell;
        (top - cell, top)
    }
}

impl Default for CapPlacement {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// **Where an atlas's cells sit relative to the line box**, in em
/// (`docs/design/20261023_02_PER_ATLAS_CELL_BOX_PLAN.md`, section 4.1).
///
/// A cell is the 1.3 em line box ([`CellBox::LINE_BOX`]) unless a face's ink
/// does not fit it; then the cell reaches past the line box's top by
/// `above_em` and past its bottom by `below_em` (either may be negative,
/// which moves that edge inside the line box). The line box, the line pitch
/// and where the baseline sits in the line box do not change: ink outside
/// the line box overhangs it. Cells stay square, so a taller cell is wider
/// by the same amount.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellBox {
    /// How far the cell's top is above the line box's top, in em (negative:
    /// below it).
    pub above_em: f64,
    /// How far the cell's bottom is below the line box's bottom, in em
    /// (negative: above it).
    pub below_em: f64,
}

impl CellBox {
    /// The cell is the line box: every atlas before this option.
    pub const LINE_BOX: Self = Self {
        above_em: 0.0,
        below_em: 0.0,
    };

    /// The cell's height (and width) in em: [`CELL_EM_RATIO`] `+ above_em +
    /// below_em`.
    pub fn cell_em(&self) -> f64 {
        CELL_EM_RATIO + self.above_em + self.below_em
    }

    /// Whether this is [`CellBox::LINE_BOX`], which no `CELL` table records.
    pub fn is_line_box(&self) -> bool {
        self.above_em == 0.0 && self.below_em == 0.0
    }

    /// The `CELL` table body: `above_em` then `below_em`, `f32`
    /// little-endian.
    fn to_body(self) -> Vec<u8> {
        let mut body = Vec::with_capacity(8);
        body.extend_from_slice(&(self.above_em as f32).to_le_bytes());
        body.extend_from_slice(&(self.below_em as f32).to_le_bytes());
        body
    }

    /// Read and check a `CELL` table body.
    fn from_body(body: &[u8]) -> Result<Self, &'static str> {
        if body.len() != 8 {
            return Err("atlas CELL table is not 8 bytes");
        }
        let f = |o: usize| f32::from_le_bytes(body[o..o + 4].try_into().unwrap());
        let (above, below) = (f(0), f(4));
        if !above.is_finite() || !below.is_finite() {
            return Err("atlas CELL table holds a value that is not finite");
        }
        let cell = Self {
            above_em: above as f64,
            below_em: below as f64,
        };
        if !(0.5..=4.0).contains(&cell.cell_em()) {
            return Err("atlas CELL table gives a cell outside 0.5 to 4.0 em");
        }
        Ok(cell)
    }
}

impl Default for CellBox {
    fn default() -> Self {
        Self::LINE_BOX
    }
}

/// [`glyph_projection`] with a [`CapPlacement`].
#[cfg(feature = "shaper")]
pub fn glyph_projection_with(
    face: &ttf_parser::Face,
    glyph_id: u16,
    glyph_size: u32,
    cap: CapPlacement,
) -> Option<GlyphProjection> {
    let gs = glyph_size as f64;
    let upem = face.units_per_em() as f64;
    let em_scale = gs / (upem * CELL_EM_RATIO);

    let gid = ttf_parser::GlyphId(glyph_id);
    let advance_x = face.glyph_hor_advance(gid).unwrap_or(0) as f32 / upem as f32;

    let cap_y_max = cap.cap_units(face);

    let bbox = face.glyph_bounding_box(gid)?;

    // Horizontal: center the ink in the cell.
    let ink_w = (bbox.x_max - bbox.x_min) as f64 * em_scale;
    let target_px_x = (gs - ink_w) * 0.5;
    let g_tx = target_px_x / em_scale - bbox.x_min as f64;

    // Vertical: align the font's cap height to 15% from the cell top so all
    // digits and caps share a level top boundary.
    let target_top_px = gs * cap.top_frac;
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
/// The same ratio as [`crate::LINE_BOX_RATIO`] and it must stay that
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
#[cfg(feature = "shaper")]
pub fn default_cell_metrics(face: &ttf_parser::Face, glyph_size: u32) -> (f32, f32, f32) {
    default_cell_metrics_with(face, glyph_size, CapPlacement::DEFAULT)
}

/// [`default_cell_metrics`] with a [`CapPlacement`].
#[cfg(feature = "shaper")]
pub fn default_cell_metrics_with(
    face: &ttf_parser::Face,
    glyph_size: u32,
    cap: CapPlacement,
) -> (f32, f32, f32) {
    let gs = glyph_size as f64;
    let upem = face.units_per_em() as f64;
    let em_scale = gs / (upem * CELL_EM_RATIO);
    (
        (gs * cap.top_frac + em_scale * cap.cap_units(face)) as f32,
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
#[cfg(feature = "shaper")]
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
/// 215 cells (201 declared codepoints plus 14 shaped forms), 81 are free, and
/// nothing about ROWS could make room. **[`AtlasLayer`] retired that
/// argument.** A style is a second LAYER, with its own 320 cells, so widening
/// this buys nothing emphasis needs and still costs the one-time re-bake in
/// which every glyph's `u` moves and every review fixture is re-blessed once.
///
/// What would still want columns is a wider vocabulary AT ONE SIZE AND CUT -
/// a symbol set that outgrew 320 cells of regular text plus icons. That has
/// not happened; the shipped layer is 239 of 320.
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
/// — see [`crate::GlyphSet`], which declares the order so that a glyph
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
/// What is queued today (`add_shipped_coverage`, merged face, 2026-08-29):
///
/// ```text
///   95  printable ASCII                        U+0020..U+007E
///   96  Latin-1 Supplement                     U+00A0..U+00FF
///   10  the typographic ten                    dashes, quotes, bullet,
///                                              ellipsis, euro, trade mark
///   14  the shaped-ASCII superset beyond cmap  (ligatures, GSUB forms)
///   23  SYMBOLS: 18 borrowed + 4 drawn + 1 marker
///    1  glyph 0, the placeholder box
///  ---
///  239  of 320   (81 free; the plain face is 221, being 18 borrowed short)
/// ```
///
/// **The text side was called closed here, and it was not.** That line said
/// those ranges "are not going to grow again"; on 2026-08-29 they grew by ten,
/// because the atlas being ASCII-and-Latin-1 was the sole reason the project's
/// evaluation checklist had to carry a rule about em dashes rendering as
/// invisible spaces. So this is what the budget is spent on rather than a
/// prediction about what it will be: 191 declared text codepoints became 201,
/// and [`crate::TEXT_RANGES`] argues each of the ten and names the nine
/// it turned down.
///
/// The SYMBOL side grows on its own axis, and Tim's estimate (2026-08-16) is
/// **~16 symbol glyphs total** across icons and markers; we are at 23. The 81
/// free cells are still roughly five times the entire intended symbol budget,
/// and a wave that needs to raise this number should first ask why the symbol
/// set quintupled - or, if it is the text side asking again, what the last ten
/// bought (`widening_the_text_ranges_moved_cells_but_no_texel` records what
/// they cost: 320 raw bytes, 9,343 gzipped, and no moved texel).
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
#[cfg(feature = "shaper")]
#[derive(Clone)]
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
    /// is exactly the emergent ordering [`crate::GlyphSet`] replaces.
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
    /// **Layout extraction**: `None` bakes no layout tables; otherwise the
    /// options for each style that has its own (others take the default).
    layout: Option<Vec<(GlyphStyle, crate::LayoutOptions)>>,
    /// Licence records asked for with [`FontAtlasBuilder::add_licence`]:
    /// the styles covered, the SPDX identifier and the source.
    licences: Vec<(Vec<GlyphStyle>, String, String)>,
    /// Cell columns per layer ([`ATLAS_COLS`] unless
    /// [`FontAtlasBuilder::with_columns`] says otherwise).
    columns: u32,
    /// Where every cell puts its face's cap height
    /// ([`FontAtlasBuilder::with_cap_placement`]).
    cap: CapPlacement,
}

#[cfg(feature = "shaper")]
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
            layout: None,
            licences: Vec::new(),
            columns: ATLAS_COLS,
            cap: CapPlacement::DEFAULT,
        }
    }

    /// **Place every cell's cap height** with `cap` instead of
    /// [`CapPlacement::DEFAULT`], for a face whose ink the default window
    /// clips (terminal fonts plan, section 4.2).
    pub fn with_cap_placement(&mut self, cap: CapPlacement) {
        self.cap = cap;
    }

    /// The cap placement every cell is baked with.
    pub fn cap_placement(&self) -> CapPlacement {
        self.cap
    }

    /// **Bake `columns` cell columns per layer** instead of [`ATLAS_COLS`]:
    /// a layer then holds `columns x ATLAS_ROWS` cells and is
    /// `columns x (glyph_size + 2)` texels wide. [`ATLAS_COLS`] explains why
    /// columns, not rows, are the axis with headroom: 16 columns at 48px is
    /// 800x2000, inside the 2048 floor. No format change: the width is in the
    /// header and every entry holds texel coordinates.
    pub fn with_columns(&mut self, columns: u32) {
        self.columns = columns.max(1);
    }

    /// Cell columns per layer.
    pub fn columns(&self) -> u32 {
        self.columns
    }

    /// **Bake layout tables** (`Head`, `Cmap`, `Hmtx`, `Gsub`, `Gpos`) for
    /// every face this builder bakes, so [`crate::AtlasShaper`] can lay
    /// text out from the atlas alone. `opts` applies to the regular face;
    /// [`FontAtlasBuilder::with_style_layout`] gives another style its own.
    ///
    /// [`FontAtlasBuilder::build`] then extracts each face's tables
    /// ([`crate::FaceLayout::extract`], equivalence check included),
    /// fails the bake on any [`crate::LayoutError`], and queues a cell
    /// for every glyph a face's tables can output.
    pub fn with_layout(&mut self, opts: crate::LayoutOptions) {
        self.with_style_layout(GlyphStyle::Regular, opts);
    }

    /// [`FontAtlasBuilder::with_layout`] for one style's face: enables layout
    /// tables and sets that face's options (the features apply to it alone).
    pub fn with_style_layout(&mut self, style: GlyphStyle, opts: crate::LayoutOptions) {
        let list = self.layout.get_or_insert_with(Vec::new);
        list.retain(|(s, _)| *s != style);
        list.push((style, opts));
    }

    /// The face a style's cells are baked from, if one is registered.
    pub fn face_data(&self, style: GlyphStyle) -> Option<&[u8]> {
        self.style_face(style)
    }

    /// Whether this builder bakes layout tables.
    pub fn bakes_layout(&self) -> bool {
        self.layout.is_some()
    }

    /// **Record the licence of the faces of `styles`**: an SPDX identifier
    /// from [`crate::LICENCE_REGISTRY`] and what is licensed (the
    /// family name, e.g. `Roboto`).
    ///
    /// Every style [`FontAtlasBuilder::build`] bakes needs at least one
    /// record. At build the copyright line is read from the face's `name`
    /// ID 0 (the first covered style's face) and the Reserved Font Name from
    /// it, and a face whose `name` ID 13 or 14 names a different licence is
    /// refused.
    pub fn add_licence(
        &mut self,
        styles: &[GlyphStyle],
        spdx: &str,
        source: &str,
    ) -> Result<(), String> {
        if styles.is_empty() {
            return Err("a licence must cover at least one style".into());
        }
        if crate::licence_by_spdx(spdx).is_none() {
            return Err(format!(
                "{spdx:?} is not in LICENCE_REGISTRY - add a row there (with its round-trip                  test) to bake a face under it"
            ));
        }
        let mut probe = Vec::new();
        crate::licence::put_pstr(&mut probe, source)
            .map_err(|e| format!("licence source {source:?}: {e}"))?;
        let mut styles = styles.to_vec();
        styles.sort_unstable();
        styles.dedup();
        self.licences
            .push((styles, spdx.to_string(), source.to_string()));
        Ok(())
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
        match self
            .queued_glyphs
            .iter_mut()
            .find(|k| k.glyph_id() == glyph_id)
        {
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
                if let Some(gid) = face
                    .glyph_index(ch)
                    .and_then(|g| style.styled_glyph_id(g.0))
                {
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
    /// `(set, glyph id)`, sorted — [`crate::GlyphSet`] first, glyph id
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
        let Ok(shaper) = crate::shaper::TextShaper::new(face_data.to_vec()) else {
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
    /// [`crate::TEXT_RANGES`] cannot disagree about coverage.
    ///
    /// Stated as ranges the face is asked about, never as a list of glyphs.
    /// [`crate::PRIVATE_USE`] is asked for as its three declared BLOCKS,
    /// so the merged face's borrowed icons, our own icons and both faces'
    /// [`crate::MARKERS`] come along without this ever learning an icon
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
    /// with the sets declared by [`crate::GlyphSet`]. Two consequences
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
    ///   be reordered, and [`crate::TEXT_RANGES`] can be reordered or
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
        for &(lo, hi) in crate::TEXT_RANGES {
            self.add_codepoint_range(GlyphSet::Text, lo, hi);
        }
        // ...unioned with what the SHAPER emits for ASCII, which is a superset.
        // Glyphs already claimed above stay Text (`add_glyph` keeps the lowest
        // set), so this contributes exactly the ligatures and GSUB forms.
        self.add_shaped_ascii();

        // The Private Use Area, as the three blocks that tile it: the vendor's
        // half, then ours. Still ranges, never names.
        let (pua_lo, pua_hi) = crate::PRIVATE_USE;
        let (own_lo, own_hi) = crate::OWNED_BLOCKS;
        let (icons_lo, icons_hi) = crate::HIGHBAY_ICONS_BLOCK;
        let (markers_lo, markers_hi) = crate::MARKERS;
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
    /// weight would be 18 cells spent on a distinction no caller can make. A
    /// style face is prose, and prose is [`crate::TEXT_RANGES`].
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
            return Err(format!(
                "{style:?} already has a face registered on this builder"
            ));
        }
        ttf_parser::Face::parse(&font_data, 0).map_err(|e| format!("{style:?} face: {e}"))?;
        self.style_faces.push((style, font_data));

        for &(lo, hi) in crate::TEXT_RANGES {
            self.add_codepoint_range(style.text_set(), lo, hi);
        }
        // ...and the superset ITS layout tables emit. A glyph already claimed
        // by the text set stays there (`add_glyph` keeps the lowest set), so
        // this contributes exactly that face's ligatures and GSUB forms.
        self.add_shaped_ascii_of(style);
        Ok(())
    }

    /// **Register a face for `style` without queueing anything**, for a
    /// caller that queues its own coverage (the `Baker`'s coverage groups).
    /// Refuses Regular (the builder's own face) and a style that already has
    /// one.
    pub fn add_styled_face(&mut self, style: GlyphStyle, font_data: Vec<u8>) -> Result<(), String> {
        if style == GlyphStyle::Regular {
            return Err("GlyphStyle::Regular is the face this builder was constructed with".into());
        }
        if self.style_face(style).is_some() {
            return Err(format!(
                "{style:?} already has a face registered on this builder"
            ));
        }
        ttf_parser::Face::parse(&font_data, 0).map_err(|e| format!("{style:?} face: {e}"))?;
        self.style_faces.push((style, font_data));
        Ok(())
    }

    /// The styles this builder will bake cells for, in cell order.
    pub fn styles(&self) -> Vec<GlyphStyle> {
        let mut styles: Vec<GlyphStyle> =
            self.queued_glyphs.iter().map(|k| k.set().style()).collect();
        styles.sort_unstable();
        styles.dedup();
        styles
    }
}

/// Why a bake failed: a layout error kept whole, or any other refusal.
#[cfg(all(feature = "bake", not(target_arch = "wasm32")))]
#[derive(Debug)]
pub(crate) enum BuildError {
    Layout(GlyphStyle, crate::LayoutError),
    Other(String),
}

#[cfg(all(feature = "bake", not(target_arch = "wasm32")))]
impl core::fmt::Display for BuildError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BuildError::Layout(style, e) => write!(f, "{style:?} layout: {e}"),
            BuildError::Other(e) => f.write_str(e),
        }
    }
}

#[cfg(all(feature = "bake", not(target_arch = "wasm32")))]
impl From<String> for BuildError {
    fn from(e: String) -> Self {
        BuildError::Other(e)
    }
}

/// One baked glyph cell: RGB MSDF pixels (`gs × gs × 3`, top-down rows) and
/// its glyph-table entry with `atlas_x`/`atlas_y` left at 0 — the caller
/// (atlas builder or dynamic-append manager) fills those in after packing.
#[cfg(all(feature = "bake", not(target_arch = "wasm32")))]
pub struct BakedGlyphCell {
    pub rgb: Vec<u8>,
    pub entry: GlyphEntry,
}

#[cfg(all(feature = "bake", not(target_arch = "wasm32")))]
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
        let face25 =
            ttf_parser::Face::parse(face_data, 0).map_err(|e| format!("font parse error: {e}"))?;
        let face18 = ttf_parser_018::Face::parse(face_data, 0)
            .map_err(|e| format!("font parse error (v18): {e}"))?;

        let gs = self.glyph_size;
        let channels = 3u32;
        let (baseline_def, x_margin_def, px_per_em) =
            default_cell_metrics_with(&face25, gs, self.cap);

        let gid18 = ttf_parser_018::GlyphId(glyph_id);
        let projection = glyph_projection_with(&face25, glyph_id, gs, self.cap);
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
    ///
    /// It refuses a bake with a style that no licence record covers
    /// ([`FontAtlasBuilder::add_licence`]), and, with
    /// [`FontAtlasBuilder::with_layout`], a face whose layout the tables
    /// cannot carry exactly.
    pub fn build(&self) -> Result<FontAtlas, String> {
        self.build_checked().map_err(|e| e.to_string())
    }

    /// [`FontAtlasBuilder::build`], keeping a layout error as one.
    pub(crate) fn build_checked(&self) -> Result<FontAtlas, BuildError> {
        let licences = self.licence_records()?;
        let (builder, layout) = match &self.layout {
            None => (None, None),
            Some(options) => {
                let mut builder = self.clone();
                let mut faces = Vec::new();
                for style in self.styles() {
                    let face = self
                        .style_face(style)
                        .ok_or_else(|| format!("no {style:?} face is registered"))?;
                    let opts = options
                        .iter()
                        .find(|(s, _)| *s == style)
                        .map(|(_, o)| o.clone())
                        .unwrap_or_default();
                    let layout = crate::FaceLayout::extract(face, style, &opts)
                        .map_err(|e| BuildError::Layout(style, e))?;
                    // A cell for every glyph the tables can output; glyph 0
                    // is the style-invariant placeholder cell.
                    for &(raw, _) in &layout.hmtx {
                        if raw == 0 {
                            builder.add_glyph(GlyphSet::Placeholder, 0);
                            continue;
                        }
                        if let Some(addr) = style.styled_glyph_id(raw) {
                            builder.add_glyph(style.shaped_set(), addr);
                        }
                    }
                    faces.push(layout);
                }
                let layout =
                    crate::AtlasLayout::from_faces(faces).map_err(|e| format!("layout: {e}"))?;
                (Some(builder), Some(layout))
            }
        };
        let mut atlas = builder.as_ref().unwrap_or(self).bake_cells()?;
        atlas.layout = layout;
        atlas.licences = licences;
        atlas
            .try_to_bytes()
            .map_err(|e| format!("the baked atlas breaks a table rule: {e}"))?;
        Ok(atlas)
    }

    /// The licence records of this bake, checked against the faces.
    fn licence_records(&self) -> Result<Vec<crate::LicenceRecord>, String> {
        for style in self.styles() {
            if !self
                .licences
                .iter()
                .any(|(styles, _, _)| styles.contains(&style))
            {
                return Err(format!(
                    "{style:?} is baked but no licence covers it - call add_licence (bake_atlas \
                     --licence <SPDX>=<source>[@<style>,...])"
                ));
            }
        }
        let mut out = Vec::new();
        for (styles, spdx, source) in &self.licences {
            let kind = crate::licence_by_spdx(spdx).expect("checked by add_licence");
            let mut copyright = None;
            for &style in styles {
                let data = self.style_face(style).ok_or_else(|| {
                    format!("the {spdx} licence covers {style:?}, which has no face registered")
                })?;
                let face = ttf_parser::Face::parse(data, 0)
                    .map_err(|e| format!("{style:?} face parse error: {e}"))?;
                if let Some(declared) = crate::layout_bake::declared_licence(&face)
                    && declared != spdx
                {
                    return Err(format!(
                        "{style:?}: the face's name table says {declared}, not {spdx}"
                    ));
                }
                if copyright.is_none() {
                    copyright = Some(crate::layout_bake::copyright_of(&face).unwrap_or_default());
                }
            }
            let copyright = copyright.unwrap_or_default();
            let record = crate::LicenceRecord {
                fourcc: kind.fourcc,
                version: kind.version,
                styles: styles.clone(),
                spdx: spdx.clone(),
                source: source.clone(),
                reserved_font_name: crate::layout_bake::reserved_font_name(&copyright),
                copyright,
            };
            for (field, text) in [
                ("copyright", &record.copyright),
                ("reserved font name", &record.reserved_font_name),
            ] {
                crate::licence::put_pstr(&mut Vec::new(), text)
                    .map_err(|e| format!("{:?} face's {field}: {e}", record.styles[0]))?;
            }
            out.push(record);
        }
        Ok(out)
    }

    /// The msdfgen bake of the queued cells, without layout or licences.
    fn bake_cells(&self) -> Result<FontAtlas, String> {
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

        let atlas_w = self.columns * padded;

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
            placed.push(Placed {
                cell,
                layer,
                atlas_x: x + 1,
                atlas_y: y + 1,
            });
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
                            m.baseline_frac,
                            frac,
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
            if n > (self.columns * ATLAS_ROWS) as usize {
                return Err(format!(
                    "layer {i} ({layer}) has {n} cells, but a layer is pinned at {} \
                     ({ATLAS_ROWS} rows x {} columns) - read ATLAS_COLS and then \
                     ATLAS_ROWS before raising either; columns are the axis with headroom and \
                     rows are the one without. A second STYLE is a second layer and does not \
                     spend this budget; a wider vocabulary at one size does",
                    self.columns * ATLAS_ROWS,
                    self.columns
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
            layout: None,
            licences: Vec::new(),
            read_version: AtlasFileVersion::Split(ATLAS_FORMAT_VERSION),
            coverage: Vec::new(),
            cell_box: CellBox::LINE_BOX,
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
#[cfg(all(feature = "bake", not(target_arch = "wasm32")))]
pub(crate) fn msdf_to_u8(unit_value: f32) -> u8 {
    (unit_value.clamp(0.0, 1.0) * 255.0) as u8
}

#[cfg(all(test, feature = "shaper"))]
mod tests {
    use super::*;

    fn roboto() -> Vec<u8> {
        crate::TEST_FACE.to_vec()
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
        assert_eq!(CELL_EM_RATIO as f32, crate::LINE_BOX_RATIO);
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
        v1.extend_from_slice(
            &GlyphEntry {
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
            .to_bytes(),
        );
        v1.resize(v1.len() + 400 * 2000 * 3, 0);

        let err = FontAtlas::from_bytes(&v1).expect_err("a v1 atlas must not load");
        assert!(
            err.contains("hbfa"),
            "the message must name the magic: {err}"
        );
        assert!(
            err.contains("bake_atlas"),
            "the message must name the fix: {err}"
        );
    }

    /// A file whose file-major is past ours stops too, for the same reason.
    #[test]
    fn a_future_file_major_is_refused() {
        let mut buf = Vec::new();
        buf.extend_from_slice(ATLAS_MAGIC);
        buf.extend_from_slice(&[ATLAS_FILE_MAJOR + 1, 0, 0, 0]);
        buf.resize(ATLAS_HEADER_SIZE_V5, 0);
        let err = FontAtlas::from_bytes(&buf).expect_err("a file-major 6 atlas must not load");
        assert!(err.contains("file-major"), "{err}");
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

    #[cfg(all(feature = "bake", not(target_arch = "wasm32")))]
    #[test]
    fn build_ascii_atlas() {
        let mut builder = FontAtlasBuilder::new(roboto(), 32, 4.0);
        builder
            .add_licence(&[GlyphStyle::Regular], "Apache-2.0", "Roboto")
            .unwrap();
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

    #[cfg(all(feature = "bake", not(target_arch = "wasm32")))]
    #[test]
    fn atlas_roundtrip() {
        let mut builder = FontAtlasBuilder::new(roboto(), 32, 4.0);
        builder
            .add_licence(&[GlyphStyle::Regular], "Apache-2.0", "Roboto")
            .unwrap();
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
            glyph_id: 7,
            atlas_x: 1,
            atlas_y: 1,
            atlas_w: 32,
            atlas_h: 32,
            layer: 0,
            advance_x: 0.5,
            baseline_row: 24.0,
            px_per_em: 24.6,
            x_margin: 4.8,
        };
        let idx = atlas.insert_entry(e);
        assert_eq!(idx, 0);
        assert_eq!(atlas.glyph_table_index(7), Some(0));
        // Replacement keeps the index.
        let idx2 = atlas.insert_entry(GlyphEntry {
            advance_x: 0.6,
            ..e
        });
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
            glyph_id,
            atlas_x: 1,
            atlas_y: 1,
            atlas_w: 32,
            atlas_h: 32,
            layer: 0,
            advance_x: glyph_id as f32 / 100.0,
            baseline_row: 24.0,
            px_per_em: 24.6,
            x_margin: 4.8,
        };
        for gid in [90u16, 7, 300, 0, 41] {
            atlas.insert_entry(entry(gid));
        }
        let ids: Vec<u16> = atlas.glyphs.iter().map(|e| e.glyph_id).collect();
        assert_eq!(
            ids,
            vec![0, 7, 41, 90, 300],
            "the table is not glyph-id ordered"
        );
        for gid in [0u16, 7, 41, 90, 300] {
            assert_eq!(atlas.get_glyph(gid).map(|e| e.glyph_id), Some(gid));
        }
        // ...and a glyph that was never inserted is still absent, rather than
        // matching the neighbour a bad search would land on.
        for gid in [1u16, 42, 89, 299, 301] {
            assert!(
                atlas.get_glyph(gid).is_none(),
                "glyph {gid} was never inserted"
            );
        }
    }

    /// **A glyph belongs to the lowest set that claimed it, whatever order the
    /// queue calls came in** — so the layout does not depend on the sequence
    /// of `add_*` calls, which is the failure the sets exist to remove.
    #[test]
    fn the_queue_call_order_no_longer_decides_anything() {
        let gid = |ch: char| {
            ttf_parser::Face::parse(&crate::TEST_FACE, 0)
                .unwrap()
                .glyph_index(ch)
                .unwrap()
                .0
        };

        let mut forwards = FontAtlasBuilder::new(roboto(), 48, 6.0);
        forwards
            .add_licence(&[GlyphStyle::Regular], "Apache-2.0", "Roboto")
            .unwrap();
        forwards.add_codepoint_range(GlyphSet::Text, 'a', 'z');
        forwards.add_glyph(GlyphSet::ShapedText, gid('a'));
        forwards.add_codepoint_range(GlyphSet::OwnedIcons, '\u{F800}', '\u{F8EF}');
        forwards.add_glyph(GlyphSet::Placeholder, 0);

        let mut backwards = FontAtlasBuilder::new(roboto(), 48, 6.0);
        backwards
            .add_licence(&[GlyphStyle::Regular], "Apache-2.0", "Roboto")
            .unwrap();
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
        assert!(
            !order
                .iter()
                .any(|k| k.glyph_id() == gid('a') && k.set() == GlyphSet::ShapedText)
        );
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
        for face_bytes in [crate::TEST_FACE, crate::TEST_FACE_MSYMBOLS] {
            let next_gid = ttf_parser::Face::parse(face_bytes, 0)
                .unwrap()
                .number_of_glyphs();

            let mut before = FontAtlasBuilder::new(face_bytes.to_vec(), 48, 6.0);
            before
                .add_licence(&[GlyphStyle::Regular], "Apache-2.0", "Roboto")
                .unwrap();
            before.add_shipped_coverage();
            let before = before.cell_order();

            let mut after = FontAtlasBuilder::new(face_bytes.to_vec(), 48, 6.0);
            after
                .add_licence(&[GlyphStyle::Regular], "Apache-2.0", "Roboto")
                .unwrap();
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
            assert_eq!(
                after[new_cell + 1..],
                later[..],
                "a later set shifted by more than one"
            );
            // Everything after the new cell belongs to a LATER set: it sorted
            // into its own set rather than onto the end of the queue.
            for key in &after[new_cell + 1..] {
                let set = key.set();
                assert!(
                    set > GlyphSet::OwnedIcons,
                    "a cell of set {set:?} sorted after an icon"
                );
            }
            if face_bytes == crate::TEST_FACE_MSYMBOLS {
                assert!(
                    !later.is_empty(),
                    "the merged face should have borrowed cells after ours"
                );
                assert_ne!(
                    after[new_cell..],
                    before[new_cell..],
                    "nothing moved at all"
                );
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
    #[cfg(all(feature = "bake", not(target_arch = "wasm32")))]
    #[test]
    fn a_new_icon_leaves_every_earlier_cell_untouched() {
        use std::collections::BTreeMap;

        let gid = |ch: char| {
            ttf_parser::Face::parse(&crate::TEST_FACE, 0)
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
            b.add_licence(&[GlyphStyle::Regular], "Apache-2.0", "Roboto")
                .unwrap();
            b.add_glyph(GlyphSet::Placeholder, 0);
            b.add_codepoint_range(GlyphSet::Text, 'A', 'E');
            b.add_glyph(GlyphSet::Markers, gid(crate::MARKER_ARROW));
            for &ch in icons {
                b.add_glyph(GlyphSet::OwnedIcons, gid(ch));
            }
            b.build().expect("bake")
        };

        // `graph` and `props` ship; then the face gains `table`.
        let before = bake(&['\u{F800}', '\u{F801}']);
        let after = bake(&['\u{F800}', '\u{F801}', '\u{F802}']);

        let (b, a) = (cells(&before), cells(&after));
        assert_eq!(
            b.len() + 1,
            a.len(),
            "the second bake should have one more cell"
        );
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
        assert!(
            !b.values().any(|(xy, _)| *xy == new_cell),
            "the new icon reused a cell"
        );
        // ...and this comparison can SEE a cell move: adding to an earlier set
        // (one more letter, in Text) shifts the marker and the icons.
        let mut widened = FontAtlasBuilder::new(roboto(), 32, 4.0);
        widened
            .add_licence(&[GlyphStyle::Regular], "Apache-2.0", "Roboto")
            .unwrap();
        widened.add_glyph(GlyphSet::Placeholder, 0);
        widened.add_codepoint_range(GlyphSet::Text, 'A', 'F');
        widened.add_glyph(GlyphSet::Markers, gid(crate::MARKER_ARROW));
        for ch in ['\u{F800}', '\u{F801}'] {
            widened.add_glyph(GlyphSet::OwnedIcons, gid(ch));
        }
        let widened = cells(&widened.build().expect("bake"));
        let arrow = gid(crate::MARKER_ARROW);
        assert_ne!(
            widened[&arrow].0, b[&arrow].0,
            "adding a letter should push the marker's cell along - if it does not, \
             this test cannot see movement and the assertions above prove nothing"
        );
    }
}

/// **The file-major 5 container**: the version field and the table
/// directory (plan section 4.1 with its 13:03 EDT test table, and 4.2).
#[cfg(test)]
mod format_v5 {
    use super::*;

    /// A minimal one-layer 1x1 atlas at file-major 5, version bytes replaced.
    fn v5_with_version(v: [u8; 4], tables: &[TableBody]) -> Vec<u8> {
        let mut bytes = FontAtlas::empty(1, 1, 3).write_v5(tables);
        bytes[4..8].copy_from_slice(&v);
        bytes
    }

    /// A minimal legacy file: 28-byte header, (v4: layer count and one
    /// layer), no sets, no glyphs, 1x1x3 pixels.
    fn legacy(v: [u8; 4]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(ATLAS_MAGIC);
        b.extend_from_slice(&v);
        for n in [1u32, 1, 0, 3, 0] {
            b.extend_from_slice(&n.to_le_bytes());
        }
        if v[0] == 4 {
            b.extend_from_slice(&1u32.to_le_bytes());
            b.extend_from_slice(&AtlasLayer::new(48, GlyphStyle::Bold).to_bytes());
        }
        b.extend_from_slice(&[7, 8, 9]);
        b
    }

    fn table(tag: &[u8; 4], face: u8, body: &[u8]) -> TableBody {
        TableBody {
            tag: *tag,
            face,
            body: body.to_vec(),
        }
    }

    #[test]
    fn legacy_rows() {
        let v3 = FontAtlas::from_bytes(&legacy([3, 0, 0, 0])).expect("legacy v3");
        assert_eq!(v3.read_version, AtlasFileVersion::Legacy(3));
        // One layer, measured off the (absent) cells.
        assert_eq!(v3.layers, vec![AtlasLayer::new(0, GlyphStyle::Regular)]);
        assert_eq!(v3.pixel_data, vec![7, 8, 9]);

        let v4 = FontAtlas::from_bytes(&legacy([4, 0, 0, 0])).expect("legacy v4");
        assert_eq!(v4.read_version, AtlasFileVersion::Legacy(4));
        assert_eq!(v4.layers, vec![AtlasLayer::new(48, GlyphStyle::Bold)]);
        assert_eq!(v4.pixel_data, vec![7, 8, 9]);

        let err = FontAtlas::from_bytes(&legacy([3, 0, 1, 0])).expect_err("high bytes");
        assert!(err.contains("non-zero high bytes"), "{err}");
        assert!(FontAtlas::from_bytes(&legacy([2, 0, 0, 0])).is_err());
        let err = FontAtlas::from_bytes(&v5_with_version([0, 0, 0, 0], &[])).expect_err("0");
        assert!(err.contains("file-major"), "{err}");
    }

    #[test]
    fn split_rows_against_this_reader() {
        let ok = FontAtlas::from_bytes(&v5_with_version([5, 0, 1, 0], &[])).expect("5:0.1.0");
        assert_eq!(
            ok.read_version,
            AtlasFileVersion::Split(ATLAS_FORMAT_VERSION_LINE_BOX)
        );
        assert_eq!(ok.layers, vec![AtlasLayer::new(0, GlyphStyle::Regular)]);
        assert_eq!(ok.pixel_data, vec![0, 0, 0]);

        // 0.1.1 with one table this reader does not know: skipped.
        let newer_patch = v5_with_version(
            [5, 0, 1, 1],
            &[table(b"Zzzz", ATLAS_TABLE_FACE_ALL, b"abcde")],
        );
        let a = FontAtlas::from_bytes(&newer_patch).expect("5:0.1.1 with an unknown table");
        assert_eq!(a.layout, None);
        assert!(a.licences.is_empty());
        assert_eq!(a.pixel_data, vec![0, 0, 0]);

        // 0.2.0, the cell box's version, reads without a CELL table too.
        assert!(FontAtlas::from_bytes(&v5_with_version([5, 0, 2, 0], &[])).is_ok());

        // 0.1.7 parses as 0.1.0 does.
        let p7 = FontAtlas::from_bytes(&v5_with_version([5, 0, 1, 7], &[])).expect("5:0.1.7");
        assert_eq!(p7.layers, ok.layers);
        assert_eq!(p7.pixel_data, ok.pixel_data);

        for (v, why) in [
            ([5, 0, 0, 3], "0.0.x"),
            ([5, 0, 3, 0], "newer"),
            ([5, 1, 0, 0], "newer major"),
            ([6, 0, 0, 0], "file-major"),
        ] {
            let err = FontAtlas::from_bytes(&v5_with_version(v, &[])).expect_err("refused");
            assert!(err.contains(why), "{v:?}: {err}");
        }
    }

    /// The hypothetical-reader rows: unit tests of the acceptance rule and the
    /// gate, not of a real 0.2 or 1.0 format.
    #[test]
    fn hypothetical_readers() {
        let fv = |major, minor, patch| AtlasFormatVersion {
            file_major: 5,
            major,
            minor,
            patch,
        };
        // A 5:0.2 reader with a field X gated on `v > (0, 1)`.
        let reader_02 = fv(0, 2, 0);
        let read_x = |file: AtlasFormatVersion| -> Option<u32> {
            reader_02.reads(file).ok()?;
            Some(if gate((file.major, file.minor), (0, 1)) {
                42
            } else {
                0
            })
        };
        assert_eq!(read_x(fv(0, 1, 0)), Some(0), "0.1: X takes its default");
        assert_eq!(read_x(fv(0, 2, 0)), Some(42), "0.2: X is read");
        // A 5:1.0 reader.
        let reader_10 = fv(1, 0, 0);
        assert!(reader_10.reads(fv(1, 2, 0)).is_ok(), "newer minor from 1.0");
        assert!(reader_10.reads(fv(1, 0, 5)).is_ok(), "newer patch");
        assert!(reader_10.reads(fv(2, 0, 0)).is_err(), "newer major");
        assert!(reader_10.reads(fv(0, 1, 0)).is_ok(), "older 0.x file");
        assert!(reader_10.reads(fv(0, 0, 1)).is_err(), "0.0.x");
        assert!(!gate((0, 1), (0, 1)) && gate((1, 0), (0, 9)));
    }

    #[test]
    fn unknown_tags_are_skipped() {
        let bytes = FontAtlas::empty(2, 2, 3).write_v5(&[
            table(b"ABC1", 0, b"x"),
            table(b"Zz  ", 1, b"0123456789"),
            table(b"Qqqq", ATLAS_TABLE_FACE_ALL, b""),
        ]);
        let a = FontAtlas::from_bytes(&bytes).expect("unknown tables are skipped");
        assert_eq!(a.pixel_data.len(), 12);
    }

    #[test]
    fn forbidden_and_malformed_tags_are_refused() {
        for (tag, why) in [
            (b"GSUB", "registered OpenType"),
            (b"CFF ", "registered OpenType"),
            (b"cmap", "neither first-capital"),
            (b"fpgm", "neither first-capital"),
            (b"prep", "neither first-capital"),
            (b"cvt ", "neither first-capital"),
            (b"GsuB", "neither first-capital"),
            (b" Abc", "malformed"),
            (b"A Bc", "malformed"),
        ] {
            let bytes = FontAtlas::empty(1, 1, 3).write_v5(&[table(tag, 0, b"data")]);
            let err = FontAtlas::from_bytes(&bytes).expect_err("refused");
            assert!(err.contains(why), "{tag:?}: {err}");
        }
    }

    /// The offset of the first directory record in a minimal 1x1 atlas.
    const DIR: usize = ATLAS_HEADER_SIZE_V5 + AtlasLayer::PACKED_SIZE;

    #[test]
    fn misplaced_bodies_and_dirty_padding_are_refused() {
        let two = [table(b"Aaaa", 0, b"xyz"), table(b"Bbbb", 0, b"pq")];
        let good = FontAtlas::empty(1, 1, 3).write_v5(&two);
        assert!(FontAtlas::from_bytes(&good).is_ok());
        let off = |b: &[u8], i: usize| {
            u32::from_le_bytes(b[DIR + i * 16 + 8..DIR + i * 16 + 12].try_into().unwrap())
        };
        let set_off = |b: &mut Vec<u8>, i: usize, v: u32| {
            b[DIR + i * 16 + 8..DIR + i * 16 + 12].copy_from_slice(&v.to_le_bytes())
        };

        // Overlapping: the second body starts inside the first.
        let mut overlap = good.clone();
        let first = off(&good, 0);
        set_off(&mut overlap, 1, first);
        let err = FontAtlas::from_bytes(&overlap).expect_err("overlap");
        assert!(err.contains("directory rule"), "{err}");

        // Misaligned: the first body starts 1 byte late.
        let mut misaligned = good.clone();
        set_off(&mut misaligned, 0, first + 1);
        let err = FontAtlas::from_bytes(&misaligned).expect_err("misaligned");
        assert!(err.contains("directory rule"), "{err}");

        // Non-zero padding after the 3-byte first body.
        let mut dirty = good.clone();
        dirty[first as usize + 3] = 1;
        let err = FontAtlas::from_bytes(&dirty).expect_err("padding");
        assert!(err.contains("padding"), "{err}");

        // Records out of order.
        let unsorted = FontAtlas::empty(1, 1, 3).write_v5(&[two[1].clone(), two[0].clone()]);
        let err = FontAtlas::from_bytes(&unsorted).expect_err("unsorted");
        assert!(err.contains("sorted"), "{err}");

        // A repeated (face, tag).
        let repeated = FontAtlas::empty(1, 1, 3).write_v5(&[two[0].clone(), two[0].clone()]);
        let err = FontAtlas::from_bytes(&repeated).expect_err("repeat");
        assert!(err.contains("repeats"), "{err}");

        // Non-zero flags.
        let mut flagged = good.clone();
        flagged[DIR + 5] = 1;
        let err = FontAtlas::from_bytes(&flagged).expect_err("flags");
        assert!(err.contains("flags"), "{err}");

        // A face byte that is neither a style nor the atlas.
        let err =
            FontAtlas::from_bytes(&FontAtlas::empty(1, 1, 3).write_v5(&[table(b"Aaaa", 9, b"")]))
                .expect_err("face");
        assert!(err.contains("face"), "{err}");
    }

    #[test]
    fn too_many_tables_is_refused() {
        let mut bytes = FontAtlas::empty(1, 1, 3).write_v5(&[]);
        bytes[32..36].copy_from_slice(&65u32.to_le_bytes());
        let err = FontAtlas::from_bytes(&bytes).expect_err("65 tables");
        assert!(err.contains("64"), "{err}");
    }

    #[test]
    fn a_built_atlas_says_it_is_current() {
        assert_eq!(
            FontAtlas::empty(1, 1, 3).file_version(),
            AtlasFileVersion::Split(ATLAS_FORMAT_VERSION)
        );
        assert_eq!(ATLAS_FORMAT_VERSION.to_string(), "5:0.2.0");
        assert_eq!(ATLAS_FORMAT_VERSION_LINE_BOX.to_string(), "5:0.1.0");
    }

    /// One set whose `px_per_em_frac` fits `cell`, in a 1x1 atlas with that box.
    fn boxed(cell: CellBox) -> FontAtlas {
        let mut a = FontAtlas::empty(1, 1, 3);
        a.sets = vec![SetMetrics {
            set: GlyphSet::Text,
            glyph_count: 0,
            baseline_frac: 0.7,
            px_per_em_frac: (1.0 / cell.cell_em()) as f32,
            ascent_em: 0.0,
            descent_em: 0.0,
            line_gap_em: 0.0,
            underline_pos_em: 0.0,
            underline_thickness_em: 0.0,
            ink_descent_em: 0.0,
        }];
        a.cell_box = cell;
        a
    }

    const TEXTURINA_BOX: CellBox = CellBox {
        above_em: 0.179,
        below_em: -0.073,
    };

    /// **The cell box round-trips, at 5:0.2.0 with a `CELL` table, and an
    /// atlas whose cells are the line box is written exactly as before**
    /// (per-atlas cell box plan, section 4.2).
    #[test]
    fn a_cell_box_round_trips_at_0_2_and_the_line_box_stays_0_1() {
        let line = boxed(CellBox::LINE_BOX).to_bytes();
        assert_eq!(&line[4..8], &[5, 0, 1, 0]);
        assert_eq!(u32::from_le_bytes(line[32..36].try_into().unwrap()), 0);
        assert_eq!(
            FontAtlas::from_bytes(&line).unwrap().cell_box(),
            CellBox::LINE_BOX
        );

        let bytes = boxed(TEXTURINA_BOX).to_bytes();
        assert_eq!(&bytes[4..8], &[5, 0, 2, 0]);
        let back = FontAtlas::from_bytes(&bytes).expect("5:0.2.0 with CELL");
        let c = back.cell_box();
        assert!((c.above_em - 0.179).abs() < 1e-6 && (c.below_em + 0.073).abs() < 1e-6);
        assert!((c.cell_em() - 1.406).abs() < 1e-6);
        assert_eq!(
            back.to_bytes(),
            bytes,
            "a second round trip is a fixed point"
        );
    }

    /// A reader of 5:0.1.0, libmsdf 0.5, refuses an atlas with a cell box
    /// rather than drawing its cells onto a 1.3 em box.
    #[test]
    fn a_0_1_reader_refuses_a_cell_box() {
        assert!(
            ATLAS_FORMAT_VERSION_LINE_BOX
                .reads(ATLAS_FORMAT_VERSION)
                .is_err()
        );
        assert!(
            ATLAS_FORMAT_VERSION
                .reads(ATLAS_FORMAT_VERSION_LINE_BOX)
                .is_ok()
        );
    }

    #[test]
    fn a_bad_cell_table_is_refused() {
        let body = |a: f32, b: f32| [a.to_le_bytes(), b.to_le_bytes()].concat();
        let with = |v: [u8; 4], body: &[u8]| {
            let mut bytes = boxed(CellBox::LINE_BOX).write_v5(&[table(
                &CELL_TABLE_TAG,
                ATLAS_TABLE_FACE_ALL,
                body,
            )]);
            bytes[4..8].copy_from_slice(&v);
            bytes
        };
        // A box that agrees with the set's 1/1.3 is accepted at 0.2.
        assert!(FontAtlas::from_bytes(&with([5, 0, 2, 0], &body(0.1, -0.1))).is_ok());
        for (v, b, says) in [
            ([5, 0, 1, 0], body(0.1, -0.1), "before 5:0.2.0"),
            ([5, 0, 2, 0], vec![0; 4], "8 bytes"),
            ([5, 0, 2, 0], body(f32::NAN, 0.0), "not finite"),
            ([5, 0, 2, 0], body(3.0, 0.0), "0.5 to 4.0"),
            ([5, 0, 2, 0], body(0.2, 0.0), "px_per_em_frac"),
        ] {
            let err = FontAtlas::from_bytes(&with(v, &b)).expect_err(says);
            assert!(err.contains(says), "{says}: {err}");
        }
    }
}

/// **Licences and layout at bake** (atlas layout tables plan, step 3).
#[cfg(all(test, feature = "bake", not(target_arch = "wasm32")))]
mod bake_licences {
    use super::*;

    #[test]
    fn a_bake_without_a_licence_names_the_style() {
        let mut b = FontAtlasBuilder::new(crate::TEST_FACE.to_vec(), 16, 2.0);
        b.add_codepoint_range(GlyphSet::Text, 'A', 'C');
        b.add_styled_coverage(GlyphStyle::Bold, crate::TEST_FACE_BOLD.to_vec())
            .unwrap();
        b.add_licence(&[GlyphStyle::Regular], "Apache-2.0", "Roboto")
            .unwrap();
        let err = b.build().expect_err("Bold has no licence");
        assert!(err.contains("Bold") && err.contains("licence"), "{err}");
    }

    #[test]
    fn a_licence_the_face_contradicts_is_refused() {
        let mut b = FontAtlasBuilder::new(crate::TEST_FACE_BOLD.to_vec(), 16, 2.0);
        b.add_codepoint_range(GlyphSet::Text, 'A', 'C');
        b.add_licence(&[GlyphStyle::Regular], "OFL-1.1", "Roboto")
            .unwrap();
        let err = b.build().expect_err("the name table says Apache-2.0");
        assert!(err.contains("Apache-2.0"), "{err}");
        assert!(
            b.add_licence(&[GlyphStyle::Regular], "GPL-3.0", "x")
                .is_err(),
            "a licence outside the registry"
        );
    }

    #[test]
    fn a_baked_atlas_carries_its_licence_and_layout() {
        let mut b = FontAtlasBuilder::new(crate::TEST_FACE.to_vec(), 16, 2.0);
        b.add_codepoint_range(GlyphSet::Text, 'A', 'C');
        b.add_licence(&[GlyphStyle::Regular], "Apache-2.0", "Roboto")
            .unwrap();
        b.with_layout(crate::LayoutOptions::default());
        let atlas = b.build().expect("bakes");
        let back = FontAtlas::from_bytes(&atlas.to_bytes()).expect("loads");
        assert_eq!(back.licences().len(), 1);
        let r = &back.licences()[0];
        assert_eq!(
            (r.spdx.as_str(), r.source.as_str()),
            ("Apache-2.0", "Roboto")
        );
        assert_eq!(
            r.copyright,
            "Copyright 2011 Google Inc. All Rights Reserved."
        );
        assert_eq!(&r.fourcc, b"APAC");
        let layout = back.layout.as_ref().expect("layout survives");
        let face = layout.face(GlyphStyle::Regular).expect("regular layout");
        // Every glyph the tables can output has a cell.
        for &(g, _) in &face.hmtx {
            assert!(back.get_glyph(g).is_some(), "glyph {g} has no cell");
        }
    }
}
