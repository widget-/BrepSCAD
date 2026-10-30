mod common;

use brepscad_eval::{EvalError, evaluate, evaluate_to_meshes};
use brepscad_kernel_truck::TruckKernel;
use brepscad_syntax::{TextEdit, TextRange, TextSize, apply_edit};
use common::{bounds, signed_volume};

#[test]
fn evaluates_a_cube() {
    let mut kernel = TruckKernel::new();
    let meshes = evaluate_to_meshes("cube([2, 3, 4]);", &mut kernel, 0.01).expect("evaluate");

    assert_eq!(meshes.len(), 1);
    assert!(meshes[0].triangle_count() > 0);
}

#[test]
fn exposes_numeric_parameters_for_selection() {
    let source = "size = [2, 2, 2];\ndifference() {\n    cube(size);\n    cylinder(0.5, 4);\n}\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    assert_eq!(evaluation.shapes.len(), 1);
    let parameters = &evaluation.shapes[0].parameters;
    assert_eq!(parameters.len(), 5);
    assert!(parameters.iter().any(|parameter| parameter.value == 0.5));
    assert!(parameters.iter().any(|parameter| parameter.source == "4"));
}

#[test]
fn exposes_selectable_operand_parts() {
    let source = "size = [2, 2, 2];\ndifference() { cube(size); cylinder(0.5, 4); }\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    assert_eq!(shape.parts.len(), 2);
    assert_eq!(shape.parts[0].module, "cube");
    assert_eq!(shape.parts[1].module, "cylinder");
    assert_eq!(shape.parts[0].parameters.len(), 3);
    assert_eq!(shape.parts[1].parameters.len(), 2);
    assert!(
        shape
            .parts
            .iter()
            .all(|part| !part.mesh.positions.is_empty())
    );
}

#[test]
fn parameter_ranges_edit_the_expected_literal() {
    let source = "size = [2, 2, 2];\ndifference() { cube(size); cylinder(0.5, 4); }\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let part = &evaluation.shapes[0].parts[0];
    let parameter = part
        .parameters
        .iter()
        .find(|parameter| parameter.value == 2.0)
        .expect("size literal");

    let range = TextRange::new(
        TextSize::from(parameter.range.start as u32),
        TextSize::from(parameter.range.end as u32),
    );
    let edited = apply_edit(source, &TextEdit::new(range, "3")).expect("edit");

    assert!(edited.contains("size = [3, 2, 2];"), "{edited}");
}

#[test]
fn evaluates_named_primitive_arguments() {
    let mut kernel = TruckKernel::new();
    let cube = evaluate("cube(size = [2, 4, 6]);", &mut kernel, 0.01).expect("cube");
    let (min, max) = bounds(&cube.shapes[0].mesh);
    assert!((max[0] - min[0] - 2.0).abs() < 1e-6);
    assert!((max[1] - min[1] - 4.0).abs() < 1e-6);
    assert!((max[2] - min[2] - 6.0).abs() < 1e-6);

    let cylinder = evaluate("cylinder(0.5, h = 4);", &mut kernel, 0.01).expect("cylinder");
    let (min, max) = bounds(&cylinder.shapes[0].mesh);
    assert!((max[2] - min[2] - 4.0).abs() < 1e-6);
}

#[test]
fn named_argument_names_do_not_leak_assignment_parameters() {
    let source = "size = [9, 9, 9];\ncube(size = 2);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let part = &evaluation.shapes[0].parts[0];

    assert_eq!(part.parameters.len(), 1);
    assert_eq!(part.parameters[0].value, 2.0);
}

#[test]
fn named_argument_values_still_resolve_assignment_parameters() {
    let source = "scale = [2, 3, 4];\ncube(size = scale);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    let (min, max) = bounds(&shape.mesh);
    assert!((max[0] - min[0] - 2.0).abs() < 1e-6);
    assert!((max[1] - min[1] - 3.0).abs() < 1e-6);
    assert!((max[2] - min[2] - 4.0).abs() < 1e-6);
    assert!(
        shape.parts[0]
            .parameters
            .iter()
            .any(|parameter| parameter.value == 3.0)
    );
}

#[test]
fn evaluates_a_polyhedron() {
    let source = "polyhedron(\n    points = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]],\n    faces = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]]\n);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    let volume = signed_volume(&shape.mesh);
    assert!((volume - 1.0 / 6.0).abs() < 1e-9, "volume = {volume}");
    assert_eq!(shape.parts.len(), 1);
    assert_eq!(shape.parts[0].module, "polyhedron");
}

#[test]
fn rejects_bad_polyhedron_indices() {
    let source = "polyhedron(points = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]], faces = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 9]]);\n";
    let mut kernel = TruckKernel::new();
    let error = evaluate_to_meshes(source, &mut kernel, 0.01).expect_err("bad index");

    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "polyhedron"));
}

#[test]
fn evaluates_a_sphere_and_its_diameter() {
    let mut kernel = TruckKernel::new();

    let sphere = evaluate("sphere(r = 1);", &mut kernel, 0.01).expect("sphere");
    let (min, max) = bounds(&sphere.shapes[0].mesh);
    for axis in 0..3 {
        assert!(
            (max[axis] - min[axis] - 2.0).abs() < 0.02,
            "axis {axis}: {}",
            max[axis] - min[axis]
        );
    }

    let by_diameter = evaluate("sphere(d = 2);", &mut kernel, 0.01).expect("diameter");
    let (min, max) = bounds(&by_diameter.shapes[0].mesh);
    assert!((max[0] - min[0] - 2.0).abs() < 0.02);
}
