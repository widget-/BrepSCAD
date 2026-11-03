//! Generic point, vector, and matrix aliases parameterized over scalar type.
//!
//! These mirror the `f64`-specialized aliases in [`crate::cgmath64`] but accept
//! an arbitrary scalar `T`. The existing `f64` aliases remain unchanged.

/// Generic 1D point.
pub type Point1G<T> = cgmath::Point1<T>;
/// Generic 2D point.
pub type Point2G<T> = cgmath::Point2<T>;
/// Generic 3D point.
pub type Point3G<T> = cgmath::Point3<T>;

/// Generic 1D vector.
pub type Vector1G<T> = cgmath::Vector1<T>;
/// Generic 2D vector.
pub type Vector2G<T> = cgmath::Vector2<T>;
/// Generic 3D vector.
pub type Vector3G<T> = cgmath::Vector3<T>;
/// Generic 4D vector.
pub type Vector4G<T> = cgmath::Vector4<T>;

/// Generic 2x2 matrix.
pub type Matrix2G<T> = cgmath::Matrix2<T>;
/// Generic 3x3 matrix.
pub type Matrix3G<T> = cgmath::Matrix3<T>;
/// Generic 4x4 matrix.
pub type Matrix4G<T> = cgmath::Matrix4<T>;
