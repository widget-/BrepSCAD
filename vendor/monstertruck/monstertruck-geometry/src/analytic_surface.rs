//! Analytic surface identity extracted from generic surface carriers.

use crate::prelude::*;

const EXTRUSION_IDENTITY_TOLERANCE: f64 = 1.0e-8;
const PLANE_IDENTITY_TOLERANCE: f64 = 1.0e-6;
const SPHERICAL_REVOLUTION_TOLERANCE: f64 = 1.0e-7;

/// Parameter axis used by an analytic surface identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceParameterAxis {
    /// The first surface parameter.
    U,
    /// The second surface parameter.
    V,
}

/// Homogeneous curve extruded along a constant 3D vector.
#[derive(Clone, Debug)]
pub struct HomogeneousExtrusionSurface {
    /// Homogeneous base curve of the extrusion.
    pub curve: BsplineCurve<Vector4>,
    /// Constant extrusion vector.
    pub vector: Vector3,
    /// Surface parameter axis carrying the base curve parameter.
    pub curve_axis: SurfaceParameterAxis,
    /// Surface parameter axis carrying the extrusion parameter.
    pub extrusion_axis: SurfaceParameterAxis,
    /// Finite parameter range of the base curve.
    pub curve_range: (f64, f64),
    /// Finite parameter range of the extrusion.
    pub extrusion_range: (f64, f64),
}

/// Spherical patch represented as a surface of revolution.
#[derive(Clone, Debug)]
pub struct SphericalRevolutionSurface {
    /// Center of the sphere.
    pub center: Point3,
    /// Radius of the sphere.
    pub radius: f64,
    /// Revolution axis.
    pub axis: Vector3,
    /// Radial direction at the zero revolution parameter.
    pub meridian_direction: Vector3,
    /// Profile parameter range.
    pub profile_range: (f64, f64),
    /// Revolution parameter range.
    pub revolution_range: (f64, f64),
}

/// Analytic surface kind recognized from a generic surface value.
#[derive(Clone, Debug)]
pub enum AnalyticSurfaceKind {
    /// Affine plane.
    Plane(Plane),
    /// Homogeneous curve extrusion.
    HomogeneousExtrusion(HomogeneousExtrusionSurface),
    /// Spherical patch represented by a surface of revolution.
    SphericalRevolution(SphericalRevolutionSurface),
}

/// Extracts an analytic surface identity when it is still available.
pub trait TryIntoAnalyticSurfaceKind {
    /// Returns the recognized analytic kind.
    fn try_into_analytic_surface_kind(&self) -> Option<AnalyticSurfaceKind>;
}

impl TryIntoAnalyticSurfaceKind for Plane {
    #[inline]
    fn try_into_analytic_surface_kind(&self) -> Option<AnalyticSurfaceKind> {
        Some(AnalyticSurfaceKind::Plane(*self))
    }
}

fn homogeneous_delta_vector(start: Vector4, end: Vector4) -> Option<Vector3> {
    if (end.w - start.w).abs() > EXTRUSION_IDENTITY_TOLERANCE || start.w.so_small() {
        None
    } else {
        Some(Vector3::new(
            (end.x - start.x) / start.w,
            (end.y - start.y) / start.w,
            (end.z - start.z) / start.w,
        ))
    }
}

fn all_vectors_match<'a>(
    pairs: impl IntoIterator<Item = (&'a Vector4, &'a Vector4)>,
    vector: Vector3,
) -> bool {
    pairs.into_iter().all(|(start, end)| {
        homogeneous_delta_vector(*start, *end)
            .is_some_and(|candidate| candidate.distance2(vector) <= EXTRUSION_IDENTITY_TOLERANCE)
    })
}

fn axis_v_homogeneous_extrusion(
    surface: &BsplineSurface<Vector4>,
) -> Option<HomogeneousExtrusionSurface> {
    let rows = surface.control_points();
    if surface.vdegree() != 1 || rows.iter().any(|row| row.len() != 2) {
        None
    } else {
        let first = rows.first()?;
        let vector = homogeneous_delta_vector(first[0], first[1])?;
        all_vectors_match(rows.iter().map(|row| (&row[0], &row[1])), vector).then(|| {
            let curve = BsplineCurve::new_unchecked(
                surface.knot_vector_u().clone(),
                rows.iter().map(|row| row[0]).collect(),
            );
            HomogeneousExtrusionSurface {
                curve,
                vector,
                curve_axis: SurfaceParameterAxis::U,
                extrusion_axis: SurfaceParameterAxis::V,
                curve_range: (
                    surface.knot_vector_u()[0],
                    surface.knot_vector_u()[surface.knot_vector_u().len() - 1],
                ),
                extrusion_range: (
                    surface.knot_vector_v()[0],
                    surface.knot_vector_v()[surface.knot_vector_v().len() - 1],
                ),
            }
        })
    }
}

fn axis_u_homogeneous_extrusion(
    surface: &BsplineSurface<Vector4>,
) -> Option<HomogeneousExtrusionSurface> {
    let rows = surface.control_points();
    let columns = rows.first()?.len();
    if surface.udegree() != 1 || rows.len() != 2 {
        None
    } else {
        let vector = homogeneous_delta_vector(rows[0][0], rows[1][0])?;
        let pairs = (0..columns).map(|column| (&rows[0][column], &rows[1][column]));
        all_vectors_match(pairs, vector).then(|| {
            let curve =
                BsplineCurve::new_unchecked(surface.knot_vector_v().clone(), rows[0].clone());
            HomogeneousExtrusionSurface {
                curve,
                vector,
                curve_axis: SurfaceParameterAxis::V,
                extrusion_axis: SurfaceParameterAxis::U,
                curve_range: (
                    surface.knot_vector_v()[0],
                    surface.knot_vector_v()[surface.knot_vector_v().len() - 1],
                ),
                extrusion_range: (
                    surface.knot_vector_u()[0],
                    surface.knot_vector_u()[surface.knot_vector_u().len() - 1],
                ),
            }
        })
    }
}

impl TryIntoAnalyticSurfaceKind for NurbsSurface<Vector4> {
    fn try_into_analytic_surface_kind(&self) -> Option<AnalyticSurfaceKind> {
        let surface = self.non_rationalized();
        planar_homogeneous_bspline_surface(surface)
            .map(AnalyticSurfaceKind::Plane)
            .or_else(|| {
                axis_v_homogeneous_extrusion(surface)
                    .or_else(|| axis_u_homogeneous_extrusion(surface))
                    .map(AnalyticSurfaceKind::HomogeneousExtrusion)
            })
    }
}

fn homogeneous_point(point: Vector4) -> Option<Point3> {
    (!point.w.so_small()).then_some(Point3::new(
        point.x / point.w,
        point.y / point.w,
        point.z / point.w,
    ))
}

fn spherical_revolution_from_profile(
    profile: BsplineCurve<Vector4>,
    origin: Point3,
    axis: Vector3,
    revolution_range: (f64, f64),
) -> Option<SphericalRevolutionSurface> {
    let axis = axis.normalize();
    let profile_range = profile.range_tuple();
    let samples = [0.0, 0.25, 0.5, 0.75, 1.0].map(|parameter| {
        homogeneous_point(
            profile.subs(profile_range.0 + (profile_range.1 - profile_range.0) * parameter),
        )
    });
    let samples = samples.into_iter().collect::<Option<Vec<_>>>()?;
    let reference = *samples.first()?;
    let reference_offset = reference - origin;
    let center_parameter = samples
        .iter()
        .skip(1)
        .filter_map(|point| {
            let offset = *point - origin;
            let denominator = 2.0 * axis.dot(offset - reference_offset);
            (denominator.abs() > SPHERICAL_REVOLUTION_TOLERANCE)
                .then(|| (offset.magnitude2() - reference_offset.magnitude2()) / denominator)
        })
        .next()?;
    let center = origin + axis * center_parameter;
    let radius = samples
        .iter()
        .map(|point| point.distance(center))
        .sum::<f64>()
        / samples.len() as f64;
    let radius_tolerance = SPHERICAL_REVOLUTION_TOLERANCE * radius.max(1.0);
    let meridian_vector = samples
        .iter()
        .map(|point| {
            let radial = *point - center;
            radial - axis * radial.dot(axis)
        })
        .max_by(|lhs, rhs| lhs.magnitude2().total_cmp(&rhs.magnitude2()))?;
    (radius > radius_tolerance
        && meridian_vector.magnitude() > radius_tolerance
        && samples
            .iter()
            .all(|point| (point.distance(center) - radius).abs() <= radius_tolerance))
    .then(|| SphericalRevolutionSurface {
        center,
        radius,
        axis,
        meridian_direction: meridian_vector.normalize(),
        profile_range,
        revolution_range,
    })
}

fn planar_bspline_surface(surface: &BsplineSurface<Point3>) -> Option<Plane> {
    let points = surface
        .control_points()
        .iter()
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    planar_points(&points)
}

fn planar_homogeneous_bspline_surface(surface: &BsplineSurface<Vector4>) -> Option<Plane> {
    let points = surface
        .control_points()
        .iter()
        .flatten()
        .copied()
        .map(homogeneous_point)
        .collect::<Option<Vec<_>>>()?;
    planar_points(&points)
}

fn planar_points(points: &[Point3]) -> Option<Plane> {
    let origin = points.first().copied()?;
    let one = points
        .iter()
        .copied()
        .find(|point| point.distance2(origin) > PLANE_IDENTITY_TOLERANCE.powi(2))?;
    let u_axis = one - origin;
    let another = points.iter().copied().find(|point| {
        u_axis.cross(*point - origin).magnitude2() > PLANE_IDENTITY_TOLERANCE.powi(2)
    })?;
    let plane = Plane::new(origin, one, another);
    points
        .iter()
        .copied()
        .all(|point| plane.parameter(point).z.abs() <= PLANE_IDENTITY_TOLERANCE)
        .then_some(plane)
}

impl TryIntoAnalyticSurfaceKind for BsplineSurface<Point3> {
    fn try_into_analytic_surface_kind(&self) -> Option<AnalyticSurfaceKind> {
        planar_bspline_surface(self).map(AnalyticSurfaceKind::Plane)
    }
}

impl TryIntoAnalyticSurfaceKind for Tmesh<Point3> {
    fn try_into_analytic_surface_kind(&self) -> Option<AnalyticSurfaceKind> { None }
}

/// `None`, deliberately -- see [`TryIntoAnalyticSurfaceKind for Torus`].
///
/// A sphere IS a [`SphericalRevolutionSurface`], and answering so here would
/// hand the symbolic SSI an exact sphere identity it does not get today. But
/// spec 012 U1.2 moved spheres from the rational-net variant to the analytic
/// one for their TESSELLATION, and while they were nets this call answered
/// `None` (`NurbsSurface::try_into_analytic_surface_kind` recognises only a
/// plane or a homogeneous extrusion, and a sphere net is neither). Returning
/// `Some` here would therefore be a boolean-engine change riding along on a
/// display-path fix. Recorded as follow-on work, not landed.
impl TryIntoAnalyticSurfaceKind for Sphere {
    #[inline]
    fn try_into_analytic_surface_kind(&self) -> Option<AnalyticSurfaceKind> { None }
}

/// `None`: no [`AnalyticSurfaceKind`] variant describes a torus, and its net
/// answered `None` too.
impl TryIntoAnalyticSurfaceKind for Torus {
    #[inline]
    fn try_into_analytic_surface_kind(&self) -> Option<AnalyticSurfaceKind> { None }
}

impl<C> TryIntoAnalyticSurfaceKind for RevolutionSurface<C>
where C: ParametricCurve3D + TryIntoHomogeneousBsplineCurve
{
    fn try_into_analytic_surface_kind(&self) -> Option<AnalyticSurfaceKind> {
        let profile = self.entity_curve().try_into_homogeneous_bspline_curve()?;
        spherical_revolution_from_profile(
            profile,
            self.origin(),
            self.axis(),
            finite_range(self.try_range_tuple().1)?,
        )
        .map(AnalyticSurfaceKind::SphericalRevolution)
    }
}

impl<C> TryIntoAnalyticSurfaceKind for Processor<C, Matrix4>
where C: TryIntoAnalyticSurfaceKind
{
    fn try_into_analytic_surface_kind(&self) -> Option<AnalyticSurfaceKind> {
        let kind = self.entity().try_into_analytic_surface_kind()?;
        transform_analytic_surface_kind(kind, *self.transform(), self.orientation())
    }
}

fn finite_range(range: Option<(f64, f64)>) -> Option<(f64, f64)> {
    let (start, end) = range?;
    (start.is_finite() && end.is_finite()).then_some((start, end))
}

fn transform_homogeneous_extrusion(
    mut extrusion: HomogeneousExtrusionSurface,
    transform: Matrix4,
) -> HomogeneousExtrusionSurface {
    extrusion
        .curve
        .control_points_mut()
        .for_each(|point| *point = transform * *point);
    extrusion.vector = transform.transform_vector(extrusion.vector);
    extrusion
}

fn transform_spherical_revolution(
    sphere: SphericalRevolutionSurface,
    transform: Matrix4,
) -> Option<SphericalRevolutionSurface> {
    let transverse = sphere.axis.cross(sphere.meridian_direction).normalize();
    let axis = transform.transform_vector(sphere.axis);
    let meridian = transform.transform_vector(sphere.meridian_direction);
    let transverse = transform.transform_vector(transverse);
    let meridian_scale = meridian.magnitude();
    let transverse_scale = transverse.magnitude();
    let axis_scale = axis.magnitude();
    let scale_tolerance = SPHERICAL_REVOLUTION_TOLERANCE * meridian_scale.max(1.0);
    (meridian_scale > SPHERICAL_REVOLUTION_TOLERANCE
        && axis_scale > SPHERICAL_REVOLUTION_TOLERANCE
        && (transverse_scale - meridian_scale).abs() <= scale_tolerance
        && axis.dot(meridian).abs() <= scale_tolerance
        && axis.dot(transverse).abs() <= scale_tolerance
        && meridian.dot(transverse).abs() <= scale_tolerance)
        .then(|| SphericalRevolutionSurface {
            center: transform.transform_point(sphere.center),
            radius: sphere.radius * meridian_scale,
            axis: axis.normalize(),
            meridian_direction: meridian.normalize(),
            profile_range: sphere.profile_range,
            revolution_range: sphere.revolution_range,
        })
}

fn transform_analytic_surface_kind(
    kind: AnalyticSurfaceKind,
    transform: Matrix4,
    orientation: bool,
) -> Option<AnalyticSurfaceKind> {
    match kind {
        AnalyticSurfaceKind::Plane(plane) => {
            Some(AnalyticSurfaceKind::Plane(plane.transformed(transform)))
        }
        AnalyticSurfaceKind::HomogeneousExtrusion(extrusion) => orientation.then(|| {
            AnalyticSurfaceKind::HomogeneousExtrusion(transform_homogeneous_extrusion(
                extrusion, transform,
            ))
        }),
        AnalyticSurfaceKind::SphericalRevolution(sphere) => orientation
            .then(|| transform_spherical_revolution(sphere, transform))
            .flatten()
            .map(AnalyticSurfaceKind::SphericalRevolution),
    }
}
