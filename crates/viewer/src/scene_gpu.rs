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
pub(crate) const KIND_SPLINE: u32 = 5;

/// Highest B-spline degree the shader's fixed-size basis window supports.
pub(crate) const MAX_SPLINE_DEGREE: usize = 8;

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
    /// Spline degrees and knot-vector starts: degree_u, degree_v, knot_u, knot_v.
    pub spline0: [u32; 4],
    /// Knot counts, control-point start and patch start.
    pub spline1: [u32; 4],
    /// Patch count in `w`; the rest is reserved.
    pub spline2: [u32; 4],
}

/// One parameter sub-rectangle of a spline patch and its world-space bounds.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuPatch {
    /// `(u0, u1, v0, v1)`.
    pub uv: [f32; 4],
    pub min: [f32; 4],
    pub max: [f32; 4],
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
    /// B-spline and NURBS knot vectors, shared by all spline faces.
    pub knots: Vec<f32>,
    /// Homogeneous spline control points, row-major in `(u, v)`.
    pub control: Vec<[f32; 4]>,
    /// Culling bounds for the parameter sub-rectangles of spline faces.
    pub patches: Vec<GpuPatch>,
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
            &mut packed.knots,
            &mut packed.control,
            &mut packed.patches,
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
    knots: &mut Vec<f32>,
    control: &mut Vec<[f32; 4]>,
    patches: &mut Vec<GpuPatch>,
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
        Some(Surface::Sphere {
            centre,
            radius,
            axis,
            u_reference,
        }) => {
            packed.header[0] = KIND_SPHERE;
            packed.params0 = point_and(centre, *radius);
            packed.params1 = point_and(axis, TAU);
            packed.params2 = point_and(u_reference, 0.0);
        }
        Some(Surface::Spline(surface)) => {
            if !pack_spline(surface, &mut packed, knots, control, patches) {
                return pack_triangles(face, vertices);
            }
            packed.params1[3] = face.trims.u_period().unwrap_or(0.0) as f32;
            packed.params3[3] = face.trims.v_period().unwrap_or(0.0) as f32;
        }
        // Only faces with no modelled surface ride the tessellated path.
        None => return pack_triangles(face, vertices),
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

fn pack_triangles(face: &Face, vertices: &mut Vec<[f32; 4]>) -> GpuFace {
    let packed = GpuFace {
        header: [KIND_TRIANGLES, u32::from(face.flip), 0, 0],
        triangles: [
            (vertices.len() / 3) as u32,
            face.triangles.len() as u32,
            0,
            0,
        ],
        ..Default::default()
    };
    for triangle in &face.triangles {
        for point in triangle {
            vertices.push([point[0] as f32, point[1] as f32, point[2] as f32, 0.0]);
        }
    }
    packed
}

fn pack_spline(
    surface: &brepscad_raycast::SplineSurface,
    packed: &mut GpuFace,
    knots: &mut Vec<f32>,
    control: &mut Vec<[f32; 4]>,
    patches: &mut Vec<GpuPatch>,
) -> bool {
    let (degree_u, degree_v) = surface.degrees();
    if degree_u > MAX_SPLINE_DEGREE || degree_v > MAX_SPLINE_DEGREE || surface.patches().is_empty()
    {
        return false;
    }
    let (knots_u, knots_v) = surface.knot_vecs();
    let knot_u_start = knots.len() as u32;
    knots.extend(knots_u.iter().map(|&value| value as f32));
    let knot_v_start = knots.len() as u32;
    knots.extend(knots_v.iter().map(|&value| value as f32));
    let control_start = control.len() as u32;
    control.extend(
        surface
            .control_points()
            .iter()
            .map(|point| point.map(|v| v as f32)),
    );
    let patch_start = patches.len() as u32;
    patches.extend(surface.patches().iter().map(|patch| GpuPatch {
        uv: [
            patch.u.0 as f32,
            patch.u.1 as f32,
            patch.v.0 as f32,
            patch.v.1 as f32,
        ],
        min: point_and(&patch.min, 0.0),
        max: point_and(&patch.max, 0.0),
    }));

    packed.header[0] = KIND_SPLINE;
    packed.spline0 = [degree_u as u32, degree_v as u32, knot_u_start, knot_v_start];
    packed.spline1 = [
        knots_u.len() as u32,
        knots_v.len() as u32,
        control_start,
        patch_start,
    ];
    packed.spline2 = [surface.patches().len() as u32, 0, 0, 0];
    true
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
    use brepscad_raycast::{Face, SplineSurface, TrimRegion};
    use std::sync::Arc;

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
        assert_eq!(size_of::<GpuFace>(), 144);
        assert_eq!(size_of::<GpuLoop>(), 8);
        assert_eq!(size_of::<GpuPatch>(), 48);
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
    fn packs_oriented_sphere_axis_and_reference() {
        let face = Face::new(
            Surface::Sphere {
                centre: [1.0, 2.0, 3.0],
                radius: 4.0,
                axis: [0.0, 0.0, 1.0],
                u_reference: [1.0, 0.0, 0.0],
            },
            TrimRegion::with_u_period(
                vec![vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]],
                TAU,
            ),
            ([-3.0, -2.0, -1.0], [5.0, 6.0, 7.0]),
        );
        let packed = pack(&Scene::new(vec![face]));
        let sphere = packed.faces[0];

        assert_eq!(sphere.header[0], KIND_SPHERE);
        assert_eq!(sphere.params0, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(sphere.params1[0..3], [0.0, 0.0, 1.0]);
        assert!((sphere.params1[3] - TAU as f32).abs() < 1e-6, "u period");
        assert_eq!(sphere.params2[0..3], [1.0, 0.0, 0.0]);
        assert_eq!(sphere.header[3], 1, "one trim loop");
    }

    #[test]
    fn packs_spline_knots_control_points_and_patches() {
        let surface = SplineSurface::new(
            1,
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![0.0, 0.0, 1.0, 1.0],
            vec![
                [0.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0],
                [1.0, 0.0, 0.0, 1.0],
                [1.0, 1.0, 0.0, 1.0],
            ],
        )
        .expect("bilinear surface");
        let face = Face::new(
            Surface::Spline(Arc::new(surface)),
            TrimRegion::with_periods(
                vec![vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]],
                Some(TAU),
                Some(TAU),
            ),
            ([-0.1, -0.1, -0.1], [1.1, 1.1, 0.1]),
        );
        let packed = pack(&Scene::new(vec![face]));
        let spline = packed.faces[0];

        assert_eq!(spline.header[0], KIND_SPLINE);
        assert_eq!(spline.spline0, [1, 1, 0, 4]);
        assert_eq!(spline.spline1, [4, 4, 0, 0]);
        assert_eq!(spline.spline2[0], 4, "two subdivisions in each direction");
        assert!((spline.params1[3] - TAU as f32).abs() < 1e-6, "u period");
        assert!((spline.params3[3] - TAU as f32).abs() < 1e-6, "v period");
        assert_eq!(packed.knots.len(), 8);
        assert_eq!(packed.control.len(), 4);
        assert_eq!(packed.patches.len(), 4);
        assert_eq!(packed.patches[0].uv, [0.0, 0.5, 0.0, 0.5]);
        assert_eq!(spline.header[3], 1, "one trim loop");
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
