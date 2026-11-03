//! Cached presearch samples must select exactly the same hint as a live scan.

use monstertruck_geometry::base::algo::curve::{presearch, presearch_samples};
use monstertruck_geometry::prelude::*;

#[test]
fn cached_presearch_hint_matches_live_scan() {
    let curve = BsplineCurve::new(
        KnotVector::bezier_knot(3),
        vec![
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 2.0, 0.0),
            Vector3::new(3.0, 2.0, 0.5),
            Vector3::new(4.0, 0.0, 1.0),
        ],
    );
    let range = curve.range_tuple();
    let samples = presearch_samples(&curve, range, 50);

    for point in [
        Vector3::new(0.5, 0.5, 0.5),
        Vector3::new(2.0, 1.0, 0.0),
        Vector3::new(3.5, 2.0, 0.25),
    ] {
        assert_eq!(samples.hint(point), presearch(&curve, point, range, 50));
    }
}
