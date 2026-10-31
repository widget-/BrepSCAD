mod common;

use brepscad_eval::{EvalError, evaluate_to_meshes};
use brepscad_kernel_truck::TruckKernel;
use common::bounds;

#[test]
fn rejects_unknown_named_arguments() {
    let mut kernel = TruckKernel::new();
    let error =
        evaluate_to_meshes("cube(side = 2);", &mut kernel, 0.01).expect_err("unknown argument");

    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "cube"));
}

#[test]
fn unknown_identifiers_are_undef() {
    let mut kernel = TruckKernel::new();
    let meshes = evaluate_to_meshes("cube(is_undef(size) ? 2 : 1);", &mut kernel, 0.01)
        .expect("unknown identifier should be undef");

    let (min, max) = bounds(&meshes[0]);
    assert!((max[0] - min[0] - 2.0).abs() < 1.0e-6);
}

#[test]
fn reports_unsupported_modules() {
    let mut kernel = TruckKernel::new();
    let error =
        evaluate_to_meshes("projection() cube(1);", &mut kernel, 0.01).expect_err("unsupported");

    assert!(matches!(error, EvalError::UnsupportedModule(name) if name == "projection"));
}

#[test]
fn reports_parse_errors() {
    let mut kernel = TruckKernel::new();
    let error = evaluate_to_meshes("cube([2, 2;", &mut kernel, 0.01).expect_err("parse error");

    assert!(matches!(error, EvalError::Parse(_)));
}
