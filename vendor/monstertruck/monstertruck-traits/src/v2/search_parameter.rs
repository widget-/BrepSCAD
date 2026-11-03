use monstertruck_core::scalar::NumericScalar;
use std::ops::Bound;

/// Scalar-generic search parameter dimension.
///
/// Mirrors [`crate::SearchParameterDimension`] with an associated scalar type.
pub trait SearchParameterDimension {
    /// The numeric scalar type.
    type Scalar: NumericScalar;
    /// The parameter type (scalar for curves, tuple for surfaces).
    type Parameter;
    /// The hint type for guiding parameter search.
    type Hint;
}

/// Parameter-space marker for curve geometry, generic over the scalar type.
#[derive(Clone, Copy, Debug)]
pub enum CurveParameter<T> {
    #[doc(hidden)]
    _Phantom(std::marker::PhantomData<T>),
}

/// Parameter-space marker for surface geometry, generic over the scalar type.
#[derive(Clone, Copy, Debug)]
pub enum SurfaceParameter<T> {
    #[doc(hidden)]
    _Phantom(std::marker::PhantomData<T>),
}

// See the `traits::search_parameter` module for the rename rationale --
// these aliases mirror the v1 deprecation so the v2 namespace stays in
// sync.
#[deprecated(since = "0.3.1", note = "renamed to `CurveParameter`.")]
pub use self::CurveParameter as D1;
#[deprecated(since = "0.3.1", note = "renamed to `SurfaceParameter`.")]
pub use self::SurfaceParameter as D2;

/// Scalar-generic parameter range.
pub type ParameterRange<T> = (Bound<T>, Bound<T>);

/// Hint for searching a 1D parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SearchParameterHint1D<T> {
    /// A parameter near the answer.
    Parameter(T),
    /// The range of parameter including the answer.
    Range(T, T),
    /// No hint available.
    None,
}

/// Hint for searching a 2D parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SearchParameterHint2D<T> {
    /// A parameter near the answer.
    Parameter(T, T),
    /// The range of parameter including the answer.
    Range((T, T), (T, T)),
    /// No hint available.
    None,
}

impl<T: NumericScalar> SearchParameterDimension for D1<T> {
    type Scalar = T;
    type Parameter = T;
    type Hint = SearchParameterHint1D<T>;
}

impl<T: NumericScalar> SearchParameterDimension for D2<T> {
    type Scalar = T;
    type Parameter = (T, T);
    type Hint = SearchParameterHint2D<T>;
}

/// Search parameter `t` such that `self.evaluate(t)` is near `point`.
pub trait SearchParameter<Dim: SearchParameterDimension> {
    /// The point type.
    type Point;

    /// Search parameter `t` such that `self.evaluate(t)` is near `point`.
    /// Returns `None` if no such parameter could be found.
    fn search_parameter<H: Into<Dim::Hint>>(
        &self,
        point: Self::Point,
        hint: H,
        trials: usize,
    ) -> Option<Dim::Parameter>;
}

/// Search parameter `t` such that `self.evaluate(t)` is nearest to `point`.
pub trait SearchNearestParameter<Dim: SearchParameterDimension> {
    /// The point type.
    type Point;

    /// Search nearest parameter `t` such that `self.evaluate(t)` is nearest to `point`.
    /// Returns `None` if no such parameter could be found.
    fn search_nearest_parameter<H: Into<Dim::Hint>>(
        &self,
        point: Self::Point,
        hint: H,
        trials: usize,
    ) -> Option<Dim::Parameter>;
}
