use super::curve::ParametricCurve;

/// Scalar-generic bounded parametric curve.
///
/// Mirrors [`crate::BoundedCurve`] but inherits the scalar-generic
/// [`ParametricCurve`](super::ParametricCurve) trait.
pub trait BoundedCurve: ParametricCurve {
    /// Returns the parameter range as `(t_start, t_end)`.
    fn range_tuple(&self) -> (Self::Scalar, Self::Scalar);
}
