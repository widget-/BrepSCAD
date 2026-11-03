use super::*;
use std::f64::consts::PI;

impl Sphere {
    /// Creates a sphere
    #[inline(always)]
    pub const fn new(center: Point3, radius: f64) -> Sphere { Sphere { center, radius } }
    /// Returns the center
    #[inline(always)]
    pub const fn center(&self) -> Point3 { self.center }
    /// Returns the radius
    #[inline(always)]
    pub const fn radius(&self) -> f64 { self.radius }
    /// Returns whether the point `pt` is on sphere
    #[inline(always)]
    pub fn include(&self, pt: Point3) -> bool { self.center.distance(pt).near(&self.radius) }
}

impl ParametricSurface for Sphere {
    type Point = Point3;
    type Vector = Vector3;
    #[inline(always)]
    fn derivative_mn(&self, m: usize, n: usize, u: f64, v: f64) -> Self::Vector {
        let ((su, cu), (sv, cv)) = (u.sin_cos(), v.sin_cos());
        let center = match (m, n) {
            (0, 0) => self.center().to_vec(),
            _ => Vector3::zero(),
        };
        let u_part = match m % 4 {
            0 => Vector3::new(su, su, cu),
            1 => Vector3::new(cu, cu, -su),
            2 => Vector3::new(-su, -su, -cu),
            _ => Vector3::new(-cu, -cu, su),
        };
        let v_z = if n == 0 { 1.0 } else { 0.0 };
        let v_part = match n % 4 {
            0 => Vector3::new(cv, sv, v_z),
            1 => Vector3::new(-sv, cv, 0.0),
            2 => Vector3::new(-cv, -sv, 0.0),
            _ => Vector3::new(sv, -cv, 0.0),
        };
        center + self.radius * u_part.mul_element_wise(v_part)
    }
    #[inline(always)]
    fn evaluate(&self, u: f64, v: f64) -> Point3 { self.center() + self.radius * self.normal(u, v) }
    #[inline(always)]
    fn derivative_u(&self, u: f64, v: f64) -> Vector3 {
        self.radius
            * Vector3::new(
                f64::cos(u) * f64::cos(v),
                f64::cos(u) * f64::sin(v),
                -f64::sin(u),
            )
    }
    #[inline(always)]
    fn derivative_v(&self, u: f64, v: f64) -> Vector3 {
        self.radius * f64::sin(u) * Vector3::new(-f64::sin(v), f64::cos(v), 0.0)
    }
    #[inline(always)]
    fn derivative_uu(&self, u: f64, v: f64) -> Vector3 { -self.radius * self.normal(u, v) }
    #[inline(always)]
    fn derivative_uv(&self, u: f64, v: f64) -> Vector3 {
        self.radius * f64::cos(u) * Vector3::new(-f64::sin(v), f64::cos(v), 0.0)
    }
    #[inline(always)]
    fn derivative_vv(&self, u: f64, v: f64) -> Vector3 {
        -self.radius * f64::sin(u) * Vector3::new(f64::cos(v), f64::sin(v), 0.0)
    }
    #[inline(always)]
    fn parameter_range(&self) -> (ParameterRange, ParameterRange) {
        (
            (Bound::Included(0.0), Bound::Included(PI)),
            (Bound::Included(0.0), Bound::Excluded(2.0 * PI)),
        )
    }
    #[inline(always)]
    fn period_v(&self) -> Option<f64> { Some(2.0 * PI) }
}

impl ParametricSurface3D for Sphere {
    #[inline(always)]
    fn normal(&self, u: f64, v: f64) -> Vector3 {
        Vector3::new(
            f64::sin(u) * f64::cos(v),
            f64::sin(u) * f64::sin(v),
            f64::cos(u),
        )
    }
    #[inline(always)]
    fn normal_uder(&self, u: f64, v: f64) -> Vector3 {
        Vector3::new(
            f64::cos(u) * f64::cos(v),
            f64::cos(u) * f64::sin(v),
            -f64::sin(u),
        )
    }
    #[inline(always)]
    fn normal_vder(&self, u: f64, v: f64) -> Vector3 {
        Vector3::new(-f64::sin(u) * f64::sin(v), f64::sin(u) * f64::cos(v), 0.0)
    }
}

impl BoundedSurface for Sphere {}

// -- v2 scalar-generic impls ------------------------------------------------

use monstertruck_traits::v2;

impl v2::ParametricSurface for Sphere {
    type Scalar = f64;
    type Point = Point3;
    type Vector = Vector3;

    #[inline(always)]
    fn evaluate(&self, u: f64, v: f64) -> Point3 { ParametricSurface::evaluate(self, u, v) }
    #[inline(always)]
    fn derivative_u(&self, u: f64, v: f64) -> Vector3 {
        ParametricSurface::derivative_u(self, u, v)
    }
    #[inline(always)]
    fn derivative_v(&self, u: f64, v: f64) -> Vector3 {
        ParametricSurface::derivative_v(self, u, v)
    }
    #[inline(always)]
    fn derivative_uu(&self, u: f64, v: f64) -> Vector3 {
        ParametricSurface::derivative_uu(self, u, v)
    }
    #[inline(always)]
    fn derivative_uv(&self, u: f64, v: f64) -> Vector3 {
        ParametricSurface::derivative_uv(self, u, v)
    }
    #[inline(always)]
    fn derivative_vv(&self, u: f64, v: f64) -> Vector3 {
        ParametricSurface::derivative_vv(self, u, v)
    }
    #[inline(always)]
    fn period_u(&self) -> Option<f64> { ParametricSurface::period_u(self) }
    #[inline(always)]
    fn period_v(&self) -> Option<f64> { ParametricSurface::period_v(self) }
}

impl v2::BoundedSurface for Sphere {
    #[inline(always)]
    fn range_tuple(&self) -> ((f64, f64), (f64, f64)) { BoundedSurface::range_tuple(self) }
}

impl v2::ParametricSurface3D for Sphere {}

impl v2::SearchNearestParameter<v2::D2<f64>> for Sphere {
    type Point = Point3;
    #[inline(always)]
    fn search_nearest_parameter<H: Into<v2::SearchParameterHint2D<f64>>>(
        &self,
        pt: Point3,
        _: H,
        trials: usize,
    ) -> Option<(f64, f64)> {
        SearchNearestParameter::<D2>::search_nearest_parameter(self, pt, None, trials)
    }
}

impl v2::SearchParameter<v2::D2<f64>> for Sphere {
    type Point = Point3;
    #[inline(always)]
    fn search_parameter<H: Into<v2::SearchParameterHint2D<f64>>>(
        &self,
        pt: Point3,
        _: H,
        trials: usize,
    ) -> Option<(f64, f64)> {
        SearchParameter::<D2>::search_parameter(self, pt, None, trials)
    }
}

impl IncludeCurve<BsplineCurve<Point3>> for Sphere {
    #[inline(always)]
    fn include(&self, curve: &BsplineCurve<Point3>) -> bool {
        curve.is_const() && self.include(curve.front())
    }
}

impl IncludeCurve<NurbsCurve<Vector4>> for Sphere {
    fn include(&self, curve: &NurbsCurve<Vector4>) -> bool {
        let (knots, _) = curve.knot_vector().to_single_multi();
        let degree = curve.degree() * 2;
        knots
            .windows(2)
            .flat_map(move |window| (1..degree).map(move |i| (window, i)))
            .all(move |(window, i)| {
                let t = i as f64 / degree as f64;
                let t = window[0] * (1.0 - t) + window[1] * t;
                self.include(curve.subs(t))
            })
    }
}

impl ParameterDivision2D for Sphere {
    #[inline(always)]
    fn parameter_division(
        &self,
        (urange, vrange): ((f64, f64), (f64, f64)),
        tol: f64,
    ) -> (Vec<f64>, Vec<f64>) {
        nonpositive_tolerance!(tol);
        // A chord ratio at or above 1 used to be `assert!(tol < self.radius)`.
        // That panic was unreachable while STEP spheres arrived here as rational
        // nets; spec 012 U1.2 routes them onto this closed form, so a viewer
        // asking for a chord coarser than a small sphere's radius -- an
        // assembly-scale `1e-3 * diagonal` against a 2 mm fillet ball, say --
        // now reaches it. `ParameterDivision2D` has no refusal channel and its
        // consumers (viewers, area and volume, mesh export) have nowhere to put
        // one, so a panic here is ledger C11.
        //
        // Clamping the RATIO, not the tolerance, and at 1.0 rather than
        // `UnitCircle`'s 0.8: every ratio that does not panic today is `< 1`
        // and so is left BYTE-IDENTICAL. At the clamp `delta` is `pi`, i.e. the
        // coarsest division that still says something -- which is the honest
        // answer when the chord budget exceeds the whole sphere.
        let chord_ratio = f64::min(tol / self.radius, 1.0);
        let delta = 2.0 * f64::acos(1.0 - chord_ratio);
        let u_div = 1 + ((urange.1 - urange.0) / delta).floor() as usize;
        let v_div = 1 + ((vrange.1 - vrange.0) / delta).floor() as usize;
        (
            (0..=u_div)
                .map(|i| urange.0 + (urange.1 - urange.0) * i as f64 / u_div as f64)
                .collect(),
            (0..=v_div)
                .map(|j| vrange.0 + (vrange.1 - vrange.0) * j as f64 / v_div as f64)
                .collect(),
        )
    }
}

/// The `v` seam twin of `nearest_periodic_angle` in `specifieds/torus.rs` --
/// see that function for the measurement that produced this rule.
///
/// `2 * PI - acos(cosv)` is exactly `2 * PI` when `cosv` is exactly `1` and
/// `radius[1] <= 0`, i.e. on the `v = 0` seam with a negative-zero `y`, and
/// exactly `0` for the same point with a positive-zero `y`. Both spellings are
/// correct and only the CALLER knows which one its parameter loop is written
/// in, so the hint decides.
///
/// **Not observed failing on any in-repo fixture** -- ap224 carries no spheres
/// and boxy carries neither spheres nor tori -- but spec 012 U1.2 put spheres
/// on exactly this code path, so the branch is now as reachable as the torus
/// one that WAS measured failing. Fixed here rather than left as the twin the
/// ledger's C3 recurrence guard tells us to diff. `u` is NOT touched: a
/// sphere's colatitude range `[0, PI]` is closed and non-periodic, so it has
/// no seam. Unhinted callers are BYTE-IDENTICAL.
#[inline]
fn nearest_periodic_v(v: f64, hint: Option<f64>) -> f64 {
    match hint {
        None => v,
        Some(hint) => v + 2.0 * PI * ((hint - v) / (2.0 * PI)).round(),
    }
}

/// The `v` a [`SearchParameterHint2D`] carries, or `None`.
///
/// `Range` answers `None` deliberately: the pole branch below already reads
/// `Parameter` and nothing else, and folding a range to a midpoint there would
/// change an answer this change has no business changing.
#[inline]
fn hint_v(hint: SearchParameterHint2D) -> Option<f64> {
    match hint {
        SearchParameterHint2D::Parameter(_, v) => Some(v),
        SearchParameterHint2D::Range(..) | SearchParameterHint2D::None => None,
    }
}

impl SearchParameter<SurfaceParameter> for Sphere {
    type Point = Point3;
    #[inline(always)]
    fn search_parameter<H: Into<SearchParameterHint2D>>(
        &self,
        point: Point3,
        hint: H,
        _: usize,
    ) -> Option<(f64, f64)> {
        let hint = hint_v(hint.into());
        let radius = point - self.center;
        if (self.radius * self.radius).near(&radius.magnitude2()) {
            let radius = radius.normalize();
            let u = f64::acos(radius[2]);
            let sinu = f64::sqrt(1.0 - radius[2] * radius[2]);
            let cosv = f64::clamp(radius[0] / sinu, -1.0, 1.0);
            // At a pole every `v` is the same 3D point, so the hint is not a
            // disambiguation there but the whole answer -- unchanged.
            let v = if sinu.so_small() {
                hint.unwrap_or(0.0)
            } else if radius[1] > 0.0 {
                nearest_periodic_v(f64::acos(cosv), hint)
            } else {
                nearest_periodic_v(2.0 * PI - f64::acos(cosv), hint)
            };
            Some((u, v))
        } else {
            None
        }
    }
}

impl SearchNearestParameter<SurfaceParameter> for Sphere {
    type Point = Point3;
    #[inline(always)]
    fn search_nearest_parameter<H: Into<SearchParameterHint2D>>(
        &self,
        point: Point3,
        hint: H,
        _: usize,
    ) -> Option<(f64, f64)> {
        // The seam twin of the exact solver above; see [`nearest_periodic_v`].
        // `project_onto_surface_domain` ALTERNATES the two solvers, so teaching
        // only one would leave the defect reachable on every other attempt.
        let hint = hint_v(hint.into());
        let radial_vector = point - self.center;
        // When `point` coincides with `center`, every `(u, v)` on the
        // sphere is equidistant -- there is no unique nearest parameter.
        // `radial_vector.normalize()` would divide by zero and propagate
        // `NaN` through the rest of the routine; return an arbitrary
        // valid `(u, v)` instead.
        if radial_vector.magnitude2().so_small() {
            return Some((0.0, 0.0));
        }
        let radius = radial_vector.normalize();
        // Clamp the `acos` argument to `[-1.0, 1.0]` so floating-point
        // error in `normalize()` cannot push it slightly outside the
        // domain and produce `NaN`.
        let u = f64::acos(f64::clamp(radius[2], -1.0, 1.0));
        let sinu = f64::sqrt(1.0 - radius[2] * radius[2]);
        // The poles (`sinu == 0`, i.e. `point` on the sphere's
        // axis-of-symmetry through `center`) are a coordinate
        // singularity: every value of `v` maps to the same 3D point, so
        // pick `0` rather than evaluating `radius[0] / sinu` (which
        // would be `0 / 0`, producing `NaN` that propagates through
        // every following `clamp` and `acos`).
        let v = if sinu.so_small() {
            0.0
        } else {
            let cosv = f64::clamp(radius[0] / sinu, -1.0, 1.0);
            let v = if radius[1] > 0.0 {
                f64::acos(cosv)
            } else {
                2.0 * PI - f64::acos(cosv)
            };
            nearest_periodic_v(v, hint)
        };
        Some((u, v))
    }
}
