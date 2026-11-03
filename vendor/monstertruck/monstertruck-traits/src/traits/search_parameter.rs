/// Parameter-space dimension marker used by [`SearchParameter`] /
/// [`SearchNearestParameter`].
///
/// Implemented by [`CurveParameter`] (1D) and [`SurfaceParameter`] (2D);
/// not intended to be implemented by external code.
pub trait SearchParameterDimension {
    /// Number of parameter axes (`1` for a curve, `2` for a surface).
    const DIM: usize;
    /// The parameter tuple: `f64` for a curve, `(f64, f64)` for a surface.
    type Parameter;
    /// The hint payload: [`SearchParameterHint1D`] for a curve,
    /// [`SearchParameterHint2D`] for a surface.
    type Hint;
}

/// Parameter-space marker for curve geometry (`t: f64`).
///
/// Used as the dimension type parameter of [`SearchParameter`] and
/// [`SearchNearestParameter`] on [`ParametricCurve`](crate::ParametricCurve)
/// implementors.
#[derive(Clone, Copy, Debug)]
pub enum CurveParameter {}

impl SearchParameterDimension for CurveParameter {
    const DIM: usize = 1;
    type Parameter = f64;
    type Hint = SearchParameterHint1D;
}

/// Parameter-space marker for surface geometry (`(u, v): (f64, f64)`).
///
/// Used as the dimension type parameter of [`SearchParameter`] and
/// [`SearchNearestParameter`] on [`ParametricSurface`](crate::ParametricSurface)
/// implementors.
#[derive(Clone, Copy, Debug)]
pub enum SurfaceParameter {}

impl SearchParameterDimension for SurfaceParameter {
    const DIM: usize = 2;
    type Parameter = (f64, f64);
    type Hint = SearchParameterHint2D;
}

// Upstream `truck-geotrait` names the parameter-space markers `D1` and `D2`
// (for "Dimension 1" / "Dimension 2"). The names read as abbreviated
// labels rather than self-describing markers, and at every call site
// `SearchParameter<CurveParameter>` / `SearchParameter<SurfaceParameter>` is in fact picking the
// curve vs surface flavour of the trait. The canonical names are now
// `CurveParameter` and `SurfaceParameter`; the upstream spellings stay
// as `#[deprecated]` re-exports so code ported from `truck-geotrait`
// continues to compile.
#[deprecated(since = "0.3.1", note = "renamed to `CurveParameter`.")]
pub use self::CurveParameter as D1;
#[deprecated(since = "0.3.1", note = "renamed to `SurfaceParameter`.")]
pub use self::SurfaceParameter as D2;

/// hint for searching parameter for curve
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SearchParameterHint1D {
    /// a parameter near the answer
    Parameter(f64),
    /// the range of parameter including answer
    Range(f64, f64),
    /// There are no hint. In the case of `BoundedCurve`, most of the time the parameter range is applied.
    /// Such as planes, no hinting is needed in the first place.
    None,
}

impl From<f64> for SearchParameterHint1D {
    #[inline(always)]
    fn from(x: f64) -> SearchParameterHint1D { SearchParameterHint1D::Parameter(x) }
}

impl From<(f64, f64)> for SearchParameterHint1D {
    #[inline(always)]
    fn from(range: (f64, f64)) -> SearchParameterHint1D {
        SearchParameterHint1D::Range(range.0, range.1)
    }
}

impl From<Option<f64>> for SearchParameterHint1D {
    #[inline(always)]
    fn from(x: Option<f64>) -> SearchParameterHint1D {
        match x {
            Some(x) => x.into(),
            None => SearchParameterHint1D::None,
        }
    }
}

/// hint for searching parameter for surface
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SearchParameterHint2D {
    /// a parameter near the answer
    Parameter(f64, f64),
    /// the range of parameter including answer
    Range((f64, f64), (f64, f64)),
    /// There are no hint. If the algorithm needed a hint, it always returns None.
    None,
}

impl From<(f64, f64)> for SearchParameterHint2D {
    #[inline(always)]
    fn from(x: (f64, f64)) -> Self { Self::Parameter(x.0, x.1) }
}

impl From<((f64, f64), (f64, f64))> for SearchParameterHint2D {
    #[inline(always)]
    fn from(ranges: ((f64, f64), (f64, f64))) -> Self { Self::Range(ranges.0, ranges.1) }
}

impl From<Option<(f64, f64)>> for SearchParameterHint2D {
    #[inline(always)]
    fn from(x: Option<(f64, f64)>) -> Self {
        match x {
            Some(x) => x.into(),
            None => SearchParameterHint2D::None,
        }
    }
}

/// Search parameter `t` such that `self.evaluate(t)` is near point.
pub trait SearchParameter<Dim: SearchParameterDimension> {
    /// point
    type Point;
    /// Search parameter `t` such that `self.evaluate(t)` is near point.
    /// Returns `None` if could not find such parameter.
    fn search_parameter<H: Into<Dim::Hint>>(
        &self,
        point: Self::Point,
        hint: H,
        trials: usize,
    ) -> Option<Dim::Parameter>;
}

impl<Dim: SearchParameterDimension, T: SearchParameter<Dim>> SearchParameter<Dim> for &T {
    type Point = T::Point;
    #[inline(always)]
    fn search_parameter<H: Into<Dim::Hint>>(
        &self,
        point: Self::Point,
        hint: H,
        trials: usize,
    ) -> Option<Dim::Parameter> {
        T::search_parameter(*self, point, hint, trials)
    }
}

impl<Dim: SearchParameterDimension, T: SearchParameter<Dim>> SearchParameter<Dim> for Box<T> {
    type Point = T::Point;
    #[inline(always)]
    fn search_parameter<H: Into<Dim::Hint>>(
        &self,
        point: Self::Point,
        hint: H,
        trials: usize,
    ) -> Option<Dim::Parameter> {
        T::search_parameter(&**self, point, hint, trials)
    }
}

/// Search parameter `t` such that `self.evaluate(t)` is nearest point.
pub trait SearchNearestParameter<Dim: SearchParameterDimension> {
    /// point
    type Point;
    /// Search nearest parameter `t` such that `self.evaluate(t)` is nearest point.
    /// Returns `None` if could not find such parameter.
    fn search_nearest_parameter<H: Into<Dim::Hint>>(
        &self,
        point: Self::Point,
        hint: H,
        trials: usize,
    ) -> Option<Dim::Parameter>;
}

impl<Dim: SearchParameterDimension, T: SearchNearestParameter<Dim>> SearchNearestParameter<Dim>
    for &T
{
    type Point = T::Point;
    #[inline(always)]
    fn search_nearest_parameter<H: Into<Dim::Hint>>(
        &self,
        point: Self::Point,
        hint: H,
        trials: usize,
    ) -> Option<Dim::Parameter> {
        T::search_nearest_parameter(*self, point, hint, trials)
    }
}

impl<Dim: SearchParameterDimension, T: SearchNearestParameter<Dim>> SearchNearestParameter<Dim>
    for Box<T>
{
    type Point = T::Point;
    #[inline(always)]
    fn search_nearest_parameter<H: Into<Dim::Hint>>(
        &self,
        point: Self::Point,
        hint: H,
        trials: usize,
    ) -> Option<Dim::Parameter> {
        T::search_nearest_parameter(&**self, point, hint, trials)
    }
}
