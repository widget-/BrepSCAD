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
fn exponentiation_is_right_associative_and_binds_tighter_than_unary_minus() {
    let evaluation = evaluate_source(
        "a = 2^3^2;\n\
         b = -2^2;\n\
         c = 2*3^2;\n\
         d = 2^3*2;\n\
         cube([a, abs(b), c]);\n\
         cube(d);\n",
    );

    assert_eq!(evaluation.shapes.len(), 2);
    assert_eq!(extents(&evaluation.shapes[0].mesh), [512.0, 4.0, 18.0]);
    assert_eq!(extents(&evaluation.shapes[1].mesh), [16.0; 3]);
}

#[test]
fn digit_leading_module_and_function_names_work() {
    assert_eq!(
        cube_extents(
            "module 12ptStar(size) cube(size);\n\
             function 8bit_double(x) = x * 2;\n\
             12ptStar(8bit_double(2));",
        ),
        [4.0; 3]
    );
}

#[test]
fn trailing_commas_are_accepted() {
    assert_eq!(
        cube_extents(
            "function pair(a, b,) = a + b;\n\
             cube(pair(1, 2,));",
        ),
        [3.0; 3]
    );
}

#[test]
fn undef_propagates_and_is_false() {
    assert_eq!(
        cube_extents(
            "result = is_undef(undef + 1)\n\
             && is_undef(undef)\n\
             && undef == undef\n\
             && undef != 1\n\
             && !(undef < 1)\n\
             && !undef\n\
             && ![]\n\
             && !\"\" ? 2 : 1;\n\
             cube(result);",
        ),
        [2.0; 3]
    );
}

#[test]
fn version_functions_report_the_compatibility_target() {
    assert_eq!(
        cube_extents(
            "cube([\n\
                 version_num() >= 20210100 ? 1 : 2,\n\
                 version()[0] == 2021 ? 1 : 2,\n\
                 1\n\
             ]);",
        ),
        [1.0; 3]
    );
}

#[test]
fn echo_prefix_returns_the_following_expression_and_records_a_message() {
    let evaluation = evaluate_source(
        "x = echo(\"warn\", 2) 7;\n\
         cube(x);",
    );

    assert_eq!(extents(&evaluation.shapes[0].mesh), [7.0; 3]);
    assert_eq!(evaluation.messages, ["ECHO: \"warn\", 2"]);
}

#[test]
fn echo_module_renders_its_children() {
    let evaluation = evaluate_source("echo(\"x\") cube(2);");

    assert_eq!(evaluation.shapes.len(), 1);
    assert_eq!(extents(&evaluation.shapes[0].mesh), [2.0; 3]);
    assert_eq!(evaluation.messages, ["ECHO: \"x\""]);
}

#[test]
fn echo_formats_named_arguments() {
    let evaluation = evaluate_source("echo(a = 1, b = \"x\"); cube(1);");

    assert_eq!(evaluation.messages, ["ECHO: a = 1, b = \"x\""]);
}

#[test]
fn assert_prefix_returns_the_following_expression() {
    assert_eq!(
        cube_extents("x = assert(true, \"ok\") 7;\ncube(x);"),
        [7.0; 3]
    );
}

#[test]
fn assert_module_renders_its_children() {
    assert_eq!(cube_extents("assert(true) cube(2);"), [2.0; 3]);
}

#[test]
fn failed_assertions_are_evaluation_errors() {
    let error = source_error("assert(false, \"boom\");\ncube(1);");

    assert!(matches!(error, EvalError::Assertion(message) if message == "\"boom\""));
}

#[test]
fn each_flattens_lists_ranges_and_scalars() {
    assert_eq!(
        cube_extents(
            "v = [each [1, 2], each [0:1], each 9];\n\
             cube([len(v), v[0], v[4]]);",
        ),
        [5.0, 1.0, 9.0]
    );
}

#[test]
fn each_flattens_inside_for_comprehensions() {
    assert_eq!(
        cube_extents(
            "v = [for (i = [0:1]) each [i, i + 10]];\n\
             cube([len(v), v[1], v[3]]);",
        ),
        [4.0, 10.0, 11.0]
    );
}

#[test]
fn c_style_for_comprehensions_thread_assignments() {
    assert_eq!(
        cube_extents(
            "v = [for (i = 0, j = 10; i < 2; i = i + 1, j = j + 1) i + j];\n\
             cube([len(v), v[0], v[1]]);",
        ),
        [2.0, 10.0, 12.0]
    );
}

#[test]
fn c_style_for_bodies_can_flatten_each() {
    assert_eq!(
        cube_extents(
            "v = [for (i = 0; i < 2; i = i + 1) each [i, i + 10]];\n\
             cube([len(v), v[1], v[3]]);",
        ),
        [4.0, 10.0, 11.0]
    );
}

#[test]
fn c_style_for_comprehensions_stop_at_the_iteration_limit() {
    let error = source_error("v = [for (i = 0; i >= 0; i = i + 1) i];\ncube(1);");

    assert!(
        matches!(&error, EvalError::Expression(message) if message.contains("100000")),
        "error = {error:?}"
    );
}

#[test]
fn star_modifier_disables_a_subtree() {
    let evaluation = evaluate_source("*cube(1);\n#cube(2);\n%cube(3);\n!cube(4);");

    assert_eq!(evaluation.shapes.len(), 3);
    let sizes = evaluation
        .shapes
        .iter()
        .map(|shape| extents(&shape.mesh))
        .collect::<Vec<_>>();
    assert_eq!(sizes, [[2.0; 3], [3.0; 3], [4.0; 3]]);
}

#[test]
fn let_statements_bind_sequentially_and_restore_the_outer_scope() {
    let evaluation = evaluate_source("let (a = 2, b = a + 1) cube(b);\ncube(1);");

    assert_eq!(evaluation.shapes.len(), 2);
    assert_eq!(extents(&evaluation.shapes[0].mesh), [3.0; 3]);
    assert_eq!(extents(&evaluation.shapes[1].mesh), [1.0; 3]);
}

#[test]
fn let_statement_bindings_do_not_leak() {
    let evaluation = evaluate_source("let (a = 2) cube(a);\ncube(is_undef(a) ? 2 : 1);");

    assert_eq!(evaluation.shapes.len(), 2);
    assert_eq!(extents(&evaluation.shapes[1].mesh), [2.0; 3]);
}

#[test]
fn dot_notation_reads_vector_members() {
    assert_eq!(
        cube_extents("v = [1, 2, 3];\ncube([v.x, v.y, v.z]);"),
        [1.0, 2.0, 3.0]
    );
}

#[test]
fn unknown_dot_members_are_undef() {
    assert_eq!(
        cube_extents("v = [1, 2, 3];\ncube(is_undef(v.w) ? 2 : 1);"),
        [2.0; 3]
    );
}

#[test]
fn strings_can_be_indexed_by_character() {
    assert_eq!(
        cube_extents("s = \"abc\";\ncube([len(s[1]), ord(s[0]) - 96, 1]);"),
        [1.0, 1.0, 1.0]
    );
}

#[test]
fn string_hex_and_unicode_escapes_are_decoded() {
    assert_eq!(
        cube_extents(r#"s = "\u26A0\x41"; cube([len(s), ord(s[1]) - 64, 1]);"#),
        [2.0, 1.0, 1.0]
    );
}

#[test]
fn children_accept_ranges_and_lists() {
    let evaluation = evaluate_source(
        "module first_two() { children([0:1]); }\n\
         module second() { children([1]); }\n\
         first_two() { cube(1); translate([10, 0, 0]) cube(2); }\n\
         second() { cube(3); cube(4); }",
    );

    assert_eq!(evaluation.shapes.len(), 3);
    let sizes = evaluation
        .shapes
        .iter()
        .map(|shape| extents(&shape.mesh))
        .collect::<Vec<_>>();
    assert_eq!(sizes, [[1.0; 3], [2.0; 3], [4.0; 3]]);
}

#[test]
fn stray_semicolons_are_empty_statements() {
    let evaluation = evaluate_source("; cube(1); ;");

    assert_eq!(evaluation.shapes.len(), 1);
}
