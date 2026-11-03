//! Scalar-generic algorithm helpers.
//!
//! These mirror the `f64`-bound algorithms in [`crate::algo`] but work with
//! any scalar satisfying `cgmath::BaseFloat`.

/// Curve algorithms.
pub mod curve;
/// Surface algorithms.
pub mod surface;
