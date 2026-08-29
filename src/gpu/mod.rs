//! GPU SDF renderer — wgpu fragment-shader SDF pipeline.
//!
//! Extracted from matter-stream's `matterstream-ui-gpu` (same author).
//! Accepts an existing wgpu Device/Queue and renders SdfDrawCmd lists via a
//! single full-screen-triangle SDF pipeline — one draw call per frame for
//! the whole `DrawList`.
//!
//! The renderer does NOT own the device or surface — the caller manages
//! those (the highbay_ui contract).
//!
//! ```ignore
//! let renderer = GpuSdfRenderer::new(&device, surface_format);
//! renderer.render_draw_list(&device, &queue, &view, w, h, 1.0, &list, 0.0);
//! ```

pub mod blur;
pub mod compute;

pub use blur::{blur_params, BlurPass};
pub use compute::MsdfCompute;

use crate::core::{RenderFrame, SdfDrawCmd};
use crate::drawlist::DrawList;

/// GPU anim struct (matches WGSL Anim).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuAnimEntry {
    freq: f32,
    duty: f32,
    enable_ref: u32,
    _pad: u32,
}

/// GPU texture descriptor matching WGSL layout.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuTextureDesc {
    width: u32,
    height: u32,
    layer: u32,
    flags: u32,
}

/// Minimal uniforms for the shader — includes inlined header, anim_bank,
/// and texture_bank to stay within GLES 4 storage buffer limit.
///
/// **Mirrors `GpuUniforms` in `sdf_render.wgsl` field for field.** A field
/// added, removed or reordered on one side without the other silently shifts
/// every field after it, so the two are edited together or not at all.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MinimalUniforms {
    time_delta: [f32; 4],
    resolution: [f32; 4],
    mouse: [f32; 4],
    theme: [f32; 4],
    vec4_bank: [[f32; 4]; 16],
    vec3_bank: [[f32; 4]; 16],
    int_bank: [[i32; 4]; 4],
    zero_page: [[u32; 4]; 16],
    font: [u32; 4],
    /// Lo-fi hook: [distortion_amount_px, noise_scale, 0, 0]
    style_params: [f32; 4],
    // Merged from former storage buffers:
    header: [u32; 4],           // .x = cmd_count
    anim_bank: [GpuAnimEntry; 32],
    texture_bank: [GpuTextureDesc; 8],
}

impl Default for MinimalUniforms {
    fn default() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

// SdfDrawCmd bytemuck wrapper — can't impl foreign trait pattern kept from
// upstream (SdfDrawCmd lives in core without a bytemuck dependency).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuDrawCmd {
    pos: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    params: [f32; 4],
    xform: [f32; 4],
    clip: [f32; 4],
}

impl From<&SdfDrawCmd> for GpuDrawCmd {
    fn from(cmd: &SdfDrawCmd) -> Self {
        Self {
            pos: cmd.pos,
            size: cmd.size,
            color: cmd.color,
            params: cmd.params,
            xform: cmd.xform,
            clip: cmd.clip,
        }
    }
}

const MAX_DRAW_CMDS: u32 = 4096;
const MAX_TEXTURE_LAYERS: u32 = 8;
const MAX_GLYPH_TABLE_ENTRIES: u32 = 4096;
const MAX_PARAM_BANK_ENTRIES: u32 = 4096;
const PLACEHOLDER_TEX_SIZE: u32 = 1;
const SHADER_SOURCE: &str = include_str!("sdf_render.wgsl");

fn make_storage_layout(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub struct GpuSdfRenderer {
    pipeline: wgpu::RenderPipeline,
    draw_cmd_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    glyph_bitmap_buffer: wgpu::Buffer,
    char_buffer_gpu: wgpu::Buffer,
    glyph_table_buffer: wgpu::Buffer,
    param_bank_buffer: wgpu::Buffer,
    // Held for ownership — referenced via bind_group, not read through self
    #[allow(dead_code)]
    tex_array: wgpu::Texture,
    #[allow(dead_code)]
    tex_array_view: wgpu::TextureView,
    #[allow(dead_code)]
    tex_sampler: wgpu::Sampler,
    msdf_atlas_texture: wgpu::Texture,
    #[allow(dead_code)]
    msdf_atlas_view: wgpu::TextureView,
    #[allow(dead_code)]
    msdf_sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
    max_cmds: u32,
    /// Lo-fi hook state: [distortion_amount_px, noise_scale].
    lofi: [f32; 2],
}

impl GpuSdfRenderer {
    /// Create a new renderer attached to an existing wgpu device.
    /// Does NOT take ownership of the device.
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        Self::new_with_msdf(device, surface_format, 1, 1)
    }

    /// Create a renderer with a pre-sized MSDF atlas texture - ONE layer.
    ///
    /// Every atlas this repo ships is single-layer, so this is still the whole
    /// answer for them. A consumer holding a layered [`crate::FontAtlas`] must
    /// pass its [`crate::FontAtlas::layer_count`] through
    /// [`GpuSdfRenderer::new_with_msdf_layers`] instead: the layer count is a
    /// texture-CREATION parameter in wgpu, so it cannot be inferred later from
    /// the pixels handed to `upload_msdf_atlas`.
    pub fn new_with_msdf(device: &wgpu::Device, surface_format: wgpu::TextureFormat, msdf_width: u32, msdf_height: u32) -> Self {
        Self::new_with_msdf_layers(device, surface_format, msdf_width, msdf_height, 1)
    }

    /// [`GpuSdfRenderer::new_with_msdf`] with an explicit layer count - the
    /// constructor a layered atlas needs.
    ///
    /// `msdf_layers` is [`crate::FontAtlas::layer_count`]. Every layer of an
    /// array shares `msdf_width` x `msdf_height`, so the texture costs that
    /// rectangle times the layer count; [`crate::MAX_ATLAS_LAYERS`] is the
    /// guaranteed ceiling and this clamps to it rather than asking the device
    /// for a limit the repo does not pin.
    pub fn new_with_msdf_layers(device: &wgpu::Device, surface_format: wgpu::TextureFormat, msdf_width: u32, msdf_height: u32, msdf_layers: u32) -> Self {
        let msdf_layers = msdf_layers.clamp(1, crate::MAX_ATLAS_LAYERS as u32);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sdf_render"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
        });

        let draw_cmd_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("draw_cmds"),
            size: (MAX_DRAW_CMDS as u64) * std::mem::size_of::<GpuDrawCmd>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // header/anim_bank/texture_bank live inside the uniform buffer
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<MinimalUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let glyph_bitmap_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("glyph_bitmap"), size: 8192,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let char_buffer_gpu = device.create_buffer(&wgpu::BufferDescriptor {
            // 65536 bytes = 16384 u32 char entries — headroom carried over
            // from upstream (large text frames must truncate, not crash).
            label: Some("char_buffer"), size: 65536,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let glyph_table_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("glyph_table"),
            size: (MAX_GLYPH_TABLE_ENTRIES as u64) * 32,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let param_bank_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("param_bank"),
            size: (MAX_PARAM_BANK_ENTRIES as u64) * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let tex_array = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tex_array"),
            size: wgpu::Extent3d { width: PLACEHOLDER_TEX_SIZE, height: PLACEHOLDER_TEX_SIZE, depth_or_array_layers: MAX_TEXTURE_LAYERS },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let tex_array_view = tex_array.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default()
        });
        let tex_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("tex_sampler"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default()
        });

        let msdf_atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("msdf_atlas"),
            size: wgpu::Extent3d { width: msdf_width.max(1), height: msdf_height.max(1), depth_or_array_layers: msdf_layers },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            // COPY_SRC: lets atlas contents be read back / repacked after
            // compute-generation and eviction.
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        // D2Array EXPLICITLY: wgpu's default view dimension for a texture with
        // one array layer is D2, which no longer matches the binding. A
        // one-layer D2Array view samples identically to the D2 view it
        // replaces, which is what keeps every existing frame where it is.
        let msdf_atlas_view = msdf_atlas_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let msdf_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("msdf_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Bind group layout (bindings 0-12)
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sdf_render_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                make_storage_layout(1),
                // bindings 2,3 merged into uniform buffer (header, anim_bank)
                make_storage_layout(4),
                make_storage_layout(5),
                // Binding 6: texture_2d_array for texture bank
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                // Binding 7: sampler for texture bank
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // binding 8 merged into uniform buffer (texture_bank)
                // Binding 9: MSDF atlas texture - an ARRAY, one layer per
                // `(point size, style)`; see `crate::AtlasLayer`.
                wgpu::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                // Binding 10: MSDF sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 10,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Binding 11: glyph table
                make_storage_layout(11),
                // Binding 12: aux param bank (Bézier control points)
                make_storage_layout(12),
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sdf_render_bg"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: draw_cmd_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: glyph_bitmap_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: char_buffer_gpu.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&tex_array_view) },
                wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::Sampler(&tex_sampler) },
                wgpu::BindGroupEntry { binding: 9, resource: wgpu::BindingResource::TextureView(&msdf_atlas_view) },
                wgpu::BindGroupEntry { binding: 10, resource: wgpu::BindingResource::Sampler(&msdf_sampler) },
                wgpu::BindGroupEntry { binding: 11, resource: glyph_table_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 12, resource: param_bank_buffer.as_entire_binding() },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sdf_render_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sdf_render_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            draw_cmd_buffer,
            uniform_buffer,
            glyph_bitmap_buffer,
            char_buffer_gpu,
            glyph_table_buffer,
            param_bank_buffer,
            tex_array,
            tex_array_view,
            tex_sampler,
            msdf_atlas_texture,
            msdf_atlas_view,
            msdf_sampler,
            bind_group,
            max_cmds: MAX_DRAW_CMDS,
            lofi: [0.0, 0.0],
        }
    }

    /// Set the lo-fi pencil-sketch hook parameters: `amount` in pixels
    /// (0 disables) and the noise `scale`. The effect itself lands in
    /// Phase 8; this is the Phase-5 plumbing.
    pub fn set_lofi(&mut self, amount: f32, scale: f32) {
        self.lofi = [amount, scale];
    }

    /// The MSDF atlas texture (for compute-shader generation into regions).
    pub fn msdf_atlas_texture(&self) -> &wgpu::Texture {
        &self.msdf_atlas_texture
    }

    /// Array layers the MSDF atlas texture was created with. A consumer checks
    /// this against [`crate::FontAtlas::layer_count`] before uploading; the
    /// upload checks it too, because a mismatch is silent in a frame.
    pub fn msdf_atlas_layers(&self) -> u32 {
        self.msdf_atlas_texture.depth_or_array_layers()
    }

    /// Render SdfDrawCmd list to a texture view.
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        draws: &[SdfDrawCmd],
    ) {
        self.render_animated(device, queue, target, width, height, draws, 0.0);
    }

    pub fn render_animated(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        draws: &[SdfDrawCmd],
        time_ms: f32,
    ) {
        self.render_full(device, queue, target, width, height, draws, time_ms, &[0; 16], &[], None);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_full(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        draws: &[SdfDrawCmd],
        time_ms: f32,
        int_bank: &[i32],
        anim_bank: &[crate::core::Anim],
        font: Option<&crate::core::GpuFont>,
    ) {
        self.render_full_scaled(device, queue, target, width, height, 1.0, draws, time_ms, int_bank, anim_bank, font);
    }

    /// Lower and render a [`DrawList`] in one call: uploads its packed char
    /// buffer + param bank, then draws. The single instanced draw path of
    /// the highbay_ui contract.
    #[allow(clippy::too_many_arguments)]
    pub fn render_draw_list(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        scale: f32,
        list: &DrawList,
        time_ms: f32,
    ) {
        let frame = list.lower();
        self.upload_chars(queue, &frame.char_buffer);
        self.upload_params(queue, &frame.param_bank);
        self.render_full_scaled(
            device, queue, target, width, height, scale,
            &frame.draws, time_ms, &[0; 16], &[], None,
        );
    }

    /// Upload font atlas data (call once or when font changes).
    pub fn upload_font(&self, queue: &wgpu::Queue, font: &crate::core::GpuFont, bitmap: &[u32]) {
        if !bitmap.is_empty() {
            queue.write_buffer(&self.glyph_bitmap_buffer, 0, bytemuck::cast_slice(bitmap));
        }
        let _ = font;
    }

    /// Upload character data for text rendering. Truncates to the buffer
    /// capacity rather than panicking, so an unexpectedly large frame drops
    /// trailing glyphs instead of crashing the app.
    pub fn upload_chars(&self, queue: &wgpu::Queue, chars: &[u32]) {
        if chars.is_empty() { return; }
        let cap = (self.char_buffer_gpu.size() as usize) / std::mem::size_of::<u32>();
        let clipped = if chars.len() > cap {
            log::warn!("[sdf] upload_chars: truncating {} → {} u32 (buffer cap {}B)",
                chars.len(), cap, self.char_buffer_gpu.size());
            &chars[..cap]
        } else {
            chars
        };
        queue.write_buffer(&self.char_buffer_gpu, 0, bytemuck::cast_slice(clipped));
    }

    /// Upload MSDF glyph table (array of u32, 8 per entry = 2 × vec4<u32>).
    pub fn upload_glyph_table(&self, queue: &wgpu::Queue, table: &[u32]) {
        if !table.is_empty() {
            queue.write_buffer(&self.glyph_table_buffer, 0, bytemuck::cast_slice(table));
        }
    }

    /// Upload the aux param bank (Bézier control points etc.), truncating
    /// at capacity like `upload_chars`.
    pub fn upload_params(&self, queue: &wgpu::Queue, params: &[[f32; 4]]) {
        if params.is_empty() { return; }
        let cap = (self.param_bank_buffer.size() as usize) / 16;
        let clipped = if params.len() > cap {
            log::warn!("[sdf] upload_params: truncating {} → {} entries", params.len(), cap);
            &params[..cap]
        } else {
            params
        };
        queue.write_buffer(&self.param_bank_buffer, 0, bytemuck::cast_slice(clipped));
    }

    /// Upload MSDF atlas RGBA pixel data to the whole atlas texture, **every
    /// layer the data covers**.
    ///
    /// Data is RGBA8 (4 bytes per texel), `width * height * 4 * layers` bytes,
    /// layer-major - exactly what [`crate::FontAtlas::to_rgba_bytes`] returns,
    /// so a caller passing a layered atlas's bytes uploads all of it without
    /// changing this call.
    ///
    /// **A mismatch is reported, not truncated silently.** The layer count is
    /// fixed when the texture is created
    /// ([`GpuSdfRenderer::new_with_msdf_layers`]), so an atlas with more
    /// layers than the texture cannot be uploaded - and the glyphs in the
    /// missing layers would draw NOTHING rather than draw wrongly (the shader
    /// skips an out-of-range layer). That is a build-order defect with a
    /// one-line fix, and it is logged as an error because a blank word in a
    /// frame does not say which of a dozen things went wrong.
    pub fn upload_msdf_atlas(&self, queue: &wgpu::Queue, width: u32, height: u32, rgba_data: &[u8]) {
        let per_layer = (width as usize) * (height as usize) * 4;
        if per_layer == 0 {
            return;
        }
        let have = rgba_data.len() / per_layer;
        let texture_layers = self.msdf_atlas_layers() as usize;
        if have > texture_layers {
            log::error!(
                "[sdf] upload_msdf_atlas: the atlas has {have} layers and the texture was created \
                 with {texture_layers} - glyphs in layers {texture_layers}.. will draw nothing. \
                 Build the renderer with GpuSdfRenderer::new_with_msdf_layers(.., \
                 atlas.layer_count())"
            );
        }
        for layer in 0..have.min(texture_layers) {
            let start = layer * per_layer;
            self.upload_msdf_atlas_region_in_layer(
                queue,
                layer as u32,
                0,
                0,
                width,
                height,
                &rgba_data[start..start + per_layer],
            );
        }
    }

    /// Upload RGBA8 pixels into a sub-region of layer 0 of the atlas texture
    /// (dynamic glyph appends via `AtlasManager`).
    pub fn upload_msdf_atlas_region(&self, queue: &wgpu::Queue, x: u32, y: u32, width: u32, height: u32, rgba_data: &[u8]) {
        self.upload_msdf_atlas_region_in_layer(queue, 0, x, y, width, height, rgba_data);
    }

    /// [`GpuSdfRenderer::upload_msdf_atlas_region`] into a named layer.
    ///
    /// `AtlasManager` allocates over ONE layer's rectangle - it is a 2D
    /// allocator and a layer is a 2D surface - so a runtime atlas with layers
    /// keeps one manager per layer and names the layer here.
    #[allow(clippy::too_many_arguments)]
    pub fn upload_msdf_atlas_region_in_layer(&self, queue: &wgpu::Queue, layer: u32, x: u32, y: u32, width: u32, height: u32, rgba_data: &[u8]) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.msdf_atlas_texture,
                mip_level: 0,
                // z IS the array layer for a 2D-array texture.
                origin: wgpu::Origin3d { x, y, z: layer },
                aspect: wgpu::TextureAspect::All,
            },
            rgba_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_full_scaled(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        scale: f32,
        draws: &[SdfDrawCmd],
        time_ms: f32,
        int_bank: &[i32],
        anim_bank: &[crate::core::Anim],
        font: Option<&crate::core::GpuFont>,
    ) {
        self.render_full_scaled_with_load(
            device, queue, target, width, height, scale,
            draws, time_ms, int_bank, anim_bank, font, &[],
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        );
    }

    /// Render SDF draws, preserving whatever is already on the target.
    /// Caller must have initialised the target in an earlier pass.
    #[allow(clippy::too_many_arguments)]
    pub fn render_full_scaled_load(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        scale: f32,
        draws: &[SdfDrawCmd],
        time_ms: f32,
        int_bank: &[i32],
        anim_bank: &[crate::core::Anim],
        font: Option<&crate::core::GpuFont>,
    ) {
        self.render_full_scaled_with_load(
            device, queue, target, width, height, scale,
            draws, time_ms, int_bank, anim_bank, font, &[],
            wgpu::LoadOp::Load,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn render_full_scaled_with_load(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        scale: f32,
        draws: &[SdfDrawCmd],
        time_ms: f32,
        int_bank: &[i32],
        anim_bank: &[crate::core::Anim],
        font: Option<&crate::core::GpuFont>,
        texture_bank: &[crate::core::GpuTexture],
        load: wgpu::LoadOp<wgpu::Color>,
    ) {
        // "Owns its own encoder + submits" wrapper for offscreen callers
        // whose target isn't a swapchain image and who don't need to share
        // an encoder across passes.
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("sdf_render_encoder"),
        });
        self.render_full_scaled_with_load_into(
            &mut encoder, queue, target, width, height, scale,
            draws, time_ms, int_bank, anim_bank, font, texture_bank, load,
        );
        queue.submit(std::iter::once(encoder.finish()));
    }

    /// Load-variant that appends to a caller-owned [`wgpu::CommandEncoder`]
    /// instead of creating its own + submitting. Callers must submit the
    /// encoder themselves.
    ///
    /// Purpose: consolidating a swapchain frame into a single command
    /// buffer — some drivers misbehave when a swapchain-image view is
    /// written by multiple submits per frame.
    #[allow(clippy::too_many_arguments)]
    pub fn render_full_scaled_load_into(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        scale: f32,
        draws: &[SdfDrawCmd],
        time_ms: f32,
        int_bank: &[i32],
        anim_bank: &[crate::core::Anim],
        font: Option<&crate::core::GpuFont>,
    ) {
        self.render_full_scaled_with_load_into(
            encoder, queue, target, width, height, scale,
            draws, time_ms, int_bank, anim_bank, font, &[],
            wgpu::LoadOp::Load,
        );
    }

    /// Clear+draw variant that appends to a caller-owned encoder.
    /// See [`Self::render_full_scaled_load_into`] for rationale.
    #[allow(clippy::too_many_arguments)]
    pub fn render_full_scaled_clear_into(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        scale: f32,
        draws: &[SdfDrawCmd],
        time_ms: f32,
        int_bank: &[i32],
        anim_bank: &[crate::core::Anim],
        font: Option<&crate::core::GpuFont>,
    ) {
        self.render_full_scaled_with_load_into(
            encoder, queue, target, width, height, scale,
            draws, time_ms, int_bank, anim_bank, font, &[],
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
    }

    /// Shared body for the "_into" family. Writes uniforms + draws, records
    /// ONE render pass into `encoder`. Does NOT finish or submit `encoder`.
    #[allow(clippy::too_many_arguments)]
    fn render_full_scaled_with_load_into(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        scale: f32,
        draws: &[SdfDrawCmd],
        time_ms: f32,
        int_bank: &[i32],
        anim_bank: &[crate::core::Anim],
        font: Option<&crate::core::GpuFont>,
        texture_bank: &[crate::core::GpuTexture],
        load: wgpu::LoadOp<wgpu::Color>,
    ) {
        let count = draws.len().min(self.max_cmds as usize);

        if count > 0 {
            let gpu_cmds: Vec<GpuDrawCmd> = draws[..count].iter().map(GpuDrawCmd::from).collect();
            queue.write_buffer(&self.draw_cmd_buffer, 0, bytemuck::cast_slice(&gpu_cmds));
        }

        let mut uniforms = MinimalUniforms::default();
        uniforms.time_delta = [time_ms, 0.0, 0.0, 0.0];
        uniforms.resolution = [width as f32, height as f32, scale, 0.0];
        uniforms.style_params = [self.lofi[0], self.lofi[1], 0.0, 0.0];
        uniforms.header = [count as u32, 0, 0, 0];
        for (i, val) in int_bank.iter().take(16).enumerate() {
            uniforms.int_bank[i / 4][i % 4] = *val;
        }
        if let Some(f) = font {
            uniforms.font = [f.glyph_w, f.glyph_h, f.first_cp, f.last_cp];
        }
        for (i, a) in anim_bank.iter().take(32).enumerate() {
            uniforms.anim_bank[i] = GpuAnimEntry {
                freq: a.freq, duty: a.duty, enable_ref: a.enable_ref, _pad: 0,
            };
        }
        // Texture bank rides inside the same uniform write. (Upstream wrote
        // it separately at an offset *before* this full-struct write, which
        // zeroed it again — fixed during extraction.)
        for (i, t) in texture_bank.iter().take(8).enumerate() {
            uniforms.texture_bank[i] = GpuTextureDesc {
                width: t.width, height: t.height, layer: t.layer, flags: t.flags,
            };
        }
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sdf_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Render a fully prepared `RenderFrame`.
    pub fn render_frame(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        frame: &RenderFrame,
    ) {
        if !frame.char_buffer.is_empty() {
            self.upload_chars(queue, &frame.char_buffer);
        }
        if !frame.param_bank.is_empty() {
            self.upload_params(queue, &frame.param_bank);
        }
        if !frame.glyph_bitmap.is_empty() {
            queue.write_buffer(&self.glyph_bitmap_buffer, 0, bytemuck::cast_slice(&frame.glyph_bitmap));
        }

        self.render_full_scaled_with_load(
            device, queue, target,
            frame.width, frame.height, frame.scale,
            &frame.draws, frame.time_ms,
            &frame.int_bank,
            &frame.anim_bank, Some(&frame.font),
            &frame.texture_bank,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        );
    }
}
