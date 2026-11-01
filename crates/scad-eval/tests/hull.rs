mod common;

use brepscad_eval::evaluate;
use brepscad_kernel_truck::TruckKernel;
use common::{bounds, signed_volume};

fn evaluate_source(source: &str) -> brepscad_eval::Evaluation {
    let mut kernel = TruckKernel::new();
    evaluate(source, &mut kernel, 0.01).expect("evaluate")
}

fn extents_and_volume(source: &str) -> ([f64; 3], f64) {
    let evaluation = evaluate_source(source);
    assert_eq!(evaluation.shapes.len(), 1);
    let shape = &evaluation.shapes[0];
    let (min, max) = bounds(&shape.mesh);
    (
        [max[0] - min[0], max[1] - min[1], max[2] - min[2]],
        signed_volume(&shape.mesh),
    )
}

#[test]
fn hull_builds_the_convex_hull_of_its_children() {
    let (extents, volume) = extents_and_volume(
        "hull() { translate([-2, 0, 0]) cube(1); translate([2, 0, 0]) cube(1); }",
    );

    assert_eq!(extents, [5.0, 1.0, 1.0]);
    assert!((volume - 5.0).abs() < 1.0e-6, "volume = {volume}");
}

#[test]
fn minkowski_sums_convex_children() {
    let (extents, volume) =
        extents_and_volume("minkowski() { cube(1); translate([2, 0, 0]) cube(1); }");

    assert_eq!(extents, [2.0, 2.0, 2.0]);
    assert!((volume - 8.0).abs() < 1.0e-6, "volume = {volume}");
}

#[test]
fn minkowski_with_one_child_is_the_identity() {
    let (extents, volume) = extents_and_volume("minkowski() cube(1);");

    assert_eq!(extents, [1.0, 1.0, 1.0]);
    assert!((volume - 1.0).abs() < 1.0e-6, "volume = {volume}");
}

#[test]
fn hull_with_one_child_convexifies_it() {
    let (extents, volume) = extents_and_volume("hull() cube(2);");

    assert_eq!(extents, [2.0, 2.0, 2.0]);
    assert!((volume - 8.0).abs() < 1.0e-6, "volume = {volume}");
}

#[test]
fn hull_of_a_polytope_uses_its_topology() {
    // Eight vertices in, twelve triangles out: no tessellation of the cube's
    // faces, which is what the old path produced.
    let evaluation = evaluate_source("hull() cube(2);");
    let mesh = &evaluation.shapes[0].mesh;
    assert_eq!(
        mesh.triangle_count(),
        12,
        "hull should be the exact polytope"
    );
}

#[test]
fn minkowski_of_polytopes_is_exact_and_coarse() {
    let evaluation = evaluate_source("minkowski() { cube(1); translate([2, 0, 0]) cube(1); }");
    let mesh = &evaluation.shapes[0].mesh;
    let (min, max) = bounds(mesh);
    assert_eq!(
        [max[0] - min[0], max[1] - min[1], max[2] - min[2]],
        [2.0, 2.0, 2.0]
    );
    assert!(
        (signed_volume(mesh) - 8.0).abs() < 1.0e-6,
        "volume = {}",
        signed_volume(mesh)
    );
    // Every vertex lies on the box surface: the sum came from topology
    // vertices, so nothing interior or tessellated leaked in. (Pairwise sums
    // of the two cubes' corners also land on edges and faces, which the hull
    // keeps as redundant vertices.)
    assert!(
        mesh.positions.iter().all(|position| position
            .iter()
            .any(|axis| (axis.abs() - 1.0).abs() < 1.0e-9)),
        "unexpected interior vertex in {:?}",
        mesh.positions
    );
}

#[test]
fn minkowski_with_a_curved_operand_still_sums() {
    // A cylinder is not a polytope, so this still goes through the tessellated
    // path; it must keep working.
    let (extents, volume) =
        extents_and_volume("minkowski() { cube(1); cylinder(r=0.5, h=1, $fn=16); }");
    assert!((extents[0] - 2.0).abs() < 0.05, "extents = {extents:?}");
    assert!((extents[1] - 2.0).abs() < 0.05, "extents = {extents:?}");
    assert!((extents[2] - 2.0).abs() < 0.05, "extents = {extents:?}");
    assert!(volume > 7.0, "volume = {volume}");
}
