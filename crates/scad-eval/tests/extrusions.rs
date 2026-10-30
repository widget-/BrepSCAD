mod common;

use brepscad_eval::{EvalError, evaluate, evaluate_to_meshes};
use brepscad_kernel_truck::TruckKernel;
use common::{bounds, signed_volume};

#[test]
fn evaluates_top_level_sketches() {
    let mut kernel = TruckKernel::new();

    let square = evaluate("square([2, 3]);", &mut kernel, 0.01).expect("square");
    let shape = &square.shapes[0];
    let (min, max) = bounds(&shape.mesh);
    assert!((min[0] + 1.0).abs() < 1e-6, "min = {min:?}");
    assert!((max[0] - 1.0).abs() < 1e-6, "max = {max:?}");
    assert!((min[1] + 1.5).abs() < 1e-6);
    assert!((max[1] - 1.5).abs() < 1e-6);
    assert!(min[2].abs() < 1e-6 && max[2].abs() < 1e-6);
    assert_eq!(shape.parts.len(), 1);
    assert_eq!(shape.parts[0].module, "square");

    let polygon =
        evaluate("polygon([[0, 0], [2, 0], [0, 2]]);", &mut kernel, 0.01).expect("polygon");
    let (min, max) = bounds(&polygon.shapes[0].mesh);
    assert!((min[0] - 0.0).abs() < 1e-6);
    assert!((max[0] - 2.0).abs() < 1e-6);
    assert!((max[1] - 2.0).abs() < 1e-6);
}

#[test]
fn extrudes_a_square() {
    let source = "linear_extrude(2) square([2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    let (min, max) = bounds(&shape.mesh);
    assert!((min[0] + 1.0).abs() < 1e-6);
    assert!((max[0] - 1.0).abs() < 1e-6);
    assert!((min[2] + 1.0).abs() < 1e-6);
    assert!((max[2] - 1.0).abs() < 1e-6);

    let volume = signed_volume(&shape.mesh);
    assert!((volume - 8.0).abs() < 0.01, "volume = {volume}");
    assert_eq!(shape.parts.len(), 1);
    assert_eq!(shape.parts[0].module, "linear_extrude");
}

#[test]
fn transforms_apply_inside_and_outside_extrusion() {
    let source = "translate([0, 0, 5]) linear_extrude(2) translate([1, 0, 0]) square([2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!((min[0] - 0.0).abs() < 1e-6, "min_x = {}", min[0]);
    assert!((max[0] - 2.0).abs() < 1e-6, "max_x = {}", max[0]);
    assert!((min[2] - 4.0).abs() < 1e-6, "min_z = {}", min[2]);
    assert!((max[2] - 6.0).abs() < 1e-6, "max_z = {}", max[2]);
}

#[test]
fn extrudes_a_circle() {
    let source = "linear_extrude(height = 2) circle(r = 1);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!((min[2] + 1.0).abs() < 1e-6);
    assert!((max[2] - 1.0).abs() < 1e-6);
    let volume = signed_volume(&evaluation.shapes[0].mesh);
    let expected = 2.0 * std::f64::consts::PI;
    assert!(
        (volume - expected).abs() < expected * 0.02,
        "volume = {volume}"
    );
}

#[test]
fn rejects_mixed_dimensions_and_2d_csg() {
    let mut kernel = TruckKernel::new();

    let error = evaluate_to_meshes("linear_extrude(2) cube(1);", &mut kernel, 0.01)
        .expect_err("3D extrusion child");
    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "linear_extrude"));

    let error = evaluate_to_meshes("difference() { square(2); square(1); }", &mut kernel, 0.01)
        .expect_err("2D boolean");
    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "difference"));

    let error = evaluate_to_meshes(
        "translate([0, 0, 0]) { square(1); square(2); }",
        &mut kernel,
        0.01,
    )
    .expect_err("2D union");
    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "translate"));
}
