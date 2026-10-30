mod common;

use common::bounds;
use visual_openscad_kernel_truck::TruckKernel;
use visual_openscad_scad_eval::{evaluate, evaluate_to_meshes};

#[test]
fn evaluates_a_cube_with_a_hole() {
    let source = "size = [2, 2, 2];\ndifference() {\n    cube(size);\n    cylinder(0.5, 4);\n}\n";
    let mut kernel = TruckKernel::new();
    let meshes = evaluate_to_meshes(source, &mut kernel, 0.01).expect("evaluate");

    assert_eq!(meshes.len(), 1);
    assert!(meshes[0].triangle_count() > 0);
}

#[test]
fn exposes_a_source_shaped_tree() {
    let source =
        "difference() {\n    cube([2, 2, 2]);\n    translate([0.2, 0, 0]) cylinder(0.5, 4);\n}\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    let tree = &shape.tree;
    assert_eq!(tree.module, "difference");
    assert_eq!(tree.part_index, None);
    assert_eq!(tree.children.len(), 2);

    let cube = &tree.children[0];
    assert_eq!(cube.module, "cube");
    assert_eq!(cube.part_index, Some(0));
    assert!(cube.children.is_empty());
    assert_eq!(shape.parts[0].module, "cube");
    assert_eq!(shape.parts[0].source_range, cube.source_range);

    let translate = &tree.children[1];
    assert_eq!(translate.module, "translate");
    assert_eq!(translate.part_index, None);
    assert_eq!(translate.children[0].module, "cylinder");
    assert_eq!(translate.children[0].part_index, Some(1));
    assert_eq!(shape.parts[1].module, "cylinder");
    assert_eq!(
        shape.parts[1].source_range,
        translate.children[0].source_range
    );
}

#[test]
fn evaluates_single_child_boolean_forms() {
    let source = "difference() translate([1, 0, 0]) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    assert_eq!(shape.parts.len(), 1);
    assert!(shape.mesh.triangle_count() > 0);
    let (min, _) = bounds(&shape.mesh);
    assert!((min[0] - 0.0).abs() < 1e-6, "min_x = {}", min[0]);
}

#[test]
fn difference_with_a_sphere_tessellates() {
    let source = "difference() { cube([2, 2, 2]); sphere(1.2); }\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    assert!(evaluation.shapes[0].mesh.triangle_count() > 0);
    assert_eq!(evaluation.shapes[0].parts.len(), 2);
}
