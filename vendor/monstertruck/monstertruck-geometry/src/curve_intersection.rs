//! Exact intersections between curves, and between a straight segment and a
//! curve.
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

/// A point where two curves meet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveCurveHit {
    /// Parameter on the first curve.
    pub first_parameter: f64,
    /// Parameter on the second curve.
    pub second_parameter: f64,
    /// The meeting point, evaluated on the first curve.
    pub point: Point3,
}

/// Intersections of two curves.
///
/// Both curves are sampled only to seed candidate parameter pairs from the
/// closest approach of their chords; each candidate is then refined by
/// alternating projection on the curves themselves, so the result is not
/// limited by the sampling tolerance. A candidate is kept when the two curves
/// come within `tol`, which also covers tangencies and a shared endpoint.
pub fn curve_curve_intersections<C0, C1>(
    first: &C0,
    first_range: (f64, f64),
    second: &C1,
    second_range: (f64, f64),
    tol: f64,
) -> Vec<CurveCurveHit>
where
    C0: ParametricCurve<Point = Point3> + ParameterDivision1D<Point = Point3>,
    C1: ParametricCurve<Point = Point3> + ParameterDivision1D<Point = Point3>,
{
    let (first_params, first_points) = first.parameter_division(first_range, tol);
    let (second_params, second_points) = second.parameter_division(second_range, tol);
    if first_points.len() < 2 || second_points.len() < 2 {
        return Vec::new();
    }
    // Each polyline stays within `tol` of its curve, so a genuine intersection
    // brings the two chords within about `2 * tol`.
    let reach = 4.0 * tol;
    let mut seeds: Vec<(f64, f64)> = Vec::new();
    for i in 0..first_points.len() - 1 {
        for j in 0..second_points.len() - 1 {
            let Some((s, t, distance)) = closest_on_segments(
                first_points[i],
                first_points[i + 1],
                second_points[j],
                second_points[j + 1],
            ) else {
                continue;
            };
            if distance > reach {
                continue;
            }
            seeds.push((
                first_params[i] + (first_params[i + 1] - first_params[i]) * s,
                second_params[j] + (second_params[j + 1] - second_params[j]) * t,
            ));
        }
    }

    let mut hits: Vec<CurveCurveHit> = Vec::new();
    for (mut first_parameter, mut second_parameter) in seeds {
        let mut first_window = (first_range.1 - first_range.0) * 0.05;
        let mut second_window = (second_range.1 - second_range.0) * 0.05;
        for _ in 0..40 {
            let anchor = second.evaluate(second_parameter);
            if let Some(found) = minimize_window(
                (first_parameter - first_window).max(first_range.0),
                (first_parameter + first_window).min(first_range.1),
                |t| first.evaluate(t).distance(anchor),
            ) {
                first_parameter = found;
            }
            let anchor = first.evaluate(first_parameter);
            if let Some(found) = minimize_window(
                (second_parameter - second_window).max(second_range.0),
                (second_parameter + second_window).min(second_range.1),
                |t| second.evaluate(t).distance(anchor),
            ) {
                second_parameter = found;
            }
            first_window *= 0.6;
            second_window *= 0.6;
        }
        let point = first.evaluate(first_parameter);
        if point.distance(second.evaluate(second_parameter)) > tol {
            continue;
        }
        let duplicate = hits.iter().any(|hit| {
            hit.point.distance(point) <= tol
                || ((hit.first_parameter - first_parameter).abs() <= 1.0e-6
                    && (hit.second_parameter - second_parameter).abs() <= 1.0e-6)
        });
        if !duplicate {
            hits.push(CurveCurveHit {
                first_parameter,
                second_parameter,
                point,
            });
        }
    }
    hits.sort_by(|left, right| left.first_parameter.total_cmp(&right.first_parameter));
    hits
}

/// Golden-section minimum of `cost` on `[low, high]`.
fn minimize_window<F>(low: f64, high: f64, cost: F) -> Option<f64>
where F: Fn(f64) -> f64 {
    if !(low < high) {
        return None;
    }
    let ratio = 0.618_033_988_749_894_9;
    let mut a = low;
    let mut b = high;
    let mut c = b - (b - a) * ratio;
    let mut d = a + (b - a) * ratio;
    let (mut fc, mut fd) = (cost(c), cost(d));
    for _ in 0..80 {
        if fc <= fd {
            b = d;
            d = c;
            fd = fc;
            c = b - (b - a) * ratio;
            fc = cost(c);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + (b - a) * ratio;
            fd = cost(d);
        }
        if b - a <= 1.0e-15 {
            break;
        }
    }
    Some(0.5 * (a + b))
}

/// Closest approach between two segments, as `(s, t, distance)` with both
/// parameters clamped to `[0, 1]`.
fn closest_on_segments(
    first_from: Point3,
    first_to: Point3,
    second_from: Point3,
    second_to: Point3,
) -> Option<(f64, f64, f64)> {
    let along_first = first_to - first_from;
    let along_second = second_to - second_from;
    let offset = first_from - second_from;
    let (a, e) = (along_first.magnitude2(), along_second.magnitude2());
    let epsilon = 1.0e-24;
    if a <= epsilon && e <= epsilon {
        return None;
    }
    let f = along_second.dot(offset);
    let c = along_first.dot(offset);
    let (s, t) = if a <= epsilon {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else if e <= epsilon {
        ((-c / a).clamp(0.0, 1.0), 0.0)
    } else {
        let b = along_first.dot(along_second);
        let denominator = a * e - b * b;
        let s = match denominator.abs() > epsilon {
            true => ((b * f - c * e) / denominator).clamp(0.0, 1.0),
            false => 0.0,
        };
        let t = (b * s + f) / e;
        match t {
            t if t < 0.0 => (s.min(1.0), 0.0),
            t if t > 1.0 => (((b - c) / a).clamp(0.0, 1.0), 1.0),
            t => (s, t),
        }
    };
    let distance = (first_from + along_first * s).distance(second_from + along_second * t);
    Some((s, t, distance))
}
