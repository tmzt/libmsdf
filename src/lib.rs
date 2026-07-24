//! libmsdf — wgpu SDF/MSDF render engine + typography (Phase 1 contract stubs;
//! Phase 5 extracts matter-stream's render stack and adds the wasm32/WebGPU
//! port plus runtime compute-shader MSDF generation).

#![forbid(unsafe_code)]

/// Human-readable crate status, printed by the root `highbay` bin.
pub const PHASE_STATUS: &str = "Phase 1 contract stubs (Phase 5: wgpu SDF/MSDF engine)";

/// The SDF shape an instance renders (mirrors the extracted shader's coverage;
/// Bézier strokes feed the Phase 8 nav-graph arcs).
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SdfKind {
    Box,
    RoundedBox,
    Circle,
    Line,
    /// Cubic Bézier stroke (containment / flow arcs).
    BezierStroke,
    /// MSDF glyph, indexed into the font atlas glyph table.
    MsdfGlyph { glyph: u32 },
}

/// One GPU instance in the SDF pipeline.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SdfInstance {
    pub kind: SdfKind,
    /// Top-left position in logical pixels.
    pub position: [f32; 2],
    pub size: [f32; 2],
    /// Corner radius (RoundedBox) or stroke width (Line/BezierStroke).
    pub radius: f32,
    /// Premultiplied RGBA.
    pub color: [f32; 4],
}

/// The frame contract consumed by the renderer and produced by highbay_ui's
/// node-graph lowering (Phase 7). Caller-owned; renderer never retains it.
#[derive(Clone, Debug, Default)]
pub struct DrawList {
    pub instances: Vec<SdfInstance>,
}

impl DrawList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, instance: SdfInstance) {
        self.instances.push(instance);
    }

    pub fn clear(&mut self) {
        self.instances.clear();
    }
}

/// Errors from the render backend.
#[derive(Debug)]
pub enum RenderError {
    Backend(String),
}

/// The render seam. Phase 5 implements it over wgpu (`GpuSdfRenderer`
/// pattern: caller owns the device/queue); a headless golden-image
/// implementation backs tests.
pub trait Renderer {
    /// Render one frame's draw list.
    fn render(&mut self, list: &DrawList) -> Result<(), RenderError>;
}
