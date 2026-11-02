//! Flat scene data for the compute preview.
//!
//! `brepscad_raycast::Scene` is a tree of Rust values; the shader needs flat
//! arrays with a fixed layout. This module is the only place that knows that
//! layout, and it is testable without a GPU.

use std::f64::consts::TAU;

use brepscad_raycast::{Face, Scene, Surface};

/// Surface kinds, matching the `KIND_*` constants in `preview.wgsl`.
pub(crate) const KIND_PLANE: u32 = 0;
pub(crate) const KIND_CYLINDER: u32 = 1;
pub(crate) const KIND_CONE: u32 = 2;
pub(crate) const KIND_TRIANGLES: u32 = 3;
pub(crate) const KIND_SPHERE: u32 = 4;

/// One bounding-volume node; `meta` holds left, right, start, count.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuNode {
    pub min: [f32; 4],
    pub max: [f32; 4],
    pub meta: [u32; 4],
}

/// One face: kind, orientation, trim range, triangle range and surface data.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuFace {
    /// kind, flip, first trim loop, trim loop count.
    pub header: [u32; 4],
    /// first triangle vertex, triangle count.
    pub triangles: [u32; 4],
    /// Origin or apex, with the radius or half-angle tangent in `w`.
    pub params0: [f32; 4],
    /// Plane normal or revolution axis, with the `u` period in `w`.
    pub params1: [f32; 4],
    /// Plane `u` axis or revolution reference direction.
    pub params2: [f32; 4],
    /// Plane `v` axis.
    pub params3: [f32; 4],
}

/// A trim loop as a range into the shared point array.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuLoop {
    pub range: [u32; 2],
}

/// Everything the shader needs, in the order it binds them.
#[derive(Debug, Default)]
pub(crate) struct PackedScene {
    pub nodes: Vec<GpuNode>,
    pub faces: Vec<GpuFace>,
    pub loops: Vec<GpuLoop>,
    /// Trim points in surface parameter space, `(u, v)`.
    pub points: Vec<[f32; 2]>,
    /// Three vertices per triangle, for faces covered by tessellation. Stored
    /// as `vec4` because a `vec3` array has a 16-byte stride in WGSL.
    pub vertices: Vec<[f32; 4]>,
}

pub(crate) fn pack(scene: &Scene) -> PackedScene {
    let mut packed = PackedScene::default();

    for node in scene.nodes() {
        packed.nodes.push(GpuNode {
            min: [
                node.bounds.0[0] as f32,
                node.bounds.0[1] as f32,
                node.bounds.0[2] as f32,
                0.0,
            ],
            max: [
                node.bounds.1[0] as f32,
                node.bounds.1[1] as f32,
                node.bounds.1[2] as f32,
                0.0,
            ],
            meta: [node.left, node.right, node.start, node.count],
        });
    }

    // Pack in BVH leaf order, so a leaf's `start..start + count` addresses the
    // face array directly and the shader needs no separate indirection.
    for &face_index in scene.order() {
        let face = &scene.faces()[face_index as usize];
        let packed_face = pack_face(
            face,
            &mut packed.loops,
            &mut packed.points,
            &mut packed.vertices,
        );
        packed.faces.push(packed_face);
    }

    packed
}

fn pack_face(
    face: &Face,
    loops: &mut Vec<GpuLoop>,
    points: &mut Vec<[f32; 2]>,
    vertices: &mut Vec<[f32; 4]>,
) -> GpuFace {
    let mut packed = GpuFace {
        header: [0, u32::from(face.flip), 0, 0],
        ..Default::default()
    };

    match &face.surface {
        Some(Surface::Plane {
            origin,
            normal,
            u_axis,
            v_axis,
        }) => {
            packed.header[0] = KIND_PLANE;
            packed.params0 = point_and(origin, 0.0);
            packed.params1 = point_and(normal, 0.0);
            packed.params2 = point_and(u_axis, 0.0);
            packed.params3 = point_and(v_axis, 0.0);
        }
        Some(Surface::Cylinder {
            origin,
            axis,
            radius,
            u_reference,
        }) => {
            packed.header[0] = KIND_CYLINDER;
            packed.params0 = point_and(origin, *radius);
            packed.params1 = point_and(axis, TAU);
            packed.params2 = point_and(u_reference, 0.0);
        }
        Some(Surface::Cone {
            apex,
            axis,
            half_angle_tan,
            u_reference,
        }) => {
            packed.header[0] = KIND_CONE;
            packed.params0 = point_and(apex, *half_angle_tan);
            packed.params1 = point_and(axis, TAU);
            packed.params2 = point_and(u_reference, 0.0);
        }
        Some(Surface::Sphere { centre, radius }) => {
            packed.header[0] = KIND_SPHERE;
            packed.params0 = point_and(centre, *radius);
        }
        // Spline faces have no GPU intersection yet, so they ride the
        // tessellated path like any other unmodelled surface.
        Some(Surface::Spline(_)) | None => {
            packed.header[0] = KIND_TRIANGLES;
            packed.triangles = [
                (vertices.len() / 3) as u32,
                face.triangles.len() as u32,
                0,
                0,
            ];
            for triangle in &face.triangles {
                for point in triangle {
                    vertices.push([point[0] as f32, point[1] as f32, point[2] as f32, 0.0]);
                }
            }
            return packed;
        }
    }

    packed.header[2] = loops.len() as u32;
    packed.header[3] = face.trims.loops().len() as u32;
    for trim in face.trims.loops() {
        let start = points.len() as u32;
        points.extend(trim.iter().map(|point| [point[0] as f32, point[1] as f32]));
        loops.push(GpuLoop {
            range: [start, trim.len() as u32],
        });
    }
    packed
}

fn point_and(point: &[f64; 3], extra: f64) -> [f32; 4] {
    [
        point[0] as f32,
        point[1] as f32,
        point[2] as f32,
        extra as f32,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use brepscad_raycast::{Face, TrimRegion};

    fn plane_face() -> Face {
        Face::new(
            Surface::Plane {
                origin: [0.0, 0.0, 1.0],
                normal: [0.0, 0.0, 1.0],
                u_axis: [1.0, 0.0, 0.0],
                v_axis: [0.0, 1.0, 0.0],
            },
            TrimRegion::new(vec![
                vec![[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]],
                vec![[-0.5, -0.5], [-0.5, 0.5], [0.5, 0.5], [0.5, -0.5]],
            ]),
            ([-1.0, -1.0, 1.0], [1.0, 1.0, 1.0]),
        )
        .with_flip(true)
    }

    fn cylinder_face() -> Face {
        Face::new(
            Surface::Cylinder {
                origin: [0.0, 0.0, 0.0],
                axis: [0.0, 0.0, 1.0],
                radius: 2.0,
                u_reference: [1.0, 0.0, 0.0],
            },
            TrimRegion::with_u_period(
                vec![vec![[0.0, -1.0], [TAU, -1.0], [TAU, 1.0], [0.0, 1.0]]],
                TAU,
            ),
            ([-2.0, -2.0, -1.0], [2.0, 2.0, 1.0]),
        )
    }

    fn triangle_face() -> Face {
        Face::from_triangles(vec![[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]])
    }

    fn sample_scene() -> Scene {
        Scene::new(vec![plane_face(), cylinder_face(), triangle_face()])
    }

    #[test]
    fn struct_sizes_are_pinned() {
        // The shader reads these layouts; a change here is a shader change.
        assert_eq!(size_of::<GpuNode>(), 48);
        assert_eq!(size_of::<GpuFace>(), 96);
        assert_eq!(size_of::<GpuLoop>(), 8);
    }

    #[test]
    fn packs_nodes_one_to_one() {
        let scene = sample_scene();
        let packed = pack(&scene);
        assert_eq!(packed.nodes.len(), scene.nodes().len());
        for (node, gpu) in scene.nodes().iter().zip(&packed.nodes) {
            assert_eq!(gpu.meta, [node.left, node.right, node.start, node.count]);
            assert!((gpu.min[0] as f64 - node.bounds.0[0]).abs() < 1e-6);
            assert!((gpu.max[2] as f64 - node.bounds.1[2]).abs() < 1e-6);
        }
    }

    #[test]
    fn packs_plane_with_trim_loops_and_flip() {
        let scene = sample_scene();
        let packed = pack(&scene);
        let plane = packed.faces[0];

        assert_eq!(plane.header[0], KIND_PLANE);
        assert_eq!(plane.header[1], 1, "flip is carried through");
        assert_eq!(plane.header[3], 2, "outer loop and hole");
        assert_eq!(plane.params0, [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(plane.params1, [0.0, 0.0, 1.0, 0.0], "normal, not periodic");

        // Eight trim points for the plane's two loops, plus the cylinder's one.
        assert_eq!(packed.points.len(), 12);
        assert_eq!(packed.loops[0].range, [0, 4]);
        assert_eq!(packed.loops[1].range, [4, 4]);
        assert_eq!(packed.points[4], [-0.5, -0.5]);
    }

    #[test]
    fn packs_cylinder_with_its_period() {
        let scene = sample_scene();
        let packed = pack(&scene);
        let cylinder = packed.faces[1];

        assert_eq!(cylinder.header[0], KIND_CYLINDER);
        assert_eq!(cylinder.params0[3], 2.0, "radius");
        assert_eq!(cylinder.params1[2], 1.0, "axis");
        assert!((cylinder.params1[3] - TAU as f32).abs() < 1e-6, "u period");
        assert_eq!(cylinder.header[3], 1, "one trim loop");
    }

    #[test]
    fn packs_triangle_faces_as_flat_vertices() {
        let scene = sample_scene();
        let packed = pack(&scene);
        let triangles = packed.faces[2];

        assert_eq!(triangles.header[0], KIND_TRIANGLES);
        assert_eq!(triangles.triangles, [0, 1, 0, 0]);
        assert_eq!(packed.vertices.len(), 3, "three vertices per triangle");
        assert_eq!(packed.vertices[1], [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(triangles.header[3], 0, "triangle faces carry no trims");
    }

    #[test]
    fn faces_are_packed_in_leaf_order() {
        // Six faces force a BVH split, so `order` is not the identity any more.
        let faces = (0..6)
            .map(|index| {
                Face::new(
                    Surface::Plane {
                        origin: [0.0, 0.0, index as f64],
                        normal: [0.0, 0.0, 1.0],
                        u_axis: [1.0, 0.0, 0.0],
                        v_axis: [0.0, 1.0, 0.0],
                    },
                    TrimRegion::new(vec![vec![
                        [-1.0, -1.0],
                        [1.0, -1.0],
                        [1.0, 1.0],
                        [-1.0, 1.0],
                    ]]),
                    ([0.0, 0.0, index as f64], [0.0, 0.0, index as f64]),
                )
            })
            .collect::<Vec<_>>();
        let scene = Scene::new(faces);
        let packed = pack(&scene);

        // Each leaf addresses the packed face array directly and, together, the
        // leaves cover every face exactly once.
        let mut covered = vec![0_u32; packed.faces.len()];
        let mut leaves = 0;
        for node in &packed.nodes {
            let (start, count) = (node.meta[2], node.meta[3]);
            if count == 0 {
                continue;
            }
            leaves += 1;
            for slot in start..start + count {
                covered[slot as usize] += 1;
            }
        }
        assert!(leaves > 1, "six faces should split into several leaves");
        assert!(
            covered.iter().all(|count| *count == 1),
            "every face covered once: {covered:?}"
        );
    }

    #[test]
    fn triangle_offsets_accumulate_across_faces() {
        let two_triangles = Face::from_triangles(vec![
            [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            [[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [0.0, 1.0, 1.0]],
        ]);
        let scene = Scene::new(vec![two_triangles.clone(), two_triangles]);
        let packed = pack(&scene);

        assert_eq!(packed.faces[0].triangles, [0, 2, 0, 0]);
        assert_eq!(packed.faces[1].triangles, [2, 2, 0, 0]);
        assert_eq!(packed.vertices.len(), 12);
    }
}
