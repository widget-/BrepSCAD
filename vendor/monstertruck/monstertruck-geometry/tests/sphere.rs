use monstertruck_geometry::prelude::*;
use proptest::prelude::*;
use std::f64::consts::PI;

proptest! {
    #[test]
    fn test_der_mn(
        (u, v) in (0f64..=PI, 0f64..=2.0 * PI),
        (m, n) in (0usize..=4, 0usize..=4),
        center in prop::array::uniform3(-100f64..=100.0),
        radius in 0.1f64..=10.0,
        u_derivate in prop::bool::ANY,
    ) {
        let sphere = Sphere::new(Point3::from(center), radius);

        const EPS: f64 = 1.0e-4;
        let (der0, der1) = if u_derivate {
            let der0 = sphere.der_mn(m + 1, n, u, v);
            let der1 = (sphere.der_mn(m, n, u + EPS, v) - sphere.der_mn(m, n, u - EPS, v)) / (2.0 * EPS);
            (der0, der1)
        } else {
            let der0 = sphere.der_mn(m, n + 1, u, v);
            let der1 = (sphere.der_mn(m, n, u, v + EPS) - sphere.der_mn(m, n, u, v - EPS)) / (2.0 * EPS);
            (der0, der1)
        };
        prop_assert!((der0 - der1).magnitude() < 0.01 * der0.magnitude());
    }
}

fn exec_search_parameter_test(
    center: [f64; 3],
    radius: f64,
    (u, v): (f64, f64),
    disp: [f64; 3],
    sign: [bool; 3],
) -> std::result::Result<(), TestCaseError> {
    let center = Point3::from(center);
    let sphere = Sphere::new(center, radius);
    let pt = sphere.subs(u, v);
    let (u0, v0) = sphere.search_parameter(pt, None, 100).unwrap();
    prop_assert_near!(Vector2::new(u, v), Vector2::new(u0, v0));
    let boolnum = |t: bool| if t { 1.0 } else { -1.0 };
    let pt = pt
        + Vector3::new(
            disp[0] * boolnum(sign[0]),
            disp[1] * boolnum(sign[1]),
            disp[2] * boolnum(sign[2]),
        );
    prop_assert!(sphere.search_parameter(pt, None, 100).is_none());
    let (u, v) = sphere.search_nearest_parameter(pt, None, 100).unwrap();
    prop_assert_near!(
        sphere.subs(u, v),
        center + (pt - center).normalize() * radius
    );
    Ok(())
}

proptest! {
    #[test]
    fn search_parameter_test(
        center in prop::array::uniform3(-50f64..=50f64),
        radius in 0.1f64..100f64,
        (u, v) in (0f64..=PI, 0f64..=(2.0 * PI)),
        disp in prop::array::uniform3(0.01f64..0.1f64),
        sign in prop::array::uniform3(prop::bool::ANY),
    ) {
        exec_search_parameter_test(center, radius, (u, v), disp, sign)?;
    }

}

proptest! {
    /// `search_nearest_parameter` must return a finite `(u, v)` for any
    /// point on the sphere's axis-of-symmetry through `center`, including
    /// the exact poles. Without the `sinu == 0` guard, the algorithm
    /// computes `0 / 0` and propagates `NaN` through `clamp` and `acos`.
    ///
    /// Three regimes all hit the pole singularity:
    /// - `axis_displacement == radius`: point sits exactly at a pole on
    ///   the sphere surface.
    /// - `axis_displacement < radius` (non-zero): point is inside the
    ///   sphere on the axis; nearest sphere point is still a pole.
    /// - `axis_displacement > radius`: point is outside the sphere on
    ///   the axis; nearest sphere point is still a pole.
    ///
    /// `axis_displacement` is bounded away from zero because
    /// `point == center` is a *different* singularity, handled by its
    /// own guard and covered by `search_nearest_parameter_at_exact_center_is_finite`.
    #[test]
    fn search_nearest_parameter_on_axis_is_finite(
        center in prop::array::uniform3(-50f64..=50f64),
        radius in 0.1f64..50f64,
        axis_displacement in 0.01f64..100f64,
        positive_axis in prop::bool::ANY,
    ) {
        let center = Point3::from(center);
        let sphere = Sphere::new(center, radius);
        let direction = if positive_axis { 1.0 } else { -1.0 };
        let axis_point = center + Vector3::new(0.0, 0.0, direction * axis_displacement);
        let (u, v) = sphere
            .search_nearest_parameter(axis_point, None, 100)
            .expect("a point on the axis-of-symmetry must yield a (u, v).");
        prop_assert!(u.is_finite() && v.is_finite());
        let projected = sphere.evaluate(u, v);
        let expected = center + Vector3::new(0.0, 0.0, direction * radius);
        // The pole evaluation drifts by ~`sqrt(2 * ulp(1.0))` in `u`,
        // which becomes ~`radius * 1.5e-8` in 3D distance. Accept up to
        // the kernel's standard `TOLERANCE`.
        prop_assert_near!(projected, expected);
    }
}

#[test]
fn sphere_derivation_test() {
    let center = Point3::new(1.0, 2.0, 3.0);
    let radius = 4.56;
    let sphere = Sphere::new(center, radius);
    const N: usize = 100;
    for i in 0..N {
        for j in 0..N {
            let u = PI * i as f64 / N as f64;
            let v = 2.0 * PI * j as f64 / N as f64;
            let normal = sphere.normal(u, v);
            assert!(normal.dot(sphere.uder(u, v)).so_small());
            assert!(normal.dot(sphere.vder(u, v)).so_small());
        }
    }
}

#[test]
fn search_nearest_parameter_at_exact_north_pole_is_finite() {
    // At `u = 0` the sphere parameterisation has a coordinate singularity:
    // `sinu == 0` and `cosv = radius[0] / sinu` would be `0 / 0 = NaN`.
    // The pole guard picks `v = 0` arbitrarily so the result is finite.
    let center = Point3::new(0.5, 0.5, 0.5);
    let sphere = Sphere::new(center, 1.0);
    let north_pole = center + Vector3::unit_z();
    let (u, v) = sphere
        .search_nearest_parameter(north_pole, None, 100)
        .expect("north pole should map to a valid (u, v).");
    assert!(u.is_finite() && v.is_finite());
    assert!(sphere.evaluate(u, v).distance(north_pole) < 1.0e-9);
}

#[test]
fn search_nearest_parameter_at_exact_south_pole_is_finite() {
    // Same singularity at `u = π`. Symmetric to the north-pole case.
    let center = Point3::origin();
    let sphere = Sphere::new(center, 2.0);
    let south_pole = center + Vector3::new(0.0, 0.0, -2.0);
    let (u, v) = sphere
        .search_nearest_parameter(south_pole, None, 100)
        .expect("south pole should map to a valid (u, v).");
    assert!(u.is_finite() && v.is_finite());
    assert!(sphere.evaluate(u, v).distance(south_pole) < 1.0e-9);
}

#[test]
fn search_nearest_parameter_at_exact_center_is_finite() {
    // `point == center` is the second coordinate singularity (the first
    // being the poles): `radial_vector.normalize()` would divide by zero
    // and propagate `NaN`. Every (u, v) on the sphere is equidistant
    // from the center; the guard returns the arbitrary `(0, 0)`.
    let center = Point3::new(1.5, -2.5, 3.5);
    let sphere = Sphere::new(center, 0.7);
    let (u, v) = sphere
        .search_nearest_parameter(center, None, 100)
        .expect("the sphere's center must yield a (u, v).");
    assert!(u.is_finite() && v.is_finite());
    // The returned `(u, v)` must be a valid point on the sphere.
    assert_near!(sphere.evaluate(u, v).distance(center), 0.7);
}

#[test]
fn search_nearest_parameter_near_north_pole_is_finite() {
    // Catastrophic cancellation in `point - center` for a center far
    // from the origin produces a unit vector whose `z`-coordinate is
    // slightly above `1.0` after `normalize()`. Without the
    // `f64::clamp` guard the subsequent `acos` returns `NaN`.
    let center = Point3::new(1.0e6, 1.0e6, 1.0e6);
    let sphere = Sphere::new(center, 1.0);
    let near_north_pole = center + Vector3::new(1.0e-10, 1.0e-10, 1.0);
    let (u, v) = sphere
        .search_nearest_parameter(near_north_pole, None, 100)
        .expect("near-pole point should map to a valid (u, v).");
    assert!(u.is_finite() && v.is_finite());
}

/// **A coarse chord on a small sphere must not panic** -- spec 012 U1.2.
///
/// `Sphere::parameter_division` carried `assert!(tol < self.radius)`. While
/// STEP spheres reached the tessellator as rational nets, nothing dispatched
/// here and the assert was unreachable; U1.2 routes them onto this closed form
/// so that they stop costing 2,876,754 adaptive refinement cells over ROTOR's
/// ten sphere faces, and the assert became reachable from the DISPLAY path.
///
/// The reaching input is ordinary: a viewer picks its chord from the assembly's
/// bbox diagonal, so a 2 mm fillet ball inside a 5 m assembly is asked for a
/// chord several times its own radius. `ParameterDivision2D` returns
/// `(Vec<f64>, Vec<f64>)` and has no refusal channel, and its ~20 impls are
/// consumed by viewers, area/volume and mesh export, none of which has anywhere
/// to put one -- so a panic there is ledger C11, not a diagnostic.
///
/// Two claims, and the second is what makes the first safe:
///  1. a chord at or beyond the radius yields a usable division, and
///  2. every chord that already worked (`tol < radius`) is UNCHANGED.
#[test]
fn a_chord_coarser_than_the_radius_divides_instead_of_panicking() {
    let sphere = Sphere::new(Point3::new(1.0, -2.0, 3.0), 0.002);
    let range = ((0.0, PI), (0.0, 2.0 * PI));

    for tol in [0.002, 0.005, 5.0, 5000.0] {
        let (udiv, vdiv) = sphere.parameter_division(range, tol);
        assert!(
            udiv.len() >= 2 && vdiv.len() >= 2,
            "tol {tol} must still yield a division, got {}x{}",
            udiv.len(),
            vdiv.len(),
        );
        assert_eq!((udiv[0], *udiv.last().unwrap()), range.0);
        assert_eq!((vdiv[0], *vdiv.last().unwrap()), range.1);
        assert!(
            udiv.iter().chain(vdiv.iter()).all(|t| t.is_finite()),
            "tol {tol} produced a non-finite parameter",
        );
    }

    // The clamp is at ratio 1.0, so nothing that worked before moves. Verified
    // against the pre-clamp formula spelled out here rather than against a
    // frozen array, so this stays honest if the sphere's radius changes.
    let big = Sphere::new(Point3::origin(), 53.0);
    for tol in [1.0e-3, 1.0e-2, 1.0e-1, 1.0, 10.0] {
        let delta = 2.0 * f64::acos(1.0 - tol / big.radius());
        let expected_u = 1 + ((range.0.1 - range.0.0) / delta).floor() as usize;
        let expected_v = 1 + ((range.1.1 - range.1.0) / delta).floor() as usize;
        let (udiv, vdiv) = big.parameter_division(range, tol);
        assert_eq!(
            (udiv.len(), vdiv.len()),
            (expected_u + 1, expected_v + 1),
            "tol {tol} is below the clamp and must be byte-identical",
        );
    }
}
