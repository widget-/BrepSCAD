mod common;

use brepscad_eval::{EvalError, Evaluation, evaluate};
use brepscad_kernel_api::Mesh;
use brepscad_kernel_truck::TruckKernel;
use common::bounds;

fn extents(mesh: &Mesh) -> [f64; 3] {
    let (min, max) = bounds(mesh);
    [max[0] - min[0], max[1] - min[1], max[2] - min[2]]
}

fn evaluate_source(source: &str) -> Evaluation {
    let mut kernel = TruckKernel::new();
    evaluate(source, &mut kernel, 0.01).expect("evaluate")
}

fn cube_extents(source: &str) -> [f64; 3] {
    let evaluation = evaluate_source(source);
    assert_eq!(evaluation.shapes.len(), 1);
    extents(&evaluation.shapes[0].mesh)
}

fn source_error(source: &str) -> EvalError {
    let mut kernel = TruckKernel::new();
    evaluate(source, &mut kernel, 0.01).expect_err("expected an error")
}

#[test]
fn function_literals_evaluate_and_bind_arguments() {
    assert_eq!(
        cube_extents("f = function(x) x * 2;\ncube(f(3));"),
        [6.0; 3]
    );
}

#[test]
fn function_literals_support_defaults_and_named_arguments() {
    assert_eq!(
        cube_extents(
            "f = function(a, b = 2) a + b;\n\
             cube([f(3), f(3, b = 4), f(a = 1, b = 1)]);",
        ),
        [5.0, 7.0, 2.0]
    );
}

#[test]
fn function_literal_defaults_reference_earlier_parameters() {
    assert_eq!(
        cube_extents("f = function(a, b = a + 1) a + b;\ncube(f(2));"),
        [5.0; 3]
    );
}

#[test]
fn function_literals_capture_their_defining_scope() {
    assert_eq!(
        cube_extents(
            "function make(a) = function(x) x + a;\n\
             f = make(10);\n\
             cube(f(1));",
        ),
        [11.0; 3]
    );
}

#[test]
fn function_literals_are_lexical_not_dynamic() {
    assert_eq!(
        cube_extents(
            "a = 10;\n\
             f = function(x) x + a;\n\
             module m() { a = 20; cube(f(1)); }\n\
             m();",
        ),
        [11.0; 3]
    );
}

#[test]
fn function_literals_see_hoisted_assignment_values() {
    assert_eq!(
        cube_extents("a = 10;\nf = function(x) x + a;\na = 20;\ncube(f(1));"),
        [21.0; 3]
    );
}

#[test]
fn function_literals_can_recurse() {
    assert_eq!(
        cube_extents("f = function(n) n <= 1 ? 1 : n * f(n - 1);\ncube(f(5));"),
        [120.0; 3]
    );
}

#[test]
fn function_values_can_be_returned_and_called_immediately() {
    assert_eq!(
        cube_extents("f = function(x) function(y) x + y;\ncube(f(2)(3));"),
        [5.0; 3]
    );
}

#[test]
fn function_values_can_be_passed_to_functions() {
    assert_eq!(
        cube_extents(
            "function apply(f, x) = f(x);\n\
             cube(apply(function(y) y + 1, 2));",
        ),
        [3.0; 3]
    );
}

#[test]
fn function_values_can_live_in_lists() {
    assert_eq!(
        cube_extents("v = [function(x) x * 2, 3];\ncube(v[0](v[1]));"),
        [6.0; 3]
    );
}

#[test]
fn function_literals_with_no_parameters_work() {
    assert_eq!(cube_extents("f = function() 5;\ncube(f());"), [5.0; 3]);
}

#[test]
fn is_function_recognizes_function_values() {
    assert_eq!(
        cube_extents("cube(is_function(function(x) x) && !is_function(sin) ? 1 : 2);"),
        [1.0; 3]
    );
}

#[test]
fn str_formats_function_literals_as_source() {
    let evaluation = evaluate_source("f = function(x) x;\necho(str(f));\ncube(1);");

    assert_eq!(evaluation.messages, ["ECHO: \"function(x) x\""]);
}

#[test]
fn function_value_argument_errors_are_reported() {
    let too_many = source_error("f = function(x) x;\ncube(f(1, 2));");
    assert!(matches!(too_many, EvalError::BadArguments { module, .. } if module == "function"));

    let unknown_named = source_error("f = function(x) x;\ncube(f(y = 1));");
    assert!(
        matches!(unknown_named, EvalError::BadArguments { module, .. } if module == "function")
    );
}

#[test]
fn invoking_a_non_function_value_is_an_error() {
    let error = source_error("x = 1;\ncube(x(2));");

    assert!(matches!(error, EvalError::UnknownFunction(name) if name == "x"));
}
