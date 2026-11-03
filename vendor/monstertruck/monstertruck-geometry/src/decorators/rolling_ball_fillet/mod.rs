use super::*;
use monstertruck_traits::ParametricCurve as ParametricCurveTrait;
use std::f64::consts::PI;

impl<C, S0, S1, R> RollingBallFilletSurface<C, S0, S1, R> {
    /// constructor
    #[inline]
    pub const fn new(edge_curve: C, surface0: S0, surface1: S1, radius: R) -> Self {
        Self {
            edge_curve,
            surface0,
            surface1,
            radius,
        }
    }

    /// returns edge curve
    #[inline]
    pub const fn edge_curve(&self) -> &C { &self.edge_curve }
    /// returns first surface
    #[inline]
    pub const fn surface0(&self) -> &S0 { &self.surface0 }
    /// returns second surface
    #[inline]
    pub const fn surface1(&self) -> &S1 { &self.surface1 }
    /// returns radius function
    #[inline]
    pub const fn radius(&self) -> &R { &self.radius }

    /// returns the orbit curve of contact point with `surface0`.
    #[inline]
    pub fn contact_curve0(&self) -> RollingBallFilletContactCurve<C, S0, S1, R>
    where Self: Clone {
        RollingBallFilletContactCurve {
            surface: self.clone(),
            index: 0,
        }
    }
    /// returns the orbit curve of contact point with `surface1`.
    #[inline]
    pub fn contact_curve1(&self) -> RollingBallFilletContactCurve<C, S0, S1, R>
    where Self: Clone {
        RollingBallFilletContactCurve {
            surface: self.clone(),
            index: 1,
        }
    }
}

/// Trait for radius functions.
pub trait RadiusFunction: Clone {
    /// Returns the `n`-th derivative at parameter `t`.
    fn derivative_n(&self, n: usize, t: f64) -> f64 { self.der_n(n, t) }
    /// Returns the `n`-th derivative at parameter `t`.
    fn der_n(&self, _n: usize, _t: f64) -> f64 {
        panic!("`RadiusFunction` implementors must override `derivative_n` or `der_n`.")
    }
    /// Evaluates the radius at parameter `t`.
    #[inline]
    fn evaluate(&self, t: f64) -> f64 { self.derivative_n(0, t) }
    /// Substitutes parameter `t` and returns the radius.
    #[inline]
    fn subs(&self, t: f64) -> f64 { self.evaluate(t) }
    /// Returns the first derivative at parameter `t`.
    #[inline]
    fn derivative(&self, t: f64) -> f64 { self.derivative_n(1, t) }
    /// Returns the second derivative at parameter `t`.
    #[inline]
    fn derivative_2(&self, t: f64) -> f64 { self.derivative_n(2, t) }
    /// Returns all derivatives at parameter `t` with order `0..=max_order`.
    #[inline]
    fn derivatives(&self, max_order: usize, t: f64) -> CurveDerivatives<f64> {
        (0..=max_order).map(|n| self.derivative_n(n, t)).collect()
    }
}

impl RadiusFunction for f64 {
    #[inline]
    fn derivative_n(&self, n: usize, _: f64) -> f64 {
        match n {
            0 => *self,
            _ => 0.0,
        }
    }
}

macro_rules! impl_radius_1dim {
    ($ty: ty) => {
        impl RadiusFunction for $ty {
            #[inline]
            fn derivative_n(&self, n: usize, t: f64) -> f64 {
                ParametricCurveTrait::derivative_n(self, n, t).x
            }
        }
    };
}
impl_radius_1dim!(BsplineCurve<Point1>);
impl_radius_1dim!(NurbsCurve<Vector2>);

/// Contact point of the rolling ball and surface
#[derive(Clone, Copy, Debug)]
pub struct ContactPoint {
    /// the 3d-coordinate of contact point
    pub point: Point3,
    /// the parameter on the surface
    pub uv: Point2,
}

impl From<(Point3, Point2)> for ContactPoint {
    #[inline]
    fn from((point, uv): (Point3, Point2)) -> Self { Self { point, uv } }
}

impl From<ContactPoint> for (Point3, (f64, f64)) {
    #[inline]
    fn from(cp: ContactPoint) -> Self { (cp.point, (cp.uv.x, cp.uv.y)) }
}

/// Contact circle for rolling ball fillet.
#[derive(Clone, Copy, Debug)]
pub struct ContactCircle {
    center: Point3,
    axis: Vector3,
    angle: Rad<f64>,
    t: f64,
    contact_point0: ContactPoint,
    contact_point1: ContactPoint,
}

mod algo;
mod contact_circle;

impl<C, S0, S1, R> RollingBallFilletSurface<C, S0, S1, R>
where
    C: ParametricCurve3D,
    S0: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    S1: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    R: RadiusFunction,
{
    fn sub_der_mn(&self, m: usize, n: usize, u: f64, cc: ContactCircle) -> Vector3 {
        match (m, n) {
            (_, 0) => cc.derivative_n(m, u),
            (0, 1) => self.vder_info(cc, 1).derivative_v(u),
            (1, 1) => self.vder_info(cc, 1).derivative_uv(u),
            (0, 2) => self.vder_info(cc, 2).derivative_vv(u),
            _ => unimplemented!(
                "higher order derivation of RollingBallFilletSurface is not implemented."
            ),
        }
    }
}

impl<C, S0, S1, R> ParametricSurface for RollingBallFilletSurface<C, S0, S1, R>
where
    C: ParametricCurve3D,
    S0: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    S1: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    R: RadiusFunction,
{
    type Point = Point3;
    type Vector = Vector3;
    // SAFETY (all `contact_circle().unwrap()` below): the `ParametricSurface` trait
    // requires `v` to lie within the parameter range of the edge curve, where a valid
    // contact circle always exists.
    fn derivatives(&self, max_order: usize, u: f64, v: f64) -> SurfaceDerivatives<Vector3> {
        let cc = self.contact_circle(v).unwrap();
        let mut out = SurfaceDerivatives::new(max_order);
        (0..=max_order).for_each(|i| {
            (0..=max_order - i).for_each(|j| {
                out[i][j] = self.sub_der_mn(i, j, u, cc);
            });
        });
        out
    }
    fn derivative_mn(&self, m: usize, n: usize, u: f64, v: f64) -> Vector3 {
        self.sub_der_mn(m, n, u, self.contact_circle(v).unwrap())
    }
    fn evaluate(&self, u: f64, v: f64) -> Point3 { self.contact_circle(v).unwrap().evaluate(u) }
    fn derivative_u(&self, u: f64, v: f64) -> Vector3 {
        self.contact_circle(v).unwrap().derivative(u)
    }
    fn derivative_v(&self, u: f64, v: f64) -> Vector3 {
        self.vder_info(self.contact_circle(v).unwrap(), 1)
            .derivative_v(u)
    }
    fn derivative_uu(&self, u: f64, v: f64) -> Self::Vector {
        self.contact_circle(v).unwrap().derivative_2(u)
    }
    fn derivative_uv(&self, u: f64, v: f64) -> Self::Vector {
        self.vder_info(self.contact_circle(v).unwrap(), 1)
            .derivative_uv(u)
    }
    fn derivative_vv(&self, u: f64, v: f64) -> Self::Vector {
        self.vder_info(self.contact_circle(v).unwrap(), 2)
            .derivative_vv(u)
    }
    fn parameter_range(&self) -> (ParameterRange, ParameterRange) {
        use std::ops::Bound::*;
        (
            (Included(0.0), Included(1.0)),
            self.edge_curve.parameter_range(),
        )
    }
    fn period_v(&self) -> Option<f64> { self.edge_curve.period() }
}

impl<C, S0, S1, R> ParametricSurface3D for RollingBallFilletSurface<C, S0, S1, R>
where
    C: ParametricCurve3D,
    S0: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    S1: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    R: RadiusFunction,
{
}

impl<C, S0, S1, R> BoundedSurface for RollingBallFilletSurface<C, S0, S1, R>
where
    C: ParametricCurve3D + BoundedCurve,
    S0: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    S1: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    R: RadiusFunction,
{
}

impl<C, S0, S1, R> ParameterDivision2D for RollingBallFilletSurface<C, S0, S1, R>
where
    C: ParametricCurve3D,
    S0: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    S1: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    R: RadiusFunction,
{
    fn parameter_division(
        &self,
        range: ((f64, f64), (f64, f64)),
        tol: f64,
    ) -> (Vec<f64>, Vec<f64>) {
        nonpositive_tolerance!(tol);
        // SAFETY: `parameter_division_u` returns `None` only when `contact_circle` fails;
        // within a valid parameter range, this always succeeds.
        let udiv = self.parameter_division_u(range, tol).unwrap();
        let mut vdiv = vec![range.1.0, range.1.1];
        algo::parameter_division_v_for_fillet(self, &udiv, &mut vdiv, tol);
        (udiv, vdiv)
    }
}

impl<C, S0, S1, R> SearchParameter<SurfaceParameter> for RollingBallFilletSurface<C, S0, S1, R>
where
    C: ParametricCurve3D + SearchNearestParameter<CurveParameter, Point = Point3>,
    S0: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    S1: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    R: RadiusFunction,
{
    type Point = Point3;
    fn search_parameter<H: Into<SearchParameterHint2D>>(
        &self,
        point: Self::Point,
        hint: H,
        trials: usize,
    ) -> Option<(f64, f64)> {
        let curve_hint = match hint.into() {
            SearchParameterHint2D::Parameter(_, v) => SearchParameterHint1D::Parameter(v),
            SearchParameterHint2D::Range(_, (v0, v1)) => SearchParameterHint1D::Range(v0, v1),
            SearchParameterHint2D::None => SearchParameterHint1D::None,
        };
        let edge_curve = &self.edge_curve;
        let v = edge_curve.search_nearest_parameter(point, curve_hint, trials)?;
        let cc = self.contact_circle(v)?;

        let cp0 = cc.contact_point0.point - cc.center;
        let cp = point - cc.center;
        let u = cp.angle(cp0).0 / cc.angle.0;
        match cp.magnitude2().near(&cp0.magnitude2()) {
            true => Some((u, v)),
            false => None,
        }
    }
}

impl<C, S0, S1, R> RollingBallFilletContactCurve<C, S0, S1, R> {
    /// original fillet surface
    #[inline]
    pub const fn fillet_surface(&self) -> &RollingBallFilletSurface<C, S0, S1, R> { &self.surface }
    /// curve index: curve on `surface0` => 0, curve on `surface1` => 1.
    #[inline]
    pub const fn index(&self) -> usize { self.index }
}

impl<C, S0, S1, R> ParametricCurveTrait for RollingBallFilletContactCurve<C, S0, S1, R>
where
    C: ParametricCurve3D,
    S0: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    S1: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    R: RadiusFunction,
{
    type Point = Point3;
    type Vector = Vector3;
    fn derivatives(&self, n: usize, t: f64) -> CurveDerivatives<Vector3> {
        if n == 0 {
            // SAFETY: a single-element array always produces a valid `CurveDerivatives`.
            return CurveDerivatives::try_from([self.evaluate(t).to_vec()]).unwrap();
        }
        // SAFETY: `t` is within the edge curve parameter range, so `contact_circle` succeeds.
        let cc = self.surface.contact_circle(t).unwrap();
        let rders = self.surface.radius.derivatives(n, t);
        let cc_ders = self.surface.sub_center_contacts_ders(cc, &rders, n);
        match self.index {
            0 => cc_ders.contact0_ders,
            _ => cc_ders.contact1_ders,
        }
    }
    fn derivative_n(&self, n: usize, t: f64) -> Self::Vector { self.derivatives(n, t)[n] }
    fn evaluate(&self, t: f64) -> Self::Point {
        // SAFETY: `t` is within the edge curve parameter range, so `contact_circle` succeeds.
        let cc = self.surface.contact_circle(t).unwrap();
        match self.index {
            0 => cc.contact_point0.point,
            _ => cc.contact_point1.point,
        }
    }
    fn derivative(&self, t: f64) -> Self::Vector { self.derivative_n(1, t) }
    fn derivative_2(&self, t: f64) -> Self::Vector { self.derivative_n(2, t) }
    #[inline]
    fn parameter_range(&self) -> ParameterRange { self.surface.edge_curve.parameter_range() }
    #[inline]
    fn period(&self) -> Option<f64> { self.surface.edge_curve.period() }
}

impl<C, S0, S1, R> BoundedCurve for RollingBallFilletContactCurve<C, S0, S1, R>
where
    C: ParametricCurve3D + BoundedCurve,
    S0: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    S1: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    R: RadiusFunction,
{
}

impl<C, S0, S1, R> ParameterDivision1D for RollingBallFilletContactCurve<C, S0, S1, R>
where
    C: ParametricCurve3D,
    S0: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    S1: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    R: RadiusFunction,
{
    type Point = Point3;
    fn parameter_division(&self, range: (f64, f64), tol: f64) -> (Vec<f64>, Vec<Self::Point>) {
        monstertruck_traits::algo::curve::parameter_division(self, range, tol)
    }
}

impl<C, S0, S1, R> SearchParameter<CurveParameter> for RollingBallFilletContactCurve<C, S0, S1, R>
where
    C: ParametricCurve3D + SearchNearestParameter<CurveParameter, Point = Point3>,
    S0: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    S1: ParametricSurface3D + SearchParameter<SurfaceParameter, Point = Point3>,
    R: RadiusFunction,
{
    type Point = Point3;
    fn search_parameter<H: Into<SearchParameterHint1D>>(
        &self,
        point: Self::Point,
        hint: H,
        trials: usize,
    ) -> Option<f64> {
        let edge_curve = &self.surface.edge_curve;
        let t = edge_curve.search_nearest_parameter(point, hint, trials)?;
        let cc = self.surface.contact_circle(t)?;
        let q = match self.index {
            0 => cc.contact_point0.point,
            _ => cc.contact_point1.point,
        };
        match point.near(&q) {
            true => Some(t),
            false => None,
        }
    }
}
