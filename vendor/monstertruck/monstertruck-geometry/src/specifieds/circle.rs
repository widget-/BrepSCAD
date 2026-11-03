use super::*;
use std::f64::consts::TAU;

impl<P> UnitCircle<P> {
    /// constructor
    #[inline]
    pub const fn new() -> Self { Self(std::marker::PhantomData) }
}

impl ParametricCurve for UnitCircle<Point2> {
    type Point = Point2;
    type Vector = Vector2;
    #[inline]
    fn derivative_n(&self, n: usize, t: f64) -> Vector2 {
        match n % 4 {
            0 => Vector2::new(f64::cos(t), f64::sin(t)),
            1 => Vector2::new(-f64::sin(t), f64::cos(t)),
            2 => Vector2::new(-f64::cos(t), -f64::sin(t)),
            _ => Vector2::new(f64::sin(t), -f64::cos(t)),
        }
    }
    #[inline]
    fn evaluate(&self, t: f64) -> Point2 { Point2::from_vec(self.derivative_n(0, t)) }
    #[inline]
    fn derivative(&self, t: f64) -> Vector2 { self.derivative_n(1, t) }
    #[inline]
    fn derivative_2(&self, t: f64) -> Vector2 { self.derivative_n(2, t) }
    #[inline]
    fn parameter_range(&self) -> ParameterRange { (Bound::Included(0.0), Bound::Excluded(TAU)) }
}

impl BoundedCurve for UnitCircle<Point2> {}

impl ParametricCurve for UnitCircle<Point3> {
    type Point = Point3;
    type Vector = Vector3;
    #[inline]
    fn derivative_n(&self, n: usize, t: f64) -> Vector3 {
        match n % 4 {
            0 => Vector3::new(f64::cos(t), f64::sin(t), 0.0),
            1 => Vector3::new(-f64::sin(t), f64::cos(t), 0.0),
            2 => Vector3::new(-f64::cos(t), -f64::sin(t), 0.0),
            _ => Vector3::new(f64::sin(t), -f64::cos(t), 0.0),
        }
    }
    #[inline]
    fn evaluate(&self, t: f64) -> Point3 { Point3::from_vec(self.derivative_n(0, t)) }
    #[inline]
    fn derivative(&self, t: f64) -> Vector3 { self.derivative_n(1, t) }
    #[inline]
    fn derivative_2(&self, t: f64) -> Vector3 { self.derivative_n(2, t) }
    #[inline]
    fn period(&self) -> Option<f64> { Some(TAU) }
    #[inline]
    fn parameter_range(&self) -> ParameterRange { (Bound::Included(0.0), Bound::Excluded(TAU)) }
}

impl BoundedCurve for UnitCircle<Point3> {}

// -- v2 scalar-generic impls ------------------------------------------------

use monstertruck_traits::v2;

/// Bridges the scalar-generic v2 hint to the `f64` hint consumed by the
/// hardcoded `SearchParameter` impls. The two enums are shaped identically
/// but unrelated, so there is no blanket `From`; this rebuilds it by hand for
/// the `f64` scalar so the v2 search forwards the hint instead of dropping it.
fn v2_hint_to_hint(hint: v2::SearchParameterHint1D<f64>) -> SearchParameterHint1D {
    match hint {
        v2::SearchParameterHint1D::Parameter(t) => SearchParameterHint1D::Parameter(t),
        v2::SearchParameterHint1D::Range(t0, t1) => SearchParameterHint1D::Range(t0, t1),
        v2::SearchParameterHint1D::None => SearchParameterHint1D::None,
    }
}

macro_rules! impl_v2_circle {
    ($point:ty, $vector:ty) => {
        impl v2::ParametricCurve for UnitCircle<$point> {
            type Scalar = f64;
            type Point = $point;
            type Vector = $vector;

            #[inline]
            fn evaluate(&self, t: f64) -> $point { ParametricCurve::evaluate(self, t) }
            #[inline]
            fn derivative(&self, t: f64) -> $vector { ParametricCurve::derivative(self, t) }
            #[inline]
            fn derivative_2(&self, t: f64) -> $vector { ParametricCurve::derivative_2(self, t) }
            #[inline]
            fn derivative_n(&self, n: usize, t: f64) -> $vector {
                ParametricCurve::derivative_n(self, n, t)
            }
            #[inline]
            fn period(&self) -> Option<f64> { ParametricCurve::period(self) }
            #[inline]
            fn try_range_tuple(&self) -> Option<(f64, f64)> {
                ParametricCurve::try_range_tuple(self)
            }
        }

        impl v2::BoundedCurve for UnitCircle<$point> {
            #[inline]
            fn range_tuple(&self) -> (f64, f64) { BoundedCurve::range_tuple(self) }
        }
    };
}

impl_v2_circle!(Point2, Vector2);
impl_v2_circle!(Point3, Vector3);

macro_rules! impl_v2_circle_search {
    ($point:ty) => {
        impl v2::SearchNearestParameter<v2::D1<f64>> for UnitCircle<$point> {
            type Point = $point;
            #[inline]
            fn search_nearest_parameter<H: Into<v2::SearchParameterHint1D<f64>>>(
                &self,
                pt: $point,
                hint: H,
                _: usize,
            ) -> Option<f64> {
                SearchNearestParameter::<D1>::search_nearest_parameter(
                    self,
                    pt,
                    v2_hint_to_hint(hint.into()),
                    0,
                )
            }
        }

        impl v2::SearchParameter<v2::D1<f64>> for UnitCircle<$point> {
            type Point = $point;
            #[inline]
            fn search_parameter<H: Into<v2::SearchParameterHint1D<f64>>>(
                &self,
                pt: $point,
                hint: H,
                _: usize,
            ) -> Option<f64> {
                SearchParameter::<D1>::search_parameter(self, pt, v2_hint_to_hint(hint.into()), 0)
            }
        }
    };
}

impl_v2_circle_search!(Point2);
impl_v2_circle_search!(Point3);

impl<P> ParameterDivision1D for UnitCircle<P>
where UnitCircle<P>: ParametricCurve<Point = P>
{
    type Point = P;
    fn parameter_division(&self, range: (f64, f64), tol: f64) -> (Vec<f64>, Vec<P>) {
        nonpositive_tolerance!(tol);
        let tol = f64::min(tol, 0.8);
        let delta = 2.0 * f64::acos(1.0 - tol);
        let n = 1 + ((range.1 - range.0) / delta) as usize;
        let params = (0..=n)
            .map(|i| {
                let t = i as f64 / n as f64;
                range.0 * (1.0 - t) + range.1 * t
            })
            .collect::<Vec<_>>();
        let pts = params.iter().map(|t| self.subs(*t)).collect();
        (params, pts)
    }
}

impl SearchNearestParameter<CurveParameter> for UnitCircle<Point2> {
    type Point = Point2;
    fn search_nearest_parameter<H: Into<SearchParameterHint1D>>(
        &self,
        pt: Point2,
        hint: H,
        _: usize,
    ) -> Option<f64> {
        let v = pt.to_vec();
        if v.magnitude().so_small() {
            return None;
        }
        let v = v.normalize();
        let theta = f64::acos(f64::clamp(v.x, -1.0, 1.0));
        let theta = match v.y > 0.0 {
            true => theta,
            false => TAU - theta,
        };
        Some(round_theta(theta, hint.into()))
    }
}

impl SearchParameter<CurveParameter> for UnitCircle<Point2> {
    type Point = Point2;
    fn search_parameter<H: Into<SearchParameterHint1D>>(
        &self,
        pt: Point2,
        hint: H,
        _: usize,
    ) -> Option<f64> {
        let v = pt.to_vec();
        if !v.magnitude().near(&1.0) {
            return None;
        }
        let v = v.normalize();
        let theta = f64::acos(f64::clamp(v.x, -1.0, 1.0));
        let theta = match v.y > 0.0 {
            true => theta,
            false => TAU - theta,
        };
        Some(round_theta(theta, hint.into()))
    }
}

/// Shifts the base angle `theta` (in `[0, TAU)`) into the period nearest the
/// `hint`. Without this, searching a circle's parameter always folds into the
/// canonical first period, so a trimmed or multiply-wound arc loses the
/// parameter the caller actually expects. With a `Parameter` hint we pick
/// whichever of `theta - TAU`, `theta`, `theta + TAU` (offset into the hint's
/// period) lands closest to the hint; with a `Range` hint we land inside the
/// range when possible, else at the nearer endpoint's period.
fn round_theta(theta: f64, hint: SearchParameterHint1D) -> f64 {
    match hint {
        SearchParameterHint1D::None => theta,
        SearchParameterHint1D::Parameter(hint) => {
            let floor = (hint / TAU).floor() * TAU;
            [theta + floor - TAU, theta + floor, theta + floor + TAU]
                .into_iter()
                .fold(theta, |nearest, candidate| {
                    match (candidate - hint).abs() < (nearest - hint).abs() {
                        true => candidate,
                        false => nearest,
                    }
                })
        }
        SearchParameterHint1D::Range(hint0, hint1) => {
            let floor = (hint0 / TAU).floor() * TAU;
            let theta = match theta + floor > hint0 {
                true => theta + floor,
                false => theta + floor + TAU,
            };
            if theta < hint1 {
                return theta;
            }
            let theta0 = theta - TAU;
            match hint0 - theta0 < theta - hint1 {
                true => theta0,
                false => theta,
            }
        }
    }
}

impl SearchNearestParameter<CurveParameter> for UnitCircle<Point3> {
    type Point = Point3;
    fn search_nearest_parameter<H: Into<SearchParameterHint1D>>(
        &self,
        pt: Point3,
        hint: H,
        _: usize,
    ) -> Option<f64> {
        UnitCircle::<Point2>::new().search_nearest_parameter(Point2::new(pt.x, pt.y), hint, 0)
    }
}

impl SearchParameter<CurveParameter> for UnitCircle<Point3> {
    type Point = Point3;
    fn search_parameter<H: Into<SearchParameterHint1D>>(
        &self,
        pt: Point3,
        hint: H,
        _: usize,
    ) -> Option<f64> {
        if !f64::abs(pt.z).so_small() {
            return None;
        }
        UnitCircle::<Point2>::new().search_parameter(Point2::new(pt.x, pt.y), hint, 0)
    }
}

impl ToSameGeometry<NurbsCurve<Vector3>> for TrimmedCurve<UnitCircle<Point2>> {
    fn to_same_geometry(&self) -> NurbsCurve<Vector3> {
        let (t0, t1) = self.range_tuple();
        let angle = t1 - t0;
        let (cos2, sin2) = (f64::cos(angle / 2.0), f64::sin(angle / 2.0));
        let rot = Matrix3::from(Matrix2::from_angle(Rad(t0)));
        NurbsCurve::new(BsplineCurve::new_unchecked(
            KnotVector::bezier_knot(2),
            vec![
                rot * Vector3::new(1.0, 0.0, 1.0),
                rot * Vector3::new(cos2, sin2, cos2),
                rot * Vector3::new(f64::cos(angle), f64::sin(angle), 1.0),
            ],
        ))
    }
}

impl ToSameGeometry<NurbsCurve<Vector4>> for TrimmedCurve<UnitCircle<Point3>> {
    fn to_same_geometry(&self) -> NurbsCurve<Vector4> {
        let (t0, t1) = self.range_tuple();
        let bsp: NurbsCurve<Vector3> =
            TrimmedCurve::new(UnitCircle::<Point2>::new(), (t0, t1)).to_same_geometry();
        let (knot_vec, pts) = BsplineCurve::from(bsp).destruct();
        let mut curve = NurbsCurve::new(BsplineCurve::new_unchecked(
            knot_vec,
            vec![
                Vector4::new(pts[0].x, pts[0].y, 0.0, pts[0].z),
                Vector4::new(pts[1].x, pts[1].y, 0.0, pts[1].z),
                Vector4::new(pts[2].x, pts[2].y, 0.0, pts[2].z),
            ],
        ));
        curve.add_knot(0.25);
        curve.add_knot(0.5);
        curve.add_knot(0.75);
        curve
    }
}
