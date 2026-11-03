//! Unit tests for the parent module.
//!
//! Split out of the module file so the source stays readable; the module
//! path is unchanged.

use super::*;
use monstertruck_core::cgmath64::{Point3, Vector3, Zero};

#[test]
fn cloned_without_trims_matches_clone_then_erase_trims() {
    let shell = CompressedTrimmedShell {
        vertices: vec![0usize, 1usize],
        edges: vec![CompressedEdge {
            vertices: (0, 1),
            curve: 5usize,
        }],
        faces: vec![CompressedTrimmedFace {
            boundaries: vec![vec![CompressedEdgeUse {
                index: 0,
                orientation: true,
                trim_curve: Some(7usize),
            }]],
            orientation: true,
            surface: (),
        }],
    };

    assert_eq!(shell.cloned_without_trims(), shell.clone().erase_trims());
}

/// Rectangular planar patch over `[0, 1]^2`, used to check that a shell with
/// no vertices or edges still yields a usable bounding box.
#[derive(Clone, Debug)]
struct TestPatch;

impl ParametricSurface for TestPatch {
    type Point = Point3;
    type Vector = Vector3;
    fn evaluate(&self, u: f64, v: f64) -> Point3 { Point3::new(u * 10.0, v * 20.0, 0.0) }
    fn derivative_u(&self, _: f64, _: f64) -> Vector3 { Vector3::zero() }
    fn derivative_v(&self, _: f64, _: f64) -> Vector3 { Vector3::zero() }
    fn derivative_uu(&self, _: f64, _: f64) -> Vector3 { Vector3::zero() }
    fn derivative_uv(&self, _: f64, _: f64) -> Vector3 { Vector3::zero() }
    fn derivative_vv(&self, _: f64, _: f64) -> Vector3 { Vector3::zero() }
    fn derivative_mn(&self, _: usize, _: usize, _: f64, _: f64) -> Vector3 { Vector3::zero() }
    fn parameter_range(&self) -> (ParameterRange, ParameterRange) {
        use std::ops::Bound::Included;
        (
            (Included(0.0), Included(1.0)),
            (Included(0.0), Included(1.0)),
        )
    }
}

/// Straight segment; only needed so the shell's curve type satisfies the
/// bound on `bounding_box`. The untrimmed cases carry no edges.
#[derive(Clone, Debug)]
struct TestSegment;

impl ParametricCurve for TestSegment {
    type Point = Point3;
    type Vector = Vector3;
    fn evaluate(&self, t: f64) -> Point3 { Point3::new(t, 0.0, 0.0) }
    fn derivative(&self, _: f64) -> Vector3 { Vector3::zero() }
    fn derivative_2(&self, _: f64) -> Vector3 { Vector3::zero() }
    fn derivative_n(&self, _: usize, _: f64) -> Vector3 { Vector3::zero() }
    fn parameter_range(&self) -> ParameterRange {
        use std::ops::Bound::Included;
        (Included(0.0), Included(1.0))
    }
}

fn untrimmed_patch_shell() -> CompressedTrimmedShell<Point3, TestSegment, TestPatch, ()> {
    CompressedTrimmedShell {
        vertices: Vec::new(),
        edges: Vec::new(),
        faces: vec![CompressedTrimmedFace {
            boundaries: Vec::new(),
            orientation: true,
            surface: TestPatch,
        }],
    }
}

#[test]
fn bounding_box_falls_back_to_surfaces_without_vertices_or_edges() {
    let bdd_box = untrimmed_patch_shell().bounding_box();

    assert!(
        !bdd_box.is_empty(),
        "untrimmed shell must still produce a bounding box"
    );
    assert_eq!(bdd_box.min(), Point3::new(0.0, 0.0, 0.0));
    assert_eq!(bdd_box.max(), Point3::new(10.0, 20.0, 0.0));
}

#[test]
fn bounding_box_prefers_topology_over_surface_parameter_rectangle() {
    let mut shell = untrimmed_patch_shell();
    shell.vertices = vec![Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 1.0, 1.0)];

    let bdd_box = shell.bounding_box();

    assert_eq!(bdd_box.min(), Point3::new(0.0, 0.0, 0.0));
    assert_eq!(
        bdd_box.max(),
        Point3::new(1.0, 1.0, 1.0),
        "a trimmed shell must not be inflated to the surface parameter rectangle"
    );
}

#[test]
fn relative_tolerance_scales_with_shell_size() {
    // Patch spans 10 x 20, so the diagonal is sqrt(500).
    let diameter = 500.0_f64.sqrt();
    let tolerance = untrimmed_patch_shell().relative_tolerance(1.0e-3);

    assert!(
        (tolerance - diameter * 1.0e-3).abs() < 1.0e-12,
        "expected {} got {tolerance}",
        diameter * 1.0e-3
    );
}

#[test]
fn relative_tolerance_never_falls_below_the_floor() {
    let empty = CompressedTrimmedShell::<Point3, TestSegment, TestPatch, ()> {
        vertices: Vec::new(),
        edges: Vec::new(),
        faces: Vec::new(),
    };

    assert_eq!(
        empty.relative_tolerance(1.0e-3),
        TOLERANCE,
        "a shell with no geometry must clamp to the tolerance floor",
    );
}

#[test]
fn bounding_box_with_maps_every_sampled_point() {
    let offset = Vector3::new(100.0, 200.0, 300.0);
    let bdd_box = untrimmed_patch_shell().bounding_box_with(|point| point + offset);

    // The mapping has to reach the surface-fallback tier too, not just the
    // vertex/edge passes.
    assert_eq!(bdd_box.min(), Point3::new(100.0, 200.0, 300.0));
    assert_eq!(bdd_box.max(), Point3::new(110.0, 220.0, 300.0));
}

#[test]
fn bounding_box_with_maps_vertices_and_edges() {
    let mut shell = untrimmed_patch_shell();
    shell.vertices = vec![Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 1.0, 1.0)];
    let offset = Vector3::new(5.0, 0.0, 0.0);

    let bdd_box = shell.bounding_box_with(|point| point + offset);

    assert_eq!(bdd_box.min(), Point3::new(5.0, 0.0, 0.0));
    assert_eq!(bdd_box.max(), Point3::new(6.0, 1.0, 1.0));
}

#[test]
fn bounding_box_is_bounding_box_with_identity() {
    let shell = untrimmed_patch_shell();

    assert_eq!(shell.bounding_box(), shell.bounding_box_with(|point| point),);
}
