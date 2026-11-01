//! CPU ray picking against operand meshes and analytic part scenes.

use brepscad_kernel_api::Mesh;
use brepscad_raycast::{Ray, Scene};
use glam::Vec3;

#[derive(Debug, Clone, Copy)]
pub(crate) struct PickPart {
    pub(crate) shape_index: usize,
    pub(crate) part_index: usize,
    pub(crate) triangle_start: usize,
    pub(crate) triangle_count: usize,
}

pub(crate) fn nearest_triangle(mesh: &Mesh, origin: Vec3, direction: Vec3) -> Option<(usize, f32)> {
    let mut nearest: Option<(f32, usize)> = None;

    for (index, triangle) in mesh.indices.chunks_exact(3).enumerate() {
        if let Some(distance) = ray_triangle(mesh, triangle, origin, direction) {
            if nearest.is_none_or(|(best, _)| distance < best) {
                nearest = Some((distance, index));
            }
        }
    }

    nearest.map(|(distance, index)| (index, distance))
}

/// Distance to the nearest hit in an analytic part scene.
pub(crate) fn nearest_scene(scene: &Scene, origin: Vec3, direction: Vec3) -> Option<f32> {
    let ray = Ray::new(
        origin.to_array().map(f64::from),
        direction.to_array().map(f64::from),
    );
    scene.cast(&ray).map(|hit| hit.distance as f32)
}

fn ray_triangle(mesh: &Mesh, triangle: &[u32], origin: Vec3, direction: Vec3) -> Option<f32> {
    let a = mesh_point(mesh, triangle[0]);
    let b = mesh_point(mesh, triangle[1]);
    let c = mesh_point(mesh, triangle[2]);

    let edge1 = b - a;
    let edge2 = c - a;
    let h = direction.cross(edge2);
    let det = edge1.dot(h);
    if det.abs() < 1.0e-6 {
        return None;
    }

    let inv_det = 1.0 / det;
    let s = origin - a;
    let u = inv_det * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    let q = s.cross(edge1);
    let v = inv_det * direction.dot(q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let t = inv_det * edge2.dot(q);
    (t > 0.0).then_some(t)
}

fn mesh_point(mesh: &Mesh, index: u32) -> Vec3 {
    let point = mesh.positions[index as usize];
    Vec3::new(point[0] as f32, point[1] as f32, point[2] as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle_mesh() -> Mesh {
        Mesh {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: Vec::new(),
            indices: vec![0, 1, 2],
        }
    }

    #[test]
    fn ray_hits_a_triangle() {
        let mesh = triangle_mesh();
        let hit = nearest_triangle(&mesh, Vec3::new(0.2, 0.2, 1.0), Vec3::new(0.0, 0.0, -1.0));
        assert_eq!(hit.map(|(index, _)| index), Some(0));
        assert!((hit.unwrap().1 - 1.0).abs() < 1e-6, "distance to z = 0");
    }

    #[test]
    fn ray_misses_a_triangle() {
        let mesh = triangle_mesh();
        let hit = nearest_triangle(&mesh, Vec3::new(2.0, 2.0, 1.0), Vec3::new(0.0, 0.0, -1.0));
        assert_eq!(hit, None);
    }

    #[test]
    fn scene_cast_reports_the_distance() {
        use brepscad_raycast::{Face, Surface, TrimRegion};

        let face = Face::new(
            Surface::Plane {
                origin: [0.0, 0.0, 0.0],
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
            ([-1.0, -1.0, 0.0], [1.0, 1.0, 0.0]),
        );
        let scene = Scene::new(vec![face]);

        let hit = nearest_scene(&scene, Vec3::new(0.0, 0.0, 3.0), Vec3::new(0.0, 0.0, -1.0));
        assert!((hit.expect("hit") - 3.0).abs() < 1e-6);

        let miss = nearest_scene(&scene, Vec3::new(5.0, 0.0, 3.0), Vec3::new(0.0, 0.0, -1.0));
        assert!(miss.is_none(), "outside the trim region");
    }
}
