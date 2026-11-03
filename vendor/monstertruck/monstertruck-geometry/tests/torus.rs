use monstertruck_geometry::prelude::*;
use proptest::prelude::*;
use std::f64::consts::PI;
use std::ops::RangeBounds;

proptest! {
    #[test]
    fn surface(
        small_radius in 1f64..=5f64,
        radius_diff in 0.1f64..=5f64,
        (u, v) in (0f64..=2.0 * PI, 0f64..=2.0 * PI),
        deform in -0.4f64..=0.4
    ) {
        const EPS: f64 = 1.0e-2;
        let large_radius = 2.0 * small_radius + radius_diff;
        let torus = Torus::new(Point3::origin(), large_radius, small_radius);

        let p = torus.subs(u, v);
        let r = large_radius * Point3::new(f64::cos(u), f64::sin(u), 0.0);
        prop_assert_near!(p.distance(r), small_radius);
        prop_assert_near!(p.z, small_radius * f64::sin(v));

        let uder0 = torus.uder(u, v);
        let uder1 = (torus.subs(u + EPS, v) - torus.subs(u - EPS, v)) / (2.0 * EPS);
        prop_assert!((uder0 - uder1).magnitude() < EPS, "{:?} {:?}", uder0, uder1);

        let vder0 = torus.vder(u, v);
        let vder1 = (torus.subs(u, v + EPS) - torus.subs(u, v - EPS)) / (2.0 * EPS);
        prop_assert!((vder0 - vder1).magnitude() < EPS, "{:?} {:?}", vder0, vder1);

        let uuder0 = torus.uuder(u, v);
        let uuder1 = (torus.uder(u + EPS, v) - torus.uder(u - EPS, v)) / (2.0 * EPS);
        prop_assert!(
            (uuder0 - uuder1).magnitude() < EPS,
            "{:?} {:?}",
            uuder0,
            uuder1
        );

        let uvder0 = torus.uvder(u, v);
        let uvder1 = (torus.vder(u + EPS, v) - torus.vder(u - EPS, v)) / (2.0 * EPS);
        prop_assert!(
            (uvder0 - uvder1).magnitude() < EPS,
            "{:?} {:?}",
            uvder0,
            uvder1
        );

        let vvder0 = torus.vvder(u, v);
        let vvder1 = (torus.vder(u, v + EPS) - torus.vder(u, v - EPS)) / (2.0 * EPS);
        prop_assert!(
            (vvder0 - vvder1).magnitude() < EPS,
            "{:?} {:?}",
            vvder0,
            vvder1
        );

        let n0 = torus.normal(u, v);
        let n1 = torus.uder(u, v).cross(torus.vder(u, v)).normalize();
        prop_assert_near!(n0, n1);

        let (u0, v0) = torus.search_parameter(p, None, 1).unwrap();
        let (urange, vrange) = torus.parameter_range();
        prop_assert!(urange.contains(&u0) && vrange.contains(&v0), "{u0}, {v0}");
        prop_assert_near!(torus.subs(u0, v0), p);

        let deform = deform * small_radius;
        let q = p + deform * n0;
        let (u0, v0) = torus.search_nearest_parameter(q, None, 1).unwrap();
        let (urange, vrange) = torus.parameter_range();
        prop_assert!(urange.contains(&u0) && vrange.contains(&v0), "{u0}, {v0}");
        prop_assert_near!(torus.subs(u0, v0), p);
    }

    #[test]
    fn test_der_mn(
        (u, v) in (0f64..=PI, 0f64..=2.0 * PI),
        (m, n) in (0usize..=4, 0usize..=4),
        center in prop::array::uniform3(-100f64..=100.0),
        large_radius in 6.0f64..=10.0,
        small_radius in 0.1f64..=5.0,
        u_derivate in prop::bool::ANY,
    ) {
        let torus = Torus::new(Point3::from(center), large_radius, small_radius);

        const EPS: f64 = 1.0e-4;
        let (der0, der1) = if u_derivate {
            let der0 = torus.der_mn(m + 1, n, u, v);
            let der1 = (torus.der_mn(m, n, u + EPS, v) - torus.der_mn(m, n, u - EPS, v)) / (2.0 * EPS);
            (der0, der1)
        } else {
            let der0 = torus.der_mn(m, n + 1, u, v);
            let der1 = (torus.der_mn(m, n, u, v + EPS) - torus.der_mn(m, n, u, v - EPS)) / (2.0 * EPS);
            (der0, der1)
        };
        prop_assert!((der0 - der1).magnitude() < 0.01 * der0.magnitude());
    }
}

/// **Why the STEP loader refuses the degenerate (spindle) torus** -- spec 011 T1.
///
/// The proptests above only ever build RING tori (`large_radius` is derived as
/// `2 * small_radius + diff`), so nothing here has ever exercised
/// `|large_radius| < small_radius`, the self-intersecting regime real STEP files
/// carry in three different spellings (460 corpus records).
///
/// The tempting conclusion is that the regime is refused because the analytic
/// rational-NURBS conversion would be inexact there. It would not: with the
/// builder's spindle guard bypassed, the emitted control net measures exact to a
/// relative 8e-16..9e-16 against `Torus::subs` over the whole domain, with the
/// control hull still a superset of the analytic bbox.
///
/// What actually breaks is the INVERSE map, which is what places a face's trims:
/// on a spindle, the surface passes through itself, and both parameter searches
/// go wrong. `search_parameter` cannot find a quarter to a third of its own
/// on-surface points, and `search_nearest_parameter` answers those points with
/// parameters that evaluate somewhere else entirely -- silently. Horn tori
/// (`large == small`, the fillet form) and ring tori are unaffected, which is why
/// the loader's refusal predicate is exactly the builder's and no wider.
///
/// Measured 2026-07-30 on a 24x24 parameter grid, at radii taken from
/// `Rocky_House.stp` and `NissanGT-R.STEP`.
#[test]
fn spindle_torus_parameter_recovery_is_unsound_while_ring_and_horn_are_exact() {
    // (label, large, small, expect_sound)
    let cases: [(&str, f64, f64, bool); 5] = [
        (
            "Rocky_House witness spindle",
            0.633_974_596_215_563,
            1.0,
            false,
        ),
        (
            "NissanGT-R spindle |major|",
            20.852_342_325_613_467,
            29.5,
            false,
        ),
        ("ring", 3.0, 1.0, true),
        ("exact horn (Ai-14R fillet)", 3.0, 3.0, true),
        (
            "fp near-horn (Pi fillet)",
            0.099_999_999_992_725,
            0.099_999_999_999_987_88,
            true,
        ),
    ];
    const N: usize = 24;
    for (label, large_radius, small_radius, sound) in cases {
        let torus = Torus::new(Point3::origin(), large_radius, small_radius);
        let tol = 1.0e-9 * (large_radius + small_radius);
        let mut exact_recovered = 0usize;
        let mut nearest_landed_elsewhere = 0usize;
        let mut total = 0usize;
        for i in 0..N {
            // Offsets keep the samples off the seams and off the apex.
            let u = 2.0 * PI * (i as f64 + 0.37) / N as f64;
            for j in 0..N {
                let v = 2.0 * PI * (j as f64 + 0.21) / N as f64;
                let point = torus.subs(u, v);
                total += 1;
                if torus.search_parameter(point, None, 100).is_some() {
                    exact_recovered += 1;
                }
                if let Some((ru, rv)) = torus.search_nearest_parameter(point, None, 100)
                    && torus.subs(ru, rv).distance(point) > tol
                {
                    nearest_landed_elsewhere += 1;
                }
            }
        }
        if sound {
            assert_eq!(
                (exact_recovered, nearest_landed_elsewhere),
                (total, 0),
                "{label} ({large_radius}, {small_radius}) must recover every on-surface point",
            );
        } else {
            assert!(
                exact_recovered < total && nearest_landed_elsewhere > 0,
                "{label} ({large_radius}, {small_radius}): the spindle regime is expected to \
                 be UNSOUND here -- if this now passes, the inverse map was fixed and spec \
                 011 T1's refusal can be revisited. recovered {exact_recovered}/{total}, \
                 nearest landed elsewhere {nearest_landed_elsewhere}",
            );
        }
    }
}

/// **A seam point must be spelled the way the caller asks for** -- spec 012 W1.
///
/// A torus is exactly `2 pi`-periodic in both parameters, so the point on the
/// `u = 0` seam is `u = 0` and `u = 2 pi` equally. `search_parameter` picks by
/// the sign of `y`, and at the seam that sign is the sign of a ZERO: a
/// negative-zero `y` yields exactly `2 pi`, a positive-zero `y` exactly `0`.
/// Both are correct in isolation and only the caller knows which one its
/// parameter LOOP is written in -- so the caller's hint has to decide, and
/// before this it was discarded outright (`_: H`).
///
/// Measured consequence, ap224 with spec 012's analytic-torus routing on: two
/// fillet tori touching one seam from opposite sides both reported their `u`
/// trim extent as the WHOLE ring (`(0.0645, 6.2832)` and `(0, 6.2187)`) instead
/// of `(0, pi)` and `(pi, 2 pi)`. The SSI then traced the right curves against
/// the box cutter's planes and the trim filter discarded every one of them.
/// Ledger class C4.
///
/// Both directions are asserted, because a rule that only ever answers `0`
/// fixes one of those two faces and breaks the other -- which is exactly what
/// the first attempt at this did.
#[test]
fn a_seam_point_takes_the_periodic_spelling_its_hint_asks_for() {
    let torus = Torus::new(Point3::new(1.0, -2.0, 3.0), 7.0, 2.0);
    // Straddle the seam so the answers must NOT all collapse to one end.
    for v in [0.0, 0.7, PI, 5.0] {
        let seam = torus.subs(0.0, v);
        let below = torus
            .search_parameter(seam, Some((0.05, v)), 100)
            .expect("a point on the surface must be recovered");
        assert!(
            below.0.abs() < 0.1,
            "a hint just above the seam must keep the low spelling, got {below:?}",
        );
        let above = torus
            .search_parameter(seam, Some((2.0 * PI - 0.05, v)), 100)
            .expect("a point on the surface must be recovered");
        assert!(
            (above.0 - 2.0 * PI).abs() < 0.1,
            "a hint just below 2 pi must keep the high spelling, got {above:?}",
        );
        // Whichever spelling comes back, it is the SAME 3D point. That is what
        // makes the re-spelling exact rather than a tolerance trade.
        for (u, v) in [below, above] {
            assert!(
                torus.subs(u, v).distance(seam) < 1.0e-9,
                "re-spelled ({u}, {v}) must still evaluate to the seam point",
            );
        }
        // The nearest solver is the twin `project_onto_surface_domain`
        // alternates with, so it has to agree.
        let nearest_below = torus
            .search_nearest_parameter(seam, Some((0.05, v)), 100)
            .expect("nearest must answer for an on-surface point");
        let nearest_above = torus
            .search_nearest_parameter(seam, Some((2.0 * PI - 0.05, v)), 100)
            .expect("nearest must answer for an on-surface point");
        assert!(
            nearest_below.0.abs() < 0.1 && (nearest_above.0 - 2.0 * PI).abs() < 0.1,
            "the nearest solver must disambiguate the same way: {nearest_below:?} / \
             {nearest_above:?}",
        );
    }
}

/// An UNHINTED call is byte-identical to the pre-W1 arithmetic.
///
/// The seam disambiguation above is a no-op without a hint, so every call site
/// that existed before spec 012 routed tori onto the analytic variant sees the
/// same numbers it always saw. Asserted against the branch formula spelled out
/// here rather than a frozen array, so this stays honest if the radii change.
#[test]
fn an_unhinted_search_is_unchanged_by_the_seam_disambiguation() {
    let torus = Torus::new(Point3::origin(), 7.0, 2.0);
    for u in [0.0, 0.3, 1.9, PI, 4.4, 2.0 * PI - 0.3] {
        for v in [0.0, 0.4, 2.2, PI, 5.9] {
            let point = torus.subs(u, v);
            let (recovered_u, recovered_v) = torus
                .search_parameter(point, None, 100)
                .expect("an on-surface point must be recovered");
            // The pre-W1 routine's guarantee: the answer lies in [0, 2 pi] and
            // reproduces the point. Nothing weaker, nothing shifted.
            assert!(
                (0.0..=2.0 * PI).contains(&recovered_u) && (0.0..=2.0 * PI).contains(&recovered_v),
                "unhinted ({u}, {v}) left the declared range: \
                 ({recovered_u}, {recovered_v})",
            );
            assert!(
                torus.subs(recovered_u, recovered_v).distance(point) < 1.0e-9,
                "unhinted ({u}, {v}) did not reproduce its point",
            );
        }
    }
}
