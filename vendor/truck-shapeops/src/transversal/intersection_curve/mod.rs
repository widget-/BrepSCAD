use truck_base::cgmath64::*;
use truck_geometry::prelude::*;
use truck_meshalgo::prelude::*;

/// polyline base ntersection curve with parameter
#[derive(Debug, Clone, derive_more::Deref, derive_more::DerefMut)]
pub struct IntersectionCurveWithParameters<S0, S1> {
    #[deref]
    #[deref_mut]
    ic: IntersectionCurve<PolylineCurve<Point3>, S0, S1>,
    params0: PolylineCurve<Point2>,
    params1: PolylineCurve<Point2>,
}

impl<S0, S1> From<IntersectionCurveWithParameters<S0, S1>>
    for IntersectionCurve<PolylineCurve<Point3>, S0, S1>
{
    fn from(a: IntersectionCurveWithParameters<S0, S1>) -> Self { a.ic }
}

impl<S0, S1> IntersectionCurveWithParameters<S0, S1>
where
    S0: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
    S1: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
{
    pub fn try_new(surface0: S0, surface1: S1, poly: PolylineCurve<Point3>) -> Option<Self> {
        let ic = IntersectionCurve::new(&surface0, &surface1, poly);
        let poly = ic.leader();
        let len = poly.len();
        let mut polyline = PolylineCurve(Vec::new());
        let mut params0 = PolylineCurve(Vec::new());
        let mut params1 = PolylineCurve(Vec::new());
        for i in 0..len - 1 {
            let (q, p0, p1) = ic.search_triple(i as f64, 100)?;
            polyline.push(q);
            params0.push(p0);
            params1.push(p1);
        }
        let (q, p0, p1) = match poly[0].near(&poly[len - 1]) {
            true => (polyline[0], params0[0], params1[0]),
            false => ic.search_triple((len - 1) as f64, 100)?,
        };
        polyline.push(q);
        params0.push(p0);
        params1.push(p1);
        Some(Self {
            ic: IntersectionCurve::new(surface0, surface1, polyline),
            params0,
            params1,
        })
    }
}

impl<S0, S1> ParametricCurve for IntersectionCurveWithParameters<S0, S1>
where
    S0: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
    S1: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
{
    type Point = Point3;
    type Vector = Vector3;
    #[inline(always)]
    fn der_n(&self, n: usize, t: f64) -> Self::Vector { self.ic.der_n(n, t) }
    #[inline(always)]
    fn subs(&self, t: f64) -> Point3 { self.ic.subs(t) }
    #[inline(always)]
    fn der(&self, t: f64) -> Vector3 { self.ic.der(t) }
    #[inline(always)]
    fn der2(&self, t: f64) -> Vector3 { self.ic.der2(t) }
    #[inline(always)]
    fn parameter_range(&self) -> ParameterRange { self.ic.parameter_range() }
}

impl<S0, S1> BoundedCurve for IntersectionCurveWithParameters<S0, S1>
where
    S0: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
    S1: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
{
}

impl<S0, S1> ParameterDivision1D for IntersectionCurveWithParameters<S0, S1>
where
    S0: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
    S1: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
{
    type Point = Point3;
    #[inline(always)]
    fn parameter_division(&self, range: (f64, f64), tol: f64) -> (Vec<f64>, Vec<Point3>) {
        self.ic.parameter_division(range, tol)
    }
}

impl<S0, S1> Cut for IntersectionCurveWithParameters<S0, S1>
where
    S0: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
    S1: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
{
    #[inline(always)]
    fn cut(&mut self, t: f64) -> Self {
        Self {
            ic: self.ic.cut(t),
            params0: self.params0.cut(t),
            params1: self.params1.cut(t),
        }
    }
}

impl<S0: Clone, S1: Clone> Invertible for IntersectionCurveWithParameters<S0, S1> {
    fn invert(&mut self) {
        self.ic.invert();
        self.params0.invert();
        self.params1.invert();
    }
}

impl<S0, S1> SearchParameter<D1> for IntersectionCurveWithParameters<S0, S1>
where
    S0: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
    S1: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
{
    type Point = Point3;
    fn search_parameter<H: Into<SPHint1D>>(
        &self,
        point: Point3,
        hint: H,
        trials: usize,
    ) -> Option<f64> {
        self.ic.search_parameter(point, hint, trials)
    }
}

impl<S0, S1> SearchNearestParameter<D1> for IntersectionCurveWithParameters<S0, S1>
where
    S0: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
    S1: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
{
    type Point = Point3;
    fn search_nearest_parameter<H: Into<SPHint1D>>(
        &self,
        point: Point3,
        hint: H,
        trials: usize,
    ) -> Option<f64> {
        self.ic.search_nearest_parameter(point, hint, trials)
    }
}

type IntersectionTuple<S0, S1> = (
    PolylineCurve<Point3>,
    IntersectionCurveWithParameters<S0, S1>,
);
pub fn intersection_curves<S0, S1>(
    surface0: S0,
    polygon0: &PolygonMesh,
    surface1: S1,
    polygon1: &PolygonMesh,
) -> Option<Vec<IntersectionTuple<S0, S1>>>
where
    S0: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
    S1: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
{
    let interferences = polygon0.extract_interference(polygon1);
    let polylines = super::polyline_construction::construct_polylines(&interferences);
    polylines
        .into_iter()
        .map(|polyline| {
            Some((
                polyline.clone(),
                IntersectionCurveWithParameters::try_new(
                    surface0.clone(),
                    surface1.clone(),
                    polyline,
                )?,
            ))
        })
        .collect()
}

/// First vertex of a mesh's first triangle.
fn first_point(mesh: &PolygonMesh) -> Option<Point3> {
    let positions = mesh.positions();
    let face = mesh.tri_faces().first()?;
    Some(positions[face[0].pos])
}

/// Normal of a mesh's first triangle, if it has one.
fn first_normal(mesh: &PolygonMesh) -> Option<Vector3> {
    let positions = mesh.positions();
    let face = mesh.tri_faces().first()?;
    let a = positions[face[0].pos];
    let b = positions[face[1].pos];
    let c = positions[face[2].pos];
    let normal = (b - a).cross(c - a);
    (normal.magnitude() > 1.0e-12).then(|| normal.normalize())
}

/// True when every triangle of the mesh shares one normal, i.e. the face is flat.
fn planar(mesh: &PolygonMesh) -> bool {
    let Some(normal) = first_normal(mesh) else {
        return false;
    };
    let positions = mesh.positions();
    mesh.tri_faces().iter().all(|face| {
        let a = positions[face[0].pos];
        let b = positions[face[1].pos];
        let c = positions[face[2].pos];
        let other = (b - a).cross(c - a);
        other.magnitude() <= 1.0e-12 || other.normalize().cross(normal).magnitude() < 1.0e-6
    })
}

/// True when the two face polygons lie in the *same* plane.
///
/// Parallel normals alone are not enough, and neither is a coincident point: a
/// curved face can be tangent to a flat one, sharing both at the contact. Only
/// two genuinely flat faces that agree on plane are coplanar, and a coplanar cut
/// is meaningless for anything else.
pub(crate) fn coplanar(polygon0: &PolygonMesh, polygon1: &PolygonMesh) -> bool {
    if !planar(polygon0) || !planar(polygon1) {
        return false;
    }
    let (Some(normal0), Some(normal1)) = (first_normal(polygon0), first_normal(polygon1)) else {
        return false;
    };
    if normal0.cross(normal1).magnitude() >= 1.0e-6 {
        return false;
    }
    let (Some(point0), Some(point1)) = (first_point(polygon0), first_point(polygon1)) else {
        return false;
    };
    (point1 - point0).dot(normal0).abs() < 1.0e-6
}

/// Boundary segments of the overlap of two coplanar face polygons.
///
/// `extract_interference` only finds transversal triangle intersections, so two
/// coplanar faces that overlap produce no curve at all and the patch they share
/// is never cut out. Clipping the triangles pairwise — the intersection of two
/// convex triangles is convex — and cancelling the edges that adjacent pieces
/// share leaves the boundary of the shared region.
pub(crate) fn coplanar_overlap_segments(
    polygon0: &PolygonMesh,
    polygon1: &PolygonMesh,
) -> Vec<(Point3, Point3)> {
    let positions0 = polygon0.positions();
    let Some(face) = polygon0.tri_faces().first() else {
        return Vec::new();
    };
    let a = positions0[face[0].pos];
    let b = positions0[face[1].pos];
    let c = positions0[face[2].pos];
    let normal = (b - a).cross(c - a);
    if normal.magnitude() < 1.0e-12 {
        return Vec::new();
    }
    let normal = normal.normalize();
    let axis_u = (b - a).normalize();
    let axis_v = normal.cross(axis_u);

    let project = |point: Point3| -> Point2 {
        Point2::new((point - a).dot(axis_u), (point - a).dot(axis_v))
    };
    let triangles = |mesh: &PolygonMesh| -> Vec<[Point2; 3]> {
        let positions = mesh.positions();
        mesh.tri_faces()
            .iter()
            .map(|face| {
                [
                    project(positions[face[0].pos]),
                    project(positions[face[1].pos]),
                    project(positions[face[2].pos]),
                ]
            })
            .collect()
    };
    let triangles0 = triangles(polygon0);
    let triangles1 = triangles(polygon1);

    let mut edges: Vec<(Point2, Point2)> = Vec::new();
    for subject in &triangles0 {
        for clip in &triangles1 {
            let piece = clip_convex(subject, clip);
            if piece.len() < 3 {
                continue;
            }
            for index in 0..piece.len() {
                edges.push((piece[index], piece[(index + 1) % piece.len()]));
            }
        }
    }

    // The pieces partition the overlap, so an edge shared by two of them shows
    // up once in each direction and belongs to the interior.
    let scale = edges
        .iter()
        .flat_map(|(start, end)| [start.x, start.y, end.x, end.y])
        .fold(1.0_f64, |scale, value| scale.max(value.abs()));
    let tolerance = 1.0e-9 * scale;
    let near = |x: Point2, y: Point2| (x - y).magnitude() < tolerance;

    let mut used = vec![false; edges.len()];
    for index in 0..edges.len() {
        if used[index] {
            continue;
        }
        for other in index + 1..edges.len() {
            if used[other] {
                continue;
            }
            if near(edges[index].0, edges[other].1) && near(edges[index].1, edges[other].0) {
                used[index] = true;
                used[other] = true;
                break;
            }
        }
    }

    edges
        .iter()
        .zip(&used)
        .filter(|(_, used)| !**used)
        .map(|((start, end), _)| {
            (
                a + axis_u * start.x + axis_v * start.y,
                a + axis_u * end.x + axis_v * end.y,
            )
        })
        .collect()
}

/// Edges of a polygon mesh used by exactly one triangle, i.e. its boundary.
pub(crate) fn boundary_edges(mesh: &PolygonMesh) -> Vec<(Point3, Point3)> {
    let positions = mesh.positions();
    let key = |point: Point3| (point.x.to_bits(), point.y.to_bits(), point.z.to_bits());
    let mut edges: Vec<((u64, u64, u64), (u64, u64, u64), (Point3, Point3))> = Vec::new();
    for face in mesh.tri_faces() {
        let corners = [
            positions[face[0].pos],
            positions[face[1].pos],
            positions[face[2].pos],
        ];
        for index in 0..3 {
            let from = corners[index];
            let to = corners[(index + 1) % 3];
            let (from_key, to_key) = (key(from), key(to));
            edges.push((from_key.min(to_key), from_key.max(to_key), (from, to)));
        }
    }

    let mut boundary = Vec::new();
    for (index, edge) in edges.iter().enumerate() {
        let shared = edges.iter().enumerate().any(|(other, candidate)| {
            other != index && candidate.0 == edge.0 && candidate.1 == edge.1
        });
        if !shared {
            boundary.push(edge.2);
        }
    }
    boundary
}

/// True when the point lies on one of `edges`.
pub(crate) fn point_on_edges(point: Point3, edges: &[(Point3, Point3)], tolerance: f64) -> bool {
    edges.iter().any(|edge| point_on_edge(point, edge, tolerance))
}

fn point_on_edge(point: Point3, edge: &(Point3, Point3), tolerance: f64) -> bool {
    let direction = edge.1 - edge.0;
    let length = direction.magnitude();
    if length <= tolerance {
        return false;
    }
    let offset = point - edge.0;
    let along = offset.dot(direction) / length;
    let aside = (offset - direction * (along / length)).magnitude();
    aside < tolerance && along > -tolerance && along < length + tolerance
}

/// True when the segment runs along one of `edges`.
pub(crate) fn segment_on_edges(
    (start, end): (Point3, Point3),
    edges: &[(Point3, Point3)],
    tolerance: f64,
) -> bool {
    edges
        .iter()
        .any(|edge| point_on_edge(start, edge, tolerance) && point_on_edge(end, edge, tolerance))
}

/// Sutherland-Hodgman: clip `subject` by the convex polygon `clip`.
fn clip_convex(subject: &[Point2], clip: &[Point2]) -> Vec<Point2> {
    let mut clip = clip.to_vec();
    if signed_area(&clip) < 0.0 {
        clip.reverse();
    }
    let mut output = subject.to_vec();

    for index in 0..clip.len() {
        let start = clip[index];
        let end = clip[(index + 1) % clip.len()];
        let side = end - start;
        let inside = |point: Point2| cross2(side, point - start) >= 0.0;
        if output.is_empty() {
            return output;
        }
        let input = std::mem::take(&mut output);
        for position in 0..input.len() {
            let current = input[position];
            let previous = input[(position + input.len() - 1) % input.len()];
            match (inside(previous), inside(current)) {
                (false, true) => {
                    if let Some(point) = cross_point(previous, current, start, end) {
                        output.push(point);
                    }
                    output.push(current);
                }
                (true, true) => output.push(current),
                (true, false) => {
                    if let Some(point) = cross_point(previous, current, start, end) {
                        output.push(point);
                    }
                }
                (false, false) => {}
            }
        }
    }
    output
}

/// 2D cross product: the z component of the 3D cross.
fn cross2(a: Vector2, b: Vector2) -> f64 { a.x * b.y - a.y * b.x }

fn signed_area(points: &[Point2]) -> f64 {
    let mut area = 0.0;
    for index in 0..points.len() {
        let a = points[index];
        let b = points[(index + 1) % points.len()];
        area += a.x * b.y - b.x * a.y;
    }
    area / 2.0
}

/// Where segment `p0-p1` meets segment `q0-q1`.
fn cross_point(p0: Point2, p1: Point2, q0: Point2, q1: Point2) -> Option<Point2> {
    let r = p1 - p0;
    let s = q1 - q0;
    let denominator = cross2(r, s);
    if denominator.abs() < 1.0e-12 {
        return None;
    }
    let t = cross2(q0 - p0, s) / denominator;
    Some(p0 + r * t)
}

#[cfg(test)]
mod tests;
