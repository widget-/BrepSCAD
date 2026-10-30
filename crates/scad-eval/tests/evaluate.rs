use visual_openscad_kernel_truck::TruckKernel;
use visual_openscad_scad_eval::{EvalError, evaluate, evaluate_to_meshes};
use visual_openscad_scad_syntax::{TextEdit, TextRange, TextSize, apply_edit};

#[test]
fn evaluates_a_cube() {
    let mut kernel = TruckKernel::new();
    let meshes = evaluate_to_meshes("cube([2, 3, 4]);", &mut kernel, 0.01).expect("evaluate");

    assert_eq!(meshes.len(), 1);
    assert!(meshes[0].triangle_count() > 0);
}

#[test]
fn evaluates_a_cube_with_a_hole() {
    let source = "size = [2, 2, 2];\ndifference() {\n    cube(size);\n    cylinder(0.5, 4);\n}\n";
    let mut kernel = TruckKernel::new();
    let meshes = evaluate_to_meshes(source, &mut kernel, 0.01).expect("evaluate");

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
fn evaluates_translated_cube_and_exposes_transform_parameters() {
    let source = "translate([1, 0, 0]) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    let min_x = shape
        .mesh
        .positions
        .iter()
        .map(|position| position[0])
        .fold(f64::INFINITY, f64::min);
    assert!((min_x - 0.0).abs() < 1e-6, "min_x = {min_x}");

    let part = &shape.parts[0];
    assert_eq!(part.module, "cube");
    assert!(
        part.parameters
            .iter()
            .any(|parameter| parameter.value == 1.0)
    );
    assert!(
        part.parameters
            .iter()
            .any(|parameter| parameter.value == 2.0)
    );
    assert!(source[part.source_range.clone()].starts_with("cube"));
}

#[test]
fn evaluates_nested_transforms() {
    let source = "translate([1, 0, 0]) rotate([0, 0, 90]) cube([2, 1, 1]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    assert!(shape.mesh.triangle_count() > 0);
    assert!(
        shape.parts[0]
            .parameters
            .iter()
            .any(|parameter| parameter.value == 90.0)
    );
}

#[test]
fn transforms_each_child_in_a_block() {
    let source = "translate([1, 0, 0]) { cube(1); translate([2, 0, 0]) cube(1); }\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    assert_eq!(shape.parts.len(), 2);
    assert!(shape.mesh.triangle_count() > 0);
    for part in &shape.parts {
        assert!(
            part.parameters
                .iter()
                .any(|parameter| parameter.value == 1.0)
        );
        assert!(!part.mesh.positions.is_empty());
    }
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
    let error = evaluate_to_meshes("sphere(1);", &mut kernel, 0.01).expect_err("unsupported");

    assert!(matches!(error, EvalError::UnsupportedModule(name) if name == "sphere"));
}

#[test]
fn reports_parse_errors() {
    let mut kernel = TruckKernel::new();
    let error = evaluate_to_meshes("cube([2, 2;", &mut kernel, 0.01).expect_err("parse error");

    assert!(matches!(error, EvalError::Parse(_)));
}
