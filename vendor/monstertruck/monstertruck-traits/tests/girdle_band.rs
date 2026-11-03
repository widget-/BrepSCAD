//! Girdle and apex parameter analysis on periodic surfaces.
//!
//! These exercise the public API from outside the crate, and keep clear of
//! `algo::surface`'s division-budget tests, which drain a process-global work
//! meter and already fail in a parallel run of the lib suite on their own.

use monstertruck_core::cgmath64::*;
use monstertruck_traits::algo::surface::{
    apex_parameter, girdle_axis_and_value, girdle_band_range,
};
use monstertruck_traits::*;
use std::f64::consts::TAU;

/// Ruled surface of revolution: radius tapers linearly from `base_radius`
/// at `v = 0` to `tip_radius` at `v = 1`, with `u` the lap angle.
#[derive(Clone, Debug)]
struct TaperedTube {
    base_radius: f64,
    tip_radius: f64,
}

impl ParametricSurface for TaperedTube {
    type Point = Point3;
    type Vector = Vector3;
    fn evaluate(&self, u: f64, v: f64) -> Point3 {
        let radius = self.base_radius + (self.tip_radius - self.base_radius) * v;
        Point3::new(radius * u.cos(), radius * u.sin(), v)
    }
    fn derivative_u(&self, _: f64, _: f64) -> Vector3 { Vector3::zero() }
    fn derivative_v(&self, _: f64, _: f64) -> Vector3 { Vector3::zero() }
    fn derivative_uu(&self, _: f64, _: f64) -> Vector3 { Vector3::zero() }
    fn derivative_uv(&self, _: f64, _: f64) -> Vector3 { Vector3::zero() }
    fn derivative_vv(&self, _: f64, _: f64) -> Vector3 { Vector3::zero() }
    fn derivative_mn(&self, _: usize, _: usize, _: f64, _: f64) -> Vector3 { Vector3::zero() }
    fn parameter_range(&self) -> (ParameterRange, ParameterRange) {
        use std::ops::Bound::Included;
        (
            (Included(0.0), Included(TAU)),
            (Included(0.0), Included(1.0)),
        )
    }
    fn period_u(&self) -> Option<f64> { Some(TAU) }
}

/// Radius 2 -> 1 across `v` in `[0, 1]` keeps shrinking to zero at `v = 2`.
#[test]
fn apex_parameter_extrapolates_a_linear_taper() {
    let cone = TaperedTube {
        base_radius: 2.0,
        tip_radius: 1.0,
    };

    let apex = apex_parameter(&cone, 1, 1.0e-6).expect("a tapering surface has an apex");

    assert!((apex - 2.0).abs() < 1.0e-9, "got {apex}");
}

#[test]
fn apex_parameter_refuses_a_cylinder() {
    let cylinder = TaperedTube {
        base_radius: 1.0,
        tip_radius: 1.0,
    };

    assert!(apex_parameter(&cylinder, 1, 1.0e-6).is_none());
}

#[test]
fn girdle_axis_and_value_finds_a_full_lap_at_constant_cross_parameter() {
    let cylinder = TaperedTube {
        base_radius: 1.0,
        tip_radius: 1.0,
    };
    let cap: Vec<(f64, f64)> = (0..=8)
        .map(|step| (TAU * step as f64 / 8.0, 0.25))
        .collect();

    let (axis, value) = girdle_axis_and_value(&cap, &cylinder, 1.0e-6)
        .expect("a full lap at constant v is a girdle");

    assert_eq!(axis, 0, "u is the periodic axis");
    assert!((value - 0.25).abs() < 1.0e-9, "got {value}");
}

#[test]
fn girdle_axis_and_value_rejects_a_partial_lap() {
    let cylinder = TaperedTube {
        base_radius: 1.0,
        tip_radius: 1.0,
    };
    let arc: Vec<(f64, f64)> = (0..=4)
        .map(|step| (TAU * step as f64 / 16.0, 0.25))
        .collect();

    assert!(girdle_axis_and_value(&arc, &cylinder, 1.0e-6).is_none());
}

#[test]
fn girdle_band_range_spans_between_two_girdles() {
    let cylinder = TaperedTube {
        base_radius: 1.0,
        tip_radius: 1.0,
    };

    let (axis, (low, high)) = girdle_band_range(&[(0, 0.25), (0, 0.75)], &cylinder, 1.0e-6)
        .expect("two girdles bound a band");

    assert_eq!(axis, 0);
    assert!((low - 0.25).abs() < 1.0e-9 && (high - 0.75).abs() < 1.0e-9);
}

/// A lone girdle bounds the patch that closes at the apex, which for this
/// taper sits outside the natural parameter range.
#[test]
fn girdle_band_range_reaches_the_apex_from_a_lone_girdle() {
    let cone = TaperedTube {
        base_radius: 2.0,
        tip_radius: 1.0,
    };

    let (_, (low, high)) =
        girdle_band_range(&[(0, 0.5)], &cone, 1.0e-6).expect("a lone girdle bounds the apex patch");

    assert!((low - 0.5).abs() < 1.0e-9, "got low {low}");
    assert!((high - 2.0).abs() < 1.0e-9, "got high {high}");
}
