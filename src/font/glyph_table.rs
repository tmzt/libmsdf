//! GPU-uploadable glyph entry table.
//!
//! Each `GlyphEntry` maps a glyph ID to its atlas rectangle, standard advance,
//! and the atlas layout metrics needed by the shader.

/// Per-glyph atlas entry, 32 bytes packed.
///
/// Layout metrics for the shader:
/// - `baseline_row`: atlas row (from top of cell) where the baseline sits
/// - `px_per_em`: atlas pixels per em unit (uniform scale for all glyphs)
///
/// The shader maps screen pixels to atlas coordinates:
///   atlas_x = (screen_x - glyph_x) / font_size * px_per_em + x_margin
///   atlas_y = baseline_row - (screen_baseline_y - screen_y) / font_size * px_per_em
///
/// ```text
/// [2b glyph_id][2b atlas_x][2b atlas_y][2b atlas_w][2b atlas_h][2b layer]
/// [4b advance_x][4b baseline_row][4b px_per_em][4b x_margin][4b _pad2]
/// ```
///
/// # The layer went into the two PAD bytes, and that is why no atlas moved
///
/// `layer` occupies bytes 10..12, which were the struct's only 16-bit hole and
/// were written as zero by every bake this repo has ever produced. So the
/// packed size does not change, no offset after it moves, and a file baked
/// before layers existed reads back as `layer: 0` - which is the answer that
/// makes it draw exactly as it did. The same is true of the GPU side: `g1.w`
/// was the literal constant `0` and was read nowhere
/// ([`GlyphEntry::to_gpu_u32s`]), so the table grew a field without growing a
/// byte.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphEntry {
    pub glyph_id: u16,
    pub atlas_x: u16,
    pub atlas_y: u16,
    pub atlas_w: u16,
    pub atlas_h: u16,
    /// **Which texture-array layer holds this cell** - the index the shader
    /// samples at, not a property of the glyph.
    ///
    /// `atlas_x`/`atlas_y` locate the cell inside a layer; this says which
    /// layer. Layer 0 is what every single-layer atlas uses and what a
    /// pre-layer file decodes to, so a zero here is both "the first layer" and
    /// "this file predates layers", and those two mean the same thing.
    ///
    /// See [`crate::font::AtlasLayer`] for what a layer IS - `(point size,
    /// style)` - and [`crate::FontAtlas::layers`] for the table that names
    /// them.
    pub layer: u16,
    /// Standard horizontal advance, normalized to em (0.0–1.0)
    pub advance_x: f32,
    /// Atlas row of the baseline, measured from top of cell (in pixels)
    pub baseline_row: f32,
    /// Atlas pixels per em (uniform scale factor)
    pub px_per_em: f32,
    /// X margin offset in atlas cell (left padding)
    pub x_margin: f32,
}

impl GlyphEntry {
    pub const PACKED_SIZE: usize = 32;

    pub fn to_bytes(&self) -> [u8; 32] {
        let mut buf = [0u8; 32];
        buf[0..2].copy_from_slice(&self.glyph_id.to_le_bytes());
        buf[2..4].copy_from_slice(&self.atlas_x.to_le_bytes());
        buf[4..6].copy_from_slice(&self.atlas_y.to_le_bytes());
        buf[6..8].copy_from_slice(&self.atlas_w.to_le_bytes());
        buf[8..10].copy_from_slice(&self.atlas_h.to_le_bytes());
        buf[10..12].copy_from_slice(&self.layer.to_le_bytes());
        buf[12..16].copy_from_slice(&self.advance_x.to_le_bytes());
        buf[16..20].copy_from_slice(&self.baseline_row.to_le_bytes());
        buf[20..24].copy_from_slice(&self.px_per_em.to_le_bytes());
        buf[24..28].copy_from_slice(&self.x_margin.to_le_bytes());
        // 28..32 pad
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 32 {
            return Err("glyph entry too short");
        }
        Ok(Self {
            glyph_id: u16::from_le_bytes([data[0], data[1]]),
            atlas_x: u16::from_le_bytes([data[2], data[3]]),
            atlas_y: u16::from_le_bytes([data[4], data[5]]),
            atlas_w: u16::from_le_bytes([data[6], data[7]]),
            atlas_h: u16::from_le_bytes([data[8], data[9]]),
            // Zero in every atlas baked before layers existed, which is layer
            // 0 - see the type's doc for why that is not a coincidence.
            layer: u16::from_le_bytes([data[10], data[11]]),
            advance_x: f32::from_le_bytes([data[12], data[13], data[14], data[15]]),
            baseline_row: f32::from_le_bytes([data[16], data[17], data[18], data[19]]),
            px_per_em: f32::from_le_bytes([data[20], data[21], data[22], data[23]]),
            x_margin: f32::from_le_bytes([data[24], data[25], data[26], data[27]]),
        })
    }

    /// GPU packing: 2 x vec4<u32> = 32 bytes.
    /// g0 = [glyph_id, atlas_xy, atlas_wh, advance_x]
    /// g1 = [baseline_row, px_per_em, x_margin, layer]
    ///
    /// **`g1.w` was `0` and read nowhere.** The layer index went there rather
    /// than into the draw list, which is what keeps the draw list from ever
    /// learning that textures are plural: it packs a 16-bit glyph id, the table
    /// answers with a cell AND the layer that cell is in, and the instance
    /// stream is byte-for-byte what it was.
    pub fn to_gpu_u32s(&self) -> [u32; 8] {
        [
            self.glyph_id as u32,
            (self.atlas_x as u32) | ((self.atlas_y as u32) << 16),
            (self.atlas_w as u32) | ((self.atlas_h as u32) << 16),
            self.advance_x.to_bits(),
            self.baseline_row.to_bits(),
            self.px_per_em.to_bits(),
            self.x_margin.to_bits(),
            self.layer as u32,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_entry_roundtrip() {
        let entry = GlyphEntry {
            glyph_id: 42, atlas_x: 100, atlas_y: 200, atlas_w: 128, atlas_h: 128,
            layer: 3,
            advance_x: 0.58, baseline_row: 80.0, px_per_em: 112.0, x_margin: 8.0,
        };
        let bytes = entry.to_bytes();
        let parsed = GlyphEntry::from_bytes(&bytes).unwrap();
        assert_eq!(entry, parsed);
    }

    #[test]
    fn gpu_packing() {
        let entry = GlyphEntry {
            glyph_id: 65, atlas_x: 10, atlas_y: 20, atlas_w: 128, atlas_h: 128,
            layer: 2,
            advance_x: 0.6, baseline_row: 80.0, px_per_em: 112.0, x_margin: 8.0,
        };
        let packed = entry.to_gpu_u32s();
        assert_eq!(packed[0], 65);
        assert_eq!(packed[1] & 0xFFFF, 10);
        assert_eq!(packed[4], 80.0f32.to_bits());
        assert_eq!(packed[7], 2, "the layer must arrive in g1.w");
    }

    /// **A pre-layer entry decodes to layer 0.** Bytes 10..12 were pad and
    /// every bake wrote them as zero, so this is not a compatibility shim - it
    /// is the same bytes meaning the same thing. Written as a test because the
    /// property is what lets three committed atlases stay byte-identical
    /// through a format that grew a field.
    #[test]
    fn a_pre_layer_entry_is_layer_zero() {
        let entry = GlyphEntry {
            glyph_id: 7, atlas_x: 1, atlas_y: 1, atlas_w: 48, atlas_h: 48,
            layer: 0,
            advance_x: 0.5, baseline_row: 33.45, px_per_em: 36.923, x_margin: 7.2,
        };
        let bytes = entry.to_bytes();
        assert_eq!(&bytes[10..12], &[0, 0], "layer 0 must write the old pad bytes");
        assert_eq!(GlyphEntry::from_bytes(&bytes).unwrap().layer, 0);
        assert_eq!(entry.to_gpu_u32s()[7], 0, "g1.w must still be 0 for layer 0");
    }
}
