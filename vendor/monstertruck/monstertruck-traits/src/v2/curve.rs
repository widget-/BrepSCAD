use monstertruck_core::scalar::NumericScalar;

/// Scalar-generic parametric curve.
///
/// Mirrors [`crate::ParametricCurve`] but parameterizes the scalar type via the
/// [`Scalar`](ParametricCurve::Scalar) associated type instead of hardcoding `f64`.
pub trait ParametricCurve: Clone {
    /// The numeric scalar type for parameter values.
    type Scalar: NumericScalar;
    /// The point type the curve maps into.
    type Point;
    /// The derivative vector type.
    type Vector;

    /// Evaluates the curve at parameter `t`, returning the point `P(t)`.
    fn evaluate(&self, t: Self::Scalar) -> Self::Point;
    /// Returns the first derivative (tangent) at `t`.
    fn derivative(&self, t: Self::Scalar) -> Self::Vector;
    /// Returns the second derivative at `t`.
    fn derivative_2(&self, t: Self::Scalar) -> Self::Vector;
    /// Returns the `n`-th derivative at `t`.
    fn derivative_n(&self, n: usize, t: Self::Scalar) -> Self::Vector;
    /// `None` in default; `Some(period)` if periodic.
    fn period(&self) -> Option<Self::Scalar>;
    /// Returns the bounded parameter range as a tuple, or `None` if unbounded.
    fn try_range_tuple(&self) -> Option<(Self::Scalar, Self::Scalar)>;
}
