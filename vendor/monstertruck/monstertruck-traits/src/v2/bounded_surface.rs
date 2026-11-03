use super::division::SurfaceParameterRange;
use super::surface::ParametricSurface;

/// Scalar-generic bounded parametric surface.
///
/// Mirrors [`crate::BoundedSurface`] but inherits the scalar-generic
/// [`ParametricSurface`](super::ParametricSurface) trait.
pub trait BoundedSurface: ParametricSurface {
    /// Returns the parameter range as `((u_start, u_end), (v_start, v_end))`.
    fn range_tuple(&self) -> SurfaceParameterRange<Self::Scalar>;
}
