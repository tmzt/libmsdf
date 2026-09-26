//! Backdrop blur post-pass — a cheap gaussian blur over an already-rendered
//! scene texture, with a sharp premultiplied overlay composited on top.
//!
//! This is the renderer-side plumbing behind Highbay's Settings modal: the
//! whole app view is rendered to an offscreen `backdrop` texture, this pass
//! blurs it (a real post-process, not just a scrim), and the modal — rendered
//! to its own transparent `overlay` texture through the normal SDF path — is
//! composited over the blur so it stays crisp.
//!
//! It is deliberately self-contained: its own pipeline + shader ([`blur.wgsl`]),
//! its own sampler and uniform buffer. It never touches [`super::GpuSdfRenderer`]'s
//! REPLACE pipeline. Only run while a modal is open, so a fixed 5×5 kernel is
//! more than cheap enough.

use crate::drawlist::DrawList;

const SHADER_SOURCE: &str = include_str!("blur.wgsl");

/// Uniforms for the blur shader (matches WGSL `BlurUniforms`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurUniforms {
    params: [f32; 4],
}

/// The uniform params (`[1/width, 1/height, radius_px, 0]`) the blur shader
/// consumes. Split out (and unit-tested) so the texel-size + radius packing is
/// verifiable without a GPU.
pub fn blur_params(width: u32, height: u32, radius_px: f32) -> [f32; 4] {
    [
        1.0 / width.max(1) as f32,
        1.0 / height.max(1) as f32,
        radius_px.max(0.0),
        0.0,
    ]
}

/// A backdrop-blur + overlay-composite post-pass, attached to an existing wgpu
/// device (never owns it — the highbay_ui contract).
pub struct BlurPass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
}

impl BlurPass {
    /// Create a blur pass targeting `format` (must match the final target's
    /// format; the backdrop/overlay textures are sampled, so any format works
    /// for them, but the shell keeps them all `format` for simplicity).
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blur_shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blur_uniforms"),
            size: std::mem::size_of::<BlurUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blur_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let sampled_tex = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                sampled_tex(1),
                sampled_tex(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blur_pipeline_layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blur_pipeline"),
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
                    format,
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
            layout,
            sampler,
            uniform_buffer,
        }
    }

    /// Blur `backdrop` into `target`, compositing the sharp premultiplied
    /// `overlay` over the result. `radius_px` is the blur reach in pixels
    /// (small = "slightly blurred"). Owns its encoder + submits.
    ///
    /// `backdrop`/`overlay` must be sampleable views the caller keeps alive;
    /// `target` is the final (opaque) image. `overlay` can be a transparent
    /// texture for a pure blur.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        overlay: &wgpu::TextureView,
        width: u32,
        height: u32,
        radius_px: f32,
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("blur_encoder"),
        });
        self.render_into(
            device,
            &mut encoder,
            queue,
            target,
            backdrop,
            overlay,
            width,
            height,
            radius_px,
        );
        queue.submit(std::iter::once(encoder.finish()));
    }

    /// Variant recording into a caller-owned encoder (single-submit swapchain
    /// frames). See [`Self::render`].
    #[allow(clippy::too_many_arguments)]
    pub fn render_into(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        backdrop: &wgpu::TextureView,
        overlay: &wgpu::TextureView,
        width: u32,
        height: u32,
        radius_px: f32,
    ) {
        let uniforms = BlurUniforms {
            params: blur_params(width, height, radius_px),
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur_bg"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(backdrop),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(overlay),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blur_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
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
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Convenience: lower and render a [`DrawList`] into a transparent-cleared
/// `target` through the caller's [`super::GpuSdfRenderer`], for use as the blur
/// `overlay`. (Thin wrapper kept here so the modal-overlay recipe lives beside
/// the blur that consumes it.)
pub fn render_overlay_texture(
    renderer: &super::GpuSdfRenderer,
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
    renderer.upload_chars(queue, &frame.char_buffer);
    renderer.upload_params(queue, &frame.param_bank);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("overlay_encoder"),
    });
    renderer.render_full_scaled_clear_into(
        &mut encoder,
        queue,
        target,
        width,
        height,
        scale,
        &frame.draws,
        time_ms,
        &[0; 16],
        &[],
        None,
    );
    queue.submit(std::iter::once(encoder.finish()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_params_pack_texel_size_and_radius() {
        let p = blur_params(200, 100, 4.0);
        assert!((p[0] - 1.0 / 200.0).abs() < 1e-6, "texel width = 1/w");
        assert!((p[1] - 1.0 / 100.0).abs() < 1e-6, "texel height = 1/h");
        assert_eq!(p[2], 4.0, "radius carried");
        assert_eq!(p[3], 0.0);
    }

    #[test]
    fn blur_params_guard_degenerate_size_and_negative_radius() {
        let p = blur_params(0, 0, -3.0);
        assert!(p[0].is_finite() && p[1].is_finite(), "no divide-by-zero");
        assert_eq!(p[2], 0.0, "negative radius clamps to 0");
    }
}
