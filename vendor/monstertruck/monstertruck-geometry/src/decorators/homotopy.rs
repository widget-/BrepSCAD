use super::*;
use algo::surface::{SearchNearestParameterVector, SearchParameterVector};
use control_point::ControlPoint;
use monstertruck_traits::ParametricCurve as ParametricCurveTrait;

impl<C0, C1> HomotopySurface<C0, C1> {
    /// constructor
    #[inline(always)]
    pub fn new(curve0: C0, curve1: C1) -> Self { Self { curve0, curve1 } }
    /// Returns the first curve.
    #[inline(always)]
    pub fn first_curve(&self) -> &C0 { &self.curve0 }
    /// Returns the second curve.
    #[inline(always)]
    pub fn second_curve(&self) -> &C1 { &self.curve1 }
    /// Returns the first curve.
    #[inline(always)]
    pub fn first_curve_mut(&mut self) -> &mut C0 { &mut self.curve0 }
    /// Returns the second curve.
    #[inline(always)]
    pub fn second_curve_mut(&mut self) -> &mut C1 { &mut self.curve1 }
}

impl<C0, C1> ParametricSurface for HomotopySurface<C0, C1>
where
    C0: ParametricCurveTrait,
    C1: ParametricCurveTrait<Point = C0::Point, Vector = C0::Vector>,
    C0::Point: EuclideanSpace<Scalar = f64, Diff = C0::Vector>,
    C0::Vector: VectorSpace<Scalar = f64>,
{
    type Point = C0::Point;
    type Vector = C0::Vector;
    #[inline(always)]
    fn derivative_mn(&self, m: usize, n: usize, u: f64, v: f64) -> Self::Vector {
        match (m, n) {
            (_, 0) => {
                let v0 = self.curve0.derivative_n(m, u);
                let v1 = self.curve1.derivative_n(m, u);
                v0 + (v1 - v0) * v
            }
            (_, 1) => {
                let v0 = self.curve0.derivative_n(m, u);
                let v1 = self.curve1.derivative_n(m, u);
                v1 - v0
            }
            _ => Self::Vector::zero(),
        }
    }
    #[inline(always)]
    fn evaluate(&self, u: f64, v: f64) -> Self::Point {
        let p0 = self.curve0.evaluate(u);
        let p1 = self.curve1.evaluate(u);
        p0 + (p1 - p0) * v
    }
    #[inline(always)]
    fn derivative_u(&self, u: f64, v: f64) -> Self::Vector {
        let v0 = self.curve0.derivative(u);
        let v1 = self.curve1.derivative(u);
        v0 + (v1 - v0) * v
    }
    #[inline(always)]
    fn derivative_v(&self, u: f64, _: f64) -> Self::Vector {
        self.curve1.evaluate(u) - self.curve0.evaluate(u)
    }
    #[inline(always)]
    fn derivative_uu(&self, u: f64, v: f64) -> Self::Vector {
        let v0 = self.curve0.derivative_2(u);
        let v1 = self.curve1.derivative_2(u);
        v0 + (v1 - v0) * v
    }
    #[inline(always)]
    fn derivative_uv(&self, u: f64, _: f64) -> Self::Vector {
        self.curve1.derivative(u) - self.curve0.derivative(u)
    }
    #[inline(always)]
    fn derivative_vv(&self, _: f64, _: f64) -> Self::Vector { Self::Vector::zero() }
    #[inline(always)]
    fn parameter_range(&self) -> (ParameterRange, ParameterRange) {
        let range0 = self.curve0.parameter_range();
        let range1 = self.curve1.parameter_range();
        let range = range_common_part(&range0, &range1);
        (range, (Bound::Included(0.0), Bound::Included(1.0)))
    }
}

impl<C0, C1> ParametricSurface3D for HomotopySurface<C0, C1>
where
    C0: ParametricCurve3D,
    C1: ParametricCurve3D,
{
}

impl<C0, C1> BoundedSurface for HomotopySurface<C0, C1>
where
    C0: BoundedCurve,
    C1: BoundedCurve,
    Self: ParametricSurface,
{
}

impl<C0, C1> ParameterDivision2D for HomotopySurface<C0, C1>
where
    C0: ParameterDivision1D,
    C1: ParameterDivision1D,
{
    fn parameter_division(
        &self,
        (urange, vrange): ((f64, f64), (f64, f64)),
        tol: f64,
    ) -> (Vec<f64>, Vec<f64>) {
        let (mut div, _) = self.curve0.parameter_division(urange, tol);
        let (div0, _) = self.curve1.parameter_division(urange, tol);
        div.extend(div0);
        // SAFETY: division parameters are finite `f64` values, so `partial_cmp` always returns `Some`.
        div.sort_by(|x, y| x.partial_cmp(y).unwrap());
        div.dedup();
        (div, vec![vrange.0, vrange.1])
    }
}

impl<C0, C1> SearchNearestParameter<SurfaceParameter> for HomotopySurface<C0, C1>
where
    C0: BoundedCurve,
    C1: BoundedCurve<Point = C0::Point, Vector = C0::Vector>,
    C0::Point: EuclideanSpace<Scalar = f64, Diff = C0::Vector> + MetricSpace<Metric = f64>,
    C0::Vector: SearchNearestParameterVector<Point = C0::Point>,
{
    type Point = C0::Point;
    fn search_nearest_parameter<H: Into<SearchParameterHint2D>>(
        &self,
        point: Self::Point,
        hint: H,
        trials: usize,
    ) -> Option<(f64, f64)> {
        let hint = match hint.into() {
            SearchParameterHint2D::Parameter(x, y) => (x, y),
            SearchParameterHint2D::Range(range0, range1) => {
                algo::surface::presearch(self, point, (range0, range1), PRESEARCH_DIVISION)
            }
            SearchParameterHint2D::None => {
                algo::surface::presearch(self, point, self.range_tuple(), PRESEARCH_DIVISION)
            }
        };
        algo::surface::search_nearest_parameter(self, point, hint, trials)
    }
}

impl<C0, C1> SearchParameter<SurfaceParameter> for HomotopySurface<C0, C1>
where
    C0: BoundedCurve,
    C1: BoundedCurve<Point = C0::Point, Vector = C0::Vector>,
    C0::Point:
        EuclideanSpace<Scalar = f64, Diff = C0::Vector> + MetricSpace<Metric = f64> + Tolerance,
    C0::Vector: SearchParameterVector<Point = C0::Point>,
{
    type Point = C0::Point;
    fn search_parameter<H: Into<SearchParameterHint2D>>(
        &self,
        point: Self::Point,
        hint: H,
        trials: usize,
    ) -> Option<(f64, f64)> {
        let hint = match hint.into() {
            SearchParameterHint2D::Parameter(x, y) => (x, y),
            SearchParameterHint2D::Range(range0, range1) => {
                algo::surface::presearch(self, point, (range0, range1), PRESEARCH_DIVISION)
            }
            SearchParameterHint2D::None => {
                algo::surface::presearch(self, point, self.range_tuple(), PRESEARCH_DIVISION)
            }
        };
        algo::surface::search_parameter(self, point, hint, trials)
    }
}

impl<P> From<HomotopySurface<BsplineCurve<P>, BsplineCurve<P>>> for BsplineSurface<P>
where P: ControlPoint<f64> + Tolerance
{
    fn from(value: HomotopySurface<BsplineCurve<P>, BsplineCurve<P>>) -> Self {
        let HomotopySurface {
            curve0: mut bspcurve0,
            curve1: mut bspcurve1,
        } = value;
        bspcurve0.syncro_degree(&mut bspcurve1);
        bspcurve0.syncro_knots(&mut bspcurve1);

        let knot_vector_u = bspcurve0.knot_vector().clone();
        let knot_vector_v = KnotVector::from(vec![0.0, 0.0, 1.0, 1.0]);
        let control_points: Vec<Vec<_>> = (0..bspcurve0.control_points().len())
            .map(|i| vec![*bspcurve0.control_point(i), *bspcurve1.control_point(i)])
            .collect();
        BsplineSurface::new_unchecked((knot_vector_u, knot_vector_v), control_points)
    }
}
