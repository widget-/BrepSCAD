mod common;

use brepscad_eval::{EvaluationOptions, evaluate_with_options};
use brepscad_kernel_truck::TruckKernel;
use common::bounds;

fn evaluate(source: &str, legacy_centering: bool) -> brepscad_eval::Evaluation {
    let mut kernel = TruckKernel::new();
    evaluate_with_options(
        source,
        &mut kernel,
        EvaluationOptions {
            tolerance: 0.01,
            legacy_centering,
            library_paths: Vec::new(),
        },
    )
    .expect("evaluate")
}

#[test]
fn legacy_cubes_start_at_the_origin() {
    let centered = evaluate("cube([2, 3, 4]);", false);
    let (min, max) = bounds(&centered.shapes[0].mesh);
    assert!((min[0] + 1.0).abs() < 1.0e-6, "min = {min:?}");
    assert!((min[1] + 1.5).abs() < 1.0e-6, "min = {min:?}");
    assert!((min[2] + 2.0).abs() < 1.0e-6, "min = {min:?}");
    assert!((max[0] - 1.0).abs() < 1.0e-6, "max = {max:?}");

    let legacy = evaluate("cube([2, 3, 4]);", true);
    let (min, max) = bounds(&legacy.shapes[0].mesh);
    assert!(min[0].abs() < 1.0e-6, "min = {min:?}");
    assert!(min[1].abs() < 1.0e-6, "min = {min:?}");
    assert!(min[2].abs() < 1.0e-6, "min = {min:?}");
    assert!((max[0] - 2.0).abs() < 1.0e-6, "max = {max:?}");
    assert!((max[1] - 3.0).abs() < 1.0e-6, "max = {max:?}");
    assert!((max[2] - 4.0).abs() < 1.0e-6, "max = {max:?}");
}

#[test]
fn legacy_cylinders_start_at_z_zero() {
    let centered = evaluate("cylinder(r = 1, h = 2);", false);
    let (min, max) = bounds(&centered.shapes[0].mesh);
    assert!((min[2] + 1.0).abs() < 1.0e-6, "min = {min:?}");
    assert!((max[2] - 1.0).abs() < 1.0e-6, "max = {max:?}");

    let legacy = evaluate("cylinder(r = 1, h = 2);", true);
    let (min, max) = bounds(&legacy.shapes[0].mesh);
    assert!((min[2]).abs() < 1.0e-6, "min = {min:?}");
    assert!((max[2] - 2.0).abs() < 1.0e-6, "max = {max:?}");
    assert!((min[0] + 1.0).abs() < 1.0e-6, "min = {min:?}");
    assert!((max[0] - 1.0).abs() < 1.0e-6, "max = {max:?}");
}

#[test]
fn legacy_squares_start_at_the_origin() {
    let centered = evaluate("square([2, 3]);", false);
    let (min, _) = bounds(&centered.shapes[0].mesh);
    assert!((min[0] + 1.0).abs() < 1.0e-6, "min = {min:?}");
    assert!((min[1] + 1.5).abs() < 1.0e-6, "min = {min:?}");

    let legacy = evaluate("square([2, 3]);", true);
    let (min, max) = bounds(&legacy.shapes[0].mesh);
    assert!(min[0].abs() < 1.0e-6, "min = {min:?}");
    assert!(min[1].abs() < 1.0e-6, "min = {min:?}");
    assert!((max[0] - 2.0).abs() < 1.0e-6, "max = {max:?}");
    assert!((max[1] - 3.0).abs() < 1.0e-6, "max = {max:?}");
}

#[test]
fn legacy_linear_extrusions_start_at_z_zero() {
    let centered = evaluate("linear_extrude(2) square([2, 3]);", false);
    let (min, max) = bounds(&centered.shapes[0].mesh);
    assert!((min[2] + 1.0).abs() < 1.0e-6, "min = {min:?}");
    assert!((max[2] - 1.0).abs() < 1.0e-6, "max = {max:?}");

    let legacy = evaluate("linear_extrude(2) square([2, 3]);", true);
    let (min, max) = bounds(&legacy.shapes[0].mesh);
    assert!(min[0].abs() < 1.0e-6, "min = {min:?}");
    assert!(min[1].abs() < 1.0e-6, "min = {min:?}");
    assert!(min[2].abs() < 1.0e-6, "min = {min:?}");
    assert!((max[0] - 2.0).abs() < 1.0e-6, "max = {max:?}");
    assert!((max[1] - 3.0).abs() < 1.0e-6, "max = {max:?}");
    assert!((max[2] - 2.0).abs() < 1.0e-6, "max = {max:?}");
}

#[test]
fn legacy_linear_extrusion_center_override() {
    let centered = evaluate("linear_extrude(2, center = true) square([2, 3]);", true);
    let (min, max) = bounds(&centered.shapes[0].mesh);
    assert!((min[2] + 1.0).abs() < 1.0e-6, "min = {min:?}");
    assert!((max[2] - 1.0).abs() < 1.0e-6, "max = {max:?}");

    let based = evaluate("linear_extrude(2, center = false) square([2, 3]);", true);
    let (min, max) = bounds(&based.shapes[0].mesh);
    assert!(min[2].abs() < 1.0e-6, "min = {min:?}");
    assert!((max[2] - 2.0).abs() < 1.0e-6, "max = {max:?}");
}

#[test]
fn legacy_centering_evaluates_the_default_sample() {
    let source = "size = [2, 2, 2];\ndifference() {\n    cube(size);\n    translate([0.2, 0, 0]) cylinder(h=4, r=0.5);\n}\n";
    let evaluation = evaluate(source, true);

    assert_eq!(evaluation.shapes.len(), 1);
    assert!(evaluation.shapes[0].mesh.triangle_count() > 0);
}

#[test]
fn legacy_placement_composes_with_parent_transforms() {
    let legacy = evaluate("translate([10, 0, 0]) cube(2);", true);
    let (min, max) = bounds(&legacy.shapes[0].mesh);
    assert!((min[0] - 10.0).abs() < 1.0e-6, "min = {min:?}");
    assert!((max[0] - 12.0).abs() < 1.0e-6, "max = {max:?}");
}

#[test]
fn legacy_centering_leaves_centered_circles_alone() {
    let centered = evaluate("circle(1);", false);
    let legacy = evaluate("circle(1);", true);
    let (centered_min, centered_max) = bounds(&centered.shapes[0].mesh);
    let (legacy_min, legacy_max) = bounds(&legacy.shapes[0].mesh);

    assert_eq!(centered_min, legacy_min);
    assert_eq!(centered_max, legacy_max);
}
