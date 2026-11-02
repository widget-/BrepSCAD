//! Truck B-rep backend for the kernel API.
//!
//! This first adapter covers the primitives, boolean operations, and
//! tessellation needed by the walking-skeleton slice.

mod offset;
mod primitives;
mod raycast;

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use brepscad_kernel_api::{
    BooleanOp, Capabilities, Kernel, KernelError, Mesh, Polyhedron, Primitive, ProfileRegion,
    ShapeId, Sketch, Transform,
};
use primitives::{cuboid, cylinder, polyhedron, sphere};
use truck_meshalgo::prelude::*;
use truck_modeling::*;

type KernelResult<T> = std::result::Result<T, KernelError>;

const BOOLEAN_TOLERANCES: [f64; 3] = [0.05, 0.02, 0.01];
const BOOLEAN_SHIFTS: [[f64; 3]; 4] = [
    [0.0, 0.0, 0.0],
    [1.0e-4, 1.0e-4, -1.0e-4],
    [-1.0e-4, 2.0e-4, 1.0e-4],
    [2.0e-4, -1.0e-4, 2.0e-4],
];
/// Wall-clock budget for the retry ladder after the exact attempts. Degenerate
/// models can make individual retries pathologically slow; past this point the
/// cheap fallbacks are better than an unusable preview.
const BOOLEAN_RETRY_BUDGET: std::time::Duration = std::time::Duration::from_millis(500);
const BOOLEAN_NUDGES: [[f64; 3]; 6] = [
    [0.0, 0.0, 2.0e-5],
    [0.0, 0.0, -2.0e-5],
    [0.0, -2.0e-5, 0.0],
    [0.0, 2.0e-5, 0.0],
    [2.0e-5, 0.0, 0.0],
    [-2.0e-5, 0.0, 0.0],
];

fn solid_bounds(solid: &Solid) -> Option<([f64; 3], [f64; 3])> {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    let mut found = false;
    for vertex in solid.vertex_iter() {
        let point = vertex.point();
        found = true;
        for (axis, value) in [point.x, point.y, point.z].into_iter().enumerate() {
            min[axis] = min[axis].min(value);
            max[axis] = max[axis].max(value);
        }
    }
    found.then_some((min, max))
}

/// True when `outer`'s bounds contain `inner`'s. Master shapeops can return a
/// union that silently drops one operand (coplanar face pairs are classified as
/// "inside" and removed), so the adapter checks that a union really contains
/// what it was given before accepting it.
fn solid_covers(outer: &Solid, inner: &Solid) -> bool {
    let (Some((outer_min, outer_max)), Some((inner_min, inner_max))) =
        (solid_bounds(outer), solid_bounds(inner))
    else {
        return false;
    };
    (0..3).all(|axis| {
        let slack = 1.0e-6 * inner_max[axis].abs().max(1.0);
        outer_min[axis] <= inner_min[axis] + slack && outer_max[axis] >= inner_max[axis] - slack
    })
}

fn union_covers(result: &Solid, a: &Solid, b: &Solid) -> bool {
    solid_covers(result, a) && solid_covers(result, b)
}

fn disjoint(a: &Solid, b: &Solid) -> bool {
    let (Some((a_min, a_max)), Some((b_min, b_max))) = (solid_bounds(a), solid_bounds(b)) else {
        return false;
    };
    (0..3).any(|axis| a_max[axis] < b_min[axis] - 1.0e-6 || b_max[axis] < a_min[axis] - 1.0e-6)
}

fn attempt_boolean(op: BooleanOp, a: &Solid, b: &Solid, tolerance: f64) -> Option<Solid> {
    catch_unwind(AssertUnwindSafe(|| match op {
        BooleanOp::Union => truck_shapeops::or(a, b, tolerance),
        BooleanOp::Intersection => truck_shapeops::and(a, b, tolerance),
        BooleanOp::Difference => {
            let mut inverted = b.clone();
            inverted.not();
            truck_shapeops::and(a, &inverted, tolerance)
        }
    }))
    .ok()
    .flatten()
}

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
    boolean_shapes: std::collections::HashSet<u64>,
    /// Radius of each shape that is a ball, so `minkowski` can recognise one.
    balls: std::collections::HashMap<u64, f64>,
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

    /// Ray-casting scene for a solid shape.
    ///
    /// Faces with an analytic surface the caster models (planes, revolved
    /// lines) are exported exactly; the rest are covered by their tessellation
    /// at `tolerance`.
    pub fn raycast_scene(&self, id: ShapeId, tolerance: f64) -> Option<brepscad_raycast::Scene> {
        if let Some(radius) = self.ball_radius(id) {
            if let Some(scene) = self.ball_scene(id, radius, tolerance) {
                return Some(scene);
            }
        }
        match self.shape(id).ok()? {
            OwnedShape::Solid(solid) => Some(raycast::scene_from_solid(solid, tolerance)),
            OwnedShape::Face(_) | OwnedShape::Faces(_) => None,
        }
    }

    /// One exact sphere, for a shape the kernel knows is a ball.
    ///
    /// The B-rep stays a faceted polyhedron, which is what keeps booleans fast,
    /// but the preview and picking trace the exact ball, so the facets never
    /// show.
    fn ball_scene(
        &self,
        id: ShapeId,
        radius: f64,
        tolerance: f64,
    ) -> Option<brepscad_raycast::Scene> {
        let (min, max) = self.bounds(id, tolerance).ok()?;
        let centre = [
            (min[0] + max[0]) / 2.0,
            (min[1] + max[1]) / 2.0,
            (min[2] + max[2]) / 2.0,
        ];
        Some(brepscad_raycast::Scene::new(vec![
            brepscad_raycast::Face::new(
                brepscad_raycast::Surface::Sphere { centre, radius },
                brepscad_raycast::TrimRegion::new(Vec::new()),
                (min, max),
            ),
        ]))
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
    fn boolean_impl(
        &mut self,
        op: BooleanOp,
        a_id: ShapeId,
        b_id: ShapeId,
    ) -> KernelResult<ShapeId> {
        let a = self.solid(a_id)?.clone();
        let b = self.solid(b_id)?.clone();
        let started = std::time::Instant::now();

        // Booleans can legitimately produce a solid with no shells: an
        // intersection that misses, or a subtraction that removes the whole
        // target. shapeops cannot consume such a solid, so fold it away here
        // instead of feeding it back into the engine.
        let a_empty = a.boundaries().is_empty();
        let b_empty = b.boundaries().is_empty();
        if a_empty || b_empty {
            let surviving = match op {
                BooleanOp::Union => {
                    if a_empty {
                        b_id
                    } else {
                        a_id
                    }
                }
                BooleanOp::Difference => a_id,
                BooleanOp::Intersection => {
                    if a_empty {
                        a_id
                    } else {
                        b_id
                    }
                }
            };
            return Ok(surviving);
        }

        if op == BooleanOp::Union && disjoint(&a, &b) {
            let mut boundaries = a.boundaries().clone();
            boundaries.extend(b.boundaries().iter().cloned());
            if let Ok(result) = Solid::try_new(boundaries) {
                return Ok(self.insert(OwnedShape::Solid(result)));
            }
        }

        if op == BooleanOp::Difference && disjoint(&a, &b) {
            return Ok(self.insert(OwnedShape::Solid(a)));
        }

        // Fusing an already-compound shell set with another solid makes master
        // shapeops iterate every shell pair and can take minutes on degenerate
        // models. Keep the shells separate instead; the tessellator renders a
        // compound, and the cheap disjoint path already covers the exact case.
        if op == BooleanOp::Union && (a.boundaries().len() > 1 || b.boundaries().len() > 1) {
            let mut boundaries = a.boundaries().clone();
            boundaries.extend(b.boundaries().iter().cloned());
            return Ok(self.insert(OwnedShape::Solid(Solid::new_unchecked(boundaries))));
        }

        // A compound solid can be produced by a disjoint union. Subtract from
        // each shell independently, then recombine the surviving shells.
        if op == BooleanOp::Difference && a.boundaries().len() > 1 {
            let mut boundaries = Vec::new();
            for shell in a.boundaries().clone() {
                let single = Solid::new_unchecked(vec![shell]);
                let shape = self.insert(OwnedShape::Solid(single));
                let result = self.boolean(op, shape, b_id)?;
                boundaries.extend(self.solid(result)?.clone().into_boundaries());
            }
            if boundaries.is_empty() {
                // Every shell was consumed by the tool.
                return Ok(self.insert(OwnedShape::Solid(Solid::new_unchecked(Vec::new()))));
            }
            // Multiple shell results are already the product of the boolean
            // engine; do not reject them for being open before tessellation.
            let result = Solid::new_unchecked(boundaries);
            return Ok(self.insert(OwnedShape::Solid(result)));
        }

        // Subtracting a compound tool is equivalent to subtracting its shells
        // one after another.
        if op == BooleanOp::Difference && b.boundaries().len() > 1 {
            let mut result = a_id;
            for shell in b.boundaries().clone() {
                let single = Solid::new_unchecked(vec![shell]);
                let tool = self.insert(OwnedShape::Solid(single));
                result = self.boolean(op, result, tool)?;
            }
            return Ok(result);
        }

        // Exact geometry first. A union must contain both operands: master
        // shapeops can return an amputated result (coplanar face pairs get
        // classified as inside and dropped), which is worse than failing.
        for tolerance in BOOLEAN_TOLERANCES {
            if let Some(result) = attempt_boolean(op, &a, &b, tolerance) {
                if op != BooleanOp::Union || union_covers(&result, &a, &b) {
                    return Ok(self.insert(OwnedShape::Solid(result)));
                }
            }
        }

        let over_budget = |started: std::time::Instant| started.elapsed() > BOOLEAN_RETRY_BUDGET;

        // Degenerate corner cuts often need a tiny tool nudge. Try that before
        // the more expensive translated-operand fallbacks.
        if op == BooleanOp::Difference {
            for nudge in BOOLEAN_NUDGES {
                if over_budget(started) {
                    break;
                }
                let offset = Vector3::new(nudge[0], nudge[1], nudge[2]);
                let nudged = builder::translated(&b, offset);
                for tolerance in BOOLEAN_TOLERANCES {
                    if let Some(result) = attempt_boolean(op, &a, &nudged, tolerance) {
                        return Ok(self.insert(OwnedShape::Solid(result)));
                    }
                }
            }
        }

        // Geometry-preserving translated-operand retries, as a last resort for
        // numerical degeneracies that are not corner-cut specific.
        for &shift in BOOLEAN_SHIFTS.iter().filter(|shift| **shift != [0.0; 3]) {
            if over_budget(started) {
                break;
            }
            let offset = Vector3::new(shift[0], shift[1], shift[2]);
            let shifted_a = builder::translated(&a, offset);
            let shifted_b = builder::translated(&b, offset);
            for tolerance in BOOLEAN_TOLERANCES {
                if let Some(mut result) = attempt_boolean(op, &shifted_a, &shifted_b, tolerance) {
                    result = builder::translated(&result, -offset);
                    if op != BooleanOp::Union || union_covers(&result, &a, &b) {
                        return Ok(self.insert(OwnedShape::Solid(result)));
                    }
                }
            }
        }

        if op == BooleanOp::Union {
            // Last-resort render fallback for overlapping shells that shapeops
            // cannot fuse.
            let mut boundaries = a.boundaries().clone();
            boundaries.extend(b.boundaries().iter().cloned());
            if !boundaries.is_empty() {
                return Ok(self.insert(OwnedShape::Solid(Solid::new_unchecked(boundaries))));
            }
        }

        Err(KernelError::Backend(
            "boolean operation returned no solid at any tolerance".to_owned(),
        ))
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
        let ball = match &primitive {
            Primitive::Sphere { radius } => Some(*radius),
            _ => None,
        };
        let shape = match primitive {
            Primitive::Cuboid { size } => cuboid(size)?,
            Primitive::Cylinder { radius, height } => cylinder(radius, height)?,
            Primitive::Sphere { radius } => sphere(radius)?,
            Primitive::Polyhedron { points, faces } => polyhedron(&points, &faces)?,
        };
        let id = self.insert(OwnedShape::Solid(shape));
        if let Some(radius) = ball {
            self.balls.insert(id.0, radius);
        }
        Ok(id)
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
        let inherited_boolean = self.boolean_shapes.contains(&shape.0);
        let source = shape;
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
        let result = self.insert(transformed);
        if inherited_boolean {
            self.boolean_shapes.insert(result.0);
        }
        // A ball stays a ball through a rigid motion, and `minkowski` needs to
        // recognise it after a translate or rotate.
        if let Some(radius) = self.balls.get(&source.0).copied() {
            if (matrix.determinant().abs() - 1.0).abs() < 1.0e-9 {
                self.balls.insert(result.0, radius);
            }
        }
        Ok(result)
    }

    fn boolean(&mut self, op: BooleanOp, a_id: ShapeId, b_id: ShapeId) -> KernelResult<ShapeId> {
        let result = self.boolean_impl(op, a_id, b_id)?;
        self.boolean_shapes.insert(result.0);
        Ok(result)
    }

    fn tessellate(&self, shape: ShapeId, tolerance: f64) -> KernelResult<Mesh> {
        // Boolean-derived surfaces can trigger pathological recursive
        // subdivision in master Truck at a fine tolerance. Keep the analytic
        // B-rep and tessellate those results coarsely.
        let tolerance = if self.boolean_shapes.contains(&shape.0) {
            tolerance.max(BOOLEAN_DISPLAY_TOLERANCE)
        } else {
            tolerance
        };
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

    fn raycast_scene(&self, shape: ShapeId, tolerance: f64) -> Option<brepscad_raycast::Scene> {
        TruckKernel::raycast_scene(self, shape, tolerance)
    }

    fn bounds(&self, shape: ShapeId, tolerance: f64) -> KernelResult<([f64; 3], [f64; 3])> {
        let mesh = self.tessellate(shape, tolerance)?;
        mesh_bounds(&mesh).ok_or_else(|| KernelError::Backend("shape has no geometry".to_owned()))
    }

    fn polyhedron(&self, shape: ShapeId) -> Option<Polyhedron> {
        polyhedron_of(self.solid(shape).ok()?)
    }

    fn offset(&mut self, shape: ShapeId, radius: f64) -> KernelResult<ShapeId> {
        if !radius.is_finite() || radius <= 0.0 {
            return Err(KernelError::Backend(
                "offset radius must be positive".to_owned(),
            ));
        }
        let solid = self.solid(shape)?.clone();
        let polyhedron = polyhedron_of(&solid).ok_or(KernelError::Unsupported(
            "offset of a shape with curved faces",
        ))?;
        let rounded = offset::BoxShape::new(&polyhedron)
            .and_then(|box_shape| box_shape.offset(radius))
            .ok_or(KernelError::Unsupported(
                "offset of a shape that is not a box",
            ))?;
        Ok(self.insert(OwnedShape::Solid(rounded)))
    }

    fn ball_radius(&self, shape: ShapeId) -> Option<f64> {
        self.balls.get(&shape.0).copied()
    }
}

/// Read a solid's topology as a polyhedron, when every face is a single planar
/// loop. Curved faces, and faces with holes, have no such representation.
fn polyhedron_of(solid: &Solid) -> Option<Polyhedron> {
    let mut vertices: Vec<[f64; 3]> = Vec::new();
    let mut lookup: HashMap<[u64; 3], usize> = HashMap::new();
    let mut faces: Vec<Vec<usize>> = Vec::new();

    for face in solid.face_iter() {
        let Surface::Plane(_) = face.surface() else {
            return None;
        };
        // Outer loops only: inner boundaries (holes) carry no information the
        // hull or a face-plane test needs, and a convex solid has none.
        let wire = face.absolute_boundaries().first().cloned()?;

        // Wire edges are not guaranteed to be stored in traversal order, so
        // chain them by their endpoints instead of trusting the stored front.
        let corners = loop_corners(&wire)?;

        let mut polygon = Vec::new();
        for point in corners {
            // The same corner is the same `Vertex` object, so the coordinates
            // are bit-identical and need no tolerance.
            let key = [point.x.to_bits(), point.y.to_bits(), point.z.to_bits()];
            let index = *lookup.entry(key).or_insert_with(|| {
                vertices.push([point.x, point.y, point.z]);
                vertices.len() - 1
            });
            polygon.push(index);
        }
        if polygon.len() < 3 {
            return None;
        }
        faces.push(polygon);
    }

    (!faces.is_empty()).then_some(Polyhedron { vertices, faces })
}

/// Walk a wire's edges into a closed corner loop, without relying on the order
/// the edges happen to be stored in.
fn loop_corners(wire: &Wire) -> Option<Vec<Point3>> {
    let same = |a: Point3, b: Point3| (a - b).magnitude() < 1.0e-9;
    let edges: Vec<(Point3, Point3)> = wire
        .iter()
        .filter_map(|edge| {
            let front = edge.absolute_front().point();
            let back = edge.absolute_back().point();
            (!same(front, back)).then_some((front, back))
        })
        .collect();
    if edges.len() < 3 {
        return None;
    }

    let mut used = vec![false; edges.len()];
    used[0] = true;
    let start = edges[0].0;
    let mut current = edges[0].1;
    let mut corners = Vec::with_capacity(edges.len());
    corners.push(start);

    for _ in 1..edges.len() {
        let (index, front, back) = edges
            .iter()
            .enumerate()
            .find(|(index, (front, back))| {
                !used[*index] && (same(*front, current) || same(*back, current))
            })
            .map(|(index, (front, back))| (index, *front, *back))?;
        used[index] = true;
        corners.push(current);
        current = if same(front, current) { back } else { front };
    }

    same(current, start).then_some(corners)
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

/// Tessellation tolerance floor for boolean results, in model units. See the
/// comment in `TruckKernel::tessellate`.
const BOOLEAN_DISPLAY_TOLERANCE: f64 = 0.05;

pub(crate) fn tessellate_face(face: &Face, tolerance: f64) -> PolygonMesh {
    let shell: Shell = vec![face.clone()].into_iter().collect();
    let meshed = shell.triangulation(tolerance);
    let mut mesh = PolygonMesh::default();
    for poly_face in meshed.iter() {
        if let Some(part) = poly_face.surface() {
            // Match `MeshedShape::to_polygon`: a flipped face has to flip its
            // winding and its normals together.
            if poly_face.orientation() {
                mesh.merge(part.clone());
            } else {
                mesh.merge(part.inverse());
            }
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
    let division = if angle.abs() >= std::f64::consts::TAU - 1.0e-9 {
        2
    } else {
        1
    };
    let mut solid = builder::rsweep(
        &profile,
        Point3::origin(),
        Vector3::unit_z(),
        Rad(angle),
        division,
    );
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
    // A polygon mesh stores normals as indices separate from positions, so one
    // shared corner can carry different normals on different faces. The
    // neutral mesh indexes positions and normals together, so expand each
    // corner into a (position, normal) pair and deduplicate those pairs.
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut normals: Vec<[f64; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut lookup: HashMap<(usize, Option<usize>), u32> = HashMap::new();
    let mut all_normals = true;

    for face in mesh.faces().face_iter() {
        if face.len() < 3 {
            continue;
        }
        let mut corners = Vec::with_capacity(face.len());
        for vertex in face {
            let key = (vertex.pos, vertex.nor);
            let index = match lookup.get(&key) {
                Some(index) => *index,
                None => {
                    let index = positions.len() as u32;
                    let point = mesh.positions()[vertex.pos];
                    positions.push([point.x, point.y, point.z]);
                    match vertex.nor.and_then(|nor| mesh.normals().get(nor)) {
                        Some(normal) if all_normals => {
                            normals.push([normal.x, normal.y, normal.z]);
                        }
                        _ => all_normals = false,
                    }
                    lookup.insert(key, index);
                    index
                }
            };
            corners.push(index);
        }
        for corner in 1..corners.len() - 1 {
            indices.extend([corners[0], corners[corner], corners[corner + 1]]);
        }
    }

    if !all_normals {
        normals.clear();
    }

    Mesh {
        positions,
        normals,
        indices,
    }
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
    fn tessellation_keeps_the_surface_normals() {
        let mut kernel = TruckKernel::new();
        let cylinder = kernel
            .primitive(Primitive::Cylinder {
                radius: 0.5,
                height: 4.0,
            })
            .expect("cylinder");
        let mesh = kernel.tessellate(cylinder, 0.01).expect("tessellation");

        assert_eq!(mesh.normals.len(), mesh.positions.len());
        for normal in &mesh.normals {
            let length =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            assert!(
                (length - 1.0).abs() < 1e-9,
                "normal {normal:?} is not unit length"
            );
        }

        let mut lateral = 0;
        let mut caps = 0;
        for (position, normal) in mesh.positions.iter().zip(&mesh.normals) {
            if normal[2].abs() > 1.0 - 1e-9 {
                // Cap faces are the only ones with an axial normal.
                assert!(
                    position[2].abs() > 1.9,
                    "cap normal away from a cap: {position:?}"
                );
                caps += 1;
            } else {
                // A revolved line gives the exact outward radial direction,
                // which the flat triangle normal would only approximate.
                assert!(normal[2].abs() < 1e-9, "lateral normal tilts: {normal:?}");
                let radius = position[0].hypot(position[1]);
                assert!(radius > 0.0);
                let radial = [position[0] / radius, position[1] / radius];
                let dot = normal[0] * radial[0] + normal[1] * radial[1];
                assert!(
                    dot > 1.0 - 1e-9,
                    "normal {normal:?} is not radial at {position:?}"
                );
                lateral += 1;
            }
        }
        assert!(
            lateral > 0 && caps > 0,
            "lateral = {lateral}, caps = {caps}"
        );
    }

    #[test]
    fn planar_faces_keep_their_exact_normals() {
        let mut kernel = TruckKernel::new();
        let cube = kernel
            .primitive(Primitive::Cuboid {
                size: [2.0, 2.0, 2.0],
            })
            .expect("cube");
        let mesh = kernel.tessellate(cube, 0.01).expect("tessellation");

        assert!(mesh.has_normals());
        for normal in &mesh.normals {
            let axis = normal
                .iter()
                .filter(|component| component.abs() > 0.5)
                .count();
            assert_eq!(axis, 1, "cube normal {normal:?} is not axis aligned");
        }
    }

    #[test]
    fn boolean_results_keep_the_surface_normals() {
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
        let cut = kernel
            .boolean(BooleanOp::Difference, cube, cylinder)
            .expect("difference");
        let mesh = kernel.tessellate(cut, 0.01).expect("tessellation");

        assert!(mesh.has_normals());
        for normal in &mesh.normals {
            let length =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            assert!(
                (length - 1.0).abs() < 1e-6,
                "normal {normal:?} is not unit length"
            );
        }
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
    fn legacy_corner_difference_is_robust() {
        let mut kernel = TruckKernel::new();
        let cube = kernel
            .primitive(Primitive::Cuboid {
                size: [2.0, 2.0, 2.0],
            })
            .expect("cube");
        let cube = kernel
            .transform(cube, Transform::from_translation([1.0, 1.0, 1.0]))
            .expect("legacy cube");
        let cylinder = kernel
            .primitive(Primitive::Cylinder {
                radius: 0.5,
                height: 4.0,
            })
            .expect("cylinder");
        let cylinder = kernel
            .transform(cylinder, Transform::from_translation([0.2, 0.0, 2.0]))
            .expect("legacy cylinder");

        let result = kernel.boolean(BooleanOp::Difference, cube, cylinder);
        assert!(result.is_ok(), "difference: {result:?}");
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
    fn polyhedron_reorients_inverted_shells() {
        let mut kernel = TruckKernel::new();
        let points = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let faces = vec![vec![1, 2, 0], vec![3, 1, 0], vec![2, 3, 0], vec![3, 2, 1]];
        let solid = kernel
            .primitive(Primitive::Polyhedron { points, faces })
            .expect("inverted polyhedron");
        let mesh = kernel.tessellate(solid, 0.01).expect("tessellation");
        let volume = signed_volume(&mesh);
        assert!(volume > 0.0, "volume = {volume}");
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
    fn polytopes_expose_their_topology() {
        let mut kernel = TruckKernel::new();
        let cube = kernel
            .primitive(Primitive::Cuboid {
                size: [2.0, 2.0, 2.0],
            })
            .expect("cube");
        let polyhedron = kernel.polyhedron(cube).expect("cube is a polyhedron");
        assert_eq!(polyhedron.vertices.len(), 8);
        assert_eq!(polyhedron.faces.len(), 6);
        assert!(
            polyhedron.faces.iter().all(|face| face.len() == 4),
            "every cube face is a quad"
        );

        let cutter = kernel
            .primitive(Primitive::Cuboid {
                size: [1.0, 1.0, 4.0],
            })
            .expect("cutter");
        let cut = kernel
            .boolean(BooleanOp::Difference, cube, cutter)
            .expect("cut");
        assert!(
            kernel.polyhedron(cut).is_some(),
            "a cut with planar faces stays a polyhedron"
        );

        let cylinder = kernel
            .primitive(Primitive::Cylinder {
                radius: 1.0,
                height: 2.0,
            })
            .expect("cylinder");
        assert!(
            kernel.polyhedron(cylinder).is_none(),
            "curved faces have no polyhedron"
        );
    }

    #[test]
    fn offsets_a_box_and_rejects_curved_solids() {
        let mut kernel = TruckKernel::new();
        let cube = kernel
            .primitive(Primitive::Cuboid {
                size: [2.0, 2.0, 2.0],
            })
            .expect("cube");
        let rounded = kernel.offset(cube, 0.5).expect("offset");
        let mesh = kernel.tessellate(rounded, 0.02).expect("mesh");
        let (min, max) = bounds(&mesh);
        for axis in 0..3 {
            assert!((min[axis] + 1.5).abs() < 0.02, "min = {min:?}");
            assert!((max[axis] - 1.5).abs() < 0.02, "max = {max:?}");
        }

        let cylinder = kernel
            .primitive(Primitive::Cylinder {
                radius: 1.0,
                height: 2.0,
            })
            .expect("cylinder");
        assert!(
            kernel.offset(cylinder, 0.5).is_err(),
            "a cylinder is not a box, so it has no rounded offset yet"
        );
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
