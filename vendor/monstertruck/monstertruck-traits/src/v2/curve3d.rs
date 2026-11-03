use monstertruck_core::{Point3, Vector3};

/// Scalar-generic 3D parametric curve marker.
///
/// Blanket-implemented for any [`super::ParametricCurve`] whose point and
/// vector types are `Point3` and `Vector3`.
pub trait ParametricCurve3D: super::ParametricCurve<Point = Point3, Vector = Vector3> {}

impl<C> ParametricCurve3D for C where C: super::ParametricCurve<Point = Point3, Vector = Vector3> {}
