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
fn indexes_vectors_and_matrices() {
    let size = cube_extents(
        "v = [10, 20, 30];\n\
         m = [[1, 2], [3, 4]];\n\
         cube([v[1], m[1][0], len(v)]);",
    );

    assert_eq!(size, [20.0, 3.0, 3.0]);
}

#[test]
fn indexes_comprehension_results() {
    let size = cube_extents(
        "v = [for (i = [1:3]) i * 2];\n\
         cube([v[0], v[2], len(v)]);",
    );

    assert_eq!(size, [2.0, 6.0, 3.0]);
}

#[test]
fn rejects_bad_indexes() {
    for source in [
        "v = [1, 2];\ncube(v[5]);",
        "v = [1, 2];\ncube(v[1.5]);",
        "v = [1, 2];\ncube(v[-1]);",
        "cube(1[0]);",
    ] {
        assert!(
            matches!(source_error(source), EvalError::Expression(_)),
            "source = {source}"
        );
    }
}

#[test]
fn let_bindings_are_sequential_and_scoped() {
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(
        "x = 10;\n\
         y = let (a = 1, b = a + 1, c = x + b) a + b + c;\n\
         cube(y);\n\
         cube(x);",
        &mut kernel,
        0.01,
    )
    .expect("let expression");

    assert_eq!(evaluation.shapes.len(), 2);
    assert_eq!(extents(&evaluation.shapes[0].mesh), [15.0; 3]);
    assert_eq!(extents(&evaluation.shapes[1].mesh), [10.0; 3]);
}

#[test]
fn let_bindings_do_not_leak() {
    let error = source_error("y = let (a = 1) a;\ncube(a);");

    assert!(matches!(error, EvalError::UnknownIdentifier(name) if name == "a"));
}

#[test]
fn let_can_return_vectors() {
    let size = cube_extents(
        "v = let (a = [1, 2, 3]) a;\n\
         cube([v[0], v[2], len(v)]);",
    );

    assert_eq!(size, [1.0, 3.0, 3.0]);
}

#[test]
fn list_comprehension_maps_values() {
    let size = cube_extents(
        "v = [for (i = [0:3]) i * 2];\n\
         cube([len(v), v[0] + 1, v[3]]);",
    );

    assert_eq!(size, [4.0, 1.0, 6.0]);
}

#[test]
fn list_comprehension_supports_multiple_bindings() {
    let size = cube_extents(
        "v = [for (i = [0:1], j = [0:1]) i * 10 + j];\n\
         cube([len(v), v[1], v[2]]);",
    );

    assert_eq!(size, [4.0, 1.0, 10.0]);
}

#[test]
fn nested_list_comprehensions_flatten() {
    let size = cube_extents(
        "v = [for (i = [0:1]) for (j = [0:1]) i * 10 + j];\n\
         cube([len(v), v[1], v[3]]);",
    );

    assert_eq!(size, [4.0, 1.0, 11.0]);
}

#[test]
fn list_comprehensions_preserve_rows() {
    let size = cube_extents(
        "v = [for (i = [0:2]) [i, i * 2]];\n\
         cube([len(v), v[1][0], v[2][1]]);",
    );

    assert_eq!(size, [3.0, 1.0, 4.0]);
}

#[test]
fn list_comprehension_if_filters() {
    let size = cube_extents(
        "v = [for (i = [0:3]) if (i % 2 == 0) i];\n\
         cube([len(v), v[0] + 1, v[1]]);",
    );

    assert_eq!(size, [2.0, 1.0, 2.0]);
}

#[test]
fn list_comprehension_if_supports_else() {
    let size = cube_extents(
        "v = [for (i = [0:2]) if (i == 1) 10 else i];\n\
         cube([len(v), v[1], v[2]]);",
    );

    assert_eq!(size, [3.0, 10.0, 2.0]);
}

#[test]
fn list_comprehensions_support_let() {
    let size = cube_extents(
        "v = [for (i = [0:2]) let (j = i * 2) j + 1];\n\
         cube([len(v), v[0], v[2]]);",
    );

    assert_eq!(size, [3.0, 1.0, 5.0]);
}

#[test]
fn list_comprehensions_work_in_module_arguments() {
    let size = cube_extents(
        "function total(v) = v[0] + v[1];\n\
         module plate(v) { cube([total(v), 1, 1]); }\n\
         plate([for (i = [1:2]) i * 3]);",
    );

    assert_eq!(size, [9.0, 1.0, 1.0]);
}

#[test]
fn empty_comprehensions_produce_empty_vectors() {
    let size = cube_extents(
        "v = [for (i = []) i];\n\
         cube([len(v) + 1, 1, 1]);",
    );

    assert_eq!(size, [1.0, 1.0, 1.0]);
}

#[test]
fn comprehension_bindings_must_be_named() {
    assert!(matches!(
        source_error("v = [for (i) i];\ncube(1);"),
        EvalError::Expression(_)
    ));
}

#[test]
fn let_bindings_must_be_named() {
    assert!(matches!(
        source_error("y = let (1) 2;\ncube(y);"),
        EvalError::Expression(_)
    ));
}

#[test]
fn comprehensions_reject_booleans() {
    assert!(matches!(
        source_error("v = [for (i = [0:1]) true];\ncube(1);"),
        EvalError::Expression(_)
    ));
}
