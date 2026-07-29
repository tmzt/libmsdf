//! Compact 32-bit draw command stream.
//!
//! Extracted from matter-stream's `matterstream-mtd1-format` (`Command32`,
//! `BankedStyle`) and `mtd1_to_sdf.rs` (same author), generalized away from
//! the mtd1 container: no FourCC header, no terminal output modes — just
//! the banked styles, the 32-bit ISA, and the lowering into an
//! [`SdfFrame`](crate::drawlist::SdfFrame). This is the wire-compact form a
//! future serialized node-graph can lower text/shape runs through.

use std::collections::HashMap;

use crate::core::sdf::{DRAW_TYPE_BOX, DRAW_TYPE_MSDF_TEXT, DRAW_TYPE_TEXT, SdfDrawCmd};
use crate::drawlist::SdfFrame;

/// Font index that selects bitmap rendering for pictographic glyphs.
pub const FONT_INDEX_PICTOGRAPHIC: u8 = 255;

// ── Style bank ──────────────────────────────────────────────────────────

/// 64-bit style entry: `[32b RGBA][8b Stroke][8b Behavior][8b Shape][8b FontIndex]`
///
/// `FontIndex` selects which font/atlas to use for text rendering:
/// - `0` = legacy bitmap font (backwards compatible default)
/// - `1-254` = index into the glyph atlas bank (MSDF fonts)
/// - `255` = pictographic/emoji (bitmap path)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BankedStyle(pub u64);

impl BankedStyle {
    /// Pack a style from components (font_index defaults to 0 = bitmap).
    pub fn new(rgba: u32, stroke_weight: u8, behavior_id: u8, shape_mode: u8) -> Self {
        Self::with_font(rgba, stroke_weight, behavior_id, shape_mode, 0)
    }

    /// Pack a style with an explicit font index for MSDF atlas selection.
    pub fn with_font(
        rgba: u32,
        stroke_weight: u8,
        behavior_id: u8,
        shape_mode: u8,
        font_index: u8,
    ) -> Self {
        let val = (rgba as u64) << 32
            | (stroke_weight as u64) << 24
            | (behavior_id as u64) << 16
            | (shape_mode as u64) << 8
            | (font_index as u64);
        Self(val)
    }

    pub fn rgba(self) -> u32 {
        (self.0 >> 32) as u32
    }

    pub fn stroke_weight(self) -> u8 {
        ((self.0 >> 24) & 0xFF) as u8
    }

    pub fn behavior_id(self) -> u8 {
        ((self.0 >> 16) & 0xFF) as u8
    }

    pub fn shape_mode(self) -> u8 {
        ((self.0 >> 8) & 0xFF) as u8
    }

    /// Font index: 0 = legacy bitmap, 1+ = MSDF atlas index.
    pub fn font_index(self) -> u8 {
        (self.0 & 0xFF) as u8
    }

    pub fn to_bytes(self) -> [u8; 8] {
        self.0.to_le_bytes()
    }

    pub fn from_bytes(buf: &[u8; 8]) -> Self {
        Self(u64::from_le_bytes(*buf))
    }
}

// ── 32-bit ISA ──────────────────────────────────────────────────────────

/// Opcode constants (4-bit, upper nibble of the u32).
pub mod opcode {
    pub const OP_DRAW_GLYPH: u32 = 0x0;
    pub const OP_DRAW_SHAPE: u32 = 0x1;
    pub const OP_SET_STYLE: u32 = 0x2;
    pub const OP_SET_CURSOR: u32 = 0x3;
    pub const OP_SET_TOKEN: u32 = 0x5;
}

/// A single 32-bit instruction in the draw-stream ISA.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Command32(pub u32);

impl std::fmt::Debug for Command32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Command32(0x{:08X} = {})", self.0, self.disassemble())
    }
}

impl Command32 {
    /// Extract the 4-bit opcode (bits 31..28).
    #[inline]
    pub fn opcode(self) -> u32 {
        self.0 >> 28
    }

    // ── Constructors ────────────────────────────────────────────────────

    /// `OP_DRAW_GLYPH (0x0)`: `[4b Op][12b Advance X][16b Glyph ID]`
    pub fn draw_glyph(advance_x: u16, glyph_id: u16) -> Self {
        let advance = (advance_x as u32) & 0xFFF; // 12 bits
        let gid = glyph_id as u32; // 16 bits
        Self((opcode::OP_DRAW_GLYPH << 28) | (advance << 16) | gid)
    }

    /// `OP_DRAW_SHAPE (0x1)`: `[4b Op][14b Height][14b Width]`
    pub fn draw_shape(height: u16, width: u16) -> Self {
        let h = (height as u32) & 0x3FFF; // 14 bits
        let w = (width as u32) & 0x3FFF; // 14 bits
        Self((opcode::OP_DRAW_SHAPE << 28) | (h << 14) | w)
    }

    /// `OP_SET_STYLE (0x2)`: `[4b Op][28b Style Bank Index]`
    pub fn set_style(index: u32) -> Self {
        Self((opcode::OP_SET_STYLE << 28) | (index & 0x0FFF_FFFF))
    }

    /// `OP_SET_CURSOR (0x3)`: `[4b Op][14b Signed Y][14b Signed X]`
    ///
    /// Y and X are 14-bit two's complement values (-8192..8191).
    pub fn set_cursor(y: i16, x: i16) -> Self {
        let y14 = (y as u32) & 0x3FFF;
        let x14 = (x as u32) & 0x3FFF;
        Self((opcode::OP_SET_CURSOR << 28) | (y14 << 14) | x14)
    }

    /// `OP_SET_TOKEN (0x5)`: `[4b Op][28b Semantic Token ID]`
    pub fn set_token(token_id: u32) -> Self {
        Self((opcode::OP_SET_TOKEN << 28) | (token_id & 0x0FFF_FFFF))
    }

    // ── Decoders ────────────────────────────────────────────────────────

    /// Decode DRAW_GLYPH fields: (advance_x, glyph_id)
    pub fn decode_glyph(self) -> (u16, u16) {
        let advance = ((self.0 >> 16) & 0xFFF) as u16;
        let glyph_id = (self.0 & 0xFFFF) as u16;
        (advance, glyph_id)
    }

    /// Decode DRAW_SHAPE fields: (height, width)
    pub fn decode_shape(self) -> (u16, u16) {
        let h = ((self.0 >> 14) & 0x3FFF) as u16;
        let w = (self.0 & 0x3FFF) as u16;
        (h, w)
    }

    /// Decode SET_STYLE field: style bank index
    pub fn decode_style(self) -> u32 {
        self.0 & 0x0FFF_FFFF
    }

    /// Decode SET_CURSOR fields: (y, x) as signed 14-bit
    pub fn decode_cursor(self) -> (i16, i16) {
        let y_raw = ((self.0 >> 14) & 0x3FFF) as u16;
        let x_raw = (self.0 & 0x3FFF) as u16;
        // Sign-extend 14-bit to i16
        let y = if y_raw & 0x2000 != 0 {
            (y_raw | 0xC000) as i16
        } else {
            y_raw as i16
        };
        let x = if x_raw & 0x2000 != 0 {
            (x_raw | 0xC000) as i16
        } else {
            x_raw as i16
        };
        (y, x)
    }

    /// Decode SET_TOKEN field: semantic token ID
    pub fn decode_token(self) -> u32 {
        self.0 & 0x0FFF_FFFF
    }

    /// Human-readable disassembly string.
    pub fn disassemble(self) -> String {
        match self.opcode() {
            opcode::OP_DRAW_GLYPH => {
                let (adv, gid) = self.decode_glyph();
                format!("DRAW_GLYPH id:{}, adv:{}", gid, adv)
            }
            opcode::OP_DRAW_SHAPE => {
                let (h, w) = self.decode_shape();
                format!("DRAW_SHAPE w:{}, h:{}", w, h)
            }
            opcode::OP_SET_STYLE => {
                format!("SET_STYLE idx:{}", self.decode_style())
            }
            opcode::OP_SET_CURSOR => {
                let (y, x) = self.decode_cursor();
                format!("SET_CURSOR x:{}, y:{}", x, y)
            }
            opcode::OP_SET_TOKEN => {
                format!("SET_TOKEN id:{}", self.decode_token())
            }
            op => format!("UNKNOWN(0x{:X})", op),
        }
    }
}

// ── Stream lowering ─────────────────────────────────────────────────────

/// A style bank + command stream pair (the mtd1 document, minus container).
#[derive(Debug, Clone, Default)]
pub struct CommandStream {
    pub styles: Vec<BankedStyle>,
    pub commands: Vec<Command32>,
}

/// Lower a command stream to a GPU-renderable [`SdfFrame`].
///
/// MSDF (draw type 8) is the default text path for all characters; bitmap
/// (type 4) is reserved for pictographic/emoji glyphs, selected by
/// `font_index == 255` in the style bank.
///
/// `glyph_id_to_table_index` maps font glyph IDs to GPU glyph_table indices.
/// `standard_advances` maps glyph IDs to their standard advance (em-normalized).
///
/// MSDF char_buffer entries: `[16b glyph_table_index | 16b advance_delta_biased]`
/// Bitmap char_buffer entries: `[32b codepoint]`
pub fn lower_stream(
    stream: &CommandStream,
    glyph_id_to_table_index: &HashMap<u16, u16>,
    standard_advances: &HashMap<u16, f32>,
    font_size: f32,
    px_range: f32,
) -> SdfFrame {
    let mut draws = Vec::new();
    let mut char_buffer: Vec<u32> = Vec::new();

    let mut cursor_x: f32 = 0.0;
    let mut cursor_y: f32 = 0.0;
    let mut current_color: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    let mut is_pictographic = false;

    // Line box height matches the atlas cell proportions (1.3 = the 30%
    // safety margin added by the atlas builder).
    let line_box_h = font_size * crate::drawlist::LINE_BOX_RATIO;

    let mut batch_start_x: f32 = 0.0;
    let mut batch_y: f32 = 0.0;
    let mut batch_char_offset: u32 = 0;
    let mut batch_char_count: u32 = 0;
    let mut batch_total_advance: f32 = 0.0;
    let mut batch_color: [f32; 4] = [1.0; 4];
    let mut batch_pictographic = false;
    let mut in_batch = false;

    #[allow(clippy::too_many_arguments)]
    let flush = |draws: &mut Vec<SdfDrawCmd>,
                 start_x: f32,
                 y: f32,
                 char_offset: u32,
                 char_count: u32,
                 total_advance: f32,
                 color: [f32; 4],
                 pictographic: bool| {
        if char_count == 0 {
            return;
        }
        let packed_slot = (char_offset << 16) | (char_count & 0xFFFF);

        if pictographic {
            let total_width = char_count as f32 * font_size * 0.6;
            // Bitmap path for emoji/pictographic
            draws.push(SdfDrawCmd {
                pos: [start_x, y],
                size: [total_width, font_size],
                color,
                params: [DRAW_TYPE_TEXT, 0.0, 0.0, f32::from_bits(packed_slot)],
                xform: [0.0; 4],
            });
        } else {
            // MSDF path — default for all text. The atlas cell has a 15%
            // horizontal margin; expand the box to include it.
            let x_margin_frac = crate::drawlist::X_MARGIN_FRAC;
            let left_margin = x_margin_frac * line_box_h;
            let right_margin = x_margin_frac * line_box_h;
            let box_w = total_advance + left_margin + right_margin;

            draws.push(SdfDrawCmd {
                pos: [start_x - left_margin, y],
                size: [box_w, line_box_h],
                color,
                // params.z = x_margin_frac so the shader can re-derive the pen origin
                params: [DRAW_TYPE_MSDF_TEXT, px_range, x_margin_frac, f32::from_bits(packed_slot)],
                xform: [0.0; 4],
            });
        }
    };

    for cmd in &stream.commands {
        match cmd.opcode() {
            opcode::OP_SET_CURSOR => {
                if in_batch {
                    flush(
                        &mut draws, batch_start_x, batch_y,
                        batch_char_offset, batch_char_count, batch_total_advance,
                        batch_color, batch_pictographic,
                    );
                    in_batch = false;
                }
                let (y, x) = cmd.decode_cursor();
                cursor_x = x as f32;
                cursor_y = y as f32;
            }

            opcode::OP_SET_STYLE => {
                if in_batch {
                    flush(
                        &mut draws, batch_start_x, batch_y,
                        batch_char_offset, batch_char_count, batch_total_advance,
                        batch_color, batch_pictographic,
                    );
                    in_batch = false;
                }
                let idx = cmd.decode_style() as usize;
                if idx < stream.styles.len() {
                    current_color = crate::core::color_u32_to_f32(stream.styles[idx].rgba());
                    is_pictographic = stream.styles[idx].font_index() == FONT_INDEX_PICTOGRAPHIC;
                }
            }

            opcode::OP_DRAW_GLYPH => {
                let (advance, glyph_id) = cmd.decode_glyph();

                // Flush if switching between MSDF and pictographic
                if in_batch && batch_pictographic != is_pictographic {
                    flush(
                        &mut draws, batch_start_x, batch_y,
                        batch_char_offset, batch_char_count, batch_total_advance,
                        batch_color, batch_pictographic,
                    );
                    in_batch = false;
                }

                if !in_batch {
                    batch_start_x = cursor_x;
                    batch_y = cursor_y;
                    batch_char_offset = char_buffer.len() as u32;
                    batch_char_count = 0;
                    batch_total_advance = 0.0;
                    batch_color = current_color;
                    batch_pictographic = is_pictographic;
                    in_batch = true;
                }

                if is_pictographic {
                    // Bitmap: store codepoint directly
                    char_buffer.push(glyph_id as u32);
                } else {
                    // MSDF: pack [glyph_table_index << 16 | advance_delta_biased]
                    let gt_idx = glyph_id_to_table_index
                        .get(&glyph_id)
                        .copied()
                        .unwrap_or(0);
                    let std_advance_norm = standard_advances.get(&glyph_id).copied().unwrap_or(0.5);
                    let std_advance_px = std_advance_norm * font_size;
                    let delta_px = advance as f32 - std_advance_px;
                    let delta_fixed = ((delta_px * 16.0) as i32 + 2048).clamp(0, 0xFFFF) as u32;
                    char_buffer.push((gt_idx as u32) << 16 | delta_fixed);
                }
                batch_char_count += 1;
                batch_total_advance += advance as f32;
                cursor_x += advance as f32;
            }

            opcode::OP_DRAW_SHAPE => {
                if in_batch {
                    flush(
                        &mut draws, batch_start_x, batch_y,
                        batch_char_offset, batch_char_count, batch_total_advance,
                        batch_color, batch_pictographic,
                    );
                    in_batch = false;
                }
                let (h, w) = cmd.decode_shape();
                draws.push(SdfDrawCmd {
                    pos: [cursor_x, cursor_y],
                    size: [w as f32, h as f32],
                    color: current_color,
                    params: [DRAW_TYPE_BOX, 0.0, 0.0, 0.0],
                    xform: [0.0; 4],
                });
            }

            opcode::OP_SET_TOKEN => {}
            _ => {}
        }
    }

    if in_batch {
        flush(
            &mut draws, batch_start_x, batch_y,
            batch_char_offset, batch_char_count, batch_total_advance,
            batch_color, batch_pictographic,
        );
    }

    SdfFrame { draws, char_buffer, param_bank: Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banked_style_roundtrip() {
        let style = BankedStyle::new(0xFF0088AA, 3, 7, 2);
        assert_eq!(style.rgba(), 0xFF0088AA);
        assert_eq!(style.stroke_weight(), 3);
        assert_eq!(style.behavior_id(), 7);
        assert_eq!(style.shape_mode(), 2);

        let bytes = style.to_bytes();
        let parsed = BankedStyle::from_bytes(&bytes);
        assert_eq!(style, parsed);
    }

    #[test]
    fn draw_glyph_encode_decode() {
        let cmd = Command32::draw_glyph(8, 42);
        assert_eq!(cmd.opcode(), opcode::OP_DRAW_GLYPH);
        let (adv, gid) = cmd.decode_glyph();
        assert_eq!(adv, 8);
        assert_eq!(gid, 42);
    }

    #[test]
    fn draw_shape_encode_decode() {
        let cmd = Command32::draw_shape(100, 200);
        assert_eq!(cmd.opcode(), opcode::OP_DRAW_SHAPE);
        let (h, w) = cmd.decode_shape();
        assert_eq!(h, 100);
        assert_eq!(w, 200);
    }

    #[test]
    fn set_cursor_signed() {
        // Positive values
        let cmd = Command32::set_cursor(10, 20);
        assert_eq!(cmd.opcode(), opcode::OP_SET_CURSOR);
        let (y, x) = cmd.decode_cursor();
        assert_eq!(y, 10);
        assert_eq!(x, 20);

        // Negative values
        let cmd = Command32::set_cursor(-50, -100);
        let (y, x) = cmd.decode_cursor();
        assert_eq!(y, -50);
        assert_eq!(x, -100);
    }

    #[test]
    fn set_token_encode_decode() {
        let cmd = Command32::set_token(0x00ABCDEF);
        assert_eq!(cmd.opcode(), opcode::OP_SET_TOKEN);
        assert_eq!(cmd.decode_token(), 0x00ABCDEF);
    }

    #[test]
    fn disassembly_output() {
        let cmd = Command32::set_cursor(10, 20);
        assert_eq!(cmd.disassemble(), "SET_CURSOR x:20, y:10");
    }

    #[test]
    fn default_rendering_is_msdf() {
        let mut stream = CommandStream::default();
        // Style with font_index=1 (MSDF)
        stream.styles.push(BankedStyle::with_font(0xFFFFFFFF, 0, 0, 0, 1));
        stream.commands.push(Command32::set_style(0));
        stream.commands.push(Command32::set_cursor(10, 20));
        stream.commands.push(Command32::draw_glyph(8, 65)); // 'A'
        stream.commands.push(Command32::draw_glyph(8, 66)); // 'B'

        let gid_map = HashMap::from([(65u16, 0u16), (66, 1)]);
        let adv_map = HashMap::from([(65u16, 0.5f32), (66, 0.5)]);
        let frame = lower_stream(&stream, &gid_map, &adv_map, 16.0, 4.0);

        assert!(!frame.draws.is_empty());
        assert!(
            frame.draws.iter().all(|d| d.draw_type() == DRAW_TYPE_MSDF_TEXT || d.draw_type() == DRAW_TYPE_BOX),
            "default text should use MSDF (type 8), not bitmap (type 4)"
        );
    }

    #[test]
    fn pictographic_uses_bitmap() {
        let mut stream = CommandStream::default();
        stream.styles.push(BankedStyle::with_font(0xFFFFFFFF, 0, 0, 0, FONT_INDEX_PICTOGRAPHIC));
        stream.commands.push(Command32::set_style(0));
        stream.commands.push(Command32::set_cursor(10, 20));
        stream.commands.push(Command32::draw_glyph(16, 0xFFFE)); // emoji glyph

        let frame = lower_stream(&stream, &HashMap::new(), &HashMap::new(), 16.0, 4.0);

        let has_bitmap = frame.draws.iter().any(|d| d.draw_type() == DRAW_TYPE_TEXT);
        assert!(has_bitmap, "pictographic glyphs should use bitmap (type 4)");
    }

    #[test]
    fn mixed_msdf_and_pictographic() {
        let mut stream = CommandStream::default();
        stream.styles.push(BankedStyle::with_font(0xFFFFFFFF, 0, 0, 0, 1)); // MSDF
        stream.styles.push(BankedStyle::with_font(0xFFFFFFFF, 0, 0, 0, FONT_INDEX_PICTOGRAPHIC)); // bitmap

        // MSDF text
        stream.commands.push(Command32::set_style(0));
        stream.commands.push(Command32::set_cursor(10, 20));
        stream.commands.push(Command32::draw_glyph(8, 72)); // 'H'

        // Switch to pictographic
        stream.commands.push(Command32::set_style(1));
        stream.commands.push(Command32::draw_glyph(16, 0xFFFE));

        let gid_map = HashMap::from([(72u16, 0u16)]);
        let adv_map = HashMap::from([(72u16, 0.5f32)]);
        let frame = lower_stream(&stream, &gid_map, &adv_map, 16.0, 4.0);

        let msdf_count = frame.draws.iter().filter(|d| d.draw_type() == DRAW_TYPE_MSDF_TEXT).count();
        let bitmap_count = frame.draws.iter().filter(|d| d.draw_type() == DRAW_TYPE_TEXT).count();
        assert_eq!(msdf_count, 1, "should have 1 MSDF draw");
        assert_eq!(bitmap_count, 1, "should have 1 bitmap draw for pictographic");
    }
}
