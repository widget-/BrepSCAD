//! Geometric trait definitions: `ParametricCurve`, `ParametricSurface`, `BoundedCurve`, `Invertible`, `Transformed`, and more.
//!
//! # Examples
//!
//! ```
//! use monstertruck_traits::*;
//! use monstertruck_core::cgmath64::*;
//!
//! // `range_tuple` comes from `BoundedCurve`, so the bound needs both traits.
//! fn arc_length<C: ParametricCurve<Point = Point3> + BoundedCurve>(
//!     curve: &C,
//!     steps: usize,
//! ) -> f64 {
//!     let (t0, t1) = curve.range_tuple();
//!     let dt = (t1 - t0) / steps as f64;
//!     (0..steps)
//!         .map(|i| {
//!             let a = curve.evaluate(t0 + dt * i as f64);
//!             let b = curve.evaluate(t0 + dt * (i + 1) as f64);
//!             (b - a).magnitude()
//!         })
//!         .sum()
//! }
//! ```
//!
//! # Continuity foundations
//!
//! [`ContinuityOrder`] provides checked `G0`--`G4` requests, [`BoundarySide`]
//! names full tensor-product patch sides, and [`SurfaceContinuityCapability`]
//! carries a typed representation-specific support determination without
//! embedding representation rules in this crate. Unsupported reports preserve
//! an actionable reason and any known maximum supported order. The report does
//! not establish two-surface or solver feasibility. `G4` is explicitly
//! experimental.
//!
//! ```
//! # use std::error::Error;
//! #
//! use monstertruck_traits::{
//!     BoundarySide, ContinuityOrder, SurfaceContinuityCapability,
//!     UnsupportedContinuityCapability,
//! };
//!
//! # fn main() -> Result<(), Box<dyn Error>> {
//! let order = ContinuityOrder::new(3)?;
//! let capability = SurfaceContinuityCapability::try_unsupported(
//!     BoundarySide::MinU,
//!     order,
//!     UnsupportedContinuityCapability::InsufficientDegree {
//!         available: 2,
//!         required: 3,
//!     },
//!     Some(ContinuityOrder::G2),
//! )?;
//!
//! assert_eq!(capability.side(), BoundarySide::MinU);
//! assert_eq!(capability.maximum_supported_order(), Some(ContinuityOrder::G2));
//! assert!(matches!(
//!     capability.unsupported_reason(),
//!     Some(UnsupportedContinuityCapability::InsufficientDegree { .. })
//! ));
//! assert!(ContinuityOrder::new(5).is_err());
//! assert!(ContinuityOrder::G4.is_experimental());
//! # Ok(())
//! # }
//! ```

#![cfg_attr(not(debug_assertions), deny(warnings))]
#![deny(clippy::all, rust_2018_idioms)]
#![warn(
    missing_docs,
    missing_debug_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]

#[macro_export]
#[doc(hidden)]
macro_rules! nonpositive_tolerance {
    ($tol: expr, $minimum: expr) => {
        assert!(
            $tol >= $minimum,
            "tolerance must be no less than {:e}",
            $minimum
        );
    };
    ($tol: expr) => {
        nonpositive_tolerance!($tol, TOLERANCE)
    };
}

/// Abstract traits: `Curve` and `Surface`.
pub mod traits;
pub use traits::*;
/// Checked continuity requests and capability diagnostics.
pub mod surface_continuity;
pub use surface_continuity::*;
/// Algorithms for curves and surfaces.
pub mod algo;
/// Scalar-generic v2 trait family.
pub mod v2;
#[cfg(feature = "derive")]
pub use monstertruck_derive::{
    BoundedCurve, BoundedSurface, Cut, Invertible, ParameterDivision1D, ParameterDivision2D,
    ParametricCurve, ParametricSurface, ParametricSurface3D, SearchNearestParameterD1,
    SearchNearestParameterD2, SearchParameterD1, SearchParameterD2, SelfSameGeometry,
    TransformedM3, TransformedM4,
};
#[cfg(feature = "polynomial")]
/// Implementation sample using polynomials as an example
pub mod polynomial;
