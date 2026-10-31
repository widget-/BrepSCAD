mod common;

use brepscad_eval::{Evaluation, evaluate};
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

#[test]
fn lists_can_hold_strings_booleans_lists_and_undef() {
    assert_eq!(
        cube_extents(
            "v = [\"a\", 2, [3, true], undef];\n\
             cube([len(v), len(v[2]), is_bool(v[2][1]) && is_undef(v[3]) ? 1 : 2]);",
        ),
        [4.0, 2.0, 1.0]
    );
}

#[test]
fn nested_lists_index_like_matrices() {
    assert_eq!(
        cube_extents("m = [[1, 2], [3, 4]];\ncube([m[1][0], m[0][1], len(m)]);"),
        [3.0, 2.0, 2.0]
    );
}

#[test]
fn dot_members_work_on_nested_lists() {
    assert_eq!(
        cube_extents("v = [[1, 2], [3, 4]];\ncube([v[0].y, v[1].x, 1]);"),
        [2.0, 3.0, 1.0]
    );
}

#[test]
fn list_arithmetic_is_recursive_and_elementwise() {
    assert_eq!(
        cube_extents(
            "a = [1, [2, 3]];\n\
             b = [10, [20, 30]];\n\
             c = a + b;\n\
             cube([c[0], c[1][0], c[1][1]]);",
        ),
        [11.0, 22.0, 33.0]
    );
}

#[test]
fn scalar_broadcast_is_undefined() {
    assert_eq!(
        cube_extents(
            "cube([\n\
                 is_undef([1, 2] + 1) ? 1 : 2,\n\
                 is_undef(2 * [3, 4]) ? 1 : 2,\n\
                 1\n\
             ]);",
        ),
        [1.0; 3]
    );
}

#[test]
fn list_truthiness_follows_open_scad() {
    assert_eq!(
        cube_extents("cube(![] && [0] && [false] && \"false\" ? 1 : 2);"),
        [1.0; 3]
    );
}

#[test]
fn concat_flattens_lists_and_appends_scalars() {
    assert_eq!(
        cube_extents(
            "v = concat([1, 2], 3, [\"a\"], \"b\");\n\
             cube([len(v), v[0], is_string(v[3]) && is_string(v[4]) ? 1 : 2]);",
        ),
        [5.0, 1.0, 1.0]
    );
}

#[test]
fn echo_formats_heterogeneous_lists() {
    let evaluation = evaluate_source("echo([\"a\", 2, [3, true], undef]);\ncube(1);");

    assert_eq!(evaluation.messages, ["ECHO: [\"a\", 2, [3, true], undef]"]);
}

#[test]
fn str_formats_nested_strings_with_quotes() {
    assert_eq!(
        cube_extents(r#"cube(str(["a", 2]) == "[\"a\", 2]" ? 1 : 2);"#),
        [1.0; 3]
    );
}

#[test]
fn lookup_finds_values_and_returns_undef_for_missing_keys() {
    assert_eq!(
        cube_extents(
            "table = [[1, \"a\"], [2, \"b\"]];\n\
             cube([\n\
                 lookup(2, table) == \"b\" ? 1 : 2,\n\
                 is_undef(lookup(3, table)) ? 1 : 2,\n\
                 1\n\
             ]);",
        ),
        [1.0; 3]
    );
}

#[test]
fn search_finds_strings_numbers_and_multiple_targets() {
    assert_eq!(
        cube_extents(
            "a = search(\"b\", [\"a\", \"b\", \"c\"]);\n\
             b = search(2, [[1, 10], [2, 20]], 1);\n\
             c = search([\"a\", \"c\"], [\"a\", \"b\", \"c\"]);\n\
             cube([a[0], b[0], c[1][0]]);",
        ),
        [1.0, 1.0, 2.0]
    );
}

#[test]
fn search_zero_returns_all_matches() {
    assert_eq!(
        cube_extents(
            "r = search(1, [1, 1, 1], 0);\n\
             cube([len(r), r[0] == 0 ? 1 : 2, r[2] == 2 ? 1 : 2]);",
        ),
        [3.0, 1.0, 1.0]
    );
}

#[test]
fn rands_is_deterministic_for_a_given_seed() {
    assert_eq!(
        cube_extents(
            "a = rands(0, 1, 5, 42);\n\
             b = rands(0, 1, 5, 42);\n\
             cube([len(a), a == b ? 1 : 2, a[0] >= 0 && a[0] <= 1 ? 1 : 2]);",
        ),
        [5.0, 1.0, 1.0]
    );
}

#[test]
fn rands_supports_vector_bounds() {
    assert_eq!(
        cube_extents(
            "r = rands([0, 0], [1, 1], 2, 7);\n\
             cube([len(r), len(r[0]), 1]);",
        ),
        [2.0, 2.0, 1.0]
    );
}

#[test]
fn len_and_is_list_handle_nested_lists() {
    assert_eq!(
        cube_extents("cube([len([[1], [2, 3]]), is_list([[]]) ? 1 : 2, 1]);"),
        [2.0, 1.0, 1.0]
    );
}

#[test]
fn undef_inside_a_list_is_preserved() {
    assert_eq!(
        cube_extents("v = [1, undef, 3];\ncube([len(v), is_undef(v[1]) ? 1 : 2, 1]);"),
        [3.0, 1.0, 1.0]
    );
}
