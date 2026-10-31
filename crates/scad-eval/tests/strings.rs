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
fn string_literals_support_escapes() {
    let size = cube_extents("cube([len(\"abc\"), ord(\"A\"), len(\"a\\nb\")]);");

    assert_eq!(size, [3.0, 65.0, 3.0]);
}

#[test]
fn str_formats_values_without_quotes() {
    let size = cube_extents(
        "s = str(\"x\", 2, [1, 2], true);\n\
         cube([len(s), len(str(1.5)), len(str()) + 1]);",
    );

    assert_eq!(size, [12.0, 3.0, 1.0]);
}

#[test]
fn chr_and_ord_convert_code_points() {
    let size = cube_extents("cube([ord(chr(65)), len(chr(9731)), ord(\"A\")]);");

    assert_eq!(size, [65.0, 1.0, 65.0]);
}

#[test]
fn strings_are_recognized_by_predicates() {
    let size = cube_extents("cube(is_string(\"x\") && !is_string(1) ? 2 : 1);");

    assert_eq!(size, [2.0; 3]);
}

#[test]
fn strings_compare_by_value() {
    let size = cube_extents("cube(\"a\" == \"a\" && \"a\" != \"b\" ? 2 : 1);");

    assert_eq!(size, [2.0; 3]);
}

#[test]
fn non_numeric_operands_are_undef() {
    let size = cube_extents("cube(is_undef(1 + \"a\") ? 2 : 1);");

    assert_eq!(size, [2.0; 3]);
}

#[test]
fn lists_accept_strings() {
    let size = cube_extents("v = [\"a\", \"b\"];\ncube([len(v), len(v[0]), 1]);");

    assert_eq!(size, [2.0, 1.0, 1.0]);
}

#[test]
fn chr_rejects_invalid_code_points() {
    assert!(matches!(
        source_error("x = chr(1114112);\ncube(x);"),
        EvalError::BadArguments { module, .. } if module == "chr"
    ));
}

#[test]
fn ord_rejects_empty_strings() {
    assert!(matches!(
        source_error("x = ord(\"\");\ncube(x);"),
        EvalError::BadArguments { module, .. } if module == "ord"
    ));
}
