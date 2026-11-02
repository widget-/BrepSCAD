//! Truck primitive builders.

use std::collections::HashMap;

use brepscad_kernel_api::{KernelError, ProfileRegion};
use truck_modeling::*;

type KernelResult<T> = std::result::Result<T, KernelError>;

pub(crate) fn profile_face(region: &ProfileRegion) -> KernelResult<Face> {
    let outer = contour_wire(&region.outer)?;
    let mut wires = Vec::with_capacity(1 + region.holes.len());
    wires.push(outer);
    for hole in &region.holes {
        wires.push(contour_wire(hole)?);
    }
    builder::try_attach_plane(wires)
        .map_err(|error| KernelError::Backend(format!("profile face: {error:?}")))
}

fn contour_wire(points: &[[f64; 2]]) -> KernelResult<Wire> {
    if points.iter().flatten().any(|value| !value.is_finite()) {
        return Err(KernelError::Backend(
            "profile points must be finite".to_owned(),
        ));
    }

    let mut cleaned: Vec<[f64; 2]> = Vec::with_capacity(points.len());
    for point in points {
        if cleaned.last() != Some(point) {
            cleaned.push(*point);
        }
    }
    if cleaned.len() > 1 && cleaned.first() == cleaned.last() {
        cleaned.pop();
    }
    if cleaned.len() < 3 {
        return Err(KernelError::Backend(
            "profile contours need at least three distinct points".to_owned(),
        ));
    }

    let vertices: Vec<Vertex> = cleaned
        .iter()
        .map(|[x, y]| builder::vertex(Point3::new(*x, *y, 0.0)))
        .collect();
    let edges: Vec<Edge> = (0..vertices.len())
        .map(|index| builder::line(&vertices[index], &vertices[(index + 1) % vertices.len()]))
        .collect();
    Ok(edges.into_iter().collect())
}

pub(crate) fn polyhedron(points: &[[f64; 3]], faces: &[Vec<usize>]) -> KernelResult<Solid> {
    if points.len() < 4 || faces.len() < 4 {
        return Err(KernelError::Backend(
            "polyhedron needs at least four points and four faces".to_owned(),
        ));
    }
    if points.iter().flatten().any(|value| !value.is_finite()) {
        return Err(KernelError::Backend(
            "polyhedron points must be finite".to_owned(),
        ));
    }

    let scale = points
        .iter()
        .flatten()
        .fold(1.0_f64, |scale, value| scale.max(value.abs()));
    let weld_epsilon = (scale * 1.0e-9).max(1.0e-9);
    let mut welded = Vec::new();
    let mut remap = Vec::with_capacity(points.len());
    let mut seen = HashMap::new();
    for point in points {
        let key = (
            (point[0] / weld_epsilon).round() as i64,
            (point[1] / weld_epsilon).round() as i64,
            (point[2] / weld_epsilon).round() as i64,
        );
        let index = *seen.entry(key).or_insert_with(|| {
            welded.push(*point);
            welded.len() - 1
        });
        remap.push(index);
    }

    let mut welded_faces = Vec::with_capacity(faces.len());
    for face in faces {
        if face.len() < 3 {
            return Err(KernelError::Backend(
                "polyhedron faces need at least three vertices".to_owned(),
            ));
        }
        if face.iter().any(|index| *index >= points.len()) {
            return Err(KernelError::Backend(
                "polyhedron face index out of range".to_owned(),
            ));
        }
        let mut remapped = Vec::with_capacity(face.len());
        for index in face {
            let index = remap[*index];
            if remapped.last().copied() != Some(index) {
                remapped.push(index);
            }
        }
        if remapped.len() > 1 && remapped.first() == remapped.last() {
            remapped.pop();
        }
        if remapped.len() >= 3
            && remapped
                .iter()
                .all(|index| remapped.iter().filter(|other| *other == index).count() == 1)
        {
            welded_faces.push(remapped);
        }
    }
    if welded.len() < 4 || welded_faces.len() < 4 {
        return Err(KernelError::Backend(
            "polyhedron needs at least four points and four faces".to_owned(),
        ));
    }

    if signed_volume(&welded, &welded_faces) < 0.0 {
        for face in &mut welded_faces {
            face.reverse();
        }
    }

    let vertices: Vec<Vertex> = welded
        .iter()
        .map(|[x, y, z]| builder::vertex(Point3::new(*x, *y, *z)))
        .collect();
    let mut edges: HashMap<(usize, usize), Edge> = HashMap::new();
    let mut uses: HashMap<(usize, usize), (u32, u32)> = HashMap::new();
    let mut built_faces = Vec::with_capacity(welded_faces.len());

    for face in &welded_faces {
        if face.iter().any(|index| *index >= welded.len()) {
            return Err(KernelError::Backend(
                "polyhedron face index out of range".to_owned(),
            ));
        }

        let mut wire_edges = Vec::with_capacity(face.len());
        for index in 0..face.len() {
            let from = face[index];
            let to = face[(index + 1) % face.len()];
            if from == to {
                return Err(KernelError::Backend(
                    "polyhedron face repeats a vertex".to_owned(),
                ));
            }

            let key = if from < to { (from, to) } else { (to, from) };
            let edge = match edges.get(&key) {
                Some(edge) => edge.clone(),
                None => {
                    let edge = builder::line(&vertices[key.0], &vertices[key.1]);
                    edges.insert(key, edge.clone());
                    edge
                }
            };

            let oriented = if from < to {
                uses.entry(key).or_insert((0, 0)).0 += 1;
                edge
            } else {
                uses.entry(key).or_insert((0, 0)).1 += 1;
                edge.inverse()
            };
            wire_edges.push(oriented);
        }

        let wire: Wire = wire_edges.into_iter().collect();
        built_faces.push(
            builder::try_attach_plane(vec![wire])
                .map_err(|error| KernelError::Backend(format!("polyhedron face: {error:?}")))?,
        );
    }

    if uses
        .values()
        .any(|(forward, backward)| *forward != 1 || *backward != 1)
    {
        return Err(KernelError::Backend(
            "polyhedron faces must form a closed, consistently wound shell".to_owned(),
        ));
    }

    let shell: Shell = built_faces.into_iter().collect();
    Solid::try_new(vec![shell])
        .map_err(|error| KernelError::Backend(format!("polyhedron shell: {error:?}")))
}

pub(crate) fn cuboid(size: [f64; 3]) -> KernelResult<Solid> {
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

pub(crate) fn square(size: [f64; 2]) -> KernelResult<Face> {
    if size.iter().any(|value| *value <= 0.0) {
        return Err(KernelError::Backend(
            "square dimensions must be positive".to_owned(),
        ));
    }
    let [width, height] = size;
    polygon(&[
        [-width / 2.0, -height / 2.0],
        [width / 2.0, -height / 2.0],
        [width / 2.0, height / 2.0],
        [-width / 2.0, height / 2.0],
    ])
}

pub(crate) fn circle(radius: f64) -> KernelResult<Face> {
    if radius <= 0.0 {
        return Err(KernelError::Backend(
            "circle radius must be positive".to_owned(),
        ));
    }

    let vertex = builder::vertex(Point3::new(radius, 0.0, 0.0));
    let wire = builder::rsweep(
        &vertex,
        Point3::origin(),
        Vector3::unit_z(),
        Rad(std::f64::consts::TAU),
        2,
    );
    builder::try_attach_plane(vec![wire])
        .map_err(|error| KernelError::Backend(format!("circle failed: {error:?}")))
}

pub(crate) fn polygon(points: &[[f64; 2]]) -> KernelResult<Face> {
    if points.len() < 3 {
        return Err(KernelError::Backend(
            "polygon needs at least three points".to_owned(),
        ));
    }
    if points.iter().flatten().any(|value| !value.is_finite()) {
        return Err(KernelError::Backend(
            "polygon points must be finite".to_owned(),
        ));
    }

    let area = signed_area(points);
    if area.abs() < f64::EPSILON {
        return Err(KernelError::Backend("polygon has zero area".to_owned()));
    }

    let ordered: Vec<[f64; 2]> = if area < 0.0 {
        points.iter().rev().copied().collect()
    } else {
        points.to_vec()
    };
    let vertices: Vec<Vertex> = ordered
        .iter()
        .map(|[x, y]| builder::vertex(Point3::new(*x, *y, 0.0)))
        .collect();
    let edges: Vec<Edge> = (0..vertices.len())
        .map(|index| builder::line(&vertices[index], &vertices[(index + 1) % vertices.len()]))
        .collect();
    let wire: Wire = edges.into_iter().collect();
    builder::try_attach_plane(vec![wire])
        .map_err(|error| KernelError::Backend(format!("polygon failed: {error:?}")))
}

fn signed_area(points: &[[f64; 2]]) -> f64 {
    let mut area = 0.0;
    for index in 0..points.len() {
        let [x0, y0] = points[index];
        let [x1, y1] = points[(index + 1) % points.len()];
        area += x0 * y1 - x1 * y0;
    }
    area / 2.0
}

pub(crate) fn cylinder(radius: f64, height: f64) -> KernelResult<Solid> {
    if radius <= 0.0 || height <= 0.0 {
        return Err(KernelError::Backend(
            "cylinder dimensions must be positive".to_owned(),
        ));
    }

    // Revolving the generator line gives the side as analytic
    // `Surface::RevolutedCurve` patches. Extruding a disk instead would leave a
    // homotopy B-spline wall, which the boolean engine subdivides slowly and
    // the ray caster can only approximate.
    let bottom = builder::vertex(Point3::new(radius, 0.0, -height / 2.0));
    let top = builder::vertex(Point3::new(radius, 0.0, height / 2.0));
    let generator = builder::line(&bottom, &top);
    let side: Shell = builder::rsweep(
        &generator,
        Point3::origin(),
        Vector3::unit_z(),
        Rad(std::f64::consts::TAU),
        REVOLVE_DIVISION,
    );

    // Caps are planes trimmed by the side's own boundary arcs, so the shell
    // shares its edges and stays closed. Sweeping a radial line instead would
    // collapse at the axis, where the surface normals are undefined.
    //
    // Truck makes no promise about the direction of those arcs, so try the
    // winding combinations until the shell validates, then make sure the solid
    // faces outwards.
    let bottom_ring = boundary_ring(&side, -height / 2.0)
        .ok_or_else(|| KernelError::Backend("cylinder has no bottom boundary".to_owned()))?;
    let top_ring = boundary_ring(&side, height / 2.0)
        .ok_or_else(|| KernelError::Backend("cylinder has no top boundary".to_owned()))?;

    let mut built = None;
    for reverse_bottom in [false, true] {
        for reverse_top in [false, true] {
            let mut faces: Vec<Face> = side.face_iter().cloned().collect();
            faces.push(cap_face(&bottom_ring, reverse_bottom)?);
            faces.push(cap_face(&top_ring, reverse_top)?);
            let shell: Shell = faces.into_iter().collect();
            if let Ok(solid) = Solid::try_new(vec![shell]) {
                built = Some(solid);
                break;
            }
        }
        if built.is_some() {
            break;
        }
    }
    let mut solid = built
        .ok_or_else(|| KernelError::Backend("cylinder caps do not close the shell".to_owned()))?;
    if !faces_outwards(&solid) {
        solid.not();
    }
    Ok(solid)
}

/// Builds a planar cap from a boundary ring, optionally traversed in reverse.
fn cap_face(ring: &[Edge], reverse: bool) -> KernelResult<Face> {
    let edges: Vec<Edge> = if reverse {
        ring.iter().rev().map(Edge::inverse).collect()
    } else {
        ring.to_vec()
    };
    builder::try_attach_plane(vec![edges.into()])
        .map_err(|error| KernelError::Backend(format!("cylinder cap is not planar: {error:?}")))
}

/// True when the topmost planar face points along +Z.
fn faces_outwards(solid: &Solid) -> bool {
    let mut highest: Option<(f64, Vector3)> = None;
    for face in solid.face_iter() {
        let Surface::Plane(plane) = face.surface() else {
            continue;
        };
        let mut height = 0.0;
        let mut count = 0.0;
        for vertex in face.vertex_iter() {
            height += vertex.point().z;
            count += 1.0;
        }
        if count == 0.0 {
            continue;
        }
        let centroid_height = height / count;
        let mut normal = plane.normal();
        if !face.orientation() {
            normal = -normal;
        }
        if highest
            .map(|(best, _)| centroid_height > best)
            .unwrap_or(true)
        {
            highest = Some((centroid_height, normal));
        }
    }
    highest.map(|(_, normal)| normal.z > 0.0).unwrap_or(true)
}

/// Chains the edges of `shell` that lie at height `z` into one closed ring.
fn boundary_ring(shell: &Shell, z: f64) -> Option<Vec<Edge>> {
    let at_height = |edge: &Edge| {
        let front = edge.absolute_front().point();
        let back = edge.absolute_back().point();
        (front.z - z).abs() < 1.0e-9 && (back.z - z).abs() < 1.0e-9
    };

    let mut edges: Vec<Edge> = shell
        .face_iter()
        .flat_map(|face| face.absolute_boundaries())
        .flat_map(|wire| wire.iter())
        .filter(|edge| at_height(edge))
        .cloned()
        .collect();
    if edges.is_empty() {
        return None;
    }

    // Walk the arcs end to end, flipping each one as needed, so the result
    // satisfies the wire invariant: every edge starts where the last ended.
    let mut ring: Vec<Edge> = Vec::with_capacity(edges.len());
    let mut current = edges.remove(0);
    let start = current.front().clone();
    loop {
        let end = current.back().clone();
        ring.push(current);
        if end == start {
            break;
        }
        let index = edges
            .iter()
            .position(|edge| *edge.front() == end || *edge.back() == end)?;
        let candidate = edges.remove(index);
        current = if *candidate.front() == end {
            candidate
        } else {
            candidate.inverse()
        };
    }
    Some(ring)
}

/// Segments per half turn used to build revolved primitives. The B-rep keeps
/// one analytic surface patch per segment.
const REVOLVE_DIVISION: usize = 4;

const SPHERE_SEGMENTS: usize = 32;
const SPHERE_RINGS: usize = 16;

pub(crate) fn sphere(radius: f64) -> KernelResult<Solid> {
    if radius <= 0.0 {
        return Err(KernelError::Backend(
            "sphere radius must be positive".to_owned(),
        ));
    }

    // `$fn` is deliberately ignored. In every library checked (BOSL2, MCAD) it
    // is a smoothness knob on spheres, never a request for a polyhedron —
    // BOSL2 asks for facets through `spheroid(style = "icosa")` instead. The
    // preview draws spheres analytically (see `raycast_scene`), so this facet
    // count only decides how fine the B-rep is for booleans.
    let (segments, rings) = (SPHERE_SEGMENTS, SPHERE_RINGS);
    let north = builder::vertex(Point3::new(0.0, 0.0, radius));
    let south = builder::vertex(Point3::new(0.0, 0.0, -radius));

    let mut latitude = Vec::with_capacity(rings - 1);
    for ring in 1..rings {
        let theta = std::f64::consts::PI * ring as f64 / rings as f64;
        let (sin_theta, cos_theta) = theta.sin_cos();
        let mut row = Vec::with_capacity(segments);
        for segment in 0..segments {
            let phi = std::f64::consts::TAU * segment as f64 / segments as f64;
            let (sin_phi, cos_phi) = phi.sin_cos();
            row.push(builder::vertex(Point3::new(
                radius * sin_theta * cos_phi,
                radius * sin_theta * sin_phi,
                radius * cos_theta,
            )));
        }
        latitude.push(row);
    }

    let mut horizontal = Vec::with_capacity(latitude.len());
    for row in &latitude {
        let mut edges = Vec::with_capacity(segments);
        for segment in 0..segments {
            edges.push(builder::line(&row[segment], &row[(segment + 1) % segments]));
        }
        horizontal.push(edges);
    }

    let mut vertical = Vec::with_capacity(latitude.len().saturating_sub(1));
    let mut diagonal = Vec::with_capacity(latitude.len().saturating_sub(1));
    for ring in 0..latitude.len().saturating_sub(1) {
        let mut vertical_edges = Vec::with_capacity(segments);
        let mut diagonal_edges = Vec::with_capacity(segments);
        for segment in 0..segments {
            vertical_edges.push(builder::line(
                &latitude[ring][segment],
                &latitude[ring + 1][segment],
            ));
            diagonal_edges.push(builder::line(
                &latitude[ring][segment],
                &latitude[ring + 1][(segment + 1) % segments],
            ));
        }
        vertical.push(vertical_edges);
        diagonal.push(diagonal_edges);
    }

    let mut north_edges = Vec::with_capacity(segments);
    let mut south_edges = Vec::with_capacity(segments);
    let last_ring = latitude.len() - 1;
    for (north_vertex, south_vertex) in latitude[0].iter().zip(&latitude[last_ring]) {
        north_edges.push(builder::line(&north, north_vertex));
        south_edges.push(builder::line(&south, south_vertex));
    }

    let mut faces = Vec::with_capacity(2 * segments * latitude.len());
    for segment in 0..segments {
        let wire: Wire = vec![
            north_edges[segment].clone(),
            horizontal[0][segment].clone(),
            north_edges[(segment + 1) % segments].inverse(),
        ]
        .into_iter()
        .collect();
        faces.push(
            builder::try_attach_plane(vec![wire])
                .map_err(|error| KernelError::Backend(format!("sphere cap failed: {error:?}")))?,
        );

        let wire: Wire = vec![
            south_edges[(segment + 1) % segments].clone(),
            horizontal[last_ring][segment].inverse(),
            south_edges[segment].inverse(),
        ]
        .into_iter()
        .collect();
        faces.push(
            builder::try_attach_plane(vec![wire])
                .map_err(|error| KernelError::Backend(format!("sphere cap failed: {error:?}")))?,
        );
    }

    for ring in 0..vertical.len() {
        for segment in 0..segments {
            let wire: Wire = vec![
                vertical[ring][segment].clone(),
                horizontal[ring + 1][segment].clone(),
                diagonal[ring][segment].inverse(),
            ]
            .into_iter()
            .collect();
            faces.push(builder::try_attach_plane(vec![wire]).map_err(|error| {
                KernelError::Backend(format!("sphere facet failed: {error:?}"))
            })?);

            let wire: Wire = vec![
                diagonal[ring][segment].clone(),
                vertical[ring][(segment + 1) % segments].inverse(),
                horizontal[ring][segment].inverse(),
            ]
            .into_iter()
            .collect();
            faces.push(builder::try_attach_plane(vec![wire]).map_err(|error| {
                KernelError::Backend(format!("sphere facet failed: {error:?}"))
            })?);
        }
    }

    let shell: Shell = faces.into_iter().collect();
    Solid::try_new(vec![shell])
        .map_err(|error| KernelError::Backend(format!("sphere shell failed: {error:?}")))
}
fn signed_volume(points: &[[f64; 3]], faces: &[Vec<usize>]) -> f64 {
    let mut volume = 0.0;
    for face in faces {
        let Some(first) = face.first() else {
            continue;
        };
        let origin = points[*first];
        for index in 1..face.len().saturating_sub(1) {
            let left = points[face[index]];
            let right = points[face[index + 1]];
            volume += origin[0] * (left[1] * right[2] - left[2] * right[1])
                - origin[1] * (left[0] * right[2] - left[2] * right[0])
                + origin[2] * (left[0] * right[1] - left[1] * right[0]);
        }
    }
    volume / 6.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welds_coincident_vertices_before_shell_validation() {
        let points = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
        ];
        let faces = vec![vec![4, 2, 1], vec![0, 1, 3], vec![0, 3, 2], vec![1, 2, 3]];
        assert!(polyhedron(&points, &faces).is_ok());
    }
}
