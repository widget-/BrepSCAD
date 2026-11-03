use super::*;

impl CurveScalarFunction for f64 {
    #[inline]
    fn derivative_n(&self, n: usize, _: f64) -> f64 {
        match n {
            0 => *self,
            _ => 0.0,
        }
    }
}

// 1D spline types whose evaluated point's `x` coordinate carries the scalar value.
macro_rules! impl_curve_scalar_function {
    ($ty:ty) => {
        impl CurveScalarFunction for $ty {
            #[inline]
            fn derivative_n(&self, n: usize, t: f64) -> f64 {
                ParametricCurve::derivative_n(self, n, t).x
            }
        }
    };
}
impl_curve_scalar_function!(BsplineCurve<Vector1>);
impl_curve_scalar_function!(NurbsCurve<Vector2>);

impl<T: CurveScalarFunction> CurveScalarFunction for &T {
    #[inline(always)]
    fn derivative_n(&self, n: usize, t: f64) -> f64 { (**self).derivative_n(n, t) }
}

impl SurfaceScalarFunction for f64 {
    #[inline]
    fn derivative_mn(&self, m: usize, n: usize, _: f64, _: f64) -> f64 {
        match (m, n) {
            (0, 0) => *self,
            _ => 0.0,
        }
    }
}

// 2D spline surface types whose evaluated point's `x` coordinate carries the scalar value.
macro_rules! impl_surface_scalar_function {
    ($ty:ty) => {
        impl SurfaceScalarFunction for $ty {
            #[inline]
            fn derivative_mn(&self, m: usize, n: usize, u: f64, v: f64) -> f64 {
                ParametricSurface::derivative_mn(self, m, n, u, v).x
            }
        }
    };
}
impl_surface_scalar_function!(BsplineSurface<Vector1>);
impl_surface_scalar_function!(NurbsSurface<Vector2>);

impl<T: SurfaceScalarFunction> SurfaceScalarFunction for &T {
    #[inline(always)]
    fn derivative_mn(&self, m: usize, n: usize, u: f64, v: f64) -> f64 {
        (**self).derivative_mn(m, n, u, v)
    }
}
