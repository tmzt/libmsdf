//! Dependency-free core types shared by the CPU and GPU render paths.
//!
//! Extracted from matter-stream's `matterstream-common` (same author);
//! VM-era naming and bank plumbing retained only where the shader consumes it.

pub mod color;
pub mod frame;
pub mod raster;
pub mod sdf;
pub mod text;

pub use color::{color_u32_to_f32, rgba, rgba_unpack};
pub use frame::RenderFrame;
pub use raster::Rasterizer;
pub use sdf::{
    Anim, GpuTexture, SdfDrawCmd, any_animation_active, sd_box, sd_circle, sd_cubic_stroke,
    sd_rounded_box, sd_segment, sdf_eval, sdf_eval_animated, sdf_eval_with_params,
};
pub use sdf::{
    DRAW_TYPE_BEZIER, DRAW_TYPE_BOX, DRAW_TYPE_CIRCLE, DRAW_TYPE_LINE, DRAW_TYPE_MSDF_TEXT,
    DRAW_TYPE_OUTLINE, DRAW_TYPE_RIBBON_BEGIN, DRAW_TYPE_RIBBON_END, DRAW_TYPE_SLAB,
    DRAW_TYPE_TEXT, DRAW_TYPE_TEXTURE, MAX_ANIMS, MAX_DRAW_CMDS, MAX_TEXTURES,
};
pub use text::{GpuFont, MAX_FONTS, StringOffset, pack_bitmap, pack_strings, truncate_str, wordwrap};
