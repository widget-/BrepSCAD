//! Exact intersections between a straight segment and a curve.
//!
//! PATCH(BrepSCAD): the planar boolean fallback used to take its cut endpoints
//! off the chord polylines that [`ParameterDivision1D`] produces, then compare
//! them against the curve with the same tolerance the polyline was allowed to
//! deviate by. Two chord approximations feed that comparison (the
//! cross-section's own polyline and the face outline's), so borderline points
//! failed their own acceptance test and the divided face kept a wire gap.
//!
//! The routines here sample the curve only to bracket a root and then refine on
//! the curve itself, so a returned point lies on the curve rather than on a
//! chord, and the caller no longer needs a tolerance wide enough to absorb the
//! sampling error.

use crate::prelude::*;
use monstertruck_core::cgmath64::{Point3, Vector3};

/// A point where a straight segment meets a curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentCurveHit {
    /// Curve parameter at the intersection.
    pub parameter: f64,
    /// Intersection point, evaluated on the curve.
    pub point: Point3,
}

/// Intersections of the segment `from..to` with `curve`, both lying in the
/// plane whose normal is `plane_normal`.
///
/// The curve is sampled only to bracket sign changes of the in-plane distance
/// to the segment's line; each bracket is bisected on the curve, and a window
/// where the distance dips without crossing (a tangency) is refined by a
/// ternary search on that distance. Hits that fall outside the segment's own
/// extent are discarded, and the result is sorted by curve parameter.
pub fn segment_curve_intersections<C>(
    curve: &C,
    range: (f64, f64),
    from: Point3,
    to: Point3,
    plane_normal: Vector3,
    tol: f64,
) -> Vec<SegmentCurveHit>
where
    C: ParametricCurve<Point = Point3> + ParameterDivision1D<Point = Point3>,
{
    let along = to - from;
    let length2 = along.magnitude2();
    if length2 <= tol * tol {
        return Vec::new();
    }
    let side = plane_normal.cross(along);
    if side.magnitude2() <= 1.0e-24 {
        // The segment runs along the plane normal, so it has no in-plane line.
        return Vec::new();
    }
    let side = side.normalize();
    let offset = |t: f64| (curve.evaluate(t) - from).dot(side);

    let (parameters, _) = curve.parameter_division(range, tol);
    let mut hits: Vec<SegmentCurveHit> = Vec::new();
    for window in parameters.windows(2) {
        let (mut t0, mut t1) = (window[0], window[1]);
        let mut f0 = offset(t0);
        let f1 = offset(t1);
        if f0.abs() <= tol {
            accept(curve, from, along, length2, tol, t0, &mut hits);
        }
        if f0 * f1 < 0.0 {
            for _ in 0..80 {
                let middle = 0.5 * (t0 + t1);
                let fm = offset(middle);
                if (f0 < 0.0) == (fm < 0.0) {
                    t0 = middle;
                    f0 = fm;
                } else {
                    t1 = middle;
                }
                if t1 - t0 <= 1.0e-15 {
                    break;
                }
            }
            accept(curve, from, along, length2, tol, 0.5 * (t0 + t1), &mut hits);
        }
    }
    // A tangency dips through zero without changing sign, so it never brackets.
    // Refine every local minimum of the distance instead of guessing how close
    // a sample has to come, and keep the minimum when it reaches the segment.
    let distances: Vec<f64> = parameters.iter().map(|&t| offset(t).abs()).collect();
    for index in 1..distances.len().saturating_sub(1) {
        if distances[index] > distances[index - 1] || distances[index] > distances[index + 1] {
            continue;
        }
        let (mut low, mut high) = (parameters[index - 1], parameters[index + 1]);
        for _ in 0..80 {
            let a = low + (high - low) / 3.0;
            let b = high - (high - low) / 3.0;
            if offset(a).abs() <= offset(b).abs() {
                high = b;
            } else {
                low = a;
            }
        }
        let t = 0.5 * (low + high);
        if offset(t).abs() <= tol {
            accept(curve, from, along, length2, tol, t, &mut hits);
        }
    }
    if let Some(&last) = parameters.last()
        && offset(last).abs() <= tol
    {
        accept(curve, from, along, length2, tol, last, &mut hits);
    }
    hits.sort_by(|left, right| left.parameter.total_cmp(&right.parameter));
    hits
}

/// Keeps `t` when the curve point there lies on the segment and no hit at that
/// parameter is already recorded.
fn accept<C>(
    curve: &C,
    from: Point3,
    along: Vector3,
    length2: f64,
    tol: f64,
    t: f64,
    hits: &mut Vec<SegmentCurveHit>,
) where
    C: ParametricCurve<Point = Point3>,
{
    let point = curve.evaluate(t);
    let along_segment = (point - from).dot(along) / length2;
    // A caller looking for the crossing at one of the segment's own ends gets a
    // hit that lands just outside `[0, 1]`, so allow a `tol`-sized overshoot
    // along the segment's direction.
    let slack = tol / length2.sqrt().max(f64::MIN_POSITIVE);
    if !(-slack..=1.0 + slack).contains(&along_segment) {
        return;
    }
    if (from + along * along_segment.clamp(0.0, 1.0)).distance(point) > tol {
        return;
    }
    // The same tangency can be reported by the bracketing pass and by the
    // local-minimum pass at parameters that differ by more than rounding, so
    // compare locations as well.
    if hits
        .iter()
        .any(|hit| (hit.parameter - t).abs() <= 1.0e-6 || hit.point.distance(point) <= tol)
    {
        return;
    }
    hits.push(SegmentCurveHit { parameter: t, point });
}

