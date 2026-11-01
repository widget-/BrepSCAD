//! CPU ray casting over trimmed analytic B-rep faces.
//!
//! The scene is deliberately neutral: it carries analytic surfaces and their
//! trim loops in surface parameter space, so a kernel adapter can build it from
//! its B-rep and a GPU tracer can upload the same data later. Nothing here
//! depends on Truck.

mod bvh;
mod spline;
mod surface;
mod trim;

pub use bvh::Node;
pub use spline::SplineSurface;
pub use surface::{Surface, SurfaceHit};
pub use trim::TrimRegion;

/// A ray with a normalized direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin: [f64; 3],
    pub direction: [f64; 3],
}

impl Ray {
    pub fn new(origin: [f64; 3], direction: [f64; 3]) -> Self {
        let length = (direction[0] * direction[0]
            + direction[1] * direction[1]
            + direction[2] * direction[2])
            .sqrt();
        debug_assert!(length > 0.0, "ray direction must not be zero");
        let scale = if length > 0.0 { 1.0 / length } else { 0.0 };
        Self {
            origin,
            direction: [
                direction[0] * scale,
                direction[1] * scale,
                direction[2] * scale,
            ],
        }
    }
}

/// One B-rep face: either an analytic surface with boundary loops in that
/// surface's parameter space, or the triangles of a face whose surface the
/// caster does not model (B-splines, NURBS).
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    pub surface: Option<Surface>,
    pub trims: TrimRegion,
    pub triangles: Vec<[[f64; 3]; 3]>,
    pub bounds: ([f64; 3], [f64; 3]),
    /// True when the B-rep face points into the solid, so its normal has to be
    /// flipped. Mirrored solids are stored this way.
    pub flip: bool,
}

impl Face {
    /// An analytic face trimmed by boundary loops.
    pub fn new(surface: Surface, trims: TrimRegion, bounds: ([f64; 3], [f64; 3])) -> Self {
        Self {
            surface: Some(surface),
            trims,
            triangles: Vec::new(),
            bounds,
            flip: false,
        }
    }

    /// A face covered by tessellation because its surface is not modelled.
    pub fn from_triangles(triangles: Vec<[[f64; 3]; 3]>) -> Self {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for triangle in &triangles {
            for point in triangle {
                for axis in 0..3 {
                    min[axis] = min[axis].min(point[axis]);
                    max[axis] = max[axis].max(point[axis]);
                }
            }
        }
        Self {
            surface: None,
            trims: TrimRegion::default(),
            triangles,
            bounds: (min, max),
            flip: false,
        }
    }

    pub fn with_flip(mut self, flip: bool) -> Self {
        self.flip = flip;
        self
    }

    /// Attach tessellated coverage. The CPU caster keeps using the surface; the
    /// GPU tracer, which has no spline intersection yet, renders the triangles.
    pub fn with_triangles(mut self, triangles: Vec<[[f64; 3]; 3]>) -> Self {
        self.triangles = triangles;
        self
    }
}

/// A nearest-hit result in a [`Scene`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub face: usize,
    pub distance: f64,
    pub position: [f64; 3],
    pub normal: [f64; 3],
    /// Surface parameters of the hit, matching the trim loops.
    pub uv: [f64; 2],
}

/// Trimmed analytic faces plus a bounding volume hierarchy over them.
#[derive(Debug, Clone, Default)]
pub struct Scene {
    faces: Vec<Face>,
    nodes: Vec<bvh::Node>,
    order: Vec<u32>,
}

impl Scene {
    pub fn new(faces: Vec<Face>) -> Self {
        let (nodes, order) = bvh::build(&faces);
        Self {
            faces,
            nodes,
            order,
        }
    }

    pub fn faces(&self) -> &[Face] {
        &self.faces
    }

    /// Bounding-volume nodes, in the layout the shader consumes.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Face indices in leaf order; leaf `start..start + count` indexes this.
    pub fn order(&self) -> &[u32] {
        &self.order
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// Nearest hit along the ray that lands inside the face's trim region.
    pub fn cast(&self, ray: &Ray) -> Option<Hit> {
        if self.faces.is_empty() {
            return None;
        }
        let mut best: Option<Hit> = None;
        let mut stack = vec![0_u32];
        let mut candidates = Vec::new();

        while let Some(index) = stack.pop() {
            let node = &self.nodes[index as usize];
            let Some(entry) = bvh::slab_hit(&node.bounds, ray) else {
                continue;
            };
            if let Some(best) = best {
                if entry >= best.distance {
                    continue;
                }
            }
            if !node.is_leaf() {
                stack.push(node.left);
                stack.push(node.right);
                continue;
            }
            for slot in node.start..node.start + node.count {
                let face_index = self.order[slot as usize] as usize;
                let face = &self.faces[face_index];
                candidates.clear();
                if let Some(surface) = &face.surface {
                    surface.intersect(ray, &mut candidates);
                } else {
                    for triangle in &face.triangles {
                        if let Some(candidate) = triangle_hit(ray, *triangle) {
                            candidates.push(candidate);
                        }
                    }
                }
                for candidate in &candidates {
                    if candidate.distance <= 0.0 {
                        continue;
                    }
                    if let Some(best) = best {
                        if candidate.distance >= best.distance {
                            continue;
                        }
                    }
                    if !face.trims.contains(candidate.uv) {
                        continue;
                    }
                    let normal = if face.flip {
                        [
                            -candidate.normal[0],
                            -candidate.normal[1],
                            -candidate.normal[2],
                        ]
                    } else {
                        candidate.normal
                    };
                    best = Some(Hit {
                        face: face_index,
                        distance: candidate.distance,
                        position: candidate.position,
                        normal,
                        uv: candidate.uv,
                    });
                }
            }
        }

        best
    }
}

/// Ray/triangle hit for faces that are covered by tessellation.
fn triangle_hit(ray: &Ray, triangle: [[f64; 3]; 3]) -> Option<SurfaceHit> {
    let [a, b, c] = triangle;
    let edge1 = sub(b, a);
    let edge2 = sub(c, a);
    let pvec = cross(ray.direction, edge2);
    let determinant = dot(edge1, pvec);
    if determinant.abs() < 1.0e-12 {
        return None;
    }
    let inverse = 1.0 / determinant;
    let tvec = sub(ray.origin, a);
    let u = dot(tvec, pvec) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let qvec = cross(tvec, edge1);
    let v = dot(ray.direction, qvec) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = dot(edge2, qvec) * inverse;
    if distance <= 0.0 {
        return None;
    }
    let normal = cross(edge1, edge2);
    let normal = normalize(normal);
    let u_axis = normalize(edge1);
    let v_axis = cross(normal, u_axis);
    let local = sub(along(ray.origin, ray.direction, distance), a);
    Some(SurfaceHit {
        distance,
        position: along(ray.origin, ray.direction, distance),
        normal,
        uv: [dot(local, u_axis), dot(local, v_axis)],
    })
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn along(origin: [f64; 3], direction: [f64; 3], t: f64) -> [f64; 3] {
    [
        origin[0] + direction[0] * t,
        origin[1] + direction[1] * t,
        origin[2] + direction[2] * t,
    ]
}

fn normalize(a: [f64; 3]) -> [f64; 3] {
    let length = dot(a, a).sqrt();
    if length > 0.0 {
        [a[0] / length, a[1] / length, a[2] / length]
    } else {
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_plane() -> Surface {
        Surface::Plane {
            origin: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            u_axis: [1.0, 0.0, 0.0],
            v_axis: [0.0, 1.0, 0.0],
        }
    }

    fn plane_face(size: f64) -> Face {
        let half = size / 2.0;
        Face::new(
            square_plane(),
            TrimRegion::new(vec![vec![
                [-half, -half],
                [half, -half],
                [half, half],
                [-half, half],
            ]]),
            ([-half, -half, 0.0], [half, half, 0.0]),
        )
    }

    #[test]
    fn ray_hits_inside_the_trim_region() {
        let scene = Scene::new(vec![plane_face(2.0)]);
        let ray = Ray::new([0.0, 0.0, 5.0], [0.0, 0.0, -1.0]);
        let hit = scene.cast(&ray).expect("hit");
        assert!((hit.distance - 5.0).abs() < 1e-9);
        assert_eq!(hit.face, 0);
        assert_eq!(hit.normal, [0.0, 0.0, 1.0]);
        assert_eq!(hit.position, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn ray_misses_outside_the_trim_region() {
        let scene = Scene::new(vec![plane_face(2.0)]);
        let ray = Ray::new([5.0, 0.0, 5.0], [0.0, 0.0, -1.0]);
        assert!(scene.cast(&ray).is_none());
    }

    #[test]
    fn ray_through_a_hole_misses() {
        let mut face = plane_face(4.0);
        face.trims = TrimRegion::new(vec![
            vec![[-2.0, -2.0], [2.0, -2.0], [2.0, 2.0], [-2.0, 2.0]],
            vec![[-1.0, -1.0], [-1.0, 1.0], [1.0, 1.0], [1.0, -1.0]],
        ]);
        let scene = Scene::new(vec![face]);

        let centre = Ray::new([0.0, 0.0, 5.0], [0.0, 0.0, -1.0]);
        assert!(scene.cast(&centre).is_none());

        let ring = Ray::new([1.5, 0.0, 5.0], [0.0, 0.0, -1.0]);
        assert!(scene.cast(&ring).is_some());
    }

    #[test]
    fn returns_the_nearest_face() {
        let near = plane_face(2.0);
        let mut far = plane_face(2.0);
        far.surface = Some(Surface::Plane {
            origin: [0.0, 0.0, -3.0],
            normal: [0.0, 0.0, 1.0],
            u_axis: [1.0, 0.0, 0.0],
            v_axis: [0.0, 1.0, 0.0],
        });
        far.bounds = ([-1.0, -1.0, -3.0], [1.0, 1.0, -3.0]);
        let scene = Scene::new(vec![far, near]);

        let ray = Ray::new([0.0, 0.0, 5.0], [0.0, 0.0, -1.0]);
        let hit = scene.cast(&ray).expect("hit");
        assert_eq!(hit.face, 1, "the near face is at the origin");
        assert!((hit.distance - 5.0).abs() < 1e-9);
    }

    #[test]
    fn cylinder_side_is_hit_at_the_exact_radius() {
        let surface = Surface::Cylinder {
            origin: [0.0, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: 1.0,
            u_reference: [1.0, 0.0, 0.0],
        };
        let face = Face::new(
            surface,
            TrimRegion::new(vec![vec![
                [0.0, 0.0],
                [std::f64::consts::TAU, 0.0],
                [std::f64::consts::TAU, 2.0],
                [0.0, 2.0],
            ]]),
            ([-1.0, -1.0, 0.0], [1.0, 1.0, 2.0]),
        );
        let scene = Scene::new(vec![face]);

        let ray = Ray::new([-5.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        let hit = scene.cast(&ray).expect("hit");
        assert!((hit.position[0] + 1.0).abs() < 1e-9, "{:?}", hit.position);
        assert!((hit.distance - 4.0).abs() < 1e-9);
        assert!((hit.normal[0] + 1.0).abs() < 1e-9, "{:?}", hit.normal);
        // u is measured from +X, and the ray meets the -X side first.
        assert!(
            (hit.uv[0] - std::f64::consts::PI).abs() < 1e-9,
            "u = {}",
            hit.uv[0]
        );
        assert!((hit.uv[1] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn cylinder_trim_limits_the_height() {
        let surface = Surface::Cylinder {
            origin: [0.0, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: 1.0,
            u_reference: [1.0, 0.0, 0.0],
        };
        let face = Face::new(
            surface,
            TrimRegion::new(vec![vec![
                [0.0, 0.0],
                [std::f64::consts::TAU, 0.0],
                [std::f64::consts::TAU, 2.0],
                [0.0, 2.0],
            ]]),
            ([-1.0, -1.0, 0.0], [1.0, 1.0, 2.0]),
        );
        let scene = Scene::new(vec![face]);

        let above = Ray::new([-5.0, 0.0, 2.5], [1.0, 0.0, 0.0]);
        assert!(scene.cast(&above).is_none());
    }
}
