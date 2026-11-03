// Gated on BOTH features, not just `derive`. The body reaches
// `monstertruck_traits::polynomial` (via the glob import below), and that module
// is `#[cfg(feature = "polynomial")]` -- so `--features derive` alone failed to
// compile the whole test target. Found 2026-07-30 when a spec-012 gate that
// names this package could not build on master.
#![cfg(all(feature = "derive", feature = "polynomial"))]
#![allow(dead_code)]

use monstertruck_core::{cgmath64::*, hash::HashGen};
use monstertruck_traits::*;
use polynomial::{PolynomialCurve, PolynomialSurface};

#[test]
fn derive_build_test_is_running() {}

#[derive(Clone, Debug, ParametricCurve, BoundedCurve, ParameterDivision1D)]
enum DerivedCurve<P>
where
    P: EuclideanSpace<Scalar = f64> + MetricSpace<Metric = f64> + HashGen<f64>,
    P::Diff: std::fmt::Debug, {
    CurveA(PolynomialCurve<P>),
    CurveB { polycurve: PolynomialCurve<P> },
}

#[derive(Clone, Debug, ParametricSurface, BoundedSurface, ParameterDivision2D)]
enum DeriveSurface<P>
where
    P: EuclideanSpace<Scalar = f64> + MetricSpace<Metric = f64> + HashGen<f64>,
    P::Diff: std::fmt::Debug, {
    SurfaceA(PolynomialSurface<P>),
    SurfaceB { polysurface: PolynomialSurface<P> },
}

#[derive(Clone, Debug, ParametricCurve, BoundedCurve, ParameterDivision1D)]
struct TupledCurve<P>(PolynomialCurve<P>)
where
    P: EuclideanSpace<Scalar = f64> + MetricSpace<Metric = f64> + HashGen<f64>,
    P::Diff: std::fmt::Debug;

#[derive(Clone, Debug, ParametricSurface, BoundedSurface, ParameterDivision2D)]
struct TupledSurface<P>(PolynomialSurface<P>)
where
    P: EuclideanSpace<Scalar = f64> + MetricSpace<Metric = f64> + HashGen<f64>,
    P::Diff: std::fmt::Debug;
