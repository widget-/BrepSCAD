//! Analytic ray/surface intersection.
//!
//! Only the surfaces this project's kernel produces are modelled: planes, and
//! the surfaces of revolution of a straight generator line (cylinders and
//! cones). Everything else in a Truck B-rep (B-splines, NURBS, hyperboloids)
//! has to be approximated or skipped by the backend adapter.

use std::f64::consts::TAU;

use crate::Ray;

/// One intersection of a ray with an untrimmed surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceHit {
    pub distance: f64,
    pub position: [f64; 3],
    pub normal: [f64; 3],
    /// Surface parameters, in the same space as the face's trim loops.
    pub uv: [f64; 2],
}

/// An analytic surface with a parameterisation matching its trim loops.
#[derive(Debug, Clone, PartialEq)]
pub enum Surface {
    /// An infinite plane. `u_axis` and `v_axis` are orthonormal, in the plane,
    /// and `u_axis × v_axis` is `normal`.
    Plane {
        origin: [f64; 3],
        normal: [f64; 3],
        u_axis: [f64; 3],
        v_axis: [f64; 3],
    },
    /// A surface of revolution of a line parallel to `axis`.
    ///
    /// `u` is the angle around `axis` measured from `u_reference`, and `v` is
    /// the height along `axis` measured from `origin`.
    Cylinder {
        origin: [f64; 3],
        axis: [f64; 3],
        radius: f64,
        u_reference: [f64; 3],
    },
    /// A surface of revolution of a line that meets `axis` at `apex`.
    ///
    /// `u` is the angle around `axis` measured from `u_reference`, and `v` is
    /// the height along `axis` measured from `apex`.
    Cone {
        apex: [f64; 3],
        axis: [f64; 3],
        half_angle_tan: f64,
        u_reference: [f64; 3],
    },
}

impl Surface {
    /// Appends every intersection with the ray, unsorted.
    pub(crate) fn intersect(&self, ray: &Ray, out: &mut Vec<SurfaceHit>) {
        match self {
            Surface::Plane {
                origin,
                normal,
                u_axis,
                v_axis,
            } => {
                let normal = normalize(*normal);
                let denom = dot(normal, ray.direction);
                if denom.abs() < 1.0e-12 {
                    return;
                }
                let t = dot(sub(*origin, ray.origin), normal) / denom;
                if t <= 0.0 {
                    return;
                }
                let position = along(ray.origin, ray.direction, t);
                let local = sub(position, *origin);
                out.push(SurfaceHit {
                    distance: t,
                    position,
                    normal,
                    uv: [dot(local, *u_axis), dot(local, *v_axis)],
                });
            }
            Surface::Cylinder {
                origin,
                axis,
                radius,
                u_reference,
            } => {
                let axis = normalize(*axis);
                let rest = sub(ray.origin, *origin);
                let radial_origin = sub(rest, scale(axis, dot(rest, axis)));
                let radial_direction = sub(ray.direction, scale(axis, dot(ray.direction, axis)));
                let a = dot(radial_direction, radial_direction);
                if a < 1.0e-12 {
                    return;
                }
                let b = 2.0 * dot(radial_origin, radial_direction);
                let c = dot(radial_origin, radial_origin) - radius * radius;
                for t in quadratic_roots(a, b, c) {
                    if t <= 0.0 {
                        continue;
                    }
                    let position = along(ray.origin, ray.direction, t);
                    let local = sub(position, *origin);
                    let height = dot(local, axis);
                    let radial = sub(local, scale(axis, height));
                    let length = norm(radial);
                    if length < 1.0e-12 {
                        continue;
                    }
                    out.push(SurfaceHit {
                        distance: t,
                        position,
                        normal: scale(radial, 1.0 / length),
                        uv: [revolved_u(axis, *u_reference, radial), height],
                    });
                }
            }
            Surface::Cone {
                apex,
                axis,
                half_angle_tan,
                u_reference,
            } => {
                let axis = normalize(*axis);
                let k = *half_angle_tan;
                let open = 1.0 + k * k;
                let rest = sub(ray.origin, *apex);
                let axial_origin = dot(rest, axis);
                let axial_direction = dot(ray.direction, axis);
                // |q|² - open (w·axis)² = 0 with w = rest + t·direction.
                let a = 1.0 - open * axial_direction * axial_direction;
                let b = 2.0 * (dot(rest, ray.direction) - open * axial_origin * axial_direction);
                let c = dot(rest, rest) - open * axial_origin * axial_origin;
                for t in quadratic_roots(a, b, c) {
                    if t <= 0.0 {
                        continue;
                    }
                    let position = along(ray.origin, ray.direction, t);
                    let local = sub(position, *apex);
                    let height = dot(local, axis);
                    let radial = sub(local, scale(axis, height));
                    // ∇(|w|² - open (w·axis)²) = 2 (w - open (w·axis) axis),
                    // which points away from the axis for a solid cone.
                    let normal = sub(radial, scale(axis, k * k * height));
                    let length = norm(normal);
                    if length < 1.0e-12 {
                        continue;
                    }
                    out.push(SurfaceHit {
                        distance: t,
                        position,
                        normal: scale(normal, 1.0 / length),
                        uv: [revolved_u(axis, *u_reference, radial), height],
                    });
                }
            }
        }
    }
}

fn quadratic_roots(a: f64, b: f64, c: f64) -> [f64; 2] {
    if a.abs() < 1.0e-12 {
        // Linear case: b·t + c = 0.
        return [-c / b, f64::NAN];
    }
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return [f64::NAN, f64::NAN];
    }
    let root = discriminant.sqrt();
    [(-b - root) / (2.0 * a), (-b + root) / (2.0 * a)]
}

/// Angle of `radial` around `axis`, measured from `u_reference` in `[0, TAU)`.
fn revolved_u(axis: [f64; 3], u_reference: [f64; 3], radial: [f64; 3]) -> f64 {
    let reference = normalize(sub(u_reference, scale(axis, dot(u_reference, axis))));
    let tangent = cross(axis, reference);
    let angle = dot(radial, tangent).atan2(dot(radial, reference));
    if angle < 0.0 { angle + TAU } else { angle }
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

fn scale(a: [f64; 3], factor: f64) -> [f64; 3] {
    [a[0] * factor, a[1] * factor, a[2] * factor]
}

fn along(origin: [f64; 3], direction: [f64; 3], t: f64) -> [f64; 3] {
    [
        origin[0] + direction[0] * t,
        origin[1] + direction[1] * t,
        origin[2] + direction[2] * t,
    ]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn normalize(a: [f64; 3]) -> [f64; 3] {
    let length = norm(a);
    if length > 0.0 {
        scale(a, 1.0 / length)
    } else {
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hits(surface: &Surface, ray: &Ray) -> Vec<SurfaceHit> {
        let mut out = Vec::new();
        surface.intersect(ray, &mut out);
        out.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap());
        out
    }

    #[test]
    fn plane_reports_uv_in_plane_coordinates() {
        let surface = Surface::Plane {
            origin: [0.0, 0.0, 1.0],
            normal: [0.0, 0.0, 1.0],
            u_axis: [1.0, 0.0, 0.0],
            v_axis: [0.0, 1.0, 0.0],
        };
        let ray = Ray::new([0.5, -0.25, 4.0], [0.0, 0.0, -2.0]);
        let hits = hits(&surface, &ray);
        assert_eq!(hits.len(), 1);
        assert!((hits[0].distance - 3.0).abs() < 1e-9);
        assert!((hits[0].uv[0] - 0.5).abs() < 1e-9);
        assert!((hits[0].uv[1] + 0.25).abs() < 1e-9);
        assert_eq!(hits[0].normal, [0.0, 0.0, 1.0]);
    }

    #[test]
    fn cylinder_hits_both_sides() {
        let surface = Surface::Cylinder {
            origin: [0.0, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: 1.0,
            u_reference: [1.0, 0.0, 0.0],
        };
        let ray = Ray::new([-5.0, 0.0, 0.5], [1.0, 0.0, 0.0]);
        let hits = hits(&surface, &ray);
        assert_eq!(hits.len(), 2, "a cylinder is hit entering and leaving");
        assert!((hits[0].distance - 4.0).abs() < 1e-9);
        assert!((hits[1].distance - 6.0).abs() < 1e-9);
        assert!((hits[0].normal[0] + 1.0).abs() < 1e-9);
        assert!((hits[1].normal[0] - 1.0).abs() < 1e-9);
        // u is measured from +X: the -X hit is at π, the +X hit at 0.
        assert!(
            (hits[0].uv[0] - std::f64::consts::PI).abs() < 1e-9,
            "{:?}",
            hits[0].uv
        );
        assert!(hits[1].uv[0] < 1e-9, "{:?}", hits[1].uv);
    }

    #[test]
    fn cylinder_uv_is_measured_from_the_reference_direction() {
        let surface = Surface::Cylinder {
            origin: [0.0, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: 2.0,
            u_reference: [0.0, 1.0, 0.0],
        };
        let ray = Ray::new([-5.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        let hit = hits(&surface, &ray)[0];
        // The reference direction is +Y, so the -X hit is at u = π/2.
        assert!(
            (hit.uv[0] - std::f64::consts::FRAC_PI_2).abs() < 1e-9,
            "{:?}",
            hit.uv
        );
        assert!((hit.uv[1] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn cone_hits_at_the_expected_radius() {
        let surface = Surface::Cone {
            apex: [0.0, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            half_angle_tan: 1.0,
            u_reference: [1.0, 0.0, 0.0],
        };
        // A 45° cone from the apex: at z = 2 the radius is 2.
        let ray = Ray::new([-5.0, 0.0, 2.0], [1.0, 0.0, 0.0]);
        let hits = hits(&surface, &ray);
        assert_eq!(hits.len(), 2);
        assert!(
            (hits[0].position[0] + 2.0).abs() < 1e-9,
            "{:?}",
            hits[0].position
        );
        assert!((hits[0].uv[1] - 2.0).abs() < 1e-9);
        // The outward normal tilts away from the axis by 45°.
        let n = hits[0].normal;
        assert!(
            (n[0] + std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9,
            "{n:?}"
        );
        assert!(
            (n[2] + std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9,
            "{n:?}"
        );
    }

    #[test]
    fn cone_normal_points_outward_on_both_sides_of_the_apex() {
        let surface = Surface::Cone {
            apex: [0.0, 0.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            half_angle_tan: 1.0,
            u_reference: [1.0, 0.0, 0.0],
        };
        let ray = Ray::new([-5.0, 0.0, -2.0], [1.0, 0.0, 0.0]);
        let hit = hits(&surface, &ray)[0];
        assert!(hit.normal[2] > 0.0, "lower sheet normal {0:?}", hit.normal);
    }
}
