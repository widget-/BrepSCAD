//! Rounded offset: the Minkowski sum of a box with a ball.
//!
//! Every point within `r` of the solid is within `r` of one of its faces, edges
//! or vertices, and those three kinds of feature sweep out the offset exactly:
//! the face moved out by `r`, the edge rounded by a cylinder of radius `r`, the
//! vertex rounded by a ball of radius `r`. The result is built as one shell, with
//! no booleans and no tessellation.
//!
//! Each rounded piece is a revolution *sector*: `rsweep` by the angle the
//! adjacent face normals span. That is what keeps Truck from covering the whole
//! sphere or cylinder instead of the patch — a sector's parameter domain is the
//! patch itself, so the surface and the wire agree without any trimming.

use std::collections::HashMap;

use brepscad_kernel_api::Polyhedron;
use monstertruck_meshing::prelude::*;
use monstertruck_modeling::*;

/// Divisions per revolution sector.
const SWEEP_DIVISION: usize = 4;

struct BoxFace {
    corners: [usize; 4],
    normal: Vector3,
}

pub(crate) struct BoxShape {
    vertices: Vec<Point3>,
    faces: Vec<BoxFace>,
    /// `(a, b, face0, face1)` for each of the twelve edges.
    edges: Vec<(usize, usize, usize, usize)>,
    /// The three faces meeting at each of the eight corners.
    corners: Vec<[usize; 3]>,
}

impl BoxShape {
    pub(crate) fn new(polyhedron: &Polyhedron) -> Option<Self> {
        if polyhedron.vertices.len() != 8 || polyhedron.faces.len() != 6 {
            return None;
        }
        let vertices: Vec<Point3> = polyhedron
            .vertices
            .iter()
            .map(|point| Point3::new(point[0], point[1], point[2]))
            .collect();
        let centre = {
            let sum = vertices
                .iter()
                .fold(Vector3::zero(), |sum, point| sum + point.to_vec())
                / vertices.len() as f64;
            Point3::new(sum.x, sum.y, sum.z)
        };

        let mut faces = Vec::with_capacity(6);
        for face in &polyhedron.faces {
            if face.len() != 4 {
                return None;
            }
            let mut corners = [face[0], face[1], face[2], face[3]];
            let mut normal = newell(&corners, &vertices)?;
            let middle = corners.iter().fold(Vector3::zero(), |sum, index| {
                sum + vertices[*index].to_vec()
            }) / 4.0;
            // `absolute_boundaries` keeps each wire as stored, which is not the
            // same as counter-clockwise seen from outside. Turning the loop
            // round with its normal keeps every plate facing outwards, and a
            // shell whose faces disagree would tessellate to a cancelling volume.
            if (middle - centre.to_vec()).dot(normal) < 0.0 {
                corners.reverse();
                normal = -normal;
            }
            faces.push(BoxFace { corners, normal });
        }

        let mut edge_faces: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        let mut corner_faces: Vec<Vec<usize>> = vec![Vec::new(); vertices.len()];
        for (index, face) in faces.iter().enumerate() {
            for corner in face.corners {
                corner_faces[corner].push(index);
            }
            for side in 0..4 {
                let a = face.corners[side];
                let b = face.corners[(side + 1) % 4];
                edge_faces
                    .entry((a.min(b), a.max(b)))
                    .or_default()
                    .push(index);
            }
        }
        if edge_faces.len() != 12 || edge_faces.values().any(|list| list.len() != 2) {
            return None;
        }
        if corner_faces.iter().any(|list| list.len() != 3) {
            return None;
        }

        let edges = edge_faces
            .into_iter()
            .map(|((a, b), list)| (a, b, list[0], list[1]))
            .collect();
        let corners = corner_faces
            .into_iter()
            .map(|list| [list[0], list[1], list[2]])
            .collect();

        Some(Self {
            vertices,
            faces,
            edges,
            corners,
        })
    }

    pub(crate) fn offset(&self, radius: f64) -> Option<Solid> {
        let mut faces = Vec::new();
        let tolerance = radius * 0.05;

        // One rectangle per face, on the offset plane.
        for face in &self.faces {
            let points: Vec<Point3> = face
                .corners
                .iter()
                .map(|index| self.vertices[*index] + face.normal * radius)
                .collect();
            let vertices: Vec<Vertex> =
                points.iter().map(|point| builder::vertex(*point)).collect();
            let edges: Vec<Edge> = (0..vertices.len())
                .map(|index| {
                    builder::line(&vertices[index], &vertices[(index + 1) % vertices.len()])
                })
                .collect();
            let wire: Wire = edges.into_iter().collect();
            let outward = face.normal;
            let plate = builder::try_attach_plane(vec![wire]).ok()?;
            faces.push(face_outwards(plate, |_| outward, tolerance));
        }

        // One cylinder sector per edge: the tangency line of one face, swept
        // about the edge by the angle the two normals span.
        for (a, b, face0, face1) in &self.edges {
            let start = self.vertices[*a];
            let end = self.vertices[*b];
            let span = end - start;
            let length = span.magnitude();
            if length <= 1.0e-9 {
                return None;
            }
            let axis = span / length;
            let from = self.faces[*face0].normal;
            let to = self.faces[*face1].normal;
            let angle = signed_angle(axis, from, to);
            let low = builder::vertex(start + from * radius);
            let high = builder::vertex(end + from * radius);
            let generator = builder::line(&low, &high);
            let patch: Shell = builder::revolve(
                &generator,
                start,
                axis,
                builder::SweepAngle::Partial(Rad(angle)),
                SWEEP_DIVISION,
            );
            for face in patch.face_iter().cloned() {
                faces.push(face_outwards(
                    face,
                    |point| radial(point, start, axis),
                    tolerance,
                ));
            }
        }

        // One octant per corner: a quarter arc between two of the normals,
        // swept about the third.
        for (corner, incident) in self.corners.iter().enumerate() {
            let centre = self.vertices[corner];
            let first = self.faces[incident[0]].normal;
            let middle = self.faces[incident[1]].normal;
            let last = self.faces[incident[2]].normal;
            let angle = signed_angle(middle, first, last);
            // The generatrix has to contain the revolution axis: a quadrant from
            // the pole along `middle` down to the equator along `first`,
            // revolved about `middle`, covers the octant between the three
            // normals. An arc in the plane *perpendicular* to the axis would
            // revolve into a flat ring sector instead of a spherical patch.
            let pole = builder::vertex(centre + middle * radius);
            let equator = builder::vertex(centre + first * radius);
            let transit = (middle + first).normalize();
            let arc: Edge = builder::circle_arc(&pole, &equator, centre + transit * radius);
            let patch: Shell = builder::revolve(
                &arc,
                centre,
                middle,
                builder::SweepAngle::Partial(Rad(angle)),
                SWEEP_DIVISION,
            );
            for face in patch.face_iter().cloned() {
                faces.push(face_outwards(face, |point| point - centre, tolerance));
            }
        }

        // The pieces come from independent sweeps, so neighbouring patches carry
        // their own vertices at coincident positions. That keeps every wire
        // exactly on its own surface, which is what the tessellator needs.
        let shell: Shell = faces.into_iter().collect();
        Some(Solid::new_unchecked(vec![shell]))
    }
}

/// Outward normal of a planar polygon.
fn newell(corners: &[usize; 4], vertices: &[Point3]) -> Option<Vector3> {
    let mut normal = Vector3::zero();
    for side in 0..corners.len() {
        let a = vertices[corners[side]].to_vec();
        let b = vertices[corners[(side + 1) % corners.len()]].to_vec();
        normal += a.cross(b);
    }
    (normal.magnitude() > 1.0e-12).then(|| normal.normalize())
}

/// Signed angle from `from` to `to` about `axis`, in `(-π, π]`.
fn signed_angle(axis: Vector3, from: Vector3, to: Vector3) -> f64 {
    let sine = from.cross(to).dot(axis);
    let cosine = from.dot(to);
    sine.atan2(cosine)
}

/// Component of `point` that is perpendicular to `axis`, i.e. the radial offset
/// from the line through `origin`.
fn radial(point: Point3, origin: Point3, axis: Vector3) -> Vector3 {
    let offset = point - origin;
    offset - axis * offset.dot(axis)
}

/// Turn `face` round when its normal points against `outward`.
///
/// `rsweep` orients each division of a patch by the direction it swept, so the
/// sectors of one edge can end up facing opposite ways; a shell whose faces
/// disagree tessellates to a cancelling volume.
fn face_outwards(mut face: Face, outward: impl Fn(Point3) -> Vector3, tolerance: f64) -> Face {
    let shell: Shell = std::iter::once(face.clone()).collect();
    let mesh = shell.triangulation(tolerance).to_polygon();
    let positions = mesh.positions();
    let Some(triangle) = mesh.tri_faces().first() else {
        return face;
    };
    let a = positions[triangle[0].pos];
    let b = positions[triangle[1].pos];
    let c = positions[triangle[2].pos];
    let normal = (b - a).cross(c - a);
    let centroid = Point3::new(
        (a.x + b.x + c.x) / 3.0,
        (a.y + b.y + c.y) / 3.0,
        (a.z + b.z + c.z) / 3.0,
    );
    if normal.dot(outward(centroid)) < 0.0 {
        face.invert();
    }
    face
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{polyhedron_of, primitives};

    fn extents(mesh: &PolygonMesh) -> ([f64; 3], [f64; 3]) {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for point in mesh.positions() {
            for (axis, value) in [point.x, point.y, point.z].into_iter().enumerate() {
                min[axis] = min[axis].min(value);
                max[axis] = max[axis].max(value);
            }
        }
        (min, max)
    }

    fn area(mesh: &PolygonMesh) -> f64 {
        let positions = mesh.positions();
        let mut total = 0.0;
        for face in mesh.tri_faces() {
            let a = positions[face[0].pos];
            let b = positions[face[1].pos];
            let c = positions[face[2].pos];
            total += (b - a).cross(c - a).magnitude() / 2.0;
        }
        total
    }

    /// Distance from a point to the box of the given half-extents at the origin.
    fn distance_to_box(point: Point3, half: [f64; 3]) -> f64 {
        let mut total = 0.0;
        for (axis, value) in [point.x, point.y, point.z].into_iter().enumerate() {
            let outside = (value.abs() - half[axis]).max(0.0);
            total += outside * outside;
        }
        total.sqrt()
    }

    fn check(mesh: &PolygonMesh, half: [f64; 3], radius: f64, expected_area: f64) {
        let (min, max) = extents(mesh);
        for axis in 0..3 {
            let limit = half[axis] + radius;
            assert!((min[axis] + limit).abs() < 0.01, "min = {min:?}");
            assert!((max[axis] - limit).abs() < 0.01, "max = {max:?}");
        }
        // Every point of the offset is exactly `radius` from the solid; the
        // patches are built at that distance rather than fitted to it.
        for point in mesh.positions() {
            let distance = distance_to_box(*point, half);
            assert!(
                (distance - radius).abs() < 1.0e-6,
                "point {point:?} is {distance} from the box, not {radius}"
            );
        }
        let measured = area(mesh);
        assert!(
            (measured - expected_area).abs() < 0.02 * expected_area,
            "area = {measured}, expected about {expected_area}"
        );
    }

    #[test]
    fn offsets_a_cube_into_a_rounded_cube() {
        let cube = primitives::cuboid([2.0, 2.0, 2.0]).expect("cube");
        let polyhedron = polyhedron_of(&cube).expect("polyhedron");
        let shape = BoxShape::new(&polyhedron).expect("box");
        let solid = shape.offset(0.5).expect("offset");
        let mesh = solid.triangulation(0.01).to_polygon();

        // Six plates, twelve quarter cylinders and eight octants, each from the
        // exact surfaces rather than from sampling.
        assert_eq!(
            solid.face_iter().count(),
            6 + 12 * SWEEP_DIVISION + 8 * SWEEP_DIVISION
        );

        let radius = 0.5;
        let expected_area = 6.0 * 4.0
            + 12.0 * (std::f64::consts::FRAC_PI_2 * radius * 2.0)
            + 8.0 * (4.0 * std::f64::consts::PI * radius * radius / 8.0);
        check(&mesh, [1.0, 1.0, 1.0], radius, expected_area);
    }

    #[test]
    fn offsets_a_box_with_uneven_sides() {
        let box_shape = primitives::cuboid([4.0, 2.0, 1.0]).expect("box");
        let polyhedron = polyhedron_of(&box_shape).expect("polyhedron");
        let shape = BoxShape::new(&polyhedron).expect("box");
        let solid = shape.offset(0.25).expect("offset");
        let mesh = solid.triangulation(0.01).to_polygon();

        let radius = 0.25;
        // Two plates per axis, one quarter cylinder per edge and one octant per
        // corner. Twelve edges: four of each length.
        let edges = 4.0 * (4.0 + 2.0 + 1.0);
        let expected_area = 2.0 * (4.0 * 2.0 + 4.0 * 1.0 + 2.0 * 1.0)
            + edges * (std::f64::consts::FRAC_PI_2 * radius)
            + 4.0 * std::f64::consts::PI * radius * radius;
        check(&mesh, [2.0, 1.0, 0.5], radius, expected_area);
    }
}
