//! Exact curve-vs-segment intersection: the hit must land on the curve, not on
//! the sampling chord that bracketed it.

use monstertruck_geometry::prelude::*;

fn point(x: f64, y: f64) -> Point3 {
    Point3::new(x, y, 0.0)
}

fn bezier(control: Vec<Point3>) -> BsplineCurve<Point3> {
    let degree = control.len() - 1;
    BsplineCurve::new(KnotVector::bezier_knot(degree), control)
}

fn hits(curve: &BsplineCurve<Point3>, from: Point3, to: Point3, tol: f64) -> Vec<SegmentCurveHit> {
    segment_curve_intersections(curve, curve.range_tuple(), from, to, Vector3::unit_z(), tol)
}

#[test]
fn straight_curve_crosses_a_segment_at_its_midpoint() {
    let curve = bezier(vec![point(0.0, 0.0), point(10.0, 0.0)]);
    let found = hits(&curve, point(5.0, -5.0), point(5.0, 5.0), 0.01);
    assert_eq!(found.len(), 1);
    assert!((found[0].parameter - 0.5).abs() < 1.0e-9);
    assert!(found[0].point.distance(point(5.0, 0.0)) < 1.0e-9);
}

#[test]
fn curved_curve_crosses_once_at_its_vertex() {
    let curve = bezier(vec![point(0.0, 0.0), point(5.0, -4.0), point(10.0, 0.0)]);
    let found = hits(&curve, point(5.0, -5.0), point(5.0, 5.0), 0.01);
    assert_eq!(found.len(), 1);
    assert!(found[0].point.distance(point(5.0, -2.0)) < 1.0e-6);
}

#[test]
fn segment_that_misses_the_curve_reports_nothing() {
    let curve = bezier(vec![point(0.0, 0.0), point(10.0, 0.0)]);
    let found = hits(&curve, point(5.0, 1.0), point(5.0, 5.0), 0.01);
    assert!(found.is_empty());
}

#[test]
fn segment_that_ends_before_the_curve_reports_nothing() {
    let curve = bezier(vec![point(0.0, 0.0), point(10.0, 0.0)]);
    let found = hits(&curve, point(5.0, -5.0), point(5.0, -1.0), 0.01);
    assert!(found.is_empty());
}

#[test]
fn curve_endpoint_touching_the_segment_is_found() {
    let curve = bezier(vec![point(0.0, 0.0), point(10.0, 0.0)]);
    let found = hits(&curve, point(0.0, -5.0), point(0.0, 5.0), 0.01);
    assert_eq!(found.len(), 1);
    assert!(found[0].point.distance(point(0.0, 0.0)) < 1.0e-9);
}

#[test]
fn tangential_touch_is_found_without_a_sign_change() {
    let curve = bezier(vec![point(0.0, 0.0), point(5.0, -4.0), point(10.0, 0.0)]);
    let found = hits(&curve, point(0.0, -2.0), point(10.0, -2.0), 0.01);
    assert_eq!(found.len(), 1);
    assert!(found[0].point.distance(point(5.0, -2.0)) < 1.0e-6);
}

#[test]
fn hit_is_on_the_curve_not_on_the_sampling_chord() {
    // A coarse tolerance makes the chord cut the corner of this arc; the
    // reported point must still sit on the curve itself.
    let curve = bezier(vec![point(0.0, 0.0), point(5.0, -8.0), point(10.0, 0.0)]);
    let found = hits(&curve, point(2.5, -10.0), point(2.5, 10.0), 1.0);
    assert_eq!(found.len(), 1);
    let on_curve = curve.evaluate(found[0].parameter);
    assert!(found[0].point.distance(on_curve) < 1.0e-12);
    let chord_middle = curve.evaluate(0.0) + (curve.evaluate(1.0) - curve.evaluate(0.0)) * 0.5;
    assert!(found[0].point.distance(chord_middle) > 0.1);
}
