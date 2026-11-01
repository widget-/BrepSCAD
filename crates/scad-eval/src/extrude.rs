//! Twisted/scaled linear extrusion for simple 2D profiles.

use crate::profile::Profile;

type MeshData = (Vec<[f64; 3]>, Vec<Vec<usize>>);

/// Builds a lofted polyhedron from a single outer contour.
///
/// Returns `None` for profiles with holes, multiple regions, or degenerate
/// contours. This is the mesh fallback for `linear_extrude` features the
/// kernel's straight extrusion path cannot express.
pub(crate) fn loft(
    profile: &Profile,
    height: f64,
    twist_degrees: f64,
    scale: [f64; 2],
    slices: usize,
) -> Option<MeshData> {
    let [region] = profile.as_slice() else {
        return None;
    };
    if !region.holes.is_empty() {
        return None;
    }

    let mut contour = region.outer.clone();
    if signed_area(&contour) < 0.0 {
        contour.reverse();
    }
    let triangles = triangulate(&contour)?;
    let slices = slices.max(2);
    let twist = twist_degrees.to_radians();

    let collapsed = scale[0].abs() <= f64::EPSILON && scale[1].abs() <= f64::EPSILON;
    let ring_count = if collapsed { slices - 1 } else { slices };
    let mut points = Vec::with_capacity(ring_count * contour.len() + usize::from(collapsed));
    for slice in 0..ring_count {
        let amount = slice as f64 / (slices - 1) as f64;
        let (sin, cos) = (twist * amount).sin_cos();
        let sx = 1.0 + (scale[0] - 1.0) * amount;
        let sy = 1.0 + (scale[1] - 1.0) * amount;
        for &[x, y] in &contour {
            let x = x * sx;
            let y = y * sy;
            points.push([x * cos - y * sin, x * sin + y * cos, amount * height]);
        }
    }
    let apex = collapsed.then(|| {
        points.push([0.0, 0.0, height]);
        points.len() - 1
    });

    let mut faces = Vec::new();
    let ring_len = contour.len();
    for slice in 0..ring_count.saturating_sub(1) {
        for index in 0..ring_len {
            let next = (index + 1) % ring_len;
            let bottom = slice * ring_len + index;
            let bottom_next = slice * ring_len + next;
            let top = (slice + 1) * ring_len + index;
            let top_next = (slice + 1) * ring_len + next;
            faces.push(vec![bottom, bottom_next, top_next]);
            faces.push(vec![bottom, top_next, top]);
        }
    }

    if let Some(apex) = apex {
        let last = (ring_count - 1) * ring_len;
        for index in 0..ring_len {
            let next = (index + 1) % ring_len;
            faces.push(vec![last + index, last + next, apex]);
        }
    } else {
        let top = (ring_count - 1) * ring_len;
        for triangle in &triangles {
            faces.push(vec![
                top + triangle[0],
                top + triangle[1],
                top + triangle[2],
            ]);
        }
    }

    for triangle in triangles {
        faces.push(vec![triangle[2], triangle[1], triangle[0]]);
    }

    Some((points, faces))
}

pub(crate) fn triangulate(points: &[[f64; 2]]) -> Option<Vec<[usize; 3]>> {
    if points.len() < 3 {
        return None;
    }
    let mut indices: Vec<usize> = (0..points.len()).collect();
    let mut triangles = Vec::new();
    let mut attempts = 0;
    while indices.len() > 3 {
        let mut clipped = false;
        for index in 0..indices.len() {
            let previous = indices[(index + indices.len() - 1) % indices.len()];
            let current = indices[index];
            let next = indices[(index + 1) % indices.len()];
            if cross(points[previous], points[current], points[next]) <= 1.0e-12 {
                continue;
            }
            if indices.iter().any(|candidate| {
                *candidate != previous
                    && *candidate != current
                    && *candidate != next
                    && point_in_triangle(
                        points[*candidate],
                        points[previous],
                        points[current],
                        points[next],
                    )
            }) {
                continue;
            }
            triangles.push([previous, current, next]);
            indices.remove(index);
            clipped = true;
            break;
        }
        if !clipped {
            return None;
        }
        attempts += 1;
        if attempts > points.len() * points.len() {
            return None;
        }
    }
    triangles.push([indices[0], indices[1], indices[2]]);
    Some(triangles)
}

fn signed_area(points: &[[f64; 2]]) -> f64 {
    let mut area = 0.0;
    for index in 0..points.len() {
        let next = (index + 1) % points.len();
        area += points[index][0] * points[next][1] - points[next][0] * points[index][1];
    }
    area / 2.0
}

fn cross(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn point_in_triangle(point: [f64; 2], a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> bool {
    let ab = cross(a, b, point);
    let bc = cross(b, c, point);
    let ca = cross(c, a, point);
    ab >= -1.0e-12 && bc >= -1.0e-12 && ca >= -1.0e-12
}
