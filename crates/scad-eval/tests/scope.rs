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
fn last_assignment_wins() {
    assert_eq!(cube_extents("a = 1;\ncube(a);\na = 2;"), [2.0; 3]);
}

#[test]
fn assignments_evaluate_in_first_definition_order() {
    assert_eq!(
        cube_extents("a = 1;\nb = a + 1;\na = 2;\ncube([a, b, 1]);"),
        [2.0, 3.0, 1.0]
    );
}

#[test]
fn forward_assignment_references_are_undef() {
    assert_eq!(
        cube_extents("b = a + 1;\na = 2;\ncube(is_undef(b) ? 2 : 1);"),
        [2.0; 3]
    );
}

#[test]
fn self_referencing_assignments_are_undef() {
    assert_eq!(
        cube_extents("a = a + 1;\ncube(is_undef(a) ? 2 : 1);"),
        [2.0; 3]
    );
}

#[test]
fn module_body_assignments_are_hoisted() {
    assert_eq!(
        cube_extents("module m() { cube(a); a = 2; }\nm();"),
        [2.0; 3]
    );
}

#[test]
fn block_assignments_are_hoisted_before_children() {
    assert_eq!(
        cube_extents("translate([0, 0, 0]) { cube(a); a = 2; }"),
        [2.0; 3]
    );
}
