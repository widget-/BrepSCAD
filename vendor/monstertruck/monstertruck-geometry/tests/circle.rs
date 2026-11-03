use monstertruck_geometry::prelude::*;
use proptest::prelude::*;
use std::f64::consts::{PI, TAU};

proptest! {
    #[test]
    fn search_parameter(t in 0f64..=(2.0 * PI)) {
        let circle = UnitCircle::<Point2>::new();
        let p = circle.subs(t);
        let s = circle.search_nearest_parameter(p, None, 1).unwrap();
        prop_assert_near!(s, t);
    }
    #[test]
    fn search_nearest_parameter(t in 0f64..=(2.0 * PI), a in 0.1f64..=5f64) {
        let circle = UnitCircle::<Point2>::new();
        let p = a * circle.subs(t);
        let s = circle.search_nearest_parameter(p, None, 1).unwrap();
        let q = a * circle.subs(s);
        prop_assert_near!(p, q);
    }

    /// With a `Parameter` hint outside the canonical `[0, TAU)` period, the
    /// search must return the parameter in the period nearest the hint rather
    /// than folding back into the first period.
    #[test]
    fn search_parameter_honors_hint(
        t in 0f64..=TAU,
        winding in -3i32..=3i32,
    ) {
        let circle = UnitCircle::<Point2>::new();
        let p = circle.evaluate(t);
        let hint = t + winding as f64 * TAU;
        let s = circle.search_parameter(p, hint, 1).unwrap();
        prop_assert_near2!(s, hint);
    }

    /// The nearest-parameter search honors the hint identically, and the
    /// returned parameter still evaluates back to the queried point.
    #[test]
    fn search_nearest_parameter_honors_hint(
        t in 0f64..=TAU,
        winding in -3i32..=3i32,
    ) {
        let circle = UnitCircle::<Point2>::new();
        let p = circle.evaluate(t);
        let hint = t + winding as f64 * TAU;
        let s = circle.search_nearest_parameter(p, hint, 1).unwrap();
        prop_assert_near2!(s, hint);
        prop_assert_near!(circle.evaluate(s), p);
    }

    #[test]
    fn to_nurbs(t0 in 0f64..=PI, t1 in PI..=(2.0 * PI)) {
        let circle = UnitCircle::<Point2>::new();
        let arc = TrimmedCurve::new(circle, (t0, t1));
        let bsp: NurbsCurve<_> = arc.to_same_geometry();
        prop_assert_near!(bsp.front(), arc.front());
        prop_assert_near!(bsp.back(), arc.back());
        for i in 0..=10 {
            let t = i as f64 / 10.0;
            let p = bsp.subs(t).to_vec();
            let der = bsp.der(t);
            prop_assert_near!(p.magnitude2(), 1.0);
            prop_assert!(der.dot(p).so_small());
        }
    }
}

#[test]
fn parameter_division() {
    let c = UnitCircle::<Point2>::new();
    let (_div, pts) = c.parameter_division(c.range_tuple(), 0.05);
    for a in pts.windows(2) {
        let p = a[0].midpoint(a[1]);
        assert!(p.to_vec().magnitude() > 0.95);
    }
}
