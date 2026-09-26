//! Runtime MSDF generation host: glyph outline → edge-list SSBO →
//! compute pass (`msdf_compute.wgsl`) → RGBA8 atlas cell.
//!
//! The CPU msdfgen bake (`font::atlas`, native-only) stays the reference
//! implementation; this path generates glyph cells at runtime on any target
//! with a wgpu device — including wasm32/WebGPU, where msdfgen (C++ FFI)
//! doesn't exist.

use crate::font::atlas::GlyphProjection;
use crate::font::outline::{Edge, EdgeKind, GlyphOutline};

const COMPUTE_SHADER: &str = include_str!("msdf_compute.wgsl");
const WORKGROUP: u32 = 8;

/// Edge record matching the WGSL `Edge` struct (40 bytes).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuEdge {
    kind: u32,
    color: u32,
    p0: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
    p3: [f32; 2],
}

impl From<&Edge> for GpuEdge {
    fn from(e: &Edge) -> Self {
        Self {
            kind: match e.kind {
                EdgeKind::Line => 0,
                EdgeKind::Quad => 1,
                EdgeKind::Cubic => 2,
            },
            color: e.color,
            p0: e.pts[0],
            p1: e.pts[1],
            p2: e.pts[2],
            p3: e.pts[3],
        }
    }
}

/// Uniforms matching the WGSL `Params` struct (32 bytes).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuParams {
    scale: f32,
    range_px: f32,
    cell: u32,
    edge_count: u32,
    tx: f32,
    ty: f32,
    row_stride: u32,
    _pad: u32,
}

/// A generated MSDF cell living in a GPU buffer (packed RGBA8, row-padded
/// to the 256-byte texture-copy alignment).
pub struct MsdfCellBuffer {
    pub buffer: wgpu::Buffer,
    /// Texels per row (cell rounded up to the 256-byte row alignment).
    pub row_stride: u32,
    /// Cell size in texels (cell × cell).
    pub cell: u32,
}

/// WGSL compute-shader MSDF generator.
pub struct MsdfCompute {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl MsdfCompute {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("msdf_compute"),
            source: wgpu::ShaderSource::Wgsl(COMPUTE_SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("msdf_compute_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("msdf_compute_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("msdf_compute_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            pipeline,
            bind_group_layout,
        }
    }

    /// Record a generation pass for one glyph cell into `encoder`. Buffer
    /// writes go through `queue` (they execute at submit, before the pass).
    /// Returns the output buffer (usage STORAGE | COPY_SRC).
    fn record_cell(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        outline: &GlyphOutline,
        proj: &GlyphProjection,
        cell: u32,
        px_range: f32,
    ) -> MsdfCellBuffer {
        let row_bytes = (cell * 4).next_multiple_of(256);
        let row_stride = row_bytes / 4;

        let gpu_edges: Vec<GpuEdge> = outline.edges.iter().map(GpuEdge::from).collect();
        let edge_bytes: &[u8] = bytemuck::cast_slice(&gpu_edges);

        let params = GpuParams {
            scale: proj.scale as f32,
            range_px: px_range,
            cell,
            edge_count: gpu_edges.len() as u32,
            tx: proj.tx as f32,
            ty: proj.ty as f32,
            row_stride,
            _pad: 0,
        };

        let param_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("msdf_params"),
            size: std::mem::size_of::<GpuParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let edge_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("msdf_edges"),
            size: (edge_bytes.len() as u64).max(64),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let out_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("msdf_out"),
            size: (row_bytes as u64) * (cell as u64),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        queue.write_buffer(&param_buffer, 0, bytemuck::bytes_of(&params));
        if !edge_bytes.is_empty() {
            queue.write_buffer(&edge_buffer, 0, edge_bytes);
        }

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("msdf_compute_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: param_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: edge_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: out_buffer.as_entire_binding(),
                },
            ],
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("msdf_compute_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let groups = cell.div_ceil(WORKGROUP);
            pass.dispatch_workgroups(groups, groups, 1);
        }

        MsdfCellBuffer {
            buffer: out_buffer,
            row_stride,
            cell,
        }
    }

    /// Generate one glyph cell into a GPU buffer (packed RGBA8, row-padded).
    /// Submits its own command buffer.
    pub fn generate_cell(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        outline: &GlyphOutline,
        proj: &GlyphProjection,
        cell: u32,
        px_range: f32,
    ) -> MsdfCellBuffer {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("msdf_generate_encoder"),
        });
        let out = self.record_cell(device, queue, &mut encoder, outline, proj, cell, px_range);
        queue.submit(std::iter::once(encoder.finish()));
        out
    }

    /// Generate one glyph cell directly into a region of `texture`
    /// (RGBA8, must have COPY_DST — e.g. the renderer's MSDF atlas).
    #[allow(clippy::too_many_arguments)]
    pub fn generate_into_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        x: u32,
        y: u32,
        outline: &GlyphOutline,
        proj: &GlyphProjection,
        cell: u32,
        px_range: f32,
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("msdf_generate_to_tex_encoder"),
        });
        let out = self.record_cell(device, queue, &mut encoder, outline, proj, cell, px_range);
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &out.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(out.row_stride * 4),
                    rows_per_image: None,
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: cell,
                height: cell,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(std::iter::once(encoder.finish()));
    }
}

/// Blocking readback of a generated cell as tightly-packed RGBA8 rows
/// (native only — used by tests and the bake tooling).
#[cfg(not(target_arch = "wasm32"))]
pub fn read_cell_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    cell_buf: &MsdfCellBuffer,
) -> Vec<u8> {
    let row_bytes = (cell_buf.row_stride * 4) as u64;
    let size = row_bytes * cell_buf.cell as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("msdf_readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(&cell_buf.buffer, 0, &readback, 0, size);
    queue.submit(std::iter::once(encoder.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();

    let data = slice.get_mapped_range();
    let mut out = Vec::with_capacity((cell_buf.cell * cell_buf.cell * 4) as usize);
    for row in 0..cell_buf.cell {
        let start = (row as u64 * row_bytes) as usize;
        out.extend_from_slice(&data[start..start + (cell_buf.cell * 4) as usize]);
    }
    drop(data);
    readback.unmap();
    out
}
