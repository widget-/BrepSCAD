/// Scalar-generic 1D parameter division.
///
/// Mirrors [`crate::ParameterDivision1D`] with a generic scalar type.
pub trait ParameterDivision1D {
    /// The scalar type for parameters and tolerance.
    type Scalar;
    /// The point type the curve maps into.
    type Point;

    /// Creates the curve division (parameters, corresponding points).
    fn parameter_division(
        &self,
        range: (Self::Scalar, Self::Scalar),
        tol: Self::Scalar,
    ) -> (Vec<Self::Scalar>, Vec<Self::Point>);
}

/// Two-dimensional closed parameter range for a scalar-generic surface.
pub type SurfaceParameterRange<Scalar> = ((Scalar, Scalar), (Scalar, Scalar));

/// Scalar-generic 2D parameter division.
///
/// Mirrors [`crate::ParameterDivision2D`] with a generic scalar type.
pub trait ParameterDivision2D {
    /// The scalar type for parameters and tolerance.
    type Scalar;

    /// Creates the surface division.
    fn parameter_division(
        &self,
        range: SurfaceParameterRange<Self::Scalar>,
        tol: Self::Scalar,
    ) -> (Vec<Self::Scalar>, Vec<Self::Scalar>);
}
