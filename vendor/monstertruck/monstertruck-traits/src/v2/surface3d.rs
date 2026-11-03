use monstertruck_core::{InnerSpace, Point3, Vector3};

/// Scalar-generic 3D parametric surface.
///
/// Adds normal-vector computation to [`super::ParametricSurface`] for surfaces
/// embedded in 3D Euclidean space.
///
/// # Bridge-phase note
///
/// This trait is **parameter-scalar generic** but fixes the ambient geometry to
/// `Point3` / `Vector3` (i.e. `f64`).  Making the ambient space scalar-generic
/// as well is deferred to a later phase.
pub trait ParametricSurface3D: super::ParametricSurface<Point = Point3, Vector = Vector3> {
    /// Returns the unit normal vector at `(u, v)`.
    fn normal(&self, u: Self::Scalar, v: Self::Scalar) -> Vector3 {
        self.derivative_u(u, v)
            .cross(self.derivative_v(u, v))
            .normalize()
    }
    /// Returns the derivative of the normal vector w.r.t. `u` at `(u, v)`.
    fn normal_uder(&self, u: Self::Scalar, v: Self::Scalar) -> Vector3 {
        let uder = self.derivative_u(u, v);
        let vder = self.derivative_v(u, v);
        let uuder = self.derivative_uu(u, v);
        let uvder = self.derivative_uv(u, v);
        let cross = uder.cross(vder);
        let cross_uder = uuder.cross(vder) + uder.cross(uvder);
        let abs = cross.magnitude();
        let abs_uder = cross.dot(cross_uder) / abs;
        (cross_uder * abs - cross * abs_uder) / (abs * abs)
    }
    /// Returns the derivative of the normal vector w.r.t. `v` at `(u, v)`.
    fn normal_vder(&self, u: Self::Scalar, v: Self::Scalar) -> Vector3 {
        let uder = self.derivative_u(u, v);
        let vder = self.derivative_v(u, v);
        let uvder = self.derivative_uv(u, v);
        let vvder = self.derivative_vv(u, v);
        let cross = uder.cross(vder);
        let cross_vder = uvder.cross(vder) + uder.cross(vvder);
        let abs = cross.magnitude();
        let abs_vder = cross.dot(cross_vder) / abs;
        (cross_vder * abs - cross * abs_vder) / (abs * abs)
    }
}
