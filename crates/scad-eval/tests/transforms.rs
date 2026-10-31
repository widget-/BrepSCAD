mod common;

use brepscad_eval::{EvalError, evaluate, evaluate_to_meshes};
use brepscad_kernel_truck::TruckKernel;
use common::bounds;

#[test]
fn evaluates_translated_cube_and_exposes_transform_parameters() {
    let source = "translate([1, 0, 0]) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    let min_x = shape
        .mesh
        .positions
        .iter()
        .map(|position| position[0])
        .fold(f64::INFINITY, f64::min);
    assert!((min_x - 0.0).abs() < 1e-6, "min_x = {min_x}");

    let part = &shape.parts[0];
    assert_eq!(part.module, "cube");
    assert!(
        part.parameters
            .iter()
            .any(|parameter| parameter.value == 1.0)
    );
    assert!(
        part.parameters
            .iter()
            .any(|parameter| parameter.value == 2.0)
    );
    assert!(source[part.source_range.clone()].starts_with("cube"));
}

#[test]
fn evaluates_nested_transforms() {
    let source = "translate([1, 0, 0]) rotate([0, 0, 90]) cube([2, 1, 1]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    assert!(shape.mesh.triangle_count() > 0);
    assert!(
        shape.parts[0]
            .parameters
            .iter()
            .any(|parameter| parameter.value == 90.0)
    );
}

#[test]
fn transforms_each_child_in_a_block() {
    let source = "translate([1, 0, 0]) { cube(1); translate([2, 0, 0]) cube(1); }\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    assert_eq!(shape.parts.len(), 2);
    assert!(shape.mesh.triangle_count() > 0);
    for part in &shape.parts {
        assert!(
            part.parameters
                .iter()
                .any(|parameter| parameter.value == 1.0)
        );
        assert!(!part.mesh.positions.is_empty());
    }
}

#[test]
fn multmatrix_applies_open_scad_row_order() {
    let source = "multmatrix([[1, 0, 0, 5], [0, 1, 0, 0], [0, 0, 1, 0]]) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let min_x = evaluation.shapes[0]
        .mesh
        .positions
        .iter()
        .map(|position| position[0])
        .fold(f64::INFINITY, f64::min);
    assert!((min_x - 4.0).abs() < 1e-6, "min_x = {min_x}");
}

#[test]
fn multmatrix_accepts_a_matrix_assignment() {
    let source =
        "m = [[1, 0, 0, 5], [0, 1, 0, 0], [0, 0, 1, 0]];\nmultmatrix(m) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let min_x = evaluation.shapes[0]
        .mesh
        .positions
        .iter()
        .map(|position| position[0])
        .fold(f64::INFINITY, f64::min);
    assert!((min_x - 4.0).abs() < 1e-6, "min_x = {min_x}");
}

#[test]
fn multmatrix_forces_the_fourth_row() {
    let source =
        "multmatrix([[1, 0, 0, 5], [0, 1, 0, 0], [0, 0, 1, 0], [9, 9, 9, 9]]) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let min_x = evaluation.shapes[0]
        .mesh
        .positions
        .iter()
        .map(|position| position[0])
        .fold(f64::INFINITY, f64::min);
    assert!((min_x - 4.0).abs() < 1e-6, "min_x = {min_x}");
}

#[test]
fn mirror_reflects_geometry() {
    let source = "mirror([1, 0, 0]) translate([1, 0, 0]) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let min_x = evaluation.shapes[0]
        .mesh
        .positions
        .iter()
        .map(|position| position[0])
        .fold(f64::INFINITY, f64::min);
    assert!((min_x + 2.0).abs() < 1e-6, "min_x = {min_x}");
}

#[test]
fn rejects_singular_transforms() {
    let mut kernel = TruckKernel::new();
    let error = evaluate_to_meshes("scale([1, 0, 1]) cube([2, 2, 2]);", &mut kernel, 0.01)
        .expect_err("singular scale");

    assert!(matches!(error, EvalError::InvalidTransform(_)));
}

#[test]
fn rejects_bad_multmatrix_shapes() {
    let mut kernel = TruckKernel::new();
    let error = evaluate_to_meshes(
        "multmatrix([[1, 0], [0, 1]]) cube([2, 2, 2]);",
        &mut kernel,
        0.01,
    )
    .expect_err("bad matrix");

    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "multmatrix"));
}

#[test]
fn resize_matches_the_requested_bounding_box() {
    let source = "resize([4, 2, 0]) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    let (min, max) = bounds(&shape.mesh);
    assert!(
        (max[0] - min[0] - 4.0).abs() < 1e-6,
        "x = {}",
        max[0] - min[0]
    );
    assert!(
        (max[1] - min[1] - 2.0).abs() < 1e-6,
        "y = {}",
        max[1] - min[1]
    );
    assert!(
        (max[2] - min[2] - 2.0).abs() < 1e-6,
        "z = {}",
        max[2] - min[2]
    );

    let (part_min, part_max) = bounds(&shape.parts[0].mesh);
    assert!((part_max[0] - part_min[0] - 4.0).abs() < 1e-6);
    assert!((part_max[1] - part_min[1] - 2.0).abs() < 1e-6);
    assert!((part_max[2] - part_min[2] - 2.0).abs() < 1e-6);
}

#[test]
fn resize_is_applied_before_the_parent_transform() {
    let source = "translate([10, 0, 0]) resize([4, 2, 2]) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!((min[0] - 8.0).abs() < 1e-6, "min_x = {}", min[0]);
    assert!((max[0] - 12.0).abs() < 1e-6, "max_x = {}", max[0]);
    assert!((min[1] + 1.0).abs() < 1e-6, "min_y = {}", min[1]);
    assert!((max[1] - 1.0).abs() < 1e-6, "max_y = {}", max[1]);
}

#[test]
fn resize_uses_the_combined_child_bounds() {
    let source = "resize([4, 0, 0]) difference() { cube([2, 2, 2]); translate([0.2, 0, 0]) cylinder(h=4, r=0.5); }\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!(
        (max[0] - min[0] - 4.0).abs() < 1e-6,
        "x = {}",
        max[0] - min[0]
    );
}

#[test]
fn rejects_negative_resize_dimensions() {
    let mut kernel = TruckKernel::new();
    let error = evaluate_to_meshes("resize([-1, 2, 2]) cube([2, 2, 2]);", &mut kernel, 0.01)
        .expect_err("negative resize");

    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "resize"));
}

#[test]
fn evaluates_named_transform_arguments() {
    let source = "translate(v = [1, 0, 0]) rotate(a = [0, 0, 90]) cube(size = [2, 1, 1]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!(
        (max[0] - min[0] - 1.0).abs() < 1e-6,
        "x = {}",
        max[0] - min[0]
    );
    assert!(
        (max[1] - min[1] - 2.0).abs() < 1e-6,
        "y = {}",
        max[1] - min[1]
    );
}

#[test]
fn evaluates_a_named_multmatrix() {
    let source = "multmatrix(m = [[1, 0, 0, 5], [0, 1, 0, 0], [0, 0, 1, 0]]) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let (min, _) = bounds(&evaluation.shapes[0].mesh);
    assert!((min[0] - 4.0).abs() < 1e-6, "min_x = {}", min[0]);
}

#[test]
fn empty_transforms_emit_no_geometry() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "translate([1, 0, 0]);\nmultmatrix([[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0]]);\ncube(1);",
        &mut kernel,
        0.01,
    )
    .expect("empty transforms");

    assert_eq!(evaluation.shapes.len(), 1);
    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!((max[0] - min[0] - 1.0).abs() < 1e-6);
}
#[test]
fn translate_accepts_two_dimensional_vectors() {
    let mut kernel = TruckKernel::new();
    let evaluation =
        evaluate("translate([1, 2]) cube(1);", &mut kernel, 0.01).expect("2D translate");
    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!((min[0] - 0.5).abs() < 1e-6);
    assert!((min[1] - 1.5).abs() < 1e-6);
    assert!((max[2] - 0.5).abs() < 1e-6);
}
