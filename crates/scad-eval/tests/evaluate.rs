use visual_openscad_kernel_truck::TruckKernel;
use visual_openscad_scad_eval::{EvalError, evaluate_to_meshes};

#[test]
fn evaluates_a_cube() {
    let mut kernel = TruckKernel::new();
    let meshes = evaluate_to_meshes("cube([2, 3, 4]);", &mut kernel, 0.01).expect("evaluate");

    assert_eq!(meshes.len(), 1);
    assert!(meshes[0].triangle_count() > 0);
}

#[test]
fn evaluates_a_cube_with_a_hole() {
    let source = "size = [2, 2, 2];\ndifference() {\n    cube(size);\n    cylinder(0.5, 4);\n}\n";
    let mut kernel = TruckKernel::new();
    let meshes = evaluate_to_meshes(source, &mut kernel, 0.01).expect("evaluate");

    assert_eq!(meshes.len(), 1);
    assert!(meshes[0].triangle_count() > 0);
}

#[test]
fn reports_unknown_identifiers() {
    let mut kernel = TruckKernel::new();
    let error = evaluate_to_meshes("cube(size);", &mut kernel, 0.01).expect_err("unknown size");

    assert!(matches!(error, EvalError::UnknownIdentifier(name) if name == "size"));
}

#[test]
fn reports_unsupported_modules() {
    let mut kernel = TruckKernel::new();
    let error = evaluate_to_meshes("sphere(1);", &mut kernel, 0.01).expect_err("unsupported");

    assert!(matches!(error, EvalError::UnsupportedModule(name) if name == "sphere"));
}

#[test]
fn reports_parse_errors() {
    let mut kernel = TruckKernel::new();
    let error = evaluate_to_meshes("cube([2, 2;", &mut kernel, 0.01).expect_err("parse error");

    assert!(matches!(error, EvalError::Parse(_)));
}
