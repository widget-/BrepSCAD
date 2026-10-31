//! Convex hulls and convex Minkowski vertex sums.

use std::collections::{BTreeSet, HashMap, HashSet};

type Point = [f64; 3];

#[derive(Clone, Copy)]
struct Face {
    a: usize,
    b: usize,
    c: usize,
}

/// Returns a closed, outward-wound triangle hull, or `None` when the points
/// are coplanar or otherwise degenerate.
pub(crate) fn convex_hull(points: &[Point]) -> Option<(Vec<Point>, Vec<Vec<usize>>)> {
    let points = deduplicate(points);
    if points.len() < 4 {
        return None;
    }
    let scale = bounding_scale(&points);
    let epsilon = (scale * 1.0e-9).max(1.0e-9);
    let [p0, p1, p2, p3] = initial_tetrahedron(&points, epsilon)?;

    let mut faces = vec![
        Face {
            a: p0,
            b: p1,
            c: p2,
        },
        Face {
            a: p0,
            b: p1,
            c: p3,
        },
        Face {
            a: p0,
            b: p2,
            c: p3,
        },
        Face {
            a: p1,
            b: p2,
            c: p3,
        },
    ];
    let interior = average([points[p0], points[p1], points[p2], points[p3]]);
    for face in &mut faces {
        *face = orient(*face, &points, interior);
    }

    let initial = [p0, p1, p2, p3];
    for index in 0..points.len() {
        if initial.contains(&index) {
            continue;
        }
        let point = points[index];
        let visible: Vec<usize> = faces
            .iter()
            .enumerate()
            .filter_map(|(face_index, face)| {
                let normal = face_normal(&points, *face)?;
                (dot(normal, subtract(point, points[face.a])) > epsilon).then_some(face_index)
            })
            .collect();
        if visible.is_empty() {
            continue;
        }

        let mut horizon = HashMap::new();
        for face_index in &visible {
            let face = faces[*face_index];
            for edge in [(face.a, face.b), (face.b, face.c), (face.c, face.a)] {
                if horizon.remove(&(edge.1, edge.0)).is_none() {
                    horizon.insert(edge, ());
                }
            }
        }

        let visible_set: HashSet<usize> = visible.into_iter().collect();
        let mut kept = Vec::with_capacity(faces.len());
        for (face_index, face) in faces.into_iter().enumerate() {
            if !visible_set.contains(&face_index) {
                kept.push(face);
            }
        }
        for (a, b) in horizon.keys() {
            let face = orient(
                Face {
                    a: *a,
                    b: *b,
                    c: index,
                },
                &points,
                interior,
            );
            if face_normal(&points, face).is_some() {
                kept.push(face);
            }
        }
        faces = kept;
    }

    compact(points, faces, epsilon)
}

/// Returns the vertex set of the Minkowski sum for convex point sets.
/// Pairwise sums are sufficient for convex shapes; the caller is responsible
/// for only using this with convex geometry.
pub(crate) fn minkowski_points(groups: &[Vec<Point>]) -> Option<Vec<Point>> {
    let mut groups = groups.iter();
    let first = groups.next()?;
    if first.is_empty() {
        return None;
    }
    let mut sums = deduplicate(first);
    for group in groups {
        if group.is_empty() {
            return None;
        }
        let mut next = Vec::with_capacity(sums.len() * group.len());
        for a in &sums {
            for b in group {
                next.push([a[0] + b[0], a[1] + b[1], a[2] + b[2]]);
            }
        }
        sums = deduplicate(&next);
    }
    Some(sums)
}

fn compact(
    points: Vec<Point>,
    faces: Vec<Face>,
    epsilon: f64,
) -> Option<(Vec<Point>, Vec<Vec<usize>>)> {
    let faces: Vec<Face> = faces
        .into_iter()
        .filter(|face| face_normal(&points, *face).is_some())
        .collect();
    if faces.len() < 4 {
        return None;
    }

    let mut used = BTreeSet::new();
    for face in &faces {
        used.insert(face.a);
        used.insert(face.b);
        used.insert(face.c);
    }
    if used.len() < 4 {
        return None;
    }

    let mut mapping = HashMap::new();
    let mut compacted = Vec::with_capacity(used.len());
    for index in used {
        mapping.insert(index, compacted.len());
        compacted.push(points[index]);
    }
    let mut compacted_faces: Vec<Vec<usize>> = faces
        .iter()
        .map(|face| vec![mapping[&face.a], mapping[&face.b], mapping[&face.c]])
        .collect();

    let volume = signed_volume(&compacted, &compacted_faces);
    if !volume.is_finite() || volume.abs() <= epsilon * epsilon * epsilon {
        return None;
    }
    if volume < 0.0 {
        for face in &mut compacted_faces {
            face.swap(1, 2);
        }
    }
    Some((compacted, compacted_faces))
}

fn initial_tetrahedron(points: &[Point], epsilon: f64) -> Option<[usize; 4]> {
    let p0 = (0..points.len()).min_by(|a, b| points[*a][0].total_cmp(&points[*b][0]))?;
    let p1 = (0..points.len())
        .filter(|index| *index != p0)
        .max_by(|a, b| {
            distance_squared(points[*a], points[p0])
                .total_cmp(&distance_squared(points[*b], points[p0]))
        })?;
    if distance_squared(points[p0], points[p1]) <= epsilon * epsilon {
        return None;
    }

    let edge = subtract(points[p1], points[p0]);
    let p2 = (0..points.len())
        .filter(|index| *index != p0 && *index != p1)
        .max_by(|a, b| {
            line_distance_squared(points[*a], points[p0], edge)
                .total_cmp(&line_distance_squared(points[*b], points[p0], edge))
        })?;
    if line_distance_squared(points[p2], points[p0], edge) <= epsilon * epsilon {
        return None;
    }

    let normal = cross(edge, subtract(points[p2], points[p0]));
    let normal_length = norm(normal);
    if normal_length <= epsilon {
        return None;
    }
    let p3 = (0..points.len())
        .filter(|index| *index != p0 && *index != p1 && *index != p2)
        .max_by(|a, b| {
            dot(normal, subtract(points[*a], points[p0]))
                .abs()
                .total_cmp(&dot(normal, subtract(points[*b], points[p0])).abs())
        })?;
    if dot(normal, subtract(points[p3], points[p0])).abs() <= epsilon * normal_length {
        return None;
    }

    Some([p0, p1, p2, p3])
}

fn orient(face: Face, points: &[Point], interior: Point) -> Face {
    let Some(normal) = face_normal(points, face) else {
        return face;
    };
    if dot(normal, subtract(interior, points[face.a])) > 0.0 {
        Face {
            a: face.a,
            b: face.c,
            c: face.b,
        }
    } else {
        face
    }
}

fn face_normal(points: &[Point], face: Face) -> Option<Point> {
    let normal = cross(
        subtract(points[face.b], points[face.a]),
        subtract(points[face.c], points[face.a]),
    );
    (norm(normal) > 1.0e-18).then_some(normal)
}

fn signed_volume(points: &[Point], faces: &[Vec<usize>]) -> f64 {
    faces
        .iter()
        .map(|face| dot(points[face[0]], cross(points[face[1]], points[face[2]])) / 6.0)
        .sum()
}

fn bounding_scale(points: &[Point]) -> f64 {
    let mut min = points[0];
    let mut max = points[0];
    for point in points {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    let extent = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    extent[0].max(extent[1]).max(extent[2]).max(1.0)
}

fn deduplicate(points: &[Point]) -> Vec<Point> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for &point in points {
        let key = (
            (point[0] * 1.0e9).round() as i64,
            (point[1] * 1.0e9).round() as i64,
            (point[2] * 1.0e9).round() as i64,
        );
        if seen.insert(key) {
            result.push(point);
        }
    }
    result
}

fn average(points: [Point; 4]) -> Point {
    [
        (points[0][0] + points[1][0] + points[2][0] + points[3][0]) / 4.0,
        (points[0][1] + points[1][1] + points[2][1] + points[3][1]) / 4.0,
        (points[0][2] + points[1][2] + points[2][2] + points[3][2]) / 4.0,
    ]
}

fn subtract(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(value: Point) -> f64 {
    dot(value, value).sqrt()
}

fn distance_squared(a: Point, b: Point) -> f64 {
    dot(subtract(a, b), subtract(a, b))
}

fn line_distance_squared(point: Point, origin: Point, direction: Point) -> f64 {
    let offset = subtract(point, origin);
    let normal = cross(direction, offset);
    dot(normal, normal) / dot(direction, direction)
}
