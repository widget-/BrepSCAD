mod common;

use common::bounds;
use visual_openscad_kernel_truck::TruckKernel;
use visual_openscad_scad_eval::{EvalError, evaluate, evaluate_to_meshes};

#[test]
fn evaluates_arithmetic_in_arguments() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate("cube([1 + 1, 2 * 2, 6 / 2]);", &mut kernel, 0.01).expect("evaluate");

    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!((max[0] - min[0] - 2.0).abs() < 1e-6);
    assert!((max[1] - min[1] - 4.0).abs() < 1e-6);
    assert!((max[2] - min[2] - 3.0).abs() < 1e-6);
}

#[test]
fn evaluates_arithmetic_in_assignments_and_vectors() {
    let source = "size = [1 + 1, (2 + 2), 6 / 2];\ntranslate([2 - 1, 0, 0] * 2) cube(size);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let (min, _) = bounds(&evaluation.shapes[0].mesh);
    assert!((min[0] - 1.0).abs() < 1e-6, "min_x = {}", min[0]);
}

#[test]
fn evaluates_arithmetic_in_named_arguments() {
    let mut kernel = TruckKernel::new();
    let evaluation =
        evaluate("cylinder(r = 1 / 2, h = 2 * 2);", &mut kernel, 0.01).expect("evaluate");

    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!((max[2] - min[2] - 4.0).abs() < 1e-6);
}

#[test]
fn applies_unary_minus_and_multiplication_precedence() {
    let mut kernel = TruckKernel::new();

    let moved =
        evaluate("translate([-2, 0, 0]) cube([2, 2, 2]);", &mut kernel, 0.01).expect("moved");
    let (min, _) = bounds(&moved.shapes[0].mesh);
    assert!((min[0] + 3.0).abs() < 1e-6, "min_x = {}", min[0]);

    let sized = evaluate("cube([2 + 2 * 2, 1, 1]);", &mut kernel, 0.01).expect("sized");
    let (min, max) = bounds(&sized.shapes[0].mesh);
    assert!((max[0] - min[0] - 6.0).abs() < 1e-6);
}

#[test]
fn rejects_division_by_zero() {
    let mut kernel = TruckKernel::new();
    let error =
        evaluate_to_meshes("cube([1 / 0, 1, 1]);", &mut kernel, 0.01).expect_err("division");

    assert!(matches!(error, EvalError::Expression(_)));
}
