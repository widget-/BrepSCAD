//! Truck B-rep backend for the kernel API.
//!
//! This first adapter covers the primitives, boolean operations, and
//! tessellation needed by the walking-skeleton slice.

use truck_meshalgo::prelude::*;
use truck_modeling::*;
use visual_openscad_kernel_api::{
    BooleanOp, Capabilities, Kernel, KernelError, Mesh, Primitive, ShapeId, Transform,
};

type KernelResult<T> = std::result::Result<T, KernelError>;

const BOOLEAN_TOLERANCE: f64 = 0.05;

/// A kernel that owns Truck solids in a simple arena.
#[derive(Debug, Default)]
pub struct TruckKernel {
    shapes: Vec<Solid>,
}

impl TruckKernel {
    pub fn new() -> Self {
        Self::default()
    }

    fn insert(&mut self, shape: Solid) -> ShapeId {
        let id = ShapeId(self.shapes.len() as u64);
        self.shapes.push(shape);
        id
    }

    fn shape(&self, id: ShapeId) -> KernelResult<&Solid> {
        self.shapes
            .get(id.0 as usize)
            .ok_or(KernelError::UnknownShape(id))
    }
}

impl Kernel for TruckKernel {
    fn name(&self) -> &'static str {
        "truck"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            primitives: true,
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
        };
        Ok(self.insert(shape))
    }

    fn transform(&mut self, shape: ShapeId, transform: Transform) -> KernelResult<ShapeId> {
        let solid = self.shape(shape)?;
        let transformed = match transform {
            Transform::Translate([x, y, z]) => builder::translated(solid, Vector3::new(x, y, z)),
            Transform::Rotate([x, y, z]) => {
                let rotated = builder::rotated(solid, Point3::origin(), Vector3::unit_x(), Rad(x));
                let rotated =
                    builder::rotated(&rotated, Point3::origin(), Vector3::unit_y(), Rad(y));
                builder::rotated(&rotated, Point3::origin(), Vector3::unit_z(), Rad(z))
            }
            Transform::Scale([x, y, z]) => {
                builder::scaled(solid, Point3::origin(), Vector3::new(x, y, z))
            }
        };
        Ok(self.insert(transformed))
    }

    fn boolean(&mut self, op: BooleanOp, a: ShapeId, b: ShapeId) -> KernelResult<ShapeId> {
        let a = self.shape(a)?.clone();
        let mut b = self.shape(b)?.clone();

        let result = match op {
            BooleanOp::Union => truck_shapeops::or(&a, &b, BOOLEAN_TOLERANCE),
            BooleanOp::Intersection => truck_shapeops::and(&a, &b, BOOLEAN_TOLERANCE),
            BooleanOp::Difference => {
                b.not();
                truck_shapeops::and(&a, &b, BOOLEAN_TOLERANCE)
            }
        };

        let result = result.ok_or_else(|| {
            KernelError::Backend("boolean operation returned no solid".to_owned())
        })?;
        Ok(self.insert(result))
    }

    fn tessellate(&self, shape: ShapeId, tolerance: f64) -> KernelResult<Mesh> {
        let solid = self.shape(shape)?;
        let mut mesh = solid.triangulation(tolerance).to_polygon();
        mesh.put_together_same_attrs(TOLERANCE)
            .remove_degenerate_faces()
            .remove_unused_attrs();
        Ok(to_neutral_mesh(&mesh))
    }
}

fn cuboid(size: [f64; 3]) -> KernelResult<Solid> {
    if size.iter().any(|value| *value <= 0.0) {
        return Err(KernelError::Backend(
            "cuboid dimensions must be positive".to_owned(),
        ));
    }

    let vertex = builder::vertex(Point3::new(-size[0] / 2.0, -size[1] / 2.0, -size[2] / 2.0));
    let edge = builder::tsweep(&vertex, Vector3::unit_x() * size[0]);
    let face = builder::tsweep(&edge, Vector3::unit_y() * size[1]);
    Ok(builder::tsweep(&face, Vector3::unit_z() * size[2]))
}

fn cylinder(radius: f64, height: f64) -> KernelResult<Solid> {
    if radius <= 0.0 || height <= 0.0 {
        return Err(KernelError::Backend(
            "cylinder dimensions must be positive".to_owned(),
        ));
    }

    let vertex = builder::vertex(Point3::new(0.0, -height / 2.0, radius));
    let circle = builder::rsweep(
        &vertex,
        Point3::origin(),
        Vector3::unit_y(),
        Rad(std::f64::consts::TAU),
    );
    let disk = builder::try_attach_plane(&[circle])
        .map_err(|error| KernelError::Backend(format!("{error:?}")))?;
    Ok(builder::tsweep(&disk, Vector3::new(0.0, height, 0.0)))
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
            .transform(cube, Transform::Translate([1.0, 0.0, 0.0]))
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
}
