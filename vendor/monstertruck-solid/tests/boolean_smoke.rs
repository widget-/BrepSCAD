//! Downstream-application smoke scenario: build primitive solids, combine them
//! with the boolean operators, and confirm the results tessellate into
//! non-degenerate watertight meshes.
//!
//! This mirrors how an interactive CAD app (e.g. an OpenCADStudio-style scene
//! graph) drives the kernel: a handful of primitives fed through
//! `union`/`intersection`/`difference`, then meshed for display. It is a
//! coarse "does the pipeline stay closed end to end" check, not a numerical
//! accuracy test -- the `transversal::integrate` unit tests own the exact
//! values.
//!
//! Note: `monstertruck_solid::{and, or}` build their output through
//! `Solid::try_new`, which only returns `Ok` when every boundary shell is a
//! closed manifold. So a returned `Solid` is watertight by construction; these
//! tests additionally confirm it tessellates to a non-empty mesh whose volume
//! matches the analytic result. Because every primitive here is flat-faced, the
//! tessellated volume is exact (independent of mesh density), so an exact-value
//! assertion is both legitimate and a far stronger regression guard than a
//! "non-empty output" smoke check -- it would catch a boolean that silently
//! kept the wrong region.

use anyhow::Result;
use monstertruck_meshing::prelude::*;
use monstertruck_modeling::*;

const TOL: f64 = 0.05;
const VOLUME_EPS: f64 = 1.0e-3;

/// Axis-aligned unit cube with its minimum corner at `origin`.
fn unit_cube(origin: Point3) -> Solid {
    let v = builder::vertex(origin);
    let e = builder::extrude(&v, Vector3::unit_x());
    let f = builder::extrude(&e, Vector3::unit_y());
    builder::extrude(&f, Vector3::unit_z())
}

/// Square-section vertical column centered on `(cx, cy)`, tall enough to pierce
/// a unit cube standing on `z = 0`.
fn square_column(cx: f64, cy: f64, half: f64) -> Solid {
    let v = builder::vertex(Point3::new(cx - half, cy - half, -0.5));
    let e = builder::extrude(&v, Vector3::unit_x() * (2.0 * half));
    let f = builder::extrude(&e, Vector3::unit_y() * (2.0 * half));
    builder::extrude(&f, Vector3::unit_z() * 2.0)
}

/// Asserts a boolean result is watertight and geometrically correct: it has at
/// least one boundary shell, tessellates to a non-empty mesh, and that mesh has
/// the expected (positively oriented) volume.
fn assert_solid(label: &str, solid: &Solid, expected_volume: f64) -> Result<()> {
    anyhow::ensure!(
        !solid.boundaries().is_empty(),
        "{label}: solid has no boundary shells"
    );
    let mesh = solid.triangulation(0.01).to_polygon();
    let triangles = mesh.faces().triangle_iter().count();
    anyhow::ensure!(triangles > 0, "{label}: tessellation produced no triangles");
    let volume = mesh.volume();
    anyhow::ensure!(
        (volume - expected_volume).abs() < VOLUME_EPS,
        "{label}: tessellated volume {volume:.6}, expected {expected_volume:.6}"
    );
    Ok(())
}

#[test]
fn union_of_overlapping_cubes() -> Result<()> {
    // Two unit cubes overlapping in a `0.5` cube: `2 - 0.5^3 = 1.875`.
    let a = unit_cube(Point3::origin());
    let b = unit_cube(Point3::new(0.5, 0.5, 0.5));
    let result = monstertruck_solid::or(&a, &b, TOL)?;
    assert_solid("union of overlapping cubes", &result, 1.875)
}

#[test]
fn intersection_of_overlapping_cubes() -> Result<()> {
    // The shared `0.5` cube: `0.5^3 = 0.125`.
    let a = unit_cube(Point3::origin());
    let b = unit_cube(Point3::new(0.5, 0.5, 0.5));
    let result = monstertruck_solid::and(&a, &b, TOL)?;
    assert_solid("intersection of overlapping cubes", &result, 0.125)
}

#[test]
fn difference_of_overlapping_cubes() -> Result<()> {
    // First cube minus the shared region: `1 - 0.125 = 0.875`.
    let a = unit_cube(Point3::origin());
    let b = unit_cube(Point3::new(0.5, 0.5, 0.5));
    let result = monstertruck_solid::difference(&a, &b, TOL)?;
    assert_solid("difference of overlapping cubes", &result, 0.875)
}

#[test]
fn cube_minus_column() -> Result<()> {
    // Unit cube with a `0.4 x 0.4` column punched through: `1 - 0.4^2 = 0.84`.
    let cube = unit_cube(Point3::origin());
    let column = square_column(0.5, 0.5, 0.2);
    let result = monstertruck_solid::difference(&cube, &column, TOL)?;
    assert_solid("cube minus square column", &result, 0.84)
}
