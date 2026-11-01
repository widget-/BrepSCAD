//! Build a neutral ray-casting scene from a Truck solid.
//!
//! Only surfaces the ray caster models are exported: planes and the surfaces of
//! revolution of a straight line (cylinders and cones). Faces with any other
//! surface are dropped, so a caller that needs full coverage has to fall back to
//! the tessellated mesh.

use std::f64::consts::TAU;

use brepscad_raycast::{Face as RaycastFace, Scene, Surface as RaycastSurface, TrimRegion};
use truck_meshalgo::prelude::*;
use truck_modeling::*;

/// Sampling tolerance for trim polylines, in model units.
const TRIM_TOLERANCE: f64 = 1.0e-3;
/// Distance below which a generator line is treated as meeting the axis.
const CONE_AXIS_TOLERANCE: f64 = 1.0e-6;

/// Surface parameters of a point, matching the exported [`RaycastSurface`].
#[derive(Debug, Clone, Copy)]
enum Parameterization {
    Plane {
        origin: Point3,
        u_axis: Vector3,
        v_axis: Vector3,
    },
    Revolved {
        origin: Point3,
        axis: Vector3,
        u_reference: Vector3,
    },
}

impl Parameterization {
    fn map(&self, point: Point3) -> [f64; 2] {
        match self {
            Self::Plane {
                origin,
                u_axis,
                v_axis,
            } => {
                let local = point - origin;
                [local.dot(*u_axis), local.dot(*v_axis)]
            }
            Self::Revolved {
                origin,
                axis,
                u_reference,
            } => {
                let local = point - origin;
                let height = local.dot(*axis);
                let radial = local - height * *axis;
                [revolved_angle(*axis, *u_reference, radial), height]
            }
        }
    }

    fn periodic(&self) -> bool {
        matches!(self, Self::Revolved { .. })
    }
}

pub(crate) fn scene_from_solid(solid: &Solid, tolerance: f64) -> Scene {
    let mut faces = Vec::new();
    for face in solid.face_iter() {
        if let Some(analytic) = analytic_face(face) {
            faces.push(analytic);
        } else if let Some(triangles) = tessellated_face(face, tolerance) {
            // The surface is a B-spline or NURBS patch; cover it with its
            // tessellation so the scene still represents the whole solid.
            faces.push(RaycastFace::from_triangles(triangles).with_flip(!face.orientation()));
        }
    }
    Scene::new(faces)
}

fn tessellated_face(face: &Face, tolerance: f64) -> Option<Vec<[[f64; 3]; 3]>> {
    let mesh = crate::tessellate_face(face, tolerance);
    let positions = mesh.positions();
    let mut triangles = Vec::new();
    for polygon in mesh.faces().face_iter() {
        if polygon.len() < 3 {
            continue;
        }
        let corner = |vertex: &StandardVertex| {
            let point = positions[vertex.pos];
            [point.x, point.y, point.z]
        };
        for index in 1..polygon.len() - 1 {
            triangles.push([
                corner(&polygon[0]),
                corner(&polygon[index]),
                corner(&polygon[index + 1]),
            ]);
        }
    }
    (!triangles.is_empty()).then_some(triangles)
}

fn analytic_face(face: &Face) -> Option<RaycastFace> {
    let (surface, parameters) = classify(&face.surface())?;

    let mut loops = Vec::new();
    for wire in face.absolute_boundaries() {
        if let Some(polygon) = wire_loop(wire, &parameters) {
            loops.push(polygon);
        }
    }
    if loops.is_empty() {
        return None;
    }

    let trims = if parameters.periodic() {
        TrimRegion::with_u_period(loops, TAU)
    } else {
        TrimRegion::new(loops)
    };

    Some(RaycastFace::new(surface, trims, face_bounds(face)?).with_flip(!face.orientation()))
}

fn classify(surface: &Surface) -> Option<(RaycastSurface, Parameterization)> {
    match surface {
        Surface::Plane(plane) => {
            let origin = plane.origin();
            let u_axis = plane.u_axis();
            let v_axis = plane.v_axis();
            Some((
                RaycastSurface::Plane {
                    origin: position(origin),
                    normal: vector(plane.normal()),
                    u_axis: vector(u_axis),
                    v_axis: vector(v_axis),
                },
                Parameterization::Plane {
                    origin,
                    u_axis,
                    v_axis,
                },
            ))
        }
        Surface::RevolutedCurve(processor) => {
            let revolved = processor.entity();
            let Curve::Line(line) = revolved.entity_curve() else {
                return None;
            };
            let matrix = *processor.transform();
            let origin = matrix.transform_point(revolved.origin());
            let axis = matrix.transform_vector(revolved.axis()).normalize();
            let start = matrix.transform_point(line.front());
            let end = matrix.transform_point(line.back());
            let direction = (end - start).normalize();

            let local = start - origin;
            let height = local.dot(axis);
            let radial = local - height * axis;
            let radius = radial.magnitude();
            let u_reference = if radius > CONE_AXIS_TOLERANCE {
                radial / radius
            } else {
                perpendicular(axis)
            };

            if direction.dot(axis).abs() > 1.0 - 1.0e-9 {
                // The generator runs parallel to the axis: a cylinder.
                return Some((
                    RaycastSurface::Cylinder {
                        origin: position(origin),
                        axis: vector(axis),
                        radius,
                        u_reference: vector(u_reference),
                    },
                    Parameterization::Revolved {
                        origin,
                        axis,
                        u_reference,
                    },
                ));
            }

            // Otherwise the generator has to meet the axis for the surface to be
            // a cone; a skew pair is a hyperboloid, which is not modelled.
            let (line_parameter, axis_parameter) =
                closest_approach_parameters(origin, axis, start, direction)?;
            let on_generator = start + direction * line_parameter;
            let on_axis = origin + axis * axis_parameter;
            if (on_generator - on_axis).magnitude() > CONE_AXIS_TOLERANCE {
                return None;
            }
            let apex = (on_generator + on_axis.to_vec()) / 2.0;
            let cosine = direction.dot(axis).abs();
            let half_angle_tan = (1.0 - cosine * cosine).sqrt() / cosine;
            if !half_angle_tan.is_finite() {
                return None;
            }
            let local = start - apex;
            let height = local.dot(axis);
            let radial = local - height * axis;
            let u_reference = if radial.magnitude() > CONE_AXIS_TOLERANCE {
                radial.normalize()
            } else {
                perpendicular(axis)
            };

            Some((
                RaycastSurface::Cone {
                    apex: position(apex),
                    axis: vector(axis),
                    half_angle_tan,
                    u_reference: vector(u_reference),
                },
                Parameterization::Revolved {
                    origin: apex,
                    axis,
                    u_reference,
                },
            ))
        }
        _ => None,
    }
}

fn wire_loop(wire: &Wire, parameters: &Parameterization) -> Option<Vec<[f64; 2]>> {
    let mut polygon: Vec<[f64; 2]> = Vec::new();
    let mut previous_angle: Option<f64> = None;

    for edge in wire.iter() {
        let curve = edge.oriented_curve();
        let range = curve.range_tuple();
        let polyline = PolylineCurve::from_curve(&curve, range, TRIM_TOLERANCE);
        for point in polyline.iter() {
            let mut uv = parameters.map(*point);
            if parameters.periodic() {
                // Keep a loop that crosses the seam continuous.
                uv[0] = match previous_angle {
                    Some(previous) => unwrap_angle(uv[0], previous),
                    None => uv[0],
                };
                previous_angle = Some(uv[0]);
            }
            push_distinct(&mut polygon, uv);
        }
    }

    if polygon.len() >= 3 {
        let first = polygon[0];
        let last = polygon[polygon.len() - 1];
        if (first[0] - last[0]).abs() < 1.0e-9 && (first[1] - last[1]).abs() < 1.0e-9 {
            polygon.pop();
        }
    }
    (polygon.len() >= 3).then_some(polygon)
}

fn push_distinct(polygon: &mut Vec<[f64; 2]>, uv: [f64; 2]) {
    if let Some(last) = polygon.last() {
        if (last[0] - uv[0]).abs() < 1.0e-9 && (last[1] - uv[1]).abs() < 1.0e-9 {
            return;
        }
    }
    polygon.push(uv);
}

/// Shifts `angle` by whole turns so it stays within half a turn of `previous`.
fn unwrap_angle(angle: f64, previous: f64) -> f64 {
    let mut angle = angle;
    while angle - previous > std::f64::consts::PI {
        angle -= TAU;
    }
    while previous - angle > std::f64::consts::PI {
        angle += TAU;
    }
    angle
}

fn face_bounds(face: &Face) -> Option<([f64; 3], [f64; 3])> {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    let mut found = false;
    for vertex in face.vertex_iter() {
        let point = vertex.point();
        found = true;
        for (axis, value) in [point.x, point.y, point.z].into_iter().enumerate() {
            min[axis] = min[axis].min(value);
            max[axis] = max[axis].max(value);
        }
    }
    found.then_some((min, max))
}

/// Parameters of the closest points on an axis line and a generator line.
fn closest_approach_parameters(
    axis_origin: Point3,
    axis: Vector3,
    line_origin: Point3,
    line: Vector3,
) -> Option<(f64, f64)> {
    let offset = line_origin - axis_origin;
    let b = line.dot(axis);
    let d = line.dot(offset);
    let e = axis.dot(offset);
    let denominator = 1.0 - b * b;
    if denominator.abs() < 1.0e-12 {
        return None;
    }
    let line_parameter = (b * e - d) / denominator;
    let axis_parameter = (e - b * d) / denominator;
    Some((line_parameter, axis_parameter))
}

fn revolved_angle(axis: Vector3, u_reference: Vector3, radial: Vector3) -> f64 {
    let reference = (u_reference - axis * u_reference.dot(axis)).normalize();
    let tangent = axis.cross(reference);
    let angle = radial.dot(tangent).atan2(radial.dot(reference));
    if angle < 0.0 { angle + TAU } else { angle }
}

fn perpendicular(axis: Vector3) -> Vector3 {
    let candidate = if axis.x.abs() < 0.9 {
        Vector3::unit_x()
    } else {
        Vector3::unit_y()
    };
    (candidate - axis * candidate.dot(axis)).normalize()
}

fn position(point: Point3) -> [f64; 3] {
    [point.x, point.y, point.z]
}

fn vector(vector: Vector3) -> [f64; 3] {
    [vector.x, vector.y, vector.z]
}
