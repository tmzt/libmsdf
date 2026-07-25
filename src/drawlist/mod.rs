//! `DrawList` — the public render contract consumed by highbay_ui.
//!
//! A `DrawList` is a retained list of SDF/MSDF instances with per-instance
//! style. It lowers to an [`SdfFrame`] (packed `SdfDrawCmd`s + char buffer +
//! aux param bank) which the renderer draws in ONE full-screen-triangle
//! pass — the "single instanced draw path". The caller owns the wgpu
//! device/queue throughout (`GpuSdfRenderer` never creates them).
//!
//! Generalized from matter-stream's mtd1 lowering (`mtd1_to_sdf.rs`, same
//! author) away from the mtd1 container toward the Phase-7 node-graph:
//! highbay_ui's layout pass emits a `DrawList` per frame.

pub mod stream;

use crate::core::sdf::{
    DRAW_TYPE_BEZIER, DRAW_TYPE_BOX, DRAW_TYPE_CIRCLE, DRAW_TYPE_LINE, DRAW_TYPE_MSDF_TEXT,
    DRAW_TYPE_OUTLINE, DRAW_TYPE_RIBBON_BEGIN, DRAW_TYPE_RIBBON_END, DRAW_TYPE_SLAB,
    DRAW_TYPE_SLAB_PC, SdfDrawCmd,
};
use crate::font::atlas::FontAtlas;
use crate::font::shaper::ShapedRun;

/// Horizontal ink margin of an atlas cell as a fraction of the line box
/// (matches the 15% margin baked by `FontAtlasBuilder`).
pub const X_MARGIN_FRAC: f32 = 0.15;

/// Line box height as a multiple of font size (matches the 1.3× em-square
/// safety margin baked into atlas cells).
pub const LINE_BOX_RATIO: f32 = 1.3;

/// The SDF shape an instance renders. Geometry parameters that aren't the
/// bounding box live here; color/animation are per-instance fields.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SdfKind {
    /// Filled box; `position`/`size` are the rect.
    Box,
    /// Filled rounded box with corner `radius`.
    RoundedBox { radius: f32 },
    /// Filled rounded box with independent per-corner radii, `[top-left,
    /// top-right, bottom-right, bottom-left]` in screen space (+x right, +y
    /// down). The M3 modal navigation drawer uses this: square against the
    /// screen edge, rounded on the exposed trailing side. Lowers through the
    /// aux param bank like [`SdfKind::BezierStroke`].
    RoundedBoxPerCorner { radii: [f32; 4] },
    /// Filled circle inscribed in `size` (radius = min(w,h)/2).
    Circle,
    /// Horizontal line across the rect: length = size.x, thickness = size.y.
    Line,
    /// Rounded-rect stroke (no fill). NOTE: `thickness` occupies the anim
    /// param slot in the wire format — outlines don't animate.
    Outline { radius: f32, thickness: f32 },
    /// Cubic Bézier stroke from `position` to `end` (absolute coords) with
    /// control points `c1`, `c2` — nav-graph containment/flow arcs.
    BezierStroke {
        c1: [f32; 2],
        c2: [f32; 2],
        end: [f32; 2],
        thickness: f32,
    },
    /// A shaped MSDF text run referencing `char_count` packed entries at
    /// `char_start` in the list's char buffer. Produced by
    /// [`DrawList::push_shaped_text`].
    MsdfText {
        char_start: u32,
        char_count: u32,
        px_range: f32,
    },
    /// Begin a rectangular scissor clip: every instance pushed *after* this
    /// one (until the matching [`SdfKind::ClipEnd`]) is clipped to the rect
    /// `position`/`size`. Lets a caller confine a sub-scene — e.g. a device
    /// "screen" — so its content, text included, cannot spill past the rect.
    /// Lowers to the shader's ribbon-clip pass (no per-instance wire change).
    /// Clips are a single active region, not a stack — do not nest.
    ClipBegin,
    /// End the current [`SdfKind::ClipBegin`] region.
    ClipEnd,
}

/// One instance in the draw list.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SdfInstance {
    pub kind: SdfKind,
    /// Top-left position in logical pixels (Bézier: start point; text: the
    /// line box origin as computed by `push_shaped_text`).
    pub position: [f32; 2],
    pub size: [f32; 2],
    /// Straight RGBA, 0.0–1.0.
    pub color: [f32; 4],
    /// Anim bank index (0 = none, 1+ = AnimBank[idx-1]).
    pub anim: u32,
}

/// GPU-ready lowered frame: what [`DrawList::lower`] produces and
/// `GpuSdfRenderer` consumes.
#[derive(Clone, Debug, Default)]
pub struct SdfFrame {
    pub draws: Vec<SdfDrawCmd>,
    pub char_buffer: Vec<u32>,
    pub param_bank: Vec<[f32; 4]>,
}

/// Retained draw list: instances + packed text chars + aux params.
#[derive(Clone, Debug, Default)]
pub struct DrawList {
    pub instances: Vec<SdfInstance>,
    chars: Vec<u32>,
}

impl DrawList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.instances.clear();
        self.chars.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// The packed MSDF char entries backing `MsdfText` instances.
    pub fn chars(&self) -> &[u32] {
        &self.chars
    }

    /// Push a shape instance.
    pub fn push(&mut self, instance: SdfInstance) {
        self.instances.push(instance);
    }

    /// Convenience: push a cubic Bézier stroke from `p0` to `p3`.
    pub fn push_bezier(
        &mut self,
        p0: [f32; 2],
        c1: [f32; 2],
        c2: [f32; 2],
        p3: [f32; 2],
        thickness: f32,
        color: [f32; 4],
    ) {
        self.instances.push(SdfInstance {
            kind: SdfKind::BezierStroke { c1, c2, end: p3, thickness },
            position: p0,
            size: [0.0, 0.0],
            color,
            anim: 0,
        });
    }

    /// Begin a rectangular scissor clip at `pos`/`size`. Instances pushed
    /// after this (until [`DrawList::push_clip_end`]) are clipped to the rect.
    /// See [`SdfKind::ClipBegin`].
    pub fn push_clip(&mut self, pos: [f32; 2], size: [f32; 2]) {
        self.instances.push(SdfInstance {
            kind: SdfKind::ClipBegin,
            position: pos,
            size,
            color: [0.0, 0.0, 0.0, 0.0],
            anim: 0,
        });
    }

    /// End the current [`DrawList::push_clip`] region.
    pub fn push_clip_end(&mut self) {
        self.instances.push(SdfInstance {
            kind: SdfKind::ClipEnd,
            position: [0.0, 0.0],
            size: [0.0, 0.0],
            color: [0.0, 0.0, 0.0, 0.0],
            anim: 0,
        });
    }

    /// Push a shaped text run as an MSDF text instance.
    ///
    /// `pos` is the pen origin: x = first glyph origin, y = top of the line
    /// box (`font_size * LINE_BOX_RATIO` tall). Glyph advances come from
    /// the shaper (kerning included), encoded as deltas against the atlas's
    /// standard advances exactly like the extracted mtd1 lowering. Glyphs
    /// missing from the atlas fall back to table index 0.
    ///
    /// Returns the width of the run in pixels.
    pub fn push_shaped_text(
        &mut self,
        run: &ShapedRun,
        atlas: &FontAtlas,
        pos: [f32; 2],
        font_size: f32,
        px_range: f32,
        color: [f32; 4],
    ) -> f32 {
        let char_start = self.chars.len() as u32;
        let scale = font_size / run.units_per_em as f32;
        let mut total_advance = 0.0f32;
        let mut count = 0u32;

        for g in &run.glyphs {
            let advance_px = g.x_advance as f32 * scale;
            let (table_idx, std_advance_norm) = match atlas.glyph_table_index(g.glyph_id) {
                Some(idx) => (idx as u32, atlas.glyphs[idx].advance_x),
                None => (0, 0.5),
            };
            let std_advance_px = std_advance_norm * font_size;
            let delta_px = advance_px - std_advance_px;
            // Fixed-point 1/16 px, biased by 2048 (±128 px), like upstream.
            let delta_fixed = ((delta_px * 16.0) as i32 + 2048).clamp(0, 0xFFFF) as u32;
            self.chars.push((table_idx << 16) | delta_fixed);
            total_advance += advance_px;
            count += 1;
        }

        let line_box_h = font_size * LINE_BOX_RATIO;
        let left_margin = X_MARGIN_FRAC * line_box_h;
        let box_w = total_advance + 2.0 * left_margin;

        self.instances.push(SdfInstance {
            kind: SdfKind::MsdfText { char_start, char_count: count, px_range },
            // The shader recovers the pen origin via pos.x + margin.
            position: [pos[0] - left_margin, pos[1]],
            size: [box_w, line_box_h],
            color,
            anim: 0,
        });

        total_advance
    }

    /// Lower to the GPU wire format: one `SdfDrawCmd` per instance, plus
    /// the packed char buffer and the aux param bank (Bézier controls).
    pub fn lower(&self) -> SdfFrame {
        let mut draws = Vec::with_capacity(self.instances.len());
        let mut param_bank: Vec<[f32; 4]> = Vec::new();

        for inst in &self.instances {
            let anim = inst.anim as f32;
            let (pos, size, params) = match inst.kind {
                SdfKind::Box => (inst.position, inst.size, [DRAW_TYPE_BOX, 0.0, anim, 0.0]),
                SdfKind::RoundedBox { radius } => {
                    (inst.position, inst.size, [DRAW_TYPE_SLAB, radius, anim, 0.0])
                }
                SdfKind::RoundedBoxPerCorner { radii } => {
                    let idx = param_bank.len() as u32;
                    param_bank.push(radii);
                    (
                        inst.position,
                        inst.size,
                        [DRAW_TYPE_SLAB_PC, 0.0, anim, f32::from_bits(idx)],
                    )
                }
                SdfKind::Circle => {
                    let radius = inst.size[0].min(inst.size[1]) * 0.5;
                    (inst.position, inst.size, [DRAW_TYPE_CIRCLE, radius, anim, 0.0])
                }
                SdfKind::Line => (inst.position, inst.size, [DRAW_TYPE_LINE, 0.0, anim, 0.0]),
                SdfKind::Outline { radius, thickness } => (
                    inst.position,
                    inst.size,
                    // params.z carries thickness in the wire format (shader
                    // quirk inherited from upstream) — anim unsupported.
                    [DRAW_TYPE_OUTLINE, radius, thickness, 0.0],
                ),
                SdfKind::BezierStroke { c1, c2, end, thickness } => {
                    let idx = param_bank.len() as u32;
                    param_bank.push([c1[0], c1[1], c2[0], c2[1]]);
                    (
                        inst.position,
                        end,
                        [DRAW_TYPE_BEZIER, thickness, anim, f32::from_bits(idx)],
                    )
                }
                SdfKind::MsdfText { char_start, char_count, px_range } => {
                    let packed = (char_start << 16) | (char_count & 0xFFFF);
                    (
                        inst.position,
                        inst.size,
                        [DRAW_TYPE_MSDF_TEXT, px_range, X_MARGIN_FRAC, f32::from_bits(packed)],
                    )
                }
                // Clip control commands: pos/size carry the ribbon rect;
                // params.y (scroll slot) = 0 and params.z (dir) = 0 mean a
                // static, non-scrolling clip in the shader.
                SdfKind::ClipBegin => {
                    (inst.position, inst.size, [DRAW_TYPE_RIBBON_BEGIN, 0.0, 0.0, 0.0])
                }
                SdfKind::ClipEnd => {
                    (inst.position, inst.size, [DRAW_TYPE_RIBBON_END, 0.0, 0.0, 0.0])
                }
            };
            draws.push(SdfDrawCmd { pos, size, color: inst.color, params });
        }

        SdfFrame {
            draws,
            char_buffer: self.chars.clone(),
            param_bank,
        }
    }
}

impl SdfFrame {
    /// Wrap into a full [`crate::core::RenderFrame`] at the given size.
    pub fn into_render_frame(self, width: u32, height: u32) -> crate::core::RenderFrame {
        let mut frame = crate::core::RenderFrame::new(width, height);
        frame.draws = self.draws;
        frame.char_buffer = self.char_buffer;
        frame.param_bank = self.param_bank;
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_maps_kinds_to_draw_types() {
        let mut list = DrawList::new();
        list.push(SdfInstance {
            kind: SdfKind::RoundedBox { radius: 8.0 },
            position: [10.0, 20.0],
            size: [100.0, 50.0],
            color: [1.0, 0.0, 0.0, 1.0],
            anim: 0,
        });
        list.push(SdfInstance {
            kind: SdfKind::Circle,
            position: [0.0, 0.0],
            size: [40.0, 60.0],
            color: [0.0, 1.0, 0.0, 1.0],
            anim: 2,
        });
        list.push_bezier([0.0, 0.0], [10.0, 0.0], [20.0, 10.0], [30.0, 10.0], 3.0, [1.0; 4]);

        let frame = list.lower();
        assert_eq!(frame.draws.len(), 3);
        assert_eq!(frame.draws[0].params[0], DRAW_TYPE_SLAB);
        assert_eq!(frame.draws[0].params[1], 8.0);
        assert_eq!(frame.draws[1].params[0], DRAW_TYPE_CIRCLE);
        assert_eq!(frame.draws[1].params[1], 20.0, "circle radius = min(w,h)/2");
        assert_eq!(frame.draws[1].params[2], 2.0, "anim index carried");
        assert_eq!(frame.draws[2].params[0], DRAW_TYPE_BEZIER);
        assert_eq!(frame.param_bank.len(), 1);
        assert_eq!(frame.param_bank[0], [10.0, 0.0, 20.0, 10.0]);
        assert_eq!(frame.draws[2].size, [30.0, 10.0], "size slot = P3");
    }

    #[test]
    fn per_corner_slab_lowers_through_param_bank() {
        let mut list = DrawList::new();
        list.push(SdfInstance {
            kind: SdfKind::RoundedBoxPerCorner { radii: [0.0, 16.0, 16.0, 0.0] },
            position: [4.0, 8.0],
            size: [200.0, 400.0],
            color: [0.5; 4],
            anim: 3,
        });
        let frame = list.lower();
        assert_eq!(frame.draws[0].params[0], DRAW_TYPE_SLAB_PC);
        assert_eq!(frame.draws[0].params[2], 3.0, "anim index carried");
        // params.w bitcasts to the aux-bank index holding the four radii.
        let idx = frame.draws[0].params[3].to_bits() as usize;
        assert_eq!(frame.param_bank[idx], [0.0, 16.0, 16.0, 0.0]);
    }

    #[test]
    fn bezier_param_indices_are_dense() {
        let mut list = DrawList::new();
        for i in 0..3 {
            let y = i as f32 * 10.0;
            list.push_bezier([0.0, y], [5.0, y], [10.0, y], [15.0, y], 2.0, [1.0; 4]);
        }
        let frame = list.lower();
        for (i, d) in frame.draws.iter().enumerate() {
            assert_eq!(d.params[3].to_bits(), i as u32);
        }
        assert_eq!(frame.param_bank.len(), 3);
    }

    #[test]
    fn clip_maps_to_ribbon_control_commands() {
        let mut list = DrawList::new();
        list.push_clip([10.0, 20.0], [100.0, 50.0]);
        list.push(SdfInstance {
            kind: SdfKind::Box,
            position: [12.0, 22.0],
            size: [5.0, 5.0],
            color: [1.0; 4],
            anim: 0,
        });
        list.push_clip_end();

        let frame = list.lower();
        assert_eq!(frame.draws.len(), 3);
        // ClipBegin → ribbon begin, carrying the clip rect in pos/size.
        assert_eq!(frame.draws[0].params[0], DRAW_TYPE_RIBBON_BEGIN);
        assert_eq!(frame.draws[0].pos, [10.0, 20.0]);
        assert_eq!(frame.draws[0].size, [100.0, 50.0]);
        // No scroll: the scroll slot + direction params are zero.
        assert_eq!(frame.draws[0].params[1], 0.0);
        assert_eq!(frame.draws[0].params[2], 0.0);
        // The clipped shape passes through unchanged.
        assert_eq!(frame.draws[1].params[0], DRAW_TYPE_BOX);
        // ClipEnd → ribbon end.
        assert_eq!(frame.draws[2].params[0], DRAW_TYPE_RIBBON_END);
    }

    #[test]
    fn shaped_text_packs_chars() {
        let shaper = crate::font::TextShaper::new(crate::font::ROBOTO_REGULAR_ASCII.to_vec())
            .expect("fixture parses");
        // Atlas without real pixels: entries only, enough for packing.
        let mut atlas = FontAtlas::empty(64, 64, 3);
        for ch in ['H', 'i'] {
            let gid = shaper.glyph_id_for_char(ch).unwrap();
            atlas.insert_entry(crate::font::GlyphEntry {
                glyph_id: gid,
                atlas_x: 1, atlas_y: 1, atlas_w: 48, atlas_h: 48,
                advance_x: 0.5, baseline_row: 36.0, px_per_em: 36.9, x_margin: 7.2,
            });
        }

        let mut list = DrawList::new();
        let run = shaper.shape("Hi");
        let width = list.push_shaped_text(&run, &atlas, [100.0, 50.0], 16.0, 4.0, [1.0; 4]);
        assert!(width > 0.0);
        assert_eq!(list.chars().len(), 2);

        let frame = list.lower();
        assert_eq!(frame.draws.len(), 1);
        let d = &frame.draws[0];
        assert_eq!(d.params[0], DRAW_TYPE_MSDF_TEXT);
        // Packed slot: offset 0, count 2.
        assert_eq!(d.params[3].to_bits(), 2);
        // Line box height honors the ratio; x shifted left by the margin.
        assert_eq!(d.size[1], 16.0 * LINE_BOX_RATIO);
        assert!(d.pos[0] < 100.0);
        // Char entries reference valid table indices.
        for c in frame.char_buffer {
            assert!(((c >> 16) as usize) < atlas.glyphs.len());
        }
    }
}
