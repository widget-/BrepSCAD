//! Ray casting against B-spline and NURBS surface patches.
//!
//! Truck's boolean results and swept shapes are mostly B-spline patches, so the
//! caster needs to hit them directly: sampling every patch up front costs far
//! more than solving for the few patches a ray actually touches.

use crate::{Ray, SurfaceHit};

/// A tensor-product B-spline surface, rational when any weight is not 1.
///
/// Control points are stored homogeneous, row-major in `(u, v)`: index
/// `i * nv + j`, so one evaluation serves both B-spline and NURBS patches.
#[derive(Debug, Clone, PartialEq)]
pub struct SplineSurface {
    degree_u: usize,
    degree_v: usize,
    knots_u: Vec<f64>,
    knots_v: Vec<f64>,
    control: Vec<[f64; 4]>,
    patches: Vec<Patch>,
}

#[derive(Debug, Clone, PartialEq)]
struct Patch {
    u: (f64, f64),
    v: (f64, f64),
    min: [f64; 3],
    max: [f64; 3],
}

/// Patches per knot span, in each direction.
const PATCH_SUBDIVISIONS: usize = 2;
/// Samples per patch side when bounding it.
const PATCH_SAMPLES: usize = 3;
/// Newton iterations per patch.
const NEWTON_STEPS: usize = 24;

impl SplineSurface {
    /// `knots_u` has `nu + degree_u + 1` entries, likewise for `v`.
    pub fn new(
        degree_u: usize,
        degree_v: usize,
        knots_u: Vec<f64>,
        knots_v: Vec<f64>,
        control: Vec<[f64; 4]>,
    ) -> Option<Self> {
        let nu = knots_u.len().checked_sub(degree_u + 1)?;
        let nv = knots_v.len().checked_sub(degree_v + 1)?;
        if nu == 0 || nv == 0 || control.len() != nu * nv {
            return None;
        }
        let mut surface = Self {
            degree_u,
            degree_v,
            knots_u,
            knots_v,
            control,
            patches: Vec::new(),
        };
        surface.patches = surface.build_patches();
        (!surface.patches.is_empty()).then_some(surface)
    }

    pub fn parameter_range(&self) -> ((f64, f64), (f64, f64)) {
        (
            (
                self.knots_u[self.degree_u],
                self.knots_u[self.knots_u.len() - self.degree_u - 1],
            ),
            (
                self.knots_v[self.degree_v],
                self.knots_v[self.knots_v.len() - self.degree_v - 1],
            ),
        )
    }

    pub(crate) fn intersect(&self, ray: &Ray, out: &mut Vec<SurfaceHit>) {
        let (u_range, v_range) = self.parameter_range();
        for patch in &self.patches {
            let Some(entry) = slab_entry(patch, ray) else {
                continue;
            };
            let Some((u, v)) = self.solve(patch, ray, u_range, v_range) else {
                continue;
            };
            let Some((point, normal)) = self.point_and_normal(u, v) else {
                continue;
            };
            let offset = sub(point, ray.origin);
            let distance = dot(offset, ray.direction);
            if distance <= 1.0e-9 || distance < entry - 1.0e-6 {
                continue;
            }
            out.push(SurfaceHit {
                distance,
                position: point,
                normal,
                uv: [u, v],
            });
        }
    }

    fn build_patches(&self) -> Vec<Patch> {
        let (u_range, v_range) = self.parameter_range();
        let u_spans = span_count(&self.knots_u, u_range);
        let v_spans = span_count(&self.knots_v, v_range);
        let u_parts = (u_spans * PATCH_SUBDIVISIONS).max(1);
        let v_parts = (v_spans * PATCH_SUBDIVISIONS).max(1);

        let mut patches = Vec::new();
        for i in 0..u_parts {
            for j in 0..v_parts {
                let u0 = lerp(u_range.0, u_range.1, i as f64 / u_parts as f64);
                let u1 = lerp(u_range.0, u_range.1, (i + 1) as f64 / u_parts as f64);
                let v0 = lerp(v_range.0, v_range.1, j as f64 / v_parts as f64);
                let v1 = lerp(v_range.0, v_range.1, (j + 1) as f64 / v_parts as f64);

                let mut min = [f64::INFINITY; 3];
                let mut max = [f64::NEG_INFINITY; 3];
                for a in 0..=PATCH_SAMPLES {
                    for b in 0..=PATCH_SAMPLES {
                        let u = lerp(u0, u1, a as f64 / PATCH_SAMPLES as f64);
                        let v = lerp(v0, v1, b as f64 / PATCH_SAMPLES as f64);
                        let Some(point) = self.point(u, v) else {
                            continue;
                        };
                        for axis in 0..3 {
                            min[axis] = min[axis].min(point[axis]);
                            max[axis] = max[axis].max(point[axis]);
                        }
                    }
                }
                if !min[0].is_finite() {
                    continue;
                }
                // The samples bound the patch only approximately; grow the box
                // by its diagonal so a grazing ray is not rejected early.
                let pad = (0..3)
                    .map(|axis| max[axis] - min[axis])
                    .fold(0.0_f64, f64::max)
                    * 0.05;
                let pad = pad.max(1.0e-9);
                for axis in 0..3 {
                    min[axis] -= pad;
                    max[axis] += pad;
                }
                patches.push(Patch {
                    u: (u0, u1),
                    v: (v0, v1),
                    min,
                    max,
                });
            }
        }
        patches
    }

    /// Newton solve of `S(u, v) = origin + t·direction` inside one patch.
    fn solve(
        &self,
        patch: &Patch,
        ray: &Ray,
        u_range: (f64, f64),
        v_range: (f64, f64),
    ) -> Option<(f64, f64)> {
        // Two axes orthogonal to the ray turn the 3D hit into a 2D root find.
        let normal = perpendicular(ray.direction);
        let bitangent = cross(ray.direction, normal);

        let mut u = lerp(patch.u.0, patch.u.1, 0.5);
        let mut v = lerp(patch.v.0, patch.v.1, 0.5);
        for _ in 0..NEWTON_STEPS {
            let (point, derivative_u, derivative_v) = self.frame(u, v)?;
            let gap = sub(point, ray.origin);
            let f = dot(gap, normal);
            let g = dot(gap, bitangent);
            if f.abs() < 1.0e-9 && g.abs() < 1.0e-9 {
                return Some((u, v));
            }
            let (a, b) = (dot(derivative_u, normal), dot(derivative_v, normal));
            let (c, d) = (dot(derivative_u, bitangent), dot(derivative_v, bitangent));
            let determinant = a * d - b * c;
            if determinant.abs() < 1.0e-14 {
                return None;
            }
            let du = (f * d - b * g) / determinant;
            let dv = (a * g - f * c) / determinant;
            u = (u - du).clamp(u_range.0, u_range.1);
            v = (v - dv).clamp(v_range.0, v_range.1);
        }
        let (point, _, _) = self.frame(u, v)?;
        let gap = sub(point, ray.origin);
        let residual = dot(gap, normal).abs().max(dot(gap, bitangent).abs());
        (residual < 1.0e-6).then_some((u, v))
    }

    fn point(&self, u: f64, v: f64) -> Option<[f64; 3]> {
        let (point, _, _) = self.frame(u, v)?;
        if point.iter().any(|value| !value.is_finite()) {
            return None;
        }
        Some(point)
    }

    fn point_and_normal(&self, u: f64, v: f64) -> Option<([f64; 3], [f64; 3])> {
        let (point, derivative_u, derivative_v) = self.frame(u, v)?;
        let normal = normalize(cross(derivative_u, derivative_v));
        if !normal.iter().all(|value| value.is_finite()) || normal == [0.0; 3] {
            return None;
        }
        Some((point, normal))
    }

    /// Point and first derivatives, in non-homogeneous coordinates.
    fn frame(&self, u: f64, v: f64) -> Option<([f64; 3], [f64; 3], [f64; 3])> {
        let (basis_u, slope_u) = basis(&self.knots_u, self.degree_u, u)?;
        let (basis_v, slope_v) = basis(&self.knots_v, self.degree_v, v)?;
        let nv = basis_v.len();

        let mut point = [0.0; 4];
        let mut along_u = [0.0; 4];
        let mut along_v = [0.0; 4];
        for (i, weight_u) in basis_u.iter().enumerate() {
            for (j, weight_v) in basis_v.iter().enumerate() {
                let control = self.control[i * nv + j];
                let scale = weight_u * weight_v;
                let du = slope_u[i] * weight_v;
                let dv = weight_u * slope_v[j];
                for axis in 0..4 {
                    point[axis] += control[axis] * scale;
                    along_u[axis] += control[axis] * du;
                    along_v[axis] += control[axis] * dv;
                }
            }
        }

        let weight = point[3];
        if weight.abs() <= f64::EPSILON {
            return None;
        }
        let point = [point[0] / weight, point[1] / weight, point[2] / weight];
        // Quotient rule for the rational case: S' = (A' - w' S) / w.
        let derivative = |along: [f64; 4]| {
            [
                (along[0] - along[3] * point[0]) / weight,
                (along[1] - along[3] * point[1]) / weight,
                (along[2] - along[3] * point[2]) / weight,
            ]
        };
        let derivative_u = derivative(along_u);
        let derivative_v = derivative(along_v);
        if !point.iter().all(|value| value.is_finite())
            || !derivative_u.iter().all(|value| value.is_finite())
            || !derivative_v.iter().all(|value| value.is_finite())
        {
            return None;
        }
        Some((point, derivative_u, derivative_v))
    }
}

fn span_count(knots: &[f64], range: (f64, f64)) -> usize {
    knots
        .windows(2)
        .filter(|pair| pair[1] > pair[0] && pair[0] >= range.0 && pair[1] <= range.1)
        .count()
        .max(1)
}

/// B-spline basis values and their first derivatives at `t`.
fn basis(knots: &[f64], degree: usize, t: f64) -> Option<(Vec<f64>, Vec<f64>)> {
    let values = basis_plain(knots, degree, t)?;
    let count = values.len();
    let mut derivatives = vec![0.0; count];
    if degree > 0 {
        // N'_{i,p} = p * (N_{i,p-1} / (u_{i+p} - u_i)
        //               - N_{i+1,p-1} / (u_{i+p+1} - u_{i+1}))
        let lower = basis_plain(knots, degree - 1, t)?;
        let p = degree as f64;
        for (i, slope) in derivatives.iter_mut().enumerate() {
            let first = quotient(knots, i, i + degree, lower.get(i).copied().unwrap_or(0.0));
            let second = quotient(
                knots,
                i + 1,
                i + 1 + degree,
                lower.get(i + 1).copied().unwrap_or(0.0),
            );
            *slope = p * (first - second);
        }
    }
    Some((values, derivatives))
}

fn quotient(knots: &[f64], from: usize, to: usize, value: f64) -> f64 {
    let width = knots.get(to).copied().unwrap_or(0.0) - knots.get(from).copied().unwrap_or(0.0);
    if width.abs() > 1.0e-12 {
        value / width
    } else {
        0.0
    }
}

/// Basis values of one degree, as the de Boor triangle (The NURBS Book A2.2).
fn basis_plain(knots: &[f64], degree: usize, t: f64) -> Option<Vec<f64>> {
    let count = knots.len().checked_sub(degree + 1)?;
    if count == 0 {
        return None;
    }
    let low = knots[degree];
    let high = knots[count];
    if t < low - 1.0e-9 || t > high + 1.0e-9 {
        return None;
    }
    let t = t.clamp(low, high);
    let span = find_span(knots, degree, t, count)?;
    let mut values = vec![0.0; count];
    if degree == 0 {
        values[span] = 1.0;
        return Some(values);
    }

    let mut local = vec![0.0; degree + 1];
    local[0] = 1.0;
    let mut left = vec![0.0; degree + 1];
    let mut right = vec![0.0; degree + 1];
    for j in 1..=degree {
        left[j] = t - knots[span + 1 - j];
        right[j] = knots[span + j] - t;
        let mut saved = 0.0;
        for r in 0..j {
            let denominator = right[r + 1] + left[j - r];
            let temp = if denominator.abs() > 1.0e-12 {
                local[r] / denominator
            } else {
                0.0
            };
            local[r] = saved + right[r + 1] * temp;
            saved = left[j - r] * temp;
        }
        local[j] = saved;
    }

    for (offset, value) in local.iter().enumerate() {
        let index = span + offset;
        if index >= degree {
            values[index - degree] = *value;
        }
    }
    Some(values)
}

fn find_span(knots: &[f64], degree: usize, t: f64, count: usize) -> Option<usize> {
    if t < knots[degree] || t > knots[count] {
        return None;
    }
    let mut span = degree;
    while span + 1 < count && t >= knots[span + 1] {
        span += 1;
    }
    Some(span)
}

fn slab_entry(patch: &Patch, ray: &Ray) -> Option<f64> {
    let mut enter = f64::NEG_INFINITY;
    let mut exit = f64::INFINITY;
    for axis in 0..3 {
        let origin = ray.origin[axis];
        let direction = ray.direction[axis];
        if direction.abs() < 1.0e-12 {
            if origin < patch.min[axis] || origin > patch.max[axis] {
                return None;
            }
            continue;
        }
        let inverse = 1.0 / direction;
        let mut near = (patch.min[axis] - origin) * inverse;
        let mut far = (patch.max[axis] - origin) * inverse;
        if near > far {
            std::mem::swap(&mut near, &mut far);
        }
        enter = enter.max(near);
        exit = exit.min(far);
        if enter > exit {
            return None;
        }
    }
    if exit < 0.0 {
        None
    } else {
        Some(enter.max(0.0))
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(a: [f64; 3]) -> [f64; 3] {
    let length = dot(a, a).sqrt();
    if length > 1.0e-12 {
        [a[0] / length, a[1] / length, a[2] / length]
    } else {
        [0.0; 3]
    }
}

/// Any unit vector orthogonal to `direction`.
fn perpendicular(direction: [f64; 3]) -> [f64; 3] {
    let candidate = if direction[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    normalize(cross(direction, candidate))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bilinear_square() -> SplineSurface {
        // Unit square in z = 0: u along x, v along y.
        let control = vec![
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 0.0, 1.0],
        ];
        SplineSurface::new(
            1,
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![0.0, 0.0, 1.0, 1.0],
            control,
        )
        .expect("bilinear surface")
    }

    #[test]
    fn bilinear_patch_hits_where_the_parameters_say() {
        let surface = bilinear_square();
        let ray = Ray::new([0.25, 0.75, 2.0], [0.0, 0.0, -1.0]);
        let mut hits = Vec::new();
        surface.intersect(&ray, &mut hits);
        let hit = hits
            .iter()
            .min_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap())
            .expect("hit");

        assert!(
            (hit.distance - 2.0).abs() < 1e-6,
            "distance {}",
            hit.distance
        );
        assert!((hit.uv[0] - 0.25).abs() < 1e-6, "u {}", hit.uv[0]);
        assert!((hit.uv[1] - 0.75).abs() < 1e-6, "v {}", hit.uv[1]);
        assert!(hit.normal[2].abs() > 0.99, "normal {:?}", hit.normal);
    }

    #[test]
    fn quadratic_patch_hits_where_the_parameters_say() {
        // Quadratic in u, linear in v which maps to y: a parabolic strip in the
        // z = 0 plane, so a vertical ray crosses it transversally.
        let control = vec![
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 2.0, 0.0, 1.0],
            [0.5, 1.0, 0.0, 1.0],
            [0.5, 3.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [1.0, 2.0, 0.0, 1.0],
        ];
        let surface = SplineSurface::new(
            2,
            1,
            vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            vec![0.0, 0.0, 1.0, 1.0],
            control,
        )
        .expect("quadratic surface");

        let ray = Ray::new([0.5, 0.5, 2.0], [0.0, 0.0, -1.0]);
        let mut hits = Vec::new();
        surface.intersect(&ray, &mut hits);
        let hit = hits
            .iter()
            .min_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap())
            .expect("hit");

        assert!(
            (hit.distance - 2.0).abs() < 1e-6,
            "distance {}",
            hit.distance
        );
        assert!((hit.position[0] - 0.5).abs() < 1e-6, "{:?}", hit.position);
        assert!((hit.position[1] - 0.5).abs() < 1e-6, "{:?}", hit.position);
        assert!((hit.uv[0] - 0.5).abs() < 1e-6, "u {}", hit.uv[0]);
        assert!(hit.uv[1].abs() < 1e-6, "v {}", hit.uv[1]);
    }

    #[test]
    fn a_ray_that_misses_the_patch_reports_nothing() {
        let surface = bilinear_square();
        let ray = Ray::new([3.0, 3.0, 2.0], [0.0, 0.0, -1.0]);
        let mut hits = Vec::new();
        surface.intersect(&ray, &mut hits);
        assert!(hits.is_empty());
    }
}
