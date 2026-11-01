mod common;

use brepscad_eval::{EvalError, evaluate};
use brepscad_kernel_api::Mesh;
use brepscad_kernel_truck::TruckKernel;
use common::bounds;

fn extents(mesh: &Mesh) -> [f64; 3] {
    let (min, max) = bounds(mesh);
    [max[0] - min[0], max[1] - min[1], max[2] - min[2]]
}

fn cube_extents(source: &str) -> [f64; 3] {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    assert_eq!(evaluation.shapes.len(), 1);
    extents(&evaluation.shapes[0].mesh)
}

fn source_error(source: &str) -> EvalError {
    let mut kernel = TruckKernel::new();
    evaluate(source, &mut kernel, 0.01).expect_err("expected an error")
}

#[test]
fn functions_evaluate_and_return_values() {
    assert_eq!(
        cube_extents("function double(x) = x * 2;\ncube(double(2));"),
        [4.0; 3]
    );
}

#[test]
fn functions_accept_positional_and_named_arguments() {
    let size = cube_extents(
        "function volume(a, b, c) = a * b * c;\ncube([volume(2, 3, 4), volume(a = 1, b = 2, c = 3), 1]);",
    );

    assert_eq!(size, [24.0, 6.0, 1.0]);
}

#[test]
fn function_defaults_reference_earlier_parameters() {
    let size =
        cube_extents("function volume(a, b = a * 2) = a * b;\ncube([volume(2), volume(3, 4), 1]);");

    assert_eq!(size, [8.0, 12.0, 1.0]);
}

#[test]
fn functions_can_recurse() {
    assert_eq!(
        cube_extents("function fact(n) = n <= 1 ? 1 : n * fact(n - 1);\ncube(fact(5));"),
        [120.0; 3]
    );
}

#[test]
fn recursion_is_bounded() {
    let error = source_error("function f(n) = f(n + 1);\nx = f(0);\ncube(1);");

    assert!(
        matches!(&error, EvalError::Expression(message) if message.contains("call depth")),
        "error = {error:?}"
    );
}

#[test]
fn functions_use_their_defining_scope() {
    assert_eq!(
        cube_extents(
            "width = 2;\n\
             function area() = width * width;\n\
             module caller() { width = 10; cube(area()); }\n\
             caller();",
        ),
        [4.0; 3]
    );
}

#[test]
fn function_special_variables_are_dynamic() {
    assert_eq!(
        cube_extents(
            "$fn = 8;\n\
             function size() = $fn;\n\
             module caller() { $fn = 4; cube(size()); }\n\
             caller();",
        ),
        [4.0; 3]
    );
}

#[test]
fn functions_defined_in_modules_do_not_escape() {
    let mut kernel = TruckKernel::new();
    let inside = evaluate(
        "module outer() { function double(x) = x * 2; cube(double(2)); }\nouter();",
        &mut kernel,
        0.01,
    )
    .expect("nested function");
    assert_eq!(extents(&inside.shapes[0].mesh), [4.0; 3]);

    let error = evaluate(
        "module outer() { function double(x) = x * 2; }\nx = double(2);\ncube(x);",
        &mut kernel,
        0.01,
    )
    .expect_err("function is out of scope");
    assert!(matches!(error, EvalError::UnknownFunction(name) if name == "double"));
}

#[test]
fn function_calls_work_inside_module_arguments() {
    let size = cube_extents(
        "function area(s) = s * s;\nmodule plate(s) { cube([area(s), 1, 1]); }\nplate(3);",
    );

    assert_eq!(size, [9.0, 1.0, 1.0]);
}

#[test]
fn functions_can_be_used_before_their_declaration() {
    assert_eq!(
        cube_extents("cube(double(3));\nfunction double(x) = x * 2;"),
        [6.0; 3]
    );
}

#[test]
fn user_functions_shadow_builtins() {
    assert_eq!(
        cube_extents("function sin(x) = x;\ncube(sin(2));"),
        [2.0; 3]
    );
}

#[test]
fn builtin_hyperbolic_functions() {
    assert_eq!(
        cube_extents(
            "cube([\n\
                 acosh(2) > 1 ? 1 : 2,\n\
                 abs(cosh(0) - 1) < 1e-12 ? 1 : 2,\n\
                 is_nan(acosh(0.5)) ? 1 : 2\n\
             ]);",
        ),
        [1.0; 3]
    );
}

#[test]
fn builtin_numeric_functions() {
    assert_eq!(
        cube_extents("cube([abs(-3), max(2, 4, 3), min([5, 1, 2])]);"),
        [3.0, 4.0, 1.0]
    );

    let trig = cube_extents("cube([sin(30), cos(60), sqrt(9)]);");
    assert!((trig[0] - 0.5).abs() < 1.0e-12, "trig = {trig:?}");
    assert!((trig[1] - 0.5).abs() < 1.0e-12, "trig = {trig:?}");
    assert!((trig[2] - 3.0).abs() < 1.0e-12, "trig = {trig:?}");

    assert_eq!(
        cube_extents("cube([floor(2.9), ceil(2.1), round(2.5)]);"),
        [2.0, 3.0, 3.0]
    );

    let signs = cube_extents("cube([abs(sign(-2)), 1 + abs(sign(0)), abs(sign(3))]);");
    assert_eq!(signs, [1.0, 1.0, 1.0]);

    let power = cube_extents("cube([pow(2, 3), atan2(1, 1), log(100)]);");
    assert!((power[0] - 8.0).abs() < 1.0e-12, "power = {power:?}");
    assert!((power[1] - 45.0).abs() < 1.0e-12, "power = {power:?}");
    assert!((power[2] - 2.0).abs() < 1.0e-12, "power = {power:?}");
}

#[test]
fn builtin_vector_functions() {
    let size = cube_extents(
        "v = cross([1, 0, 0], [0, 1, 0]);\n\
         n = norm([3, 4]);\n\
         l = len(concat([1, 2], [3, 4]));\n\
         cube([l, n, len(v)]);",
    );

    assert_eq!(size, [4.0, 5.0, 3.0]);
}

#[test]
fn builtin_predicates_return_booleans() {
    assert_eq!(
        cube_extents("cube(is_num(1) && is_list([1, 2]) && is_bool(true) ? 2 : 1);"),
        [2.0; 3]
    );
    assert_eq!(cube_extents("cube(is_num([1]) ? 1 : 2);"), [2.0; 3]);
}

#[test]
fn conditional_operator_evaluates_only_the_selected_branch() {
    assert_eq!(cube_extents("cube(1 < 2 ? 3 : 1 / 0);"), [3.0; 3]);
    assert_eq!(cube_extents("cube(false ? 1 / 0 : 3);"), [3.0; 3]);
}

#[test]
fn unknown_functions_are_rejected() {
    let mut kernel = TruckKernel::new();
    let error =
        evaluate("x = nope(1);\ncube(x);", &mut kernel, 0.01).expect_err("unknown function");

    assert!(matches!(error, EvalError::UnknownFunction(name) if name == "nope"));
}

#[test]
fn missing_function_arguments_are_undef() {
    assert_eq!(
        cube_extents("function f(a) = is_undef(a) ? 2 : 1;\ncube(f());"),
        [2.0; 3]
    );
}

#[test]
fn unknown_named_function_arguments_are_rejected() {
    let mut kernel = TruckKernel::new();
    let error = evaluate(
        "function f(a) = a;\nx = f(b = 1);\ncube(x);",
        &mut kernel,
        0.01,
    )
    .expect_err("unknown argument");

    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "f"));
}

#[test]
fn too_many_function_arguments_are_rejected() {
    let mut kernel = TruckKernel::new();
    let error = evaluate(
        "function f(a) = a;\nx = f(1, 2);\ncube(x);",
        &mut kernel,
        0.01,
    )
    .expect_err("too many arguments");

    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "f"));
}

#[test]
fn min_and_max_without_numbers_are_undef() {
    assert_eq!(
        cube_extents(
            "a = min();\n\
             b = max([]);\n\
             c = min([1, undef]);\n\
             cube([is_undef(a) ? 1 : 2, is_undef(b) ? 1 : 2, is_undef(c) ? 1 : 2]);",
        ),
        [1.0; 3]
    );
}
#[test]
fn cross_accepts_two_dimensional_vectors() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "v = cross([1, 0], [0, 1]); cube([v + 1, 1, 1]);",
        &mut kernel,
        0.01,
    )
    .expect("2D cross");
    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!((max[0] - min[0] - 2.0).abs() < 1e-6);
}
