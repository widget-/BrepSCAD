use monstertruck_core::generic_math::Point2G;

/// Scalar-generic face-local 2D trim of a 3D curve on a given surface.
///
/// The scalar type is taken from the surface `S`, because the boundary
/// polyline lives in the surface parameter domain and must share its
/// numeric representation.
///
/// # Bridge-phase note
///
/// The UV output type is `Point2G<S::Scalar>` rather than the concrete
/// `Point2` alias, so that alternate scalar types are not blocked by the
/// trait signature.
pub trait ParameterBoundary2D<S: super::ParametricSurface> {
    /// Returns the boundary polyline in the parameter domain of `surface`.
    ///
    /// Returns `None` when `self` does not carry an exact boundary on
    /// `surface`.
    fn parameter_boundary_2d(
        &self,
        _surface: &S,
        _tolerance: S::Scalar,
    ) -> Option<Vec<Point2G<S::Scalar>>> {
        None
    }
}
