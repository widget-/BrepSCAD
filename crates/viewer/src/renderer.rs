use std::mem::size_of;
use std::num::NonZeroU64;

use brepscad_kernel_api::Mesh;
use bytemuck::{Pod, Zeroable};
use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};
use glam::Vec3;
use wgpu::util::DeviceExt as _;

use crate::analytic::ComputePreview;
use crate::camera::Camera;

#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
}

impl Vertex {
    const ATTRS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRS,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    light_dir: [f32; 4],
}

struct RenderTargets {
    _color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    texture_id: egui::TextureId,
    size: [u32; 2],
}

pub struct MeshRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    targets: Option<RenderTargets>,
    preview: ComputePreview,
    /// Draw with the compute ray tracer when the evaluation produced a scene.
    analytic: bool,
}

impl MeshRenderer {
    pub fn new(device: &wgpu::Device, mesh: &Mesh) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mesh"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(size_of::<Uniforms>() as u64),
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh uniforms"),
            contents: bytemuck::bytes_of(&Uniforms {
                view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
                camera_pos: [0.0; 4],
                light_dir: [0.4, 0.8, 0.6, 0.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mesh"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let (vertices, indices) = build_mesh_buffers(mesh);
        let vertex_buffer = create_vertex_buffer(device, &vertices);
        let index_buffer = create_index_buffer(device, &indices);
        let index_count = indices.len() as u32;

        Self {
            pipeline,
            bind_group,
            uniform_buffer,
            vertex_buffer,
            index_buffer,
            index_count,
            targets: None,
            preview: ComputePreview::new(device),
            analytic: false,
        }
    }

    /// Uploads the analytic scene used by the compute preview.
    pub fn set_scene(&mut self, device: &wgpu::Device, scene: Option<&brepscad_raycast::Scene>) {
        self.preview.set_scene(device, scene);
    }

    /// Turns the compute preview on or off; the mesh path is the fallback.
    pub fn set_analytic(&mut self, analytic: bool) {
        self.analytic = analytic;
    }

    pub fn set_mesh(&mut self, device: &wgpu::Device, mesh: &Mesh) {
        let (vertices, indices) = build_mesh_buffers(mesh);
        self.vertex_buffer = create_vertex_buffer(device, &vertices);
        self.index_buffer = create_index_buffer(device, &indices);
        self.index_count = indices.len() as u32;
    }

    pub fn ensure_targets(
        &mut self,
        device: &wgpu::Device,
        egui_renderer: &mut egui_wgpu::Renderer,
        size: [u32; 2],
    ) {
        let size = [size[0].max(1), size[1].max(1)];
        if self
            .targets
            .as_ref()
            .is_some_and(|targets| targets.size == size)
        {
            return;
        }

        let extent = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };

        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mesh color"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());

        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mesh depth"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

        let texture_id = if let Some(old) = self.targets.take() {
            egui_renderer.update_egui_texture_from_wgpu_texture(
                device,
                &color_view,
                wgpu::FilterMode::Linear,
                old.texture_id,
            );
            old.texture_id
        } else {
            egui_renderer.register_native_texture(device, &color_view, wgpu::FilterMode::Linear)
        };

        // The compute preview binds the color view, so it has to rebuild its
        // bind group whenever the texture is recreated.
        self.preview.invalidate_targets();
        self.targets = Some(RenderTargets {
            _color: color,
            color_view,
            depth_view,
            texture_id,
            size,
        });
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: &Camera,
        aspect: f32,
    ) -> egui::TextureId {
        let (size, color_view, texture_id) = {
            let targets = self.targets.as_ref().expect("ensure_targets first");
            (targets.size, targets.color_view.clone(), targets.texture_id)
        };

        if self.analytic && self.preview.has_scene() {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("analytic preview"),
            });
            self.preview
                .render(device, queue, &mut encoder, camera, size, &color_view);
            queue.submit(Some(encoder.finish()));
            return texture_id;
        }

        let targets = self.targets.as_ref().expect("ensure_targets first");

        let view_proj = camera.view_projection(aspect);
        let eye = camera.eye();
        let uniforms = Uniforms {
            view_proj: view_proj.to_cols_array_2d(),
            camera_pos: [eye.x, eye.y, eye.z, 0.0],
            light_dir: [0.4, 0.8, 0.6, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("mesh"),
        });

        {
            let color_attachment = Some(wgpu::RenderPassColorAttachment {
                view: &targets.color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.04,
                        g: 0.05,
                        b: 0.07,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            });
            let depth_attachment = Some(wgpu::RenderPassDepthStencilAttachment {
                view: &targets.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            });

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mesh"),
                color_attachments: &[color_attachment],
                depth_stencil_attachment: depth_attachment,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..self.index_count, 0, 0..1);
        }

        queue.submit(Some(encoder.finish()));
        targets.texture_id
    }
}

fn build_mesh_buffers(mesh: &Mesh) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::with_capacity(mesh.triangle_count() * 3);
    // The kernel fills mesh normals from the B-rep surfaces, so curved faces
    // shade smoothly. Fall back to the triangle normal when they are absent.
    let surface_normals = mesh.has_normals();

    for triangle in mesh.indices.chunks_exact(3) {
        let indices = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        if indices.iter().any(|index| *index >= mesh.positions.len()) {
            continue;
        }

        let a = mesh.positions[indices[0]];
        let b = mesh.positions[indices[1]];
        let c = mesh.positions[indices[2]];
        let flat_normal = triangle_normal(a, b, c);

        for (corner, position) in [a, b, c].into_iter().enumerate() {
            let normal = if surface_normals {
                normalize_or(mesh.normals[indices[corner]], flat_normal)
            } else {
                flat_normal
            };
            vertices.push(Vertex {
                position: [position[0] as f32, position[1] as f32, position[2] as f32],
                normal,
            });
        }
    }

    if vertices.is_empty() {
        vertices.push(Vertex::default());
    }

    let indices = (0..vertices.len() as u32).collect();
    (vertices, indices)
}

fn normalize_or(normal: [f64; 3], fallback: [f32; 3]) -> [f32; 3] {
    let normal = Vec3::new(normal[0] as f32, normal[1] as f32, normal[2] as f32);
    let length = normal.length();
    if length > f32::EPSILON {
        (normal / length).to_array()
    } else {
        fallback
    }
}

fn triangle_normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f32; 3] {
    let ab = Vec3::new(
        (b[0] - a[0]) as f32,
        (b[1] - a[1]) as f32,
        (b[2] - a[2]) as f32,
    );
    let ac = Vec3::new(
        (c[0] - a[0]) as f32,
        (c[1] - a[1]) as f32,
        (c[2] - a[2]) as f32,
    );
    let normal = ab.cross(ac);
    let length = normal.length();
    if length > f32::EPSILON {
        (normal / length).to_array()
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn create_vertex_buffer(device: &wgpu::Device, vertices: &[Vertex]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mesh vertices"),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    })
}

fn create_index_buffer(device: &wgpu::Device, indices: &[u32]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mesh indices"),
        contents: bytemuck::cast_slice(indices),
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle_with_normals(normals: Vec<[f64; 3]>) -> Mesh {
        Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals,
            indices: vec![0, 1, 2],
        }
    }

    #[test]
    fn supplied_normals_are_used_verbatim() {
        let mesh = triangle_with_normals(vec![[0.0, 0.0, -1.0]; 3]);
        let (vertices, indices) = build_mesh_buffers(&mesh);

        assert_eq!(indices, vec![0, 1, 2]);
        assert_eq!(vertices.len(), 3);
        for vertex in &vertices {
            assert_eq!(vertex.normal, [0.0, 0.0, -1.0]);
        }
    }

    #[test]
    fn missing_normals_fall_back_to_the_triangle_normal() {
        let mesh = triangle_with_normals(Vec::new());
        let (vertices, _) = build_mesh_buffers(&mesh);

        // Counter-clockwise in XY, so the geometric normal points along +Z.
        for vertex in &vertices {
            assert_eq!(vertex.normal, [0.0, 0.0, 1.0]);
        }
    }

    #[test]
    fn degenerate_normals_fall_back_per_vertex() {
        let mesh = triangle_with_normals(vec![[0.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 0.0, -1.0]]);
        let (vertices, _) = build_mesh_buffers(&mesh);

        assert_eq!(vertices[0].normal, [0.0, 0.0, 1.0]);
        assert_eq!(vertices[1].normal, [0.0, 0.0, -1.0]);
    }
}
