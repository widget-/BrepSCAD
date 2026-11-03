use monstertruck_geometry::prelude::{
    AnalyticSurfaceKind, BsplineCurve, BsplineSurface, HomogeneousSurfaceConversion,
    ParametricSurface3D, Point2, SearchNearestParameter, SearchParameter,
    SupportsExactPatchDomains, SurfaceParameterRectangle, TryIntoAnalyticSurfaceKind,
    TryIntoBsplineSurface, TryIntoHomogeneousBsplineCurve, TryIntoHomogeneousBsplineSurface,
};
use monstertruck_meshing::prelude::*;

#[derive(Clone, Debug)]
pub enum Alternative<T, U> {
    FirstType(T),
    SecondType(U),
}

#[doc(hidden)]
#[macro_export]
macro_rules! impl_from {
    ($firsttype: ty, $secondtype: ty) => {
        impl From<$firsttype> for $crate::alternative::Alternative<$firsttype, $secondtype> {
            #[inline(always)]
            fn from(t: $firsttype) -> Self { $crate::alternative::Alternative::FirstType(t) }
        }
    };
}

impl<T, U> From<U> for Alternative<T, U> {
    #[inline(always)]
    fn from(u: U) -> Self { Alternative::SecondType(u) }
}

// test for impl_from
impl_from!((), usize);

macro_rules! derive_method {
	($method: tt, $return_type: ty, $($var: ident : $paramtype: ty),*) => {
		fn $method (&self, $($var: $paramtype),*) -> $return_type {
			match &self {
				Alternative::FirstType(got) => got.$method($($var),*),
				Alternative::SecondType(got) => got.$method($($var),*),
			}
		}
	};
	($method: tt <$x: ident : $y: path>, $return_type: ty, $($var: ident : $paramtype: ty),*) => {
		fn $method<$x : $y>(&self, $($var: $paramtype),*) -> $return_type {
			match &self {
				Alternative::FirstType(got) => got.$method($($var),*),
				Alternative::SecondType(got) => got.$method($($var),*),
			}
		}
	};
}

impl<C0, C1> ParametricCurve for Alternative<C0, C1>
where
    C0: ParametricCurve,
    C1: ParametricCurve<Point = C0::Point, Vector = C0::Vector>,
{
    type Point = C0::Point;
    type Vector = C0::Vector;
    derive_method!(evaluate, C0::Point, t: f64);
    derive_method!(derivative, C0::Vector, t: f64);
    derive_method!(derivative_2, C0::Vector, t: f64);
    derive_method!(derivative_n, C0::Vector, n: usize, t: f64);
    derive_method!(
        parameter_range,
        (std::ops::Bound<f64>, std::ops::Bound<f64>),
    );
}

impl<C0, C1> BoundedCurve for Alternative<C0, C1>
where
    C0: BoundedCurve,
    C1: BoundedCurve<Point = C0::Point, Vector = C0::Vector>,
{
}

impl<S0, S1> ParametricSurface for Alternative<S0, S1>
where
    S0: ParametricSurface,
    S1: ParametricSurface<Point = S0::Point, Vector = S0::Vector>,
{
    type Point = S0::Point;
    type Vector = S0::Vector;
    derive_method!(evaluate, S0::Point, u: f64, v: f64);
    derive_method!(derivative_u, S0::Vector, u: f64, v: f64);
    derive_method!(derivative_v, S0::Vector, u: f64, v: f64);
    derive_method!(derivative_uu, S0::Vector, u: f64, v: f64);
    derive_method!(derivative_uv, S0::Vector, u: f64, v: f64);
    derive_method!(derivative_vv, S0::Vector, u: f64, v: f64);
    derive_method!(derivative_mn, S0::Vector, m: usize, n: usize, u: f64, v: f64);
}

impl<S0, S1> ParametricSurface3D for Alternative<S0, S1>
where
    S0: ParametricSurface3D,
    S1: ParametricSurface3D,
{
    derive_method!(normal, Vector3, u: f64, v: f64);
}

impl<C0, C1> Cut for Alternative<C0, C1>
where
    C0: Cut,
    C1: Cut<Point = C0::Point, Vector = C0::Vector>,
{
    fn cut(&mut self, t: f64) -> Self {
        match self {
            Self::FirstType(curve) => Self::FirstType(curve.cut(t)),
            Self::SecondType(curve) => Self::SecondType(curve.cut(t)),
        }
    }
}

impl<C0, C1> ParameterDivision1D for Alternative<C0, C1>
where
    C0: ParameterDivision1D,
    C1: ParameterDivision1D<Point = C0::Point>,
{
    type Point = C0::Point;
    derive_method!(
        parameter_division,
        (Vec<f64>, Vec<C0::Point>),
        range: (f64, f64),
        tol: f64
    );
}

impl<S0, S1> ParameterDivision2D for Alternative<S0, S1>
where
    S0: ParameterDivision2D,
    S1: ParameterDivision2D,
{
    derive_method!(
        parameter_division,
        (Vec<f64>, Vec<f64>),
        range: ((f64, f64), (f64, f64)),
        tol: f64
    );
}

impl<D: SearchParameterDimension, T, U> SearchParameter<D> for Alternative<T, U>
where
    T: SearchParameter<D>,
    U: SearchParameter<D, Point = T::Point>,
{
    type Point = T::Point;
    derive_method!(
        search_parameter<H: Into<D::Hint>>,
        Option<D::Parameter>,
        point: T::Point,
        hint: H,
        trials: usize
    );
}

impl<D: SearchParameterDimension, T, U> SearchNearestParameter<D> for Alternative<T, U>
where
    T: SearchNearestParameter<D>,
    U: SearchNearestParameter<D, Point = T::Point>,
{
    type Point = T::Point;
    derive_method!(
        search_nearest_parameter<H: Into<D::Hint>>,
        Option<D::Parameter>,
        point: T::Point,
        hint: H,
        trials: usize
    );
}

impl<T, U> TryIntoBsplineSurface for Alternative<T, U>
where
    T: TryIntoBsplineSurface,
    U: TryIntoBsplineSurface,
{
    fn try_into_bspline_surface(&self) -> Option<BsplineSurface<Point3>> {
        match self {
            Self::FirstType(entity) => entity.try_into_bspline_surface(),
            Self::SecondType(entity) => entity.try_into_bspline_surface(),
        }
    }
}

impl<T, U> TryIntoHomogeneousBsplineSurface for Alternative<T, U>
where
    T: TryIntoHomogeneousBsplineSurface,
    U: TryIntoHomogeneousBsplineSurface,
{
    fn try_into_homogeneous_bspline_surface(&self) -> Option<BsplineSurface<Vector4>> {
        match self {
            Self::FirstType(entity) => entity.try_into_homogeneous_bspline_surface(),
            Self::SecondType(entity) => entity.try_into_homogeneous_bspline_surface(),
        }
    }

    fn try_into_homogeneous_bspline_surface_over(
        &self,
        parameter_range: Option<SurfaceParameterRectangle>,
    ) -> Option<HomogeneousSurfaceConversion> {
        match self {
            Self::FirstType(entity) => {
                entity.try_into_homogeneous_bspline_surface_over(parameter_range)
            }
            Self::SecondType(entity) => {
                entity.try_into_homogeneous_bspline_surface_over(parameter_range)
            }
        }
    }
}

impl<T, U> SupportsExactPatchDomains for Alternative<T, U>
where
    T: SupportsExactPatchDomains,
    U: SupportsExactPatchDomains,
{
    fn supports_exact_patch_domains(&self) -> bool {
        match self {
            Self::FirstType(entity) => entity.supports_exact_patch_domains(),
            Self::SecondType(entity) => entity.supports_exact_patch_domains(),
        }
    }
}

impl<T, U> TryIntoAnalyticSurfaceKind for Alternative<T, U>
where
    T: TryIntoAnalyticSurfaceKind,
    U: TryIntoAnalyticSurfaceKind,
{
    fn try_into_analytic_surface_kind(&self) -> Option<AnalyticSurfaceKind> {
        match self {
            Self::FirstType(entity) => entity.try_into_analytic_surface_kind(),
            Self::SecondType(entity) => entity.try_into_analytic_surface_kind(),
        }
    }
}

impl<T, U> TryIntoHomogeneousBsplineCurve for Alternative<T, U>
where
    T: TryIntoHomogeneousBsplineCurve,
    U: TryIntoHomogeneousBsplineCurve,
{
    fn try_into_homogeneous_bspline_curve(&self) -> Option<BsplineCurve<Vector4>> {
        match self {
            Self::FirstType(entity) => entity.try_into_homogeneous_bspline_curve(),
            Self::SecondType(entity) => entity.try_into_homogeneous_bspline_curve(),
        }
    }
}

impl<T, U> Invertible for Alternative<T, U>
where
    T: Invertible,
    U: Invertible,
{
    #[inline(always)]
    fn invert(&mut self) {
        match self {
            Self::FirstType(entity) => entity.invert(),
            Self::SecondType(entity) => entity.invert(),
        }
    }
    #[inline(always)]
    fn inverse(&self) -> Self {
        match self {
            Self::FirstType(entity) => Self::FirstType(entity.inverse()),
            Self::SecondType(entity) => Self::SecondType(entity.inverse()),
        }
    }
}

impl<T, U, S> ParameterBoundary2D<S> for Alternative<T, U>
where
    T: ParameterBoundary2D<S>,
    U: ParameterBoundary2D<S>,
{
    fn parameter_boundary_2d(&self, surface: &S, tolerance: f64) -> Option<Vec<Point2>> {
        match self {
            Self::FirstType(entity) => entity.parameter_boundary_2d(surface, tolerance),
            Self::SecondType(entity) => entity.parameter_boundary_2d(surface, tolerance),
        }
    }
}

impl<T, U, S> ExactParameterBoundary2D<S> for Alternative<T, U>
where
    T: ExactParameterBoundary2D<S>,
    U: ExactParameterBoundary2D<S, BoundaryCurve = T::BoundaryCurve>,
{
    type BoundaryCurve = T::BoundaryCurve;

    fn exact_parameter_boundary_2d(&self, surface: &S) -> Option<Self::BoundaryCurve> {
        match self {
            Self::FirstType(entity) => entity.exact_parameter_boundary_2d(surface),
            Self::SecondType(entity) => entity.exact_parameter_boundary_2d(surface),
        }
    }
}

// -- v2 scalar-generic impls ------------------------------------------------

use monstertruck_traits::v2;

macro_rules! derive_v2_method {
    ($method:tt, $return_type:ty, $($var:ident : $paramtype:ty),*) => {
        fn $method(&self, $($var: $paramtype),*) -> $return_type {
            match self {
                Alternative::FirstType(got) => v2::ParametricCurve::$method(got, $($var),*),
                Alternative::SecondType(got) => v2::ParametricCurve::$method(got, $($var),*),
            }
        }
    };
}

macro_rules! derive_v2_surface_method {
    ($method:tt, $return_type:ty, $($var:ident : $paramtype:ty),*) => {
        fn $method(&self, $($var: $paramtype),*) -> $return_type {
            match self {
                Alternative::FirstType(got) => v2::ParametricSurface::$method(got, $($var),*),
                Alternative::SecondType(got) => v2::ParametricSurface::$method(got, $($var),*),
            }
        }
    };
}

impl<C0, C1> v2::ParametricCurve for Alternative<C0, C1>
where
    C0: v2::ParametricCurve,
    C1: v2::ParametricCurve<Scalar = C0::Scalar, Point = C0::Point, Vector = C0::Vector>,
{
    type Scalar = C0::Scalar;
    type Point = C0::Point;
    type Vector = C0::Vector;

    derive_v2_method!(evaluate, C0::Point, t: C0::Scalar);
    derive_v2_method!(derivative, C0::Vector, t: C0::Scalar);
    derive_v2_method!(derivative_2, C0::Vector, t: C0::Scalar);
    derive_v2_method!(derivative_n, C0::Vector, n: usize, t: C0::Scalar);

    fn period(&self) -> Option<C0::Scalar> {
        match self {
            Alternative::FirstType(got) => v2::ParametricCurve::period(got),
            Alternative::SecondType(got) => v2::ParametricCurve::period(got),
        }
    }
    fn try_range_tuple(&self) -> Option<(C0::Scalar, C0::Scalar)> {
        match self {
            Alternative::FirstType(got) => v2::ParametricCurve::try_range_tuple(got),
            Alternative::SecondType(got) => v2::ParametricCurve::try_range_tuple(got),
        }
    }
}

impl<C0, C1> v2::BoundedCurve for Alternative<C0, C1>
where
    C0: v2::BoundedCurve,
    C1: v2::BoundedCurve<Scalar = C0::Scalar, Point = C0::Point, Vector = C0::Vector>,
{
    fn range_tuple(&self) -> (C0::Scalar, C0::Scalar) {
        match self {
            Alternative::FirstType(got) => v2::BoundedCurve::range_tuple(got),
            Alternative::SecondType(got) => v2::BoundedCurve::range_tuple(got),
        }
    }
}

impl<S0, S1> v2::ParametricSurface for Alternative<S0, S1>
where
    S0: v2::ParametricSurface,
    S1: v2::ParametricSurface<Scalar = S0::Scalar, Point = S0::Point, Vector = S0::Vector>,
{
    type Scalar = S0::Scalar;
    type Point = S0::Point;
    type Vector = S0::Vector;

    derive_v2_surface_method!(evaluate, S0::Point, u: S0::Scalar, v: S0::Scalar);
    derive_v2_surface_method!(derivative_u, S0::Vector, u: S0::Scalar, v: S0::Scalar);
    derive_v2_surface_method!(derivative_v, S0::Vector, u: S0::Scalar, v: S0::Scalar);
    derive_v2_surface_method!(derivative_uu, S0::Vector, u: S0::Scalar, v: S0::Scalar);
    derive_v2_surface_method!(derivative_uv, S0::Vector, u: S0::Scalar, v: S0::Scalar);
    derive_v2_surface_method!(derivative_vv, S0::Vector, u: S0::Scalar, v: S0::Scalar);

    fn period_u(&self) -> Option<S0::Scalar> {
        match self {
            Alternative::FirstType(got) => v2::ParametricSurface::period_u(got),
            Alternative::SecondType(got) => v2::ParametricSurface::period_u(got),
        }
    }
    fn period_v(&self) -> Option<S0::Scalar> {
        match self {
            Alternative::FirstType(got) => v2::ParametricSurface::period_v(got),
            Alternative::SecondType(got) => v2::ParametricSurface::period_v(got),
        }
    }
}

impl<S0, S1> v2::BoundedSurface for Alternative<S0, S1>
where
    S0: v2::BoundedSurface,
    S1: v2::BoundedSurface<Scalar = S0::Scalar, Point = S0::Point, Vector = S0::Vector>,
{
    fn range_tuple(&self) -> ((S0::Scalar, S0::Scalar), (S0::Scalar, S0::Scalar)) {
        match self {
            Alternative::FirstType(got) => v2::BoundedSurface::range_tuple(got),
            Alternative::SecondType(got) => v2::BoundedSurface::range_tuple(got),
        }
    }
}

impl<C0, C1> v2::Cut for Alternative<C0, C1>
where
    C0: v2::Cut,
    C1: v2::Cut<Scalar = C0::Scalar, Point = C0::Point, Vector = C0::Vector>,
{
    fn cut(&mut self, t: C0::Scalar) -> Self {
        match self {
            Self::FirstType(curve) => Self::FirstType(v2::Cut::cut(curve, t)),
            Self::SecondType(curve) => Self::SecondType(v2::Cut::cut(curve, t)),
        }
    }
}

impl<S0, S1> v2::ParametricSurface3D for Alternative<S0, S1>
where
    S0: v2::ParametricSurface3D,
    S1: v2::ParametricSurface3D<Scalar = S0::Scalar>,
{
    fn normal(&self, u: S0::Scalar, v: S0::Scalar) -> Vector3 {
        match self {
            Alternative::FirstType(got) => v2::ParametricSurface3D::normal(got, u, v),
            Alternative::SecondType(got) => v2::ParametricSurface3D::normal(got, u, v),
        }
    }
}

impl<D, T, U> v2::SearchParameter<D> for Alternative<T, U>
where
    D: v2::SearchParameterDimension,
    T: v2::SearchParameter<D>,
    U: v2::SearchParameter<D, Point = T::Point>,
{
    type Point = T::Point;
    fn search_parameter<H: Into<D::Hint>>(
        &self,
        point: T::Point,
        hint: H,
        trials: usize,
    ) -> Option<D::Parameter> {
        match self {
            Alternative::FirstType(got) => got.search_parameter(point, hint, trials),
            Alternative::SecondType(got) => got.search_parameter(point, hint, trials),
        }
    }
}

impl<D, T, U> v2::SearchNearestParameter<D> for Alternative<T, U>
where
    D: v2::SearchParameterDimension,
    T: v2::SearchNearestParameter<D>,
    U: v2::SearchNearestParameter<D, Point = T::Point>,
{
    type Point = T::Point;
    fn search_nearest_parameter<H: Into<D::Hint>>(
        &self,
        point: T::Point,
        hint: H,
        trials: usize,
    ) -> Option<D::Parameter> {
        match self {
            Alternative::FirstType(got) => got.search_nearest_parameter(point, hint, trials),
            Alternative::SecondType(got) => got.search_nearest_parameter(point, hint, trials),
        }
    }
}
