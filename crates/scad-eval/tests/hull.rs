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
