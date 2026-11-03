use monstertruck_core::scalar::NumericScalar;

/// Scalar-generic parametric surface.
///
/// Mirrors [`crate::ParametricSurface`] but parameterizes the scalar type via the
/// [`Scalar`](ParametricSurface::Scalar) associated type instead of hardcoding `f64`.
pub trait ParametricSurface: Clone {
    /// The numeric scalar type for parameter values.
    type Scalar: NumericScalar;
    /// The point type the surface maps into.
    type Point;
    /// The derivative vector type.
    type Vector;

    /// Evaluates the surface at `(u, v)`, returning the point `S(u, v)`.
    fn evaluate(&self, u: Self::Scalar, v: Self::Scalar) -> Self::Point;
    /// Returns `dS/du` at `(u, v)`.
    fn derivative_u(&self, u: Self::Scalar, v: Self::Scalar) -> Self::Vector;
    /// Returns `dS/dv` at `(u, v)`.
    fn derivative_v(&self, u: Self::Scalar, v: Self::Scalar) -> Self::Vector;
    /// Returns `d²S/du²` at `(u, v)`.
    fn derivative_uu(&self, u: Self::Scalar, v: Self::Scalar) -> Self::Vector;
    /// Returns `d²S/du dv` at `(u, v)`.
    fn derivative_uv(&self, u: Self::Scalar, v: Self::Scalar) -> Self::Vector;
    /// Returns `d²S/dv²` at `(u, v)`.
    fn derivative_vv(&self, u: Self::Scalar, v: Self::Scalar) -> Self::Vector;
    /// `None` in default; `Some(period)` if periodic w.r.t. parameter `u`.
    fn period_u(&self) -> Option<Self::Scalar>;
    /// `None` in default; `Some(period)` if periodic w.r.t. parameter `v`.
    fn period_v(&self) -> Option<Self::Scalar>;
}
