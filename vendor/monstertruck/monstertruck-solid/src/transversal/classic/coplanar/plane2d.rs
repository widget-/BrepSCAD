//! The 2D picture of faces lying on one plane: a frame, outlines, and point tests.

use monstertruck_geometry::prelude::*;
use monstertruck_topology::*;

use super::PlaneOf;

pub(super) struct Frame {
    origin: Point3,
    u: Vector3,
    v: Vector3,
}

impl Frame {
    pub(super) fn new(plane: PlaneOf) -> Frame {
        let n = plane.normal;
        let helper = if n.x.abs() < 0.9 {
            Vector3::unit_x()
        } else {
            Vector3::unit_y()
        };
        let u = n.cross(helper).normalize();
        Frame {
            origin: plane.origin,
            u,
            v: n.cross(u),
        }
    }

    pub(super) fn flat(&self, p: Point3) -> Point2 {
        let d = p - self.origin;
        Point2::new(d.dot(self.u), d.dot(self.v))
    }

    pub(super) fn lift(&self, p: Point2) -> Point3 { self.origin + self.u * p.x + self.v * p.y }
}

pub(super) fn samples<C>(curve: &C, tol: f64) -> Vec<(f64, Point3)>
where C: BoundedCurve + ParameterDivision1D<Point = Point3> {
    let (params, points) = curve.parameter_division(curve.range_tuple(), tol);
    params.into_iter().zip(points).collect()
}

pub(super) fn cross2(a: Vector2, b: Vector2) -> f64 { a.x * b.y - a.y * b.x }

pub(super) fn segment_distance(p: Point2, a: Point2, b: Point2) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.magnitude2().max(1.0e-300)).clamp(0.0, 1.0);
    (a + ab * t).distance(p)
}

pub(super) fn crossing(a0: Point2, a1: Point2, b0: Point2, b1: Point2) -> Option<(f64, f64)> {
    let (da, db) = (a1 - a0, b1 - b0);
    let denom = cross2(da, db);
    if denom.abs() < 1.0e-14 * da.magnitude() * db.magnitude() {
        return None;
    }
    let w = b0 - a0;
    let s = cross2(w, db) / denom;
    let t = cross2(w, da) / denom;
    ((0.0..=1.0).contains(&s) && (0.0..=1.0).contains(&t)).then_some((s, t))
}

pub(super) fn polygon_area(points: &[Point2]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| cross2(points[i].to_vec(), points[(i + 1) % n].to_vec()))
        .sum::<f64>()
        / 2.0
}

pub(super) fn inside(polygons: &[Vec<Point2>], p: Point2) -> bool {
    let mut odd = false;
    for polygon in polygons {
        let n = polygon.len();
        for i in 0..n {
            let (a, b) = (polygon[i], polygon[(i + 1) % n]);
            if (a.y > p.y) != (b.y > p.y) {
                let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
                if x > p.x {
                    odd = !odd;
                }
            }
        }
    }
    odd
}

pub(super) fn on_outline(polygons: &[Vec<Point2>], p: Point2, tol: f64) -> bool {
    polygons.iter().any(|polygon| {
        let n = polygon.len();
        (0..n).any(|i| segment_distance(p, polygon[i], polygon[(i + 1) % n]) < tol)
    })
}

pub(super) fn oriented_points<C>(edge: &Edge<Point3, C>, tol: f64) -> Vec<Point3>
where C: Clone + BoundedCurve + ParameterDivision1D<Point = Point3> {
    let mut points: Vec<Point3> = samples(&edge.curve(), tol)
        .into_iter()
        .map(|(_, p)| p)
        .collect();
    if !edge.orientation() {
        points.reverse();
    }
    points
}

pub(super) fn outline<C, S>(
    face: &Face<Point3, C, S>,
    frame: &Frame,
    tol: f64,
) -> Vec<Vec<Point2>>
where
    C: Clone + BoundedCurve + ParameterDivision1D<Point = Point3>,
{
    face.boundaries()
        .iter()
        .map(|wire| {
            wire.edge_iter()
                .flat_map(|edge| {
                    let points = oriented_points(edge, tol);
                    let keep = points.len().saturating_sub(1);
                    points.into_iter().take(keep).collect::<Vec<_>>()
                })
                .map(|p| frame.flat(p))
                .collect()
        })
        .collect()
}

pub(super) fn interior_point(polygons: &[Vec<Point2>], tol: f64) -> Option<Point2> {
    // PATCH(BrepSCAD): the probe clearance has to be smaller than the piece.
    // A knife-edge sliver is thinner than `tol`, so no point in it is farther
    // than `tol` from its outline and an absolute clearance can never succeed.
    // `2 * area / perimeter` is roughly the piece's inradius, so scaling the
    // clearance to it keeps ordinary faces at the old `tol` scale while still
    // letting a sliver be probed.
    let area: f64 = polygons
        .iter()
        .map(|polygon| polygon_area(polygon).abs())
        .sum();
    let perimeter: f64 = polygons
        .iter()
        .flat_map(|polygon| {
            let n = polygon.len();
            (0..n).map(move |i| (polygon[(i + 1) % n] - polygon[i]).magnitude())
        })
        .sum();
    let inradius = if perimeter > 0.0 {
        2.0 * area / perimeter
    } else {
        0.0
    };
    let clearance = (tol * 0.25).min(inradius * 0.5).max(tol * 1.0e-4);
    let clear_of_outline = |point: Point2| !on_outline(polygons, point, clearance * 0.5);

    // The centroid is interior for a convex piece and is the point farthest
    // from the outline, so it is the most stable choice for the following
    // point-in-solid test. Try the largest polygon first.
    let centroid = |polygon: &[Point2]| -> Option<Point2> {
        let signed = polygon_area(polygon);
        if signed.abs() < 1.0e-18 {
            return None;
        }
        let n = polygon.len();
        let (mut x, mut y) = (0.0, 0.0);
        for i in 0..n {
            let (a, b) = (polygon[i], polygon[(i + 1) % n]);
            let cross = a.x * b.y - b.x * a.y;
            x += (a.x + b.x) * cross;
            y += (a.y + b.y) * cross;
        }
        Some(Point2::new(x / (6.0 * signed), y / (6.0 * signed)))
    };
    let mut order: Vec<usize> = (0..polygons.len()).collect();
    order.sort_by(|&a, &b| {
        polygon_area(&polygons[b])
            .abs()
            .total_cmp(&polygon_area(&polygons[a]).abs())
    });
    for index in order {
        if let Some(point) = centroid(&polygons[index])
            && inside(polygons, point)
            && clear_of_outline(point)
        {
            return Some(point);
        }
    }

    let mut candidates: Vec<(f64, Point2, Vector2)> = polygons
        .iter()
        .flat_map(|polygon| {
            let n = polygon.len();
            (0..n).map(move |i| {
                let (a, b) = (polygon[i], polygon[(i + 1) % n]);
                ((b - a).magnitude(), a + (b - a) * 0.5, b - a)
            })
        })
        .collect();
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    candidates
        .into_iter()
        .take(16)
        .find_map(|(length, middle, along)| {
            if length < clearance {
                return None;
            }
            let left = Vector2::new(-along.y, along.x).normalize();
            [1.0e-3, -1.0e-3, 1.0e-2, -1.0e-2, 1.0e-1, -1.0e-1]
                .into_iter()
                .map(|k: f64| middle + left * k.signum() * (length * k.abs()).max(clearance))
                .find(|p| inside(polygons, *p) && clear_of_outline(*p))
        })
}
