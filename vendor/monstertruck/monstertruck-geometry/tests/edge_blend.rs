use monstertruck_geometry::prelude::*;
use prop::array::*;
use proptest::{prelude::*, property_test};

#[property_test]
fn test_edge_blend_ders_by_bspline_surface(
    #[strategy = uniform5(-1.0..=1.0)] z0: [f64; 5],
    #[strategy = uniform5(0.01..=1.0)] tangent0: [f64; 5],
    #[strategy = uniform5(-1.0..=1.0)] z1: [f64; 5],
    #[strategy = uniform5(0.01..=1.0)] tangent1: [f64; 5],
    #[strategy = uniform2(0.0..=1.0)] [u, v]: [f64; 2],
    #[strategy = uniform2(0usize..=3)] [m, n]: [usize; 2],
) {
    let control_points0 = (0..=4)
        .map(|i| {
            vec![
                Point3::new(-tangent0[i] / 4.0, 0.25 * i as f64, z0[i]),
                Point3::new(0.0, 0.25 * i as f64, z0[i]),
            ]
        })
        .collect();
    let surface0 = BsplineSurface::new(
        (KnotVector::bezier_knot(4), KnotVector::bezier_knot(1)),
        control_points0,
    );

    let control_points1 = (0..=4)
        .map(|i| {
            vec![
                Point3::new(1.0, 0.25 * i as f64, z1[i]),
                Point3::new(1.0 + tangent1[i] / 4.0, 0.25 * i as f64, z1[i]),
            ]
        })
        .collect();
    let surface1 = BsplineSurface::new(
        (KnotVector::bezier_knot(4), KnotVector::bezier_knot(1)),
        control_points1,
    );

    let pcurve0 = ParameterCurve::new(Line(Point2::new(1.0, 1.0), Point2::new(0.0, 1.0)), surface0);
    let pcurve1 = ParameterCurve::new(Line(Point2::new(1.0, 0.0), Point2::new(0.0, 0.0)), surface1);

    let tangent_controls0 = (0..=4).rev().map(|i| Vector1::new(tangent0[i])).collect();
    let tangent_curve0 = BsplineCurve::new(KnotVector::bezier_knot(4), tangent_controls0);
    let tangent_controls1 = (0..=4).rev().map(|i| Vector1::new(tangent1[i])).collect();
    let tangent_curve1 = BsplineCurve::new(KnotVector::bezier_knot(4), tangent_controls1);

    let surface = EdgeBlendSurface::new(pcurve0, tangent_curve0, pcurve1, tangent_curve1);

    let control_points = (0..=4)
        .rev()
        .map(|i| {
            vec![
                Point3::new(0.0, 0.25 * i as f64, z0[i]),
                Point3::new(tangent0[i] / 3.0, 0.25 * i as f64, z0[i]),
                Point3::new(1.0 - tangent1[i] / 3.0, 0.25 * i as f64, z1[i]),
                Point3::new(1.0, 0.25 * i as f64, z1[i]),
            ]
        })
        .collect();
    let bsp_surface = BsplineSurface::new(
        (KnotVector::bezier_knot(4), KnotVector::bezier_knot(3)),
        control_points,
    );
    prop_assert_near!(surface.der_mn(m, n, u, v), bsp_surface.der_mn(m, n, u, v));
}

#[property_test]
fn test_edge_blend_ends_by_bezier_surface(
    #[strategy = uniform4(uniform4(uniform3(-10.0..=10.0)))] control_points0: [[[f64; 3]; 4]; 4],
    #[strategy = uniform4(uniform4(uniform3(-10.0..=10.0)))] control_points1: [[[f64; 3]; 4]; 4],
    #[strategy = 0.0..=1.0] t: f64,
) {
    let control_points0 = control_points0
        .into_iter()
        .map(|p| p.into_iter().map(Point3::from).collect())
        .collect();
    let surface0 = BsplineSurface::new(
        (KnotVector::bezier_knot(3), KnotVector::bezier_knot(3)),
        control_points0,
    );

    let control_points1 = control_points1
        .into_iter()
        .map(|p| p.into_iter().map(Point3::from).collect())
        .collect();
    let surface1 = BsplineSurface::new(
        (KnotVector::bezier_knot(3), KnotVector::bezier_knot(3)),
        control_points1,
    );

    let line0 = Line(Point2::new(0.0, 1.0), Point2::new(1.0, 1.0));
    let line1 = Line(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0));

    let normal0 = surface0.uder(t, 1.0).cross(surface0.vder(t, 1.0));
    let normal1 = surface1.uder(t, 0.0).cross(surface1.vder(t, 0.0));
    let axis0 = surface0.uder(t, 1.0).cross(normal0);
    let axis1 = surface1.uder(t, 0.0).cross(normal1);

    prop_assume!(!normal0.so_small());
    prop_assume!(!normal1.so_small());
    prop_assume!(!axis0.so_small());
    prop_assume!(!axis1.so_small());

    let pcurve0 = ParameterCurve::new(line0, surface0);
    let pcurve1 = ParameterCurve::new(line1, surface1);
    let surface = EdgeBlendSurface::new(pcurve0.clone(), 0.6, pcurve1.clone(), 0.4);

    prop_assert_near!(surface.subs(t, 0.0), pcurve0.subs(t));
    prop_assert_near!(surface.uder(t, 0.0), pcurve0.der(t));
    prop_assert_near!(surface.uuder(t, 0.0), pcurve0.der2(t));
    prop_assert_near!(surface.subs(t, 1.0), pcurve1.subs(t));
    prop_assert_near!(surface.uder(t, 1.0), pcurve1.der(t));
    prop_assert_near!(surface.uuder(t, 1.0), pcurve1.der2(t));
    prop_assert_near!(surface.normal(t, 0.0), -normal0.normalize());
    prop_assert_near!(surface.normal(t, 1.0), -normal1.normalize());
}

// Deterministic G1 check: monstertruck addition (upstream has only the property
// tests above). Builds a concrete blend between two tilted planar patches that
// share the edge `x = 0`/`x = 1`, then samples cross-boundary tangent continuity
// along both rails. At `v = 0` the blend must be tangent-continuous (share the
// unit normal) with `surface0` along the `pcurve0` rail, and likewise with
// `surface1` at `v = 1` -- the defining property of a G1 edge blend.
#[test]
fn test_edge_blend_g1_continuity_along_rails() {
    // Planar patch tilted about the y-axis: normal has a nonzero x-component so
    // that G1 continuity is a nontrivial constraint on the blend.
    let surface0 = BsplineSurface::new(
        (KnotVector::bezier_knot(1), KnotVector::bezier_knot(1)),
        vec![
            vec![Point3::new(0.0, 0.0, 0.0), Point3::new(0.3, 0.0, 1.0)],
            vec![Point3::new(0.0, 1.0, 0.0), Point3::new(0.3, 1.0, 1.0)],
        ],
    );
    let surface1 = BsplineSurface::new(
        (KnotVector::bezier_knot(1), KnotVector::bezier_knot(1)),
        vec![
            vec![Point3::new(1.0, 0.0, 0.0), Point3::new(0.7, 0.0, 1.0)],
            vec![Point3::new(1.0, 1.0, 0.0), Point3::new(0.7, 1.0, 1.0)],
        ],
    );

    // Rails run along the shared edges (the `v = 0` boundary of each patch).
    let pcurve0 = ParameterCurve::new(Line(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)), surface0);
    let pcurve1 = ParameterCurve::new(Line(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)), surface1);
    let blend = EdgeBlendSurface::new(pcurve0.clone(), 0.5, pcurve1.clone(), 0.5);

    for i in 0..=8 {
        let u = i as f64 / 8.0;

        // Rail positions coincide with the supporting surfaces.
        assert_near!(blend.subs(u, 0.0), pcurve0.subs(u));
        assert_near!(blend.subs(u, 1.0), pcurve1.subs(u));

        // G1: the blend's unit normal is parallel to each supporting surface's
        // unit normal across the shared boundary, so the tangent planes agree.
        // Compare up to orientation by aligning the blend normal's sign.
        let sn0 = pcurve0.surface().normal(u, 0.0);
        let bn0 = blend.normal(u, 0.0);
        assert_near!(bn0 * f64::signum(bn0.dot(sn0)), sn0);
        let sn1 = pcurve1.surface().normal(u, 0.0);
        let bn1 = blend.normal(u, 1.0);
        assert_near!(bn1 * f64::signum(bn1.dot(sn1)), sn1);

        // The cross-boundary (u-direction) tangent is continuous with the rail
        // tangent of the supporting surface at the boundary.
        assert_near!(blend.uder(u, 0.0), pcurve0.der(u));
        assert_near!(blend.uder(u, 1.0), pcurve1.der(u));
    }
}
