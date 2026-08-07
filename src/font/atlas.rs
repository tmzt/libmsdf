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
    /// Per-glyph entries indexed by position in the table
    pub glyphs: Vec<GlyphEntry>,
    /// Map from glyph_id to index in `glyphs`
    glyph_index: std::collections::HashMap<u16, usize>,
    /// Fraction from cell top to baseline (e.g. 0.75 = baseline at 75% from top).
    /// Used by the shader to align glyphs on the baseline.
    pub baseline_frac: f32,
}

impl FontAtlas {
    /// Look up a glyph entry by glyph ID.
    pub fn get_glyph(&self, glyph_id: u16) -> Option<&GlyphEntry> {
        self.glyph_index.get(&glyph_id).map(|&idx| &self.glyphs[idx])
    }

    /// Index of a glyph in the GPU glyph table (position in `glyphs`).
    pub fn glyph_table_index(&self, glyph_id: u16) -> Option<usize> {
        self.glyph_index.get(&glyph_id).copied()
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
        let mut glyph_index = std::collections::HashMap::new();
        for i in 0..num_glyphs {
            let offset = entries_start + i * GlyphEntry::PACKED_SIZE;
            let entry = GlyphEntry::from_bytes(&data[offset..offset + GlyphEntry::PACKED_SIZE])?;
            glyph_index.insert(entry.glyph_id, i);
            glyphs.push(entry);
        }

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
            glyph_index,
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
            glyph_index: std::collections::HashMap::new(),
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
    pub fn insert_entry(&mut self, entry: GlyphEntry) -> usize {
        if let Some(&idx) = self.glyph_index.get(&entry.glyph_id) {
            self.glyphs[idx] = entry;
            idx
        } else {
            let idx = self.glyphs.len();
            self.glyph_index.insert(entry.glyph_id, idx);
            self.glyphs.push(entry);
            idx
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

// ── Builder (CPU baking — native-only) ──────────────────────────────────

/// Builder for constructing MSDF font atlases.
///
/// Queueing works on every target; [`FontAtlasBuilder::build`] (the msdfgen
/// CPU bake) is native-only.
pub struct FontAtlasBuilder {
    font_data: Vec<u8>,
    glyph_size: u32,
    px_range: f64,
    /// Queued glyph IDs to generate
    queued_glyphs: Vec<u16>,
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

    /// Queue a single glyph ID for atlas generation.
    pub fn add_glyph(&mut self, glyph_id: u16) {
        if !self.queued_glyphs.contains(&glyph_id) {
            self.queued_glyphs.push(glyph_id);
        }
    }

    /// Queue all glyphs for a codepoint range (resolves via cmap).
    pub fn add_codepoint_range(&mut self, start: char, end: char) {
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
            self.add_glyph(gid);
        }
    }

    /// Queue common Latin + digit + punctuation glyphs.
    ///
    /// cmap-only: this is the glyph a codepoint maps to *in isolation*. Text is
    /// not laid out in isolation — see [`FontAtlasBuilder::add_shaped_ascii`].
    pub fn add_ascii(&mut self) {
        self.add_codepoint_range(' ', '~');
    }

    /// Queue every glyph the SHAPER can emit for printable ASCII, which is a
    /// superset of [`FontAtlasBuilder::add_ascii`]'s cmap lookups.
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
                self.add_glyph(g.glyph_id);
            }
        }
    }

    /// **Queue exactly what a bundled face ships**, so the bake and
    /// [`crate::font::TEXT_RANGES`] cannot disagree about coverage.
    ///
    /// Stated as ranges the face is asked about, never as a list of glyphs:
    /// a face that defines nothing in [`crate::font::PRIVATE_USE`] (plain
    /// Roboto) queues nothing for it, and one that does (the merged icon face)
    /// picks its icons up without this ever learning an icon set.
    ///
    /// **The order is append-only, and that is load-bearing.** Cells are packed
    /// in queue order, so a glyph's atlas coordinates and its glyph-table index
    /// are both fixed by how many glyphs were queued before it. Appending a new
    /// range leaves every existing cell exactly where it was and simply makes
    /// the atlas taller — a re-bake is then a strict superset of the last one,
    /// and a rendered frame that moves is a real finding rather than repacking
    /// noise. Inserting in the middle would renumber everything after it.
    /// Hence Latin-1 sits after the Private Use Area here even though it reads
    /// backwards: ASCII and the icons were baked first.
    pub fn add_shipped_coverage(&mut self) {
        // cmap lookups for printable ASCII...
        debug_assert_eq!(
            crate::font::TEXT_RANGES[0],
            ('\u{0020}', '\u{007E}'),
            "the first declared range is the one `add_ascii` covers, and the tail \
             below is queued on that basis - reordering TEXT_RANGES here would \
             silently drop a range or bake one twice"
        );
        self.add_ascii();
        // ...unioned with what the SHAPER emits for it, which is a superset.
        self.add_shaped_ascii();
        // Our own glyphs.
        let (pua_lo, pua_hi) = crate::font::PRIVATE_USE;
        self.add_codepoint_range(pua_lo, pua_hi);
        // Latin-1 Supplement (appended; see above).
        for &(lo, hi) in &crate::font::TEXT_RANGES[1..] {
            self.add_codepoint_range(lo, hi);
        }
        // **The placeholder.** Queued explicitly because no codepoint maps to
        // it: `cmap` cannot name glyph 0, so no `add_codepoint_range` will ever
        // reach it, and without a cell every uncovered character is back to
        // drawing nothing. Last, so it never shifts a cell that already exists.
        self.add_glyph(0);
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
    pub fn build(&self) -> Result<FontAtlas, String> {
        let face25 = ttf_parser::Face::parse(&self.font_data, 0)
            .map_err(|e| format!("font parse error: {e}"))?;

        let gs = self.glyph_size;
        let padded = gs + 2; // 1px padding on each side
        let channels = 3u32;

        // 8 columns for predictable shelf alignment.
        let cols = 8u32;
        let atlas_w = cols * padded;
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
        for &glyph_id in &self.queued_glyphs {
            let cell = self.bake_cell(glyph_id)?;
            let (x, y) = packer.pack(padded, padded);
            placed.push(Placed { cell, atlas_x: x + 1, atlas_y: y + 1 });
        }

        let atlas_h = packer.used_height().max(1);
        let mut pixel_data = vec![0u8; (atlas_w * atlas_h * channels) as usize];
        let mut glyphs = Vec::with_capacity(placed.len());
        let mut glyph_index = std::collections::HashMap::new();

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
            glyph_index.insert(entry.glyph_id, glyphs.len());
            glyphs.push(entry);
        }

        Ok(FontAtlas {
            width: atlas_w,
            height: atlas_h,
            channels,
            pixel_data,
            glyphs,
            glyph_index,
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
        builder.add_codepoint_range('A', 'Z');
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
}
