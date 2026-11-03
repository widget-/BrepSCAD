use super::*;

impl<P> UnitParabola<P> {
    /// constructor
    #[inline]
    pub const fn new() -> Self { Self(std::marker::PhantomData) }
}

impl ParametricCurve for UnitParabola<Point2> {
    type Point = Point2;
    type Vector = Vector2;
    #[inline]
    fn derivative_n(&self, n: usize, t: f64) -> Self::Vector {
        match n {
            0 => Vector2::new(t * t, 2.0 * t),
            1 => Vector2::new(2.0 * t, 2.0),
            2 => Vector2::new(2.0, 0.0),
            _ => Vector2::zero(),
        }
    }
    #[inline]
    fn evaluate(&self, t: f64) -> Self::Point { Self::Point::from_vec(self.derivative_n(0, t)) }
    #[inline]
    fn derivative(&self, t: f64) -> Self::Vector { self.derivative_n(1, t) }
    #[inline]
    fn derivative_2(&self, t: f64) -> Self::Vector { self.derivative_n(2, t) }
}

impl ParametricCurve for UnitParabola<Point3> {
    type Point = Point3;
    type Vector = Vector3;
    fn derivative_n(&self, n: usize, t: f64) -> Self::Vector {
        match n {
            0 => Vector3::new(t * t, 2.0 * t, 0.0),
            1 => Vector3::new(2.0 * t, 2.0, 0.0),
            2 => Vector3::new(2.0, 0.0, 0.0),
            _ => Vector3::zero(),
        }
    }
    #[inline]
    fn evaluate(&self, t: f64) -> Self::Point { Self::Point::from_vec(self.derivative_n(0, t)) }
    #[inline]
    fn derivative(&self, t: f64) -> Self::Vector { self.derivative_n(1, t) }
    #[inline]
    fn derivative_2(&self, t: f64) -> Self::Vector { self.derivative_n(2, t) }
}

impl<P> ParameterDivision1D for UnitParabola<P>
where
    UnitParabola<P>: ParametricCurve<Point = P>,
    P: EuclideanSpace<Scalar = f64> + MetricSpace<Metric = f64> + HashGen<f64>,
{
    type Point = P;
    fn parameter_division(&self, range: (f64, f64), tol: f64) -> (Vec<f64>, Vec<P>) {
        algo::curve::parameter_division(self, range, tol)
    }
}

impl SearchNearestParameter<CurveParameter> for UnitParabola<Point2> {
    type Point = Point2;
    #[inline]
    fn search_nearest_parameter<H: Into<SearchParameterHint1D>>(
        &self,
        pt: Point2,
        _: H,
        _: usize,
    ) -> Option<f64> {
        let p = 2.0 - pt.x;
        let q = -pt.y;
        solver::pre_solve_cubic(p, q)
            .into_iter()
            .filter_map(|x| match x.im.so_small() {
                true => Some(x.re),
                false => None,
            })
            // SAFETY: distances are non-negative finite `f64`, so `partial_cmp` always returns `Some`.
            .min_by(|s, t| {
                pt.distance2(self.subs(*s))
                    .partial_cmp(&pt.distance2(self.subs(*t)))
                    .unwrap()
            })
    }
}

impl SearchNearestParameter<CurveParameter> for UnitParabola<Point3> {
    type Point = Point3;
    #[inline]
    fn search_nearest_parameter<H: Into<SearchParameterHint1D>>(
        &self,
        pt: Point3,
        _hint: H,
        _trials: usize,
    ) -> Option<f64> {
        UnitParabola::<Point2>::new().search_nearest_parameter(
            Point2::new(pt.x, pt.y),
            _hint,
            _trials,
        )
    }
}

impl SearchParameter<CurveParameter> for UnitParabola<Point2> {
    type Point = Point2;
    #[inline]
    fn search_parameter<H: Into<SearchParameterHint1D>>(
        &self,
        pt: Point2,
        _: H,
        _: usize,
    ) -> Option<f64> {
        let t = pt.y / 2.0;
        let pt0 = self.subs(t);
        match pt.near(&pt0) {
            true => Some(t),
            false => None,
        }
    }
}

impl SearchParameter<CurveParameter> for UnitParabola<Point3> {
    type Point = Point3;
    #[inline]
    fn search_parameter<H: Into<SearchParameterHint1D>>(
        &self,
        pt: Point3,
        _hint: H,
        _trials: usize,
    ) -> Option<f64> {
        match pt.z.so_small() {
            true => UnitParabola::<Point2>::new().search_parameter(
                Point2::new(pt.x, pt.y),
                _hint,
                _trials,
            ),
            false => None,
        }
    }
}

// -- v2 scalar-generic impls ------------------------------------------------

use monstertruck_traits::v2;

impl v2::ParametricCurve for UnitParabola<Point2> {
    type Scalar = f64;
    type Point = Point2;
    type Vector = Vector2;

    #[inline]
    fn evaluate(&self, t: f64) -> Point2 { Point2::new(t * t, 2.0 * t) }
    #[inline]
    fn derivative(&self, t: f64) -> Vector2 { Vector2::new(2.0 * t, 2.0) }
    #[inline]
    fn derivative_2(&self, _: f64) -> Vector2 { Vector2::new(2.0, 0.0) }
    #[inline]
    fn derivative_n(&self, n: usize, t: f64) -> Vector2 {
        match n {
            0 => Vector2::new(t * t, 2.0 * t),
            1 => Vector2::new(2.0 * t, 2.0),
            2 => Vector2::new(2.0, 0.0),
            _ => Vector2::zero(),
        }
    }
    #[inline]
    fn period(&self) -> Option<f64> { None }
    #[inline]
    fn try_range_tuple(&self) -> Option<(f64, f64)> { None }
}

impl v2::ParametricCurve for UnitParabola<Point3> {
    type Scalar = f64;
    type Point = Point3;
    type Vector = Vector3;

    #[inline]
    fn evaluate(&self, t: f64) -> Point3 { Point3::new(t * t, 2.0 * t, 0.0) }
    #[inline]
    fn derivative(&self, t: f64) -> Vector3 { Vector3::new(2.0 * t, 2.0, 0.0) }
    #[inline]
    fn derivative_2(&self, _: f64) -> Vector3 { Vector3::new(2.0, 0.0, 0.0) }
    #[inline]
    fn derivative_n(&self, n: usize, t: f64) -> Vector3 {
        match n {
            0 => Vector3::new(t * t, 2.0 * t, 0.0),
            1 => Vector3::new(2.0 * t, 2.0, 0.0),
            2 => Vector3::new(2.0, 0.0, 0.0),
            _ => Vector3::zero(),
        }
    }
    #[inline]
    fn period(&self) -> Option<f64> { None }
    #[inline]
    fn try_range_tuple(&self) -> Option<(f64, f64)> { None }
}

macro_rules! impl_v2_parabola_search {
    ($point:ty) => {
        impl v2::SearchNearestParameter<v2::D1<f64>> for UnitParabola<$point> {
            type Point = $point;
            #[inline]
            fn search_nearest_parameter<H: Into<v2::SearchParameterHint1D<f64>>>(
                &self,
                pt: $point,
                _: H,
                _: usize,
            ) -> Option<f64> {
                SearchNearestParameter::<D1>::search_nearest_parameter(self, pt, None, 0)
            }
        }

        impl v2::SearchParameter<v2::D1<f64>> for UnitParabola<$point> {
            type Point = $point;
            #[inline]
            fn search_parameter<H: Into<v2::SearchParameterHint1D<f64>>>(
                &self,
                pt: $point,
                _: H,
                _: usize,
            ) -> Option<f64> {
                SearchParameter::<D1>::search_parameter(self, pt, None, 0)
            }
        }
    };
}

impl_v2_parabola_search!(Point2);
impl_v2_parabola_search!(Point3);

#[test]
fn snp_test() {
    let curve = UnitParabola::<Point2>::new();

    let p = Point2::new(-3.0, 0.0);
    assert_near!(curve.search_nearest_parameter(p, None, 0).unwrap(), 0.0);
    let p = Point2::new(-3.0, 6.0);
    assert_near!(curve.search_nearest_parameter(p, None, 0).unwrap(), 1.0);
    let p = Point2::new(1.5, 1.5);
    assert_near!(curve.search_nearest_parameter(p, None, 0).unwrap(), 1.0);
}

#[test]
fn sp_test() {
    let curve = UnitParabola::<Point2>::new();

    let p = Point2::new(4.0, -4.0);
    assert_near!(curve.search_parameter(p, None, 0).unwrap(), -2.0);
    let p = Point2::new(-3.0, 6.0);
    assert!(curve.search_parameter(p, None, 0).is_none());
}
