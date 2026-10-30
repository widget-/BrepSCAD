use brepscad_eval::{EvalError, evaluate_to_meshes};
use brepscad_kernel_truck::TruckKernel;

#[test]
fn rejects_unknown_named_arguments() {
    let mut kernel = TruckKernel::new();
    let error =
        evaluate_to_meshes("cube(side = 2);", &mut kernel, 0.01).expect_err("unknown argument");

    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "cube"));
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
    let error = evaluate_to_meshes("hull() cube(1);", &mut kernel, 0.01).expect_err("unsupported");

    assert!(matches!(error, EvalError::UnsupportedModule(name) if name == "hull"));
}

#[test]
fn reports_parse_errors() {
    let mut kernel = TruckKernel::new();
    let error = evaluate_to_meshes("cube([2, 2;", &mut kernel, 0.01).expect_err("parse error");

    assert!(matches!(error, EvalError::Parse(_)));
}
