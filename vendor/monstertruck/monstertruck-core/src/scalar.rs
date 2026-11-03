//! Scalar trait hierarchy for the geometry kernel.
//!
//! Phase 0 scaffolding: defines marker traits that future scalar-generic code
//! will bound on. The default (and currently only) scalar is `f64`.
//!
//! - [`NumericScalar`](crate::scalar::NumericScalar) -- minimal marker for analytic evaluation.
//! - [`ToleranceScalar`](crate::scalar::ToleranceScalar) -- approximate equality and epsilon-based comparisons.
//! - [`ToleranceV2`](crate::scalar::ToleranceV2) -- scalar-generic replacement for [`crate::tolerance::Tolerance`].
//!
//! `PredicateScalar` is intentionally deferred until the predicate lane's
//! requirements are known.

use cgmath::AbsDiffEq;
use std::fmt::Debug;

/// Core numeric marker for analytic evaluation, Newton steps, and normalization.
///
/// Intentionally kept minimal in Phase 0. Stronger numeric bounds (e.g. `Float`)
/// will be added when algorithm migration actually needs them.
pub trait NumericScalar: Copy + Debug + Default + Send + Sync + 'static {}

impl NumericScalar for f32 {}
impl NumericScalar for f64 {}

/// Approximate equality and epsilon-based comparisons.
///
/// Mirrors the constants from [`crate::tolerance::Tolerance`] but as associated
/// functions on the scalar type rather than a global trait.
pub trait ToleranceScalar: NumericScalar {
    /// The primary tolerance threshold (corresponds to `TOLERANCE`).
    fn tolerance() -> Self;
    /// The squared tolerance threshold (corresponds to `TOLERANCE2`).
    fn tolerance2() -> Self;
}

impl ToleranceScalar for f32 {
    #[inline]
    fn tolerance() -> Self { 1.0e-4 }
    #[inline]
    fn tolerance2() -> Self { 1.0e-8 }
}

impl ToleranceScalar for f64 {
    #[inline]
    fn tolerance() -> Self { 1.0e-6 }
    #[inline]
    fn tolerance2() -> Self { 1.0e-12 }
}

/// Scalar-generic tolerance trait.
///
/// Replacement for [`crate::tolerance::Tolerance`] that works with any scalar
/// whose `AbsDiffEq::Epsilon` implements [`ToleranceScalar`], rather than
/// requiring `Epsilon = f64`.
pub trait ToleranceV2: AbsDiffEq + Debug
where Self::Epsilon: ToleranceScalar {
    /// The "distance" is less than the scalar's tolerance.
    #[inline]
    fn near_v2(&self, other: &Self) -> bool { self.abs_diff_eq(other, Self::Epsilon::tolerance()) }

    /// The "distance" is less than the scalar's squared tolerance.
    #[inline]
    fn near2_v2(&self, other: &Self) -> bool {
        self.abs_diff_eq(other, Self::Epsilon::tolerance2())
    }
}

impl<T: AbsDiffEq + Debug> ToleranceV2 for T where T::Epsilon: ToleranceScalar {}

/// Scalar-generic origin test.
///
/// Replacement for [`crate::tolerance::Origin`] that works with any scalar
/// whose `AbsDiffEq::Epsilon` implements [`ToleranceScalar`].
pub trait OriginV2: ToleranceV2 + cgmath::Zero
where Self::Epsilon: ToleranceScalar {
    /// Near origin.
    #[inline]
    fn so_small_v2(&self) -> bool { self.near_v2(&Self::zero()) }

    /// Near origin in square order.
    #[inline]
    fn so_small2_v2(&self) -> bool { self.near2_v2(&Self::zero()) }
}

impl<T: ToleranceV2 + cgmath::Zero> OriginV2 for T where T::Epsilon: ToleranceScalar {}

/// Extracts the natural scalar type from a geometric point or vector.
///
/// This trait bridges the gap between cgmath's generic types (e.g.
/// `Point3<S>`) and the v2 trait layer's `Scalar` associated type.
/// It allows geometry types like `Line<P>` to derive their scalar from
/// `P` without carrying an extra type parameter.
pub trait HasScalar {
    /// The scalar type this geometric type naturally operates with.
    type Scalar: NumericScalar + cgmath::BaseFloat;
}

macro_rules! impl_has_scalar {
    ($ty:ident) => {
        impl<S: cgmath::BaseFloat + NumericScalar> HasScalar for cgmath::$ty<S> {
            type Scalar = S;
        }
    };
}

impl_has_scalar!(Point1);
impl_has_scalar!(Point2);
impl_has_scalar!(Point3);
impl_has_scalar!(Vector1);
impl_has_scalar!(Vector2);
impl_has_scalar!(Vector3);
impl_has_scalar!(Vector4);
