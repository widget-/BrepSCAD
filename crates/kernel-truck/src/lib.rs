//! Truck B-rep backend for the kernel API.
//!
//! This first adapter covers the primitives, boolean operations, and
//! tessellation needed by the walking-skeleton slice.

mod primitives;

use brepscad_kernel_api::{
    BooleanOp, Capabilities, Kernel, KernelError, Mesh, Primitive, ProfileRegion, ShapeId, Sketch,
    Transform,
};
use primitives::{cuboid, cylinder, polyhedron, sphere};
use truck_meshalgo::prelude::*;
use truck_modeling::*;

type KernelResult<T> = std::result::Result<T, KernelError>;

const BOOLEAN_TOLERANCES: [f64; 3] = [0.05, 0.02, 0.01];

#[derive(Debug)]
enum OwnedShape {
    Face(Face),
    Faces(Vec<Face>),
    Solid(Solid),
}

/// A kernel that owns Truck faces and solids in a simple arena.
#[derive(Debug, Default)]
pub struct TruckKernel {
    shapes: Vec<OwnedShape>,
}

impl TruckKernel {
    pub fn new() -> Self {
        Self::default()
    }

    fn insert(&mut self, shape: OwnedShape) -> ShapeId {
        let id = ShapeId(self.shapes.len() as u64);
        self.shapes.push(shape);
        id
    }

    fn shape(&self, id: ShapeId) -> KernelResult<&OwnedShape> {
        self.shapes
            .get(id.0 as usize)
            .ok_or(KernelError::UnknownShape(id))
    }

    fn solid(&self, id: ShapeId) -> KernelResult<&Solid> {
        match self.shape(id)? {
            OwnedShape::Solid(solid) => Ok(solid),
            OwnedShape::Face(_) | OwnedShape::Faces(_) => Err(KernelError::Backend(
                "operation requires a solid, not a sketch".to_owned(),
            )),
        }
    }

    fn faces(&self, id: ShapeId) -> KernelResult<&[Face]> {
        match self.shape(id)? {
            OwnedShape::Face(face) => Ok(std::slice::from_ref(face)),
            OwnedShape::Faces(faces) => Ok(faces),
            OwnedShape::Solid(_) => Err(KernelError::Backend(
                "operation requires a sketch, not a solid".to_owned(),
            )),
        }
    }
}

impl Kernel for TruckKernel {
    fn name(&self) -> &'static str {
        "truck"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            primitives: true,
            sketches: true,
            extrusions: true,
            transforms: true,
            booleans: true,
            tessellation: true,
            ..Capabilities::default()
        }
    }

    fn primitive(&mut self, primitive: Primitive) -> KernelResult<ShapeId> {
        let shape = match primitive {
            Primitive::Cuboid { size } => cuboid(size)?,
            Primitive::Cylinder { radius, height } => cylinder(radius, height)?,
            Primitive::Sphere { radius } => sphere(radius)?,
            Primitive::Polyhedron { points, faces } => polyhedron(&points, &faces)?,
        };
        Ok(self.insert(OwnedShape::Solid(shape)))
    }

    fn sketch(&mut self, sketch: Sketch) -> KernelResult<ShapeId> {
        let face = match sketch {
            Sketch::Square { size } => primitives::square(size)?,
            Sketch::Circle { radius } => primitives::circle(radius)?,
            Sketch::Polygon { points } => primitives::polygon(&points)?,
        };
        Ok(self.insert(OwnedShape::Face(face)))
    }

    fn sketch_profile(&mut self, regions: &[ProfileRegion]) -> KernelResult<ShapeId> {
        if regions.is_empty() {
            return Err(KernelError::Backend(
                "profile sketch needs at least one region".to_owned(),
            ));
        }
        let mut faces = Vec::with_capacity(regions.len());
        for region in regions {
            faces.push(primitives::profile_face(region)?);
        }
        let shape = if faces.len() == 1 {
            OwnedShape::Face(faces.remove(0))
        } else {
            OwnedShape::Faces(faces)
        };
        Ok(self.insert(shape))
    }

    fn extrude(&mut self, shape: ShapeId, height: f64) -> KernelResult<ShapeId> {
        if height <= 0.0 {
            return Err(KernelError::Backend(
                "extrusion height must be positive".to_owned(),
            ));
        }
        let solids = self
            .faces(shape)?
            .iter()
            .map(|face| extrude_face(face, height))
            .collect::<Vec<_>>();
        let solid = combine_solids(solids)?;
        Ok(self.insert(OwnedShape::Solid(solid)))
    }

    fn revolve(&mut self, shape: ShapeId, start: f64, angle: f64) -> KernelResult<ShapeId> {
        if !start.is_finite() {
            return Err(KernelError::Backend(
                "revolve start must be finite".to_owned(),
            ));
        }
        if !angle.is_finite()
            || angle.abs() < 1.0e-9
            || angle.abs() > std::f64::consts::TAU + 1.0e-9
        {
            return Err(KernelError::Backend(
                "revolve angle must be in (0, 2*pi]".to_owned(),
            ));
        }
        let solids = self
            .faces(shape)?
            .iter()
            .map(|face| revolve_face(face, start, angle))
            .collect::<Vec<_>>();
        let solid = combine_solids(solids)?;
        Ok(self.insert(OwnedShape::Solid(solid)))
    }

    fn transform(&mut self, shape: ShapeId, transform: Transform) -> KernelResult<ShapeId> {
        let shape = self.shape(shape)?;
        let matrix = matrix4(transform);
        let transformed = match shape {
            OwnedShape::Solid(solid) => {
                let mut transformed = builder::transformed(solid, matrix);
                // A reflection reverses handedness, so the mapped faces need
                // their orientation restored.
                if transform.determinant() < 0.0 {
                    transformed.not();
                }
                OwnedShape::Solid(transformed)
            }
            OwnedShape::Face(face) => OwnedShape::Face(builder::transformed(face, matrix)),
            OwnedShape::Faces(faces) => OwnedShape::Faces(
                faces
                    .iter()
                    .map(|face| builder::transformed(face, matrix))
                    .collect(),
            ),
        };
        Ok(self.insert(transformed))
    }

    fn boolean(&mut self, op: BooleanOp, a: ShapeId, b: ShapeId) -> KernelResult<ShapeId> {
        let a = self.solid(a)?.clone();
        let b = self.solid(b)?.clone();

        for tolerance in BOOLEAN_TOLERANCES {
            let result = match op {
                BooleanOp::Union => truck_shapeops::or(&a, &b, tolerance),
                BooleanOp::Intersection => truck_shapeops::and(&a, &b, tolerance),
                BooleanOp::Difference => {
                    let mut inverted = b.clone();
                    inverted.not();
                    truck_shapeops::and(&a, &inverted, tolerance)
                }
            };
            if let Some(result) = result {
                return Ok(self.insert(OwnedShape::Solid(result)));
            }
        }

        Err(KernelError::Backend(
            "boolean operation returned no solid at any tolerance".to_owned(),
        ))
    }

    fn tessellate(&self, shape: ShapeId, tolerance: f64) -> KernelResult<Mesh> {
        let mut mesh = match self.shape(shape)? {
            OwnedShape::Solid(solid) => solid.triangulation(tolerance).to_polygon(),
            OwnedShape::Face(face) => tessellate_face(face, tolerance),
            OwnedShape::Faces(faces) => {
                let mut mesh = PolygonMesh::default();
                for face in faces {
                    mesh.merge(tessellate_face(face, tolerance));
                }
                mesh
            }
        };
        mesh.put_together_same_attrs(TOLERANCE)
            .remove_degenerate_faces()
            .remove_unused_attrs();
        Ok(to_neutral_mesh(&mesh))
    }

    fn bounds(&self, shape: ShapeId, tolerance: f64) -> KernelResult<([f64; 3], [f64; 3])> {
        let mesh = self.tessellate(shape, tolerance)?;
        mesh_bounds(&mesh).ok_or_else(|| KernelError::Backend("shape has no geometry".to_owned()))
    }
}

fn mesh_bounds(mesh: &Mesh) -> Option<([f64; 3], [f64; 3])> {
    if mesh.positions.is_empty() {
        return None;
    }

    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for position in &mesh.positions {
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
    }
    Some((min, max))
}

fn tessellate_face(face: &Face, tolerance: f64) -> PolygonMesh {
    let shell: Shell = vec![face.clone()].into_iter().collect();
    let meshed = shell.triangulation(tolerance);
    let mut mesh = PolygonMesh::default();
    for poly_face in meshed.iter() {
        if let Some(part) = poly_face.surface() {
            mesh.merge(part);
        }
    }
    mesh
}

fn extrude_face(face: &Face, height: f64) -> Solid {
    let centered = builder::translated(face, Vector3::new(0.0, 0.0, -height / 2.0));
    builder::tsweep(&centered, Vector3::new(0.0, 0.0, height))
}

fn revolve_face(face: &Face, start: f64, angle: f64) -> Solid {
    // rotate the XY profile into XZ: local Y becomes the revolve height
    let rotation = Matrix4::from_axis_angle(Vector3::unit_x(), Rad(std::f64::consts::FRAC_PI_2));
    let profile = builder::transformed(face, rotation);
    let profile = if start == 0.0 {
        profile
    } else {
        builder::transformed(
            &profile,
            Matrix4::from_axis_angle(Vector3::unit_z(), Rad(start)),
        )
    };
    let mut solid = builder::rsweep(&profile, Point3::origin(), Vector3::unit_z(), Rad(angle));
    if angle > 0.0 {
        solid.not();
    }
    solid
}

fn combine_solids(solids: Vec<Solid>) -> KernelResult<Solid> {
    let mut boundaries = Vec::new();
    for solid in solids {
        boundaries.extend(solid.into_boundaries());
    }
    Solid::try_new(boundaries)
        .map_err(|error| KernelError::Backend(format!("extruded profile: {error:?}")))
}

fn matrix4(transform: Transform) -> Matrix4 {
    let cols = transform.cols;
    Matrix4::new(
        cols[0][0], cols[0][1], cols[0][2], cols[0][3], cols[1][0], cols[1][1], cols[1][2],
        cols[1][3], cols[2][0], cols[2][1], cols[2][2], cols[2][3], cols[3][0], cols[3][1],
        cols[3][2], cols[3][3],
    )
}

fn to_neutral_mesh(mesh: &PolygonMesh) -> Mesh {
    let positions = mesh
        .positions()
        .iter()
        .map(|point| [point.x, point.y, point.z])
        .collect();

    let mut indices = Vec::new();
    for face in mesh.tri_faces() {
        indices.extend(face.iter().map(|vertex| vertex.pos as u32));
    }
    for face in mesh.quad_faces() {
        indices.extend([face[0].pos as u32, face[1].pos as u32, face[2].pos as u32]);
        indices.extend([face[0].pos as u32, face[2].pos as u32, face[3].pos as u32]);
    }
    for face in mesh.other_faces() {
        for index in 1..face.len().saturating_sub(1) {
            indices.extend([
                face[0].pos as u32,
                face[index].pos as u32,
                face[index + 1].pos as u32,
            ]);
        }
    }

    Mesh { positions, indices }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_advertise_the_implemented_slice() {
        let kernel = TruckKernel::new();
        let capabilities = kernel.capabilities();
        assert!(capabilities.primitives);
        assert!(capabilities.sketches);
        assert!(capabilities.extrusions);
        assert!(capabilities.transforms);
        assert!(capabilities.booleans);
        assert!(capabilities.tessellation);
        assert!(!capabilities.fillet);
        assert!(!capabilities.step_io);
    }

    #[test]
    fn translates_a_cube() {
        let mut kernel = TruckKernel::new();
        let cube = kernel
            .primitive(Primitive::Cuboid {
                size: [2.0, 2.0, 2.0],
            })
            .expect("cube");
        let moved = kernel
            .transform(cube, Transform::from_translation([1.0, 0.0, 0.0]))
            .expect("translate");
        let mesh = kernel.tessellate(moved, 0.1).expect("tessellation");

        let min_x = mesh
            .positions
            .iter()
            .map(|position| position[0])
            .fold(f64::INFINITY, f64::min);
        assert!((min_x - 0.0).abs() < 1e-6, "min_x = {min_x}");
    }

    #[test]
    fn cylinder_is_centered_on_the_z_axis() {
        let mut kernel = TruckKernel::new();
        let cylinder = kernel
            .primitive(Primitive::Cylinder {
                radius: 0.5,
                height: 4.0,
            })
            .expect("cylinder");
        let mesh = kernel.tessellate(cylinder, 0.01).expect("tessellation");

        let mut min_z = f64::INFINITY;
        let mut max_z = f64::NEG_INFINITY;
        let mut max_radial: f64 = 0.0;
        for position in &mesh.positions {
            min_z = min_z.min(position[2]);
            max_z = max_z.max(position[2]);
            max_radial = max_radial.max(position[0].hypot(position[1]));
        }

        assert!((min_z + 2.0).abs() < 1e-6, "min_z = {min_z}");
        assert!((max_z - 2.0).abs() < 1e-6, "max_z = {max_z}");
        assert!((max_radial - 0.5).abs() < 1e-6, "max_radial = {max_radial}");
    }

    #[test]
    fn mirrored_solids_keep_outward_orientation() {
        let mut kernel = TruckKernel::new();
        let cube = kernel
            .primitive(Primitive::Cuboid {
                size: [2.0, 2.0, 2.0],
            })
            .expect("cube");
        let mirror = Transform::from_mirror([1.0, 0.0, 0.0]).expect("normal");
        let mirrored = kernel.transform(cube, mirror).expect("mirror");
        let mesh = kernel.tessellate(mirrored, 0.01).expect("tessellation");

        let volume = signed_volume(&mesh);
        assert!(volume > 0.0, "volume = {volume}");
    }

    fn signed_volume(mesh: &Mesh) -> f64 {
        mesh.indices
            .chunks_exact(3)
            .map(|triangle| {
                let a = mesh.positions[triangle[0] as usize];
                let b = mesh.positions[triangle[1] as usize];
                let c = mesh.positions[triangle[2] as usize];
                a[0] * (b[1] * c[2] - b[2] * c[1])
                    + a[1] * (b[2] * c[0] - b[0] * c[2])
                    + a[2] * (b[0] * c[1] - b[1] * c[0])
            })
            .sum::<f64>()
            / 6.0
    }

    #[test]
    fn cube_minus_cylinder_tessellates() {
        let mut kernel = TruckKernel::new();
        let cube = kernel
            .primitive(Primitive::Cuboid {
                size: [2.0, 2.0, 2.0],
            })
            .expect("cube");
        let cylinder = kernel
            .primitive(Primitive::Cylinder {
                radius: 0.5,
                height: 4.0,
            })
            .expect("cylinder");
        let result = kernel
            .boolean(BooleanOp::Difference, cube, cylinder)
            .expect("difference");
        let mesh = kernel.tessellate(result, 0.01).expect("tessellation");

        assert!(!mesh.positions.is_empty());
        assert!(mesh.triangle_count() > 0);
        assert_eq!(mesh.indices.len() % 3, 0);
    }

    #[test]
    fn retries_boolean_with_finer_tolerances() {
        let mut kernel = TruckKernel::new();
        for height in [2.01, 2.1] {
            let cube = kernel
                .primitive(Primitive::Cuboid {
                    size: [2.0, 2.0, 2.0],
                })
                .expect("cube");
            let cylinder = kernel
                .primitive(Primitive::Cylinder {
                    radius: 0.5,
                    height,
                })
                .expect("cylinder");
            let result = kernel
                .boolean(BooleanOp::Difference, cube, cylinder)
                .expect("difference");
            let mesh = kernel.tessellate(result, 0.01).expect("tessellation");

            assert!(mesh.triangle_count() > 0);
            let volume = signed_volume(&mesh);
            assert!(volume > 0.0, "height {height}: volume = {volume}");
        }
    }

    #[test]
    fn sphere_is_centered_and_round() {
        let mut kernel = TruckKernel::new();
        let sphere = kernel
            .primitive(Primitive::Sphere { radius: 1.0 })
            .expect("sphere");
        let mesh = kernel.tessellate(sphere, 0.01).expect("tessellation");

        let (min, max) = bounds(&mesh);
        for axis in 0..3 {
            assert!((min[axis] + 1.0).abs() < 0.01, "min = {min:?}");
            assert!((max[axis] - 1.0).abs() < 0.01, "max = {max:?}");
        }

        let volume = signed_volume(&mesh);
        let expected = 4.0 / 3.0 * std::f64::consts::PI;
        assert!(
            (volume - expected).abs() < expected * 0.02,
            "volume = {volume}"
        );
    }

    #[test]
    fn cube_minus_sphere_tessellates() {
        let mut kernel = TruckKernel::new();
        let cube = kernel
            .primitive(Primitive::Cuboid {
                size: [2.0, 2.0, 2.0],
            })
            .expect("cube");
        let sphere = kernel
            .primitive(Primitive::Sphere { radius: 1.2 })
            .expect("sphere");
        let result = kernel
            .boolean(BooleanOp::Difference, cube, sphere)
            .expect("difference");
        let mesh = kernel.tessellate(result, 0.01).expect("tessellation");

        assert!(mesh.triangle_count() > 0);
        assert!(signed_volume(&mesh) > 0.0);
    }

    #[test]
    fn square_and_circle_sketches_are_planar() {
        let mut kernel = TruckKernel::new();

        let square = kernel
            .sketch(Sketch::Square { size: [2.0, 4.0] })
            .expect("square");
        let mesh = kernel.tessellate(square, 0.01).expect("tessellation");
        let (min, max) = bounds(&mesh);
        assert!((min[0] + 1.0).abs() < 1e-6, "min = {min:?}");
        assert!((max[0] - 1.0).abs() < 1e-6, "max = {max:?}");
        assert!((min[1] + 2.0).abs() < 1e-6);
        assert!((max[1] - 2.0).abs() < 1e-6);
        assert!(min[2].abs() < 1e-6 && max[2].abs() < 1e-6);
        assert!(mesh.triangle_count() > 0);

        let circle = kernel
            .sketch(Sketch::Circle { radius: 1.0 })
            .expect("circle");
        let mesh = kernel.tessellate(circle, 0.01).expect("tessellation");
        let (min, max) = bounds(&mesh);
        assert!((min[0] + 1.0).abs() < 0.01, "min = {min:?}");
        assert!((max[1] - 1.0).abs() < 0.01, "max = {max:?}");
        assert!(min[2].abs() < 1e-6 && max[2].abs() < 1e-6);
    }

    #[test]
    fn polygon_sketch_tessellates() {
        let mut kernel = TruckKernel::new();
        let sketch = kernel
            .sketch(Sketch::Polygon {
                points: vec![[0.0, 0.0], [2.0, 0.0], [0.0, 2.0]],
            })
            .expect("polygon");
        let mesh = kernel.tessellate(sketch, 0.01).expect("tessellation");
        let (min, max) = bounds(&mesh);

        assert!((min[0] - 0.0).abs() < 1e-6);
        assert!((max[0] - 2.0).abs() < 1e-6);
        assert!((max[1] - 2.0).abs() < 1e-6);
        assert!(min[2].abs() < 1e-6 && max[2].abs() < 1e-6);
        assert!(mesh.triangle_count() > 0);
    }

    #[test]
    fn extrudes_a_square() {
        let mut kernel = TruckKernel::new();
        let sketch = kernel
            .sketch(Sketch::Square { size: [2.0, 2.0] })
            .expect("square");
        let solid = kernel.extrude(sketch, 2.0).expect("extrude");
        let mesh = kernel.tessellate(solid, 0.01).expect("tessellation");
        let (min, max) = bounds(&mesh);

        assert!((min[2] + 1.0).abs() < 1e-6, "min = {min:?}");
        assert!((max[2] - 1.0).abs() < 1e-6, "max = {max:?}");
        let volume = signed_volume(&mesh);
        assert!((volume - 8.0).abs() < 0.01, "volume = {volume}");
    }

    #[test]
    fn extrudes_a_circle() {
        let mut kernel = TruckKernel::new();
        let sketch = kernel
            .sketch(Sketch::Circle { radius: 1.0 })
            .expect("circle");
        let solid = kernel.extrude(sketch, 2.0).expect("extrude");
        let mesh = kernel.tessellate(solid, 0.01).expect("tessellation");
        let (min, max) = bounds(&mesh);

        assert!((min[2] + 1.0).abs() < 1e-6, "min = {min:?}");
        assert!((max[2] - 1.0).abs() < 1e-6, "max = {max:?}");
        let volume = signed_volume(&mesh);
        let expected = 2.0 * std::f64::consts::PI;
        assert!(
            (volume - expected).abs() < expected * 0.02,
            "volume = {volume}"
        );
    }

    #[test]
    fn transforms_sketches_before_extrusion() {
        let mut kernel = TruckKernel::new();
        let sketch = kernel
            .sketch(Sketch::Square { size: [2.0, 2.0] })
            .expect("square");
        let moved = kernel
            .transform(sketch, Transform::from_translation([1.0, 0.0, 0.0]))
            .expect("translate");
        let solid = kernel.extrude(moved, 2.0).expect("extrude");
        let mesh = kernel.tessellate(solid, 0.01).expect("tessellation");
        let (min, max) = bounds(&mesh);

        assert!((min[0] - 0.0).abs() < 1e-6, "min = {min:?}");
        assert!((max[0] - 2.0).abs() < 1e-6, "max = {max:?}");
    }

    #[test]
    fn boolean_rejects_sketches() {
        let mut kernel = TruckKernel::new();
        let cube = kernel
            .primitive(Primitive::Cuboid {
                size: [2.0, 2.0, 2.0],
            })
            .expect("cube");
        let sketch = kernel
            .sketch(Sketch::Square { size: [2.0, 2.0] })
            .expect("square");

        let error = kernel
            .boolean(BooleanOp::Union, cube, sketch)
            .expect_err("sketch boolean");
        assert!(matches!(error, KernelError::Backend(message) if message.contains("sketch")));
    }

    #[test]
    fn revolves_a_profile_into_a_tube() {
        let mut kernel = TruckKernel::new();
        let sketch = kernel
            .sketch(Sketch::Square { size: [1.0, 2.0] })
            .expect("square");
        let profile = kernel
            .transform(sketch, Transform::from_translation([1.0, 0.0, 0.0]))
            .expect("translate");
        let solid = kernel
            .revolve(profile, 0.0, std::f64::consts::TAU)
            .expect("revolve");
        let mesh = kernel.tessellate(solid, 0.01).expect("tessellation");
        let (min, max) = bounds(&mesh);

        assert!((min[2] + 1.0).abs() < 1e-6, "min = {min:?}");
        assert!((max[2] - 1.0).abs() < 1e-6, "max = {max:?}");
        let volume = signed_volume(&mesh);
        let expected = 4.0 * std::f64::consts::PI;
        assert!(
            (volume - expected).abs() < expected * 0.02,
            "volume = {volume}"
        );
    }

    #[test]
    fn revolves_a_circle_into_a_torus() {
        let mut kernel = TruckKernel::new();
        let sketch = kernel
            .sketch(Sketch::Circle { radius: 1.0 })
            .expect("circle");
        let profile = kernel
            .transform(sketch, Transform::from_translation([2.0, 0.0, 0.0]))
            .expect("translate");
        let solid = kernel
            .revolve(profile, 0.0, std::f64::consts::TAU)
            .expect("revolve");
        let mesh = kernel.tessellate(solid, 0.01).expect("tessellation");

        let volume = signed_volume(&mesh);
        let expected = 4.0 * std::f64::consts::PI * std::f64::consts::PI;
        assert!(
            (volume - expected).abs() < expected * 0.02,
            "volume = {volume}"
        );
    }

    #[test]
    fn partial_revolve_produces_a_solid() {
        let mut kernel = TruckKernel::new();
        let sketch = kernel
            .sketch(Sketch::Square { size: [1.0, 2.0] })
            .expect("square");
        let profile = kernel
            .transform(sketch, Transform::from_translation([1.0, 0.0, 0.0]))
            .expect("translate");
        let solid = kernel
            .revolve(profile, 0.0, std::f64::consts::FRAC_PI_2)
            .expect("revolve");
        let mesh = kernel.tessellate(solid, 0.01).expect("tessellation");

        let volume = signed_volume(&mesh);
        let expected = std::f64::consts::PI;
        assert!(
            (volume - expected).abs() < expected * 0.02,
            "volume = {volume}"
        );
    }

    #[test]
    fn revolve_start_rotates_the_sweep() {
        let mut kernel = TruckKernel::new();
        let sketch = kernel
            .sketch(Sketch::Square { size: [1.0, 2.0] })
            .expect("square");
        let profile = kernel
            .transform(sketch, Transform::from_translation([1.0, 0.0, 0.0]))
            .expect("translate");
        let solid = kernel
            .revolve(
                profile,
                std::f64::consts::FRAC_PI_2,
                std::f64::consts::FRAC_PI_2,
            )
            .expect("revolve");
        let mesh = kernel.tessellate(solid, 0.01).expect("tessellation");
        let (min, max) = bounds(&mesh);

        assert!(max[0] <= 1.0e-6, "max = {max:?}");
        assert!(min[0] < -1.4, "min = {min:?}");
        assert!(max[1] > 1.4, "max = {max:?}");
        let volume = signed_volume(&mesh);
        let expected = std::f64::consts::PI;
        assert!(
            (volume - expected).abs() < expected * 0.02,
            "volume = {volume}"
        );
    }

    #[test]
    fn revolve_rejects_a_non_finite_start() {
        let mut kernel = TruckKernel::new();
        let sketch = kernel
            .sketch(Sketch::Square { size: [1.0, 2.0] })
            .expect("square");
        let profile = kernel
            .transform(sketch, Transform::from_translation([1.0, 0.0, 0.0]))
            .expect("translate");

        let error = kernel
            .revolve(profile, f64::NAN, std::f64::consts::TAU)
            .expect_err("non-finite start");
        assert!(matches!(error, KernelError::Backend(message) if message.contains("start")));
    }

    #[test]
    fn negative_revolve_keeps_outward_orientation() {
        let mut kernel = TruckKernel::new();
        let sketch = kernel
            .sketch(Sketch::Square { size: [1.0, 2.0] })
            .expect("square");
        let profile = kernel
            .transform(sketch, Transform::from_translation([1.0, 0.0, 0.0]))
            .expect("translate");
        let solid = kernel
            .revolve(profile, 0.0, -std::f64::consts::FRAC_PI_2)
            .expect("revolve");
        let mesh = kernel.tessellate(solid, 0.01).expect("tessellation");

        let volume = signed_volume(&mesh);
        let expected = std::f64::consts::PI;
        assert!(
            (volume - expected).abs() < expected * 0.02,
            "volume = {volume}"
        );
    }

    #[test]
    fn revolve_rejects_solids() {
        let mut kernel = TruckKernel::new();
        let cube = kernel
            .primitive(Primitive::Cuboid {
                size: [2.0, 2.0, 2.0],
            })
            .expect("cube");

        let error = kernel
            .revolve(cube, 0.0, std::f64::consts::TAU)
            .expect_err("solid revolve");
        assert!(matches!(error, KernelError::Backend(message) if message.contains("sketch")));
    }

    #[test]
    fn polyhedron_builds_a_tetrahedron() {
        let mut kernel = TruckKernel::new();
        let points = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let faces = vec![vec![0, 2, 1], vec![0, 1, 3], vec![0, 3, 2], vec![1, 2, 3]];
        let solid = kernel
            .primitive(Primitive::Polyhedron { points, faces })
            .expect("polyhedron");
        let mesh = kernel.tessellate(solid, 0.01).expect("tessellation");

        let volume = signed_volume(&mesh);
        assert!((volume - 1.0 / 6.0).abs() < 1e-9, "volume = {volume}");
    }

    #[test]
    fn polyhedron_builds_a_cube() {
        let mut kernel = TruckKernel::new();
        let points = vec![
            [-1.0, -1.0, -1.0],
            [1.0, -1.0, -1.0],
            [1.0, 1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [-1.0, -1.0, 1.0],
            [1.0, -1.0, 1.0],
            [1.0, 1.0, 1.0],
            [-1.0, 1.0, 1.0],
        ];
        let faces = vec![
            vec![0, 3, 2, 1],
            vec![4, 5, 6, 7],
            vec![0, 1, 5, 4],
            vec![3, 7, 6, 2],
            vec![0, 4, 7, 3],
            vec![1, 2, 6, 5],
        ];
        let solid = kernel
            .primitive(Primitive::Polyhedron { points, faces })
            .expect("polyhedron");
        let mesh = kernel.tessellate(solid, 0.01).expect("tessellation");

        let volume = signed_volume(&mesh);
        assert!((volume - 8.0).abs() < 1e-9, "volume = {volume}");
    }

    #[test]
    fn polyhedron_rejects_invalid_shells() {
        let mut kernel = TruckKernel::new();
        let points = vec![
            [-1.0, -1.0, -1.0],
            [1.0, -1.0, -1.0],
            [1.0, 1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [-1.0, -1.0, 1.0],
            [1.0, -1.0, 1.0],
            [1.0, 1.0, 1.0],
            [-1.0, 1.0, 1.0],
        ];
        let mut faces = vec![
            vec![0, 3, 2, 1],
            vec![4, 5, 6, 7],
            vec![0, 1, 5, 4],
            vec![3, 7, 6, 2],
            vec![0, 4, 7, 3],
            vec![1, 2, 6, 5],
        ];

        faces.pop();
        let error = kernel
            .primitive(Primitive::Polyhedron {
                points: points.clone(),
                faces,
            })
            .expect_err("open shell");
        assert!(
            matches!(error, KernelError::Backend(ref message) if message.contains("closed")),
            "{error:?}"
        );

        let error = kernel
            .primitive(Primitive::Polyhedron {
                points,
                faces: vec![vec![0, 1, 99], vec![0, 1, 2], vec![0, 2, 3], vec![0, 3, 1]],
            })
            .expect_err("bad index");
        assert!(
            matches!(error, KernelError::Backend(ref message) if message.contains("range")),
            "{error:?}"
        );
    }

    #[test]
    fn profile_with_a_hole_extrudes() {
        let mut kernel = TruckKernel::new();
        let region = ProfileRegion {
            outer: vec![[-2.0, -2.0], [2.0, -2.0], [2.0, 2.0], [-2.0, 2.0]],
            holes: vec![vec![[-1.0, -1.0], [-1.0, 1.0], [1.0, 1.0], [1.0, -1.0]]],
        };
        let sketch = kernel.sketch_profile(&[region]).expect("profile");
        let solid = kernel.extrude(sketch, 2.0).expect("extrude");
        let mesh = kernel.tessellate(solid, 0.01).expect("tessellation");

        let volume = signed_volume(&mesh);
        assert!((volume - 24.0).abs() < 0.01, "volume = {volume}");
    }

    #[test]
    fn disjoint_profile_regions_extrude_together() {
        let mut kernel = TruckKernel::new();
        let left = ProfileRegion {
            outer: vec![[-2.0, -0.5], [-1.0, -0.5], [-1.0, 0.5], [-2.0, 0.5]],
            holes: Vec::new(),
        };
        let right = ProfileRegion {
            outer: vec![[1.0, -0.5], [2.0, -0.5], [2.0, 0.5], [1.0, 0.5]],
            holes: Vec::new(),
        };
        let sketch = kernel
            .sketch_profile(&[left, right])
            .expect("profile regions");
        let solid = kernel.extrude(sketch, 2.0).expect("extrude");
        let mesh = kernel.tessellate(solid, 0.01).expect("tessellation");

        let volume = signed_volume(&mesh);
        assert!((volume - 4.0).abs() < 0.01, "volume = {volume}");
    }

    #[test]
    fn empty_profile_regions_are_rejected() {
        let mut kernel = TruckKernel::new();
        let error = kernel.sketch_profile(&[]).expect_err("empty profile");
        assert!(matches!(error, KernelError::Backend(message) if message.contains("region")));
    }

    #[test]
    fn bounds_report_solid_and_sketch_extents() {
        let mut kernel = TruckKernel::new();
        let cube = kernel
            .primitive(Primitive::Cuboid {
                size: [2.0, 4.0, 6.0],
            })
            .expect("cube");
        let (min, max) = kernel.bounds(cube, 0.01).expect("solid bounds");
        assert!((min[0] + 1.0).abs() < 1.0e-9, "min = {min:?}");
        assert!((min[1] + 2.0).abs() < 1.0e-9, "min = {min:?}");
        assert!((min[2] + 3.0).abs() < 1.0e-9, "min = {min:?}");
        assert!((max[0] - 1.0).abs() < 1.0e-9, "max = {max:?}");
        assert!((max[1] - 2.0).abs() < 1.0e-9, "max = {max:?}");
        assert!((max[2] - 3.0).abs() < 1.0e-9, "max = {max:?}");

        let sketch = kernel
            .sketch(Sketch::Square { size: [2.0, 4.0] })
            .expect("square");
        let (min, max) = kernel.bounds(sketch, 0.01).expect("sketch bounds");
        assert!((min[0] + 1.0).abs() < 1.0e-9, "min = {min:?}");
        assert!((min[1] + 2.0).abs() < 1.0e-9, "min = {min:?}");
        assert!(min[2].abs() < 1.0e-9, "min = {min:?}");
        assert!((max[0] - 1.0).abs() < 1.0e-9, "max = {max:?}");
        assert!((max[1] - 2.0).abs() < 1.0e-9, "max = {max:?}");
        assert!(max[2].abs() < 1.0e-9, "max = {max:?}");
    }

    fn bounds(mesh: &Mesh) -> ([f64; 3], [f64; 3]) {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for position in &mesh.positions {
            for axis in 0..3 {
                min[axis] = min[axis].min(position[axis]);
                max[axis] = max[axis].max(position[axis]);
            }
        }
        (min, max)
    }
}
