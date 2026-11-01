//! Compute-shader preview: one ray per pixel against the packed B-rep scene.
//!
//! The shader mirrors `brepscad_raycast`; this module owns its pipeline, the
//! scene buffers and the dispatch. The mesh renderer stays the fallback, and
//! the CPU caster stays the reference.

use std::mem::size_of;

use brepscad_raycast::Scene;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt as _;

use crate::camera::Camera;
use crate::scene_gpu::{self, PackedScene};

/// Workgroup size in the shader (`@workgroup_size(8, 8)`).
const WORKGROUP: u32 = 8;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct PreviewCamera {
    origin: [f32; 4],
    forward: [f32; 4],
    right: [f32; 4],
    up: [f32; 4],
    light: [f32; 4],
    size: [u32; 4],
}

struct SceneBuffers {
    nodes: wgpu::Buffer,
    faces: wgpu::Buffer,
    loops: wgpu::Buffer,
    points: wgpu::Buffer,
    vertices: wgpu::Buffer,
    face_count: usize,
}

/// The compute pass and everything it binds.
pub(crate) struct ComputePreview {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    camera: wgpu::Buffer,
    scene: Option<SceneBuffers>,
    bind_group: Option<wgpu::BindGroup>,
}

impl ComputePreview {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("analytic preview"),
            source: wgpu::ShaderSource::Wgsl(include_str!("preview.wgsl").into()),
        });

        let storage_buffer = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

        let entries = [
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
            storage_buffer(1),
            storage_buffer(2),
            storage_buffer(3),
            storage_buffer(4),
            storage_buffer(5),
            wgpu::BindGroupLayoutEntry {
                binding: 6,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
        ];

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("analytic preview"),
            entries: &entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("analytic preview"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("analytic preview"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("analytic preview camera"),
            size: size_of::<PreviewCamera>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            layout,
            camera,
            scene: None,
            bind_group: None,
        }
    }

    /// Uploads a new scene, or clears it when the evaluation produced none.
    pub(crate) fn set_scene(&mut self, device: &wgpu::Device, scene: Option<&Scene>) {
        self.scene = scene.map(|scene| {
            let packed = scene_gpu::pack(scene);
            pack_buffers(device, &packed)
        });
        self.bind_group = None;
    }

    pub(crate) fn has_scene(&self) -> bool {
        self.scene
            .as_ref()
            .is_some_and(|scene| scene.face_count > 0)
    }

    /// Drops the bind group after the output texture is recreated.
    pub(crate) fn invalidate_targets(&mut self) {
        self.bind_group = None;
    }

    pub(crate) fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        camera: &Camera,
        size: [u32; 2],
        view: &wgpu::TextureView,
    ) {
        let Some(scene) = &self.scene else {
            return;
        };
        if scene.face_count == 0 {
            return;
        }

        let aspect = size[0] as f32 / size[1].max(1) as f32;
        let eye = camera.eye();
        let forward = (camera.target - eye).normalize();
        let right = forward.cross(glam::Vec3::Z).normalize();
        let up = right.cross(forward);
        let uniforms = PreviewCamera {
            origin: [eye.x, eye.y, eye.z, 0.0],
            forward: [forward.x, forward.y, forward.z, (camera.fov_y * 0.5).tan()],
            right: [right.x, right.y, right.z, aspect],
            up: [up.x, up.y, up.z, 0.0],
            light: [0.4, 0.8, 0.6, 0.0],
            size: [size[0], size[1], 0, 0],
        };
        queue.write_buffer(&self.camera, 0, bytemuck::bytes_of(&uniforms));

        let bind_group = self.bind_group.get_or_insert_with(|| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("analytic preview"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.camera.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: scene.nodes.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: scene.faces.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: scene.loops.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: scene.points.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: scene.vertices.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                ],
            })
        });

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("analytic preview"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &*bind_group, &[]);
        pass.dispatch_workgroups(size[0].div_ceil(WORKGROUP), size[1].div_ceil(WORKGROUP), 1);
    }
}

fn pack_buffers(device: &wgpu::Device, packed: &PackedScene) -> SceneBuffers {
    SceneBuffers {
        nodes: storage_buffer(device, "preview nodes", bytemuck::cast_slice(&packed.nodes)),
        faces: storage_buffer(device, "preview faces", bytemuck::cast_slice(&packed.faces)),
        loops: storage_buffer(device, "preview loops", bytemuck::cast_slice(&packed.loops)),
        points: storage_buffer(
            device,
            "preview points",
            bytemuck::cast_slice(&packed.points),
        ),
        vertices: storage_buffer(
            device,
            "preview vertices",
            bytemuck::cast_slice(&packed.vertices),
        ),
        face_count: packed.faces.len(),
    }
}

/// Creates a read-only storage buffer; empty input still needs one element.
fn storage_buffer(device: &wgpu::Device, label: &str, contents: &[u8]) -> wgpu::Buffer {
    if contents.is_empty() {
        return device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
    }
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage: wgpu::BufferUsages::STORAGE,
    })
}
