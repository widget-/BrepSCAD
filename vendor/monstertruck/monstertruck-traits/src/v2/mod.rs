//! Scalar-generic v2 trait family.
//!
//! These traits are the scalar-generic future of the monstertruck geometry
//! kernel. They mirror the existing `f64`-hardcoded traits in [`crate::traits`]
//! but parameterize the scalar type via associated types.
//!
//! The default (and currently only) scalar is `f64`. Alternate scalar support
//! is not yet exposed.
//!
//! # Phase 0
//!
//! This module is scaffolding only. No existing code is modified. The
//! `compat` module provides blanket adapters so that any type implementing
//! the old `f64` traits automatically implements the v2 traits with
//! `Scalar = f64`.

mod bounded_curve;
mod bounded_surface;
mod curve;
mod curve3d;
mod cut;
mod division;
mod parameter_boundary;
mod search_parameter;
mod surface;
mod surface3d;

mod compat;

/// Scalar-generic algorithm helpers.
pub mod algo;

pub use bounded_curve::BoundedCurve;
pub use bounded_surface::BoundedSurface;
pub use curve::ParametricCurve;
pub use curve3d::ParametricCurve3D;
pub use cut::Cut;
pub use division::{ParameterDivision1D, ParameterDivision2D, SurfaceParameterRange};
pub use parameter_boundary::ParameterBoundary2D;
pub use search_parameter::{
    CurveParameter, ParameterRange, SearchNearestParameter, SearchParameter,
    SearchParameterDimension, SearchParameterHint1D, SearchParameterHint2D, SurfaceParameter,
};
#[allow(deprecated)]
pub use search_parameter::{D1, D2};
pub use surface::ParametricSurface;
pub use surface3d::ParametricSurface3D;
