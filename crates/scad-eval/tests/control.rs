mod common;

use brepscad_eval::evaluate;
use brepscad_kernel_truck::TruckKernel;
use common::bounds;

#[test]
fn for_emits_one_shape_per_iteration() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "for (i = [0:2:4]) translate([i, 0, 0]) cube(1);",
        &mut kernel,
        0.01,
    )
    .expect("for loop");

    assert_eq!(evaluation.shapes.len(), 3);
    let mins = evaluation
        .shapes
        .iter()
        .map(|shape| bounds(&shape.mesh).0[0])
        .collect::<Vec<_>>();
    assert!((mins[0] + 0.5).abs() < 1.0e-6, "mins = {mins:?}");
    assert!((mins[1] - 1.5).abs() < 1.0e-6, "mins = {mins:?}");
    assert!((mins[2] - 3.5).abs() < 1.0e-6, "mins = {mins:?}");
}

#[test]
fn for_supports_multiple_bindings() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "for (i = [0:1], j = [0:1]) translate([i * 2, j * 2, 0]) cube(1);",
        &mut kernel,
        0.01,
    )
    .expect("nested bindings");

    assert_eq!(evaluation.shapes.len(), 4);
}

#[test]
fn for_body_can_contain_multiple_statements() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "for (i = [0:1]) { translate([i * 2, 0, 0]) cube(1); cube(1); }",
        &mut kernel,
        0.01,
    )
    .expect("loop block");

    assert_eq!(evaluation.shapes.len(), 4);
}

#[test]
fn if_uses_conditions_and_else() {
    let mut kernel = TruckKernel::new();

    let truthy = evaluate(
        "if (1 < 2 && !false) cube(1); else cube(2);",
        &mut kernel,
        0.01,
    )
    .expect("truthy if");
    assert_eq!(truthy.shapes.len(), 1);
    let (min, max) = bounds(&truthy.shapes[0].mesh);
    assert!((max[0] - min[0] - 1.0).abs() < 1.0e-6);

    let falsy = evaluate("if (false) cube(1); else cube(2);", &mut kernel, 0.01).expect("false if");
    assert_eq!(falsy.shapes.len(), 1);
    let (min, max) = bounds(&falsy.shapes[0].mesh);
    assert!((max[0] - min[0] - 2.0).abs() < 1.0e-6);

    let silent = evaluate("if (0) cube(1);", &mut kernel, 0.01).expect("empty if");
    assert!(silent.shapes.is_empty());
}

#[test]
fn else_if_chain_selects_one_branch() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "if (false) cube(1); else if (true) cube(2); else cube(3);",
        &mut kernel,
        0.01,
    )
    .expect("else if");

    assert_eq!(evaluation.shapes.len(), 1);
    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!((max[0] - min[0] - 2.0).abs() < 1.0e-6);
}

#[test]
fn loop_variable_does_not_leak() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "i = 10; for (i = [0:1]) cube(1); cube(i);",
        &mut kernel,
        0.01,
    )
    .expect("loop scope");

    assert_eq!(evaluation.shapes.len(), 3);
    let (min, max) = bounds(&evaluation.shapes[2].mesh);
    assert!((max[0] - min[0] - 10.0).abs() < 1.0e-6);
}

#[test]
fn for_and_if_nest() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "for (i = [0:1]) if (i == 1) cube(2); else cube(1);",
        &mut kernel,
        0.01,
    )
    .expect("nested control");

    assert_eq!(evaluation.shapes.len(), 2);
    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!((max[0] - min[0] - 1.0).abs() < 1.0e-6);
    let (min, max) = bounds(&evaluation.shapes[1].mesh);
    assert!((max[0] - min[0] - 2.0).abs() < 1.0e-6);
}

#[test]
fn for_over_scalars_strings_and_undef_matches_open_scad() {
    let mut kernel = TruckKernel::new();
    let scalar = evaluate("for (i = 1) cube(1);", &mut kernel, 0.01).expect("scalar loop");
    assert_eq!(scalar.shapes.len(), 1);

    let missing = evaluate("for (i = undef) cube(1);", &mut kernel, 0.01).expect("undef loop");
    assert!(missing.shapes.is_empty());

    let text = evaluate("for (i = \"ab\") cube(1);", &mut kernel, 0.01).expect("string loop");
    assert_eq!(text.shapes.len(), 2);
}
