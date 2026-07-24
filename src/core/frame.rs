//! Render pipeline types.
//!
//! `RenderFrame` is a fully prepared frame — all strings packed, all offsets
//! encoded, all data GPU-uploadable. Produced by the drawlist lowering,
//! consumed by `GpuSdfRenderer::render_frame`.

use crate::core::sdf::{Anim, GpuTexture, SdfDrawCmd};
use crate::core::text::GpuFont;

/// Fully prepared frame — input to the render stage.
pub struct RenderFrame {
    pub draws: Vec<SdfDrawCmd>,        // string offsets already in params[3]
    pub char_buffer: Vec<u32>,         // packed codepoints / glyph refs
    pub param_bank: Vec<[f32; 4]>,     // aux per-instance data (Bézier control points)
    pub anim_bank: Vec<Anim>,
    pub texture_bank: Vec<GpuTexture>, // texture descriptors
    pub font: GpuFont,
    pub glyph_bitmap: Vec<u32>,        // packed bitmap
    pub scalar_bank: [f32; 16],
    pub int_bank: [i32; 16],
    pub time_ms: f32,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}

impl RenderFrame {
    /// An empty frame at the given target size.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            draws: Vec::new(),
            char_buffer: Vec::new(),
            param_bank: Vec::new(),
            anim_bank: Vec::new(),
            texture_bank: Vec::new(),
            font: GpuFont::NONE,
            glyph_bitmap: Vec::new(),
            scalar_bank: [0.0; 16],
            int_bank: [0; 16],
            time_ms: 0.0,
            width,
            height,
            scale: 1.0,
        }
    }
}
