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

use crate::font::{CellKey, GlyphSet};
use crate::font::glyph_table::GlyphEntry;
#[cfg(all(feature = "cpu-bake", not(target_arch = "wasm32")))]
use crate::font::packer::ShelfPacker;

/// MSDF atlas containing packed glyph bitmaps and their metrics.
#[derive(Debug, Clone)]
pub struct FontAtlas {
    /// Atlas texture width in pixels
    pub width: u32,
    /// Atlas texture height in pixels
    pub height: u32,
    /// Number of channels (3 = MSDF, 4 = MTSDF)
    pub channels: u32,
    /// Raw pixel data: width * height * channels bytes, row-major
    pub pixel_data: Vec<u8>,
    /// **Per-glyph entries, ordered by glyph id** — see
    /// [`FontAtlas::get_glyph`] for why that specific order, and
    /// [`crate::font::GlyphSet`] for why it is not the order the CELLS are in.
    pub glyphs: Vec<GlyphEntry>,
    /// Fraction from cell top to baseline (e.g. 0.75 = baseline at 75% from top).
    /// Used by the shader to align glyphs on the baseline.
    pub baseline_frac: f32,
}

impl FontAtlas {
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

    /// Expand the pixel data to RGBA8 (alpha = 255) for texture upload.
    pub fn to_rgba_bytes(&self) -> Vec<u8> {
        let texels = (self.width * self.height) as usize;
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
    /// Format:
    /// ```text
    /// [4b width][4b height][4b num_glyphs][4b channels]  -- 16 byte header
    /// [GlyphEntry * num_glyphs]                          -- 32 bytes each
    /// [pixel_data]                                       -- width*height*channels bytes
    /// ```
    pub fn to_bytes(&self) -> Vec<u8> {
        let header_size = 16;
        let entries_size = self.glyphs.len() * GlyphEntry::PACKED_SIZE;
        let pixel_size = self.pixel_data.len();
        let total = header_size + entries_size + pixel_size;

        let mut buf = Vec::with_capacity(total);
        buf.extend_from_slice(&self.width.to_le_bytes());
        buf.extend_from_slice(&self.height.to_le_bytes());
        buf.extend_from_slice(&(self.glyphs.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.channels.to_le_bytes());

        for entry in &self.glyphs {
            buf.extend_from_slice(&entry.to_bytes());
        }

        buf.extend_from_slice(&self.pixel_data);
        buf
    }

    /// Deserialize from bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 16 {
            return Err("atlas data too short");
        }

        let width = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let height = u32::from_le_bytes(data[4..8].try_into().unwrap());
        let num_glyphs = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        let channels = u32::from_le_bytes(data[12..16].try_into().unwrap());

        let entries_start = 16;
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

        let pixel_start = entries_end;
        let expected_pixels = (width * height * channels) as usize;
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
            baseline_frac: 0.75, // default for deserialized atlases
        })
    }

    /// Build an atlas shell (no pixel data yet) for dynamic population via
    /// the atlas manager + compute-MSDF path.
    pub fn empty(width: u32, height: u32, channels: u32) -> Self {
        Self {
            width,
            height,
            channels,
            pixel_data: vec![0; (width * height * channels) as usize],
            glyphs: Vec::new(),
            baseline_frac: 0.75,
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
    let em_scale = gs / (upem * 1.3);

    let gid = ttf_parser::GlyphId(glyph_id);
    let advance_x = face.glyph_hor_advance(gid).unwrap_or(0) as f32 / upem as f32;

    let cap_y_max = face
        .glyph_index('A')
        .and_then(|a| face.glyph_bounding_box(a))
        .map(|b| b.y_max as f64)
        .unwrap_or(upem * 0.7);

    let bbox = face.glyph_bounding_box(gid)?;

    // Horizontal: center the ink in the cell.
    let ink_w = (bbox.x_max - bbox.x_min) as f64 * em_scale;
    let target_px_x = (gs - ink_w) * 0.5;
    let g_tx = target_px_x / em_scale - bbox.x_min as f64;

    // Vertical: align the font's cap height to 15% from the cell top so all
    // digits and caps share a level top boundary.
    let target_top_px = gs * 0.15;
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

/// Default cell metrics for glyphs without outlines (whitespace):
/// (baseline_row, x_margin, px_per_em).
pub fn default_cell_metrics(face: &ttf_parser::Face, glyph_size: u32) -> (f32, f32, f32) {
    let gs = glyph_size as f64;
    let upem = face.units_per_em() as f64;
    let em_scale = gs / (upem * 1.3);
    (
        (gs * 0.75) as f32,
        (gs * 0.15) as f32,
        (em_scale * upem) as f32,
    )
}

// ── The pinned atlas grid ───────────────────────────────────────────────

/// Columns of glyph cells. Fixed, for predictable shelf alignment — and
/// because a texture whose WIDTH changed would move every glyph's `u` in
/// `sdf_render.wgsl`, exactly as a changing height moves every `v`.
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
/// 40 rows x [`ATLAS_COLS`] = **320 cells**. At the shipped 48px cell that is
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
pub const ATLAS_ROWS: u32 = 40;

/// Glyph cells one baked atlas holds: [`ATLAS_ROWS`] x [`ATLAS_COLS`].
/// [`FontAtlasBuilder::build`] fails rather than silently growing past it.
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
        }
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
    pub fn add_codepoint_range(&mut self, set: GlyphSet, start: char, end: char) {
        let face = match ttf_parser::Face::parse(&self.font_data, 0) {
            Ok(f) => f,
            Err(_) => return,
        };

        let mut gids = Vec::new();
        for cp in (start as u32)..=(end as u32) {
            if let Some(ch) = char::from_u32(cp) {
                if let Some(gid) = face.glyph_index(ch) {
                    gids.push(gid.0);
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
        let Ok(shaper) = crate::font::shaper::TextShaper::new(self.font_data.clone()) else {
            return;
        };
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

        for probe in probes {
            for g in shaper.shape(&probe).glyphs {
                self.add_glyph(GlyphSet::ShapedText, g.glyph_id);
            }
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
        self.add_glyph(GlyphSet::Placeholder, 0);
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
        use msdfgen::{Bitmap, FillRule, FontExt, Framing, MsdfGeneratorConfig, Rgb};

        // ttf-parser 0.25 for metrics; 0.18 for msdfgen's FontExt.
        let face25 = ttf_parser::Face::parse(&self.font_data, 0)
            .map_err(|e| format!("font parse error: {e}"))?;
        let face18 = ttf_parser_018::Face::parse(&self.font_data, 0)
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
            glyph_id,
            atlas_x: 0,
            atlas_y: 0,
            atlas_w: gs as u16,
            atlas_h: gs as u16,
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

        let gs = self.glyph_size;
        let padded = gs + 2; // 1px padding on each side
        let channels = 3u32;

        let atlas_w = ATLAS_COLS * padded;
        let mut packer = ShelfPacker::new(atlas_w);

        let baseline_frac = {
            let (baseline_def, _, _) = default_cell_metrics(&face25, gs);
            baseline_def / gs as f32
        };

        struct Placed {
            cell: BakedGlyphCell,
            atlas_x: u32,
            atlas_y: u32,
        }

        let mut placed = Vec::with_capacity(self.queued_glyphs.len());
        for key in self.cell_order() {
            let glyph_id = key.glyph_id();
            let cell = self.bake_cell(glyph_id)?;
            let (x, y) = packer.pack(padded, padded);
            placed.push(Placed { cell, atlas_x: x + 1, atlas_y: y + 1 });
        }

        if placed.len() > atlas_capacity() {
            return Err(format!(
                "{} glyphs queued, but the atlas is pinned at {} cells ({ATLAS_ROWS} rows x \
                 {ATLAS_COLS} columns) - see ATLAS_ROWS before raising it",
                placed.len(),
                atlas_capacity()
            ));
        }
        // **Pinned, not fitted** — see [`ATLAS_ROWS`]. `used_height` is still the
        // floor for a degenerate bake with a huge cell, which cannot happen at
        // the shipped 48px but is not worth being wrong about.
        let atlas_h = packer.used_height().max(ATLAS_ROWS * padded).max(1);
        let mut pixel_data = vec![0u8; (atlas_w * atlas_h * channels) as usize];
        let mut glyphs = Vec::with_capacity(placed.len());

        for p in &mut placed {
            for row in 0..gs {
                for col in 0..gs {
                    let src_idx = ((row * gs + col) * channels) as usize;
                    let dst_idx =
                        (((p.atlas_y + row) * atlas_w + p.atlas_x + col) * channels) as usize;
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
            baseline_frac,
        })
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
            glyph_id: 7, atlas_x: 1, atlas_y: 1, atlas_w: 32, atlas_h: 32,
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
            glyph_id, atlas_x: 1, atlas_y: 1, atlas_w: 32, atlas_h: 32,
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
