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
fn multmatrix_applies_open_scad_row_order() {
    let source = "multmatrix([[1, 0, 0, 5], [0, 1, 0, 0], [0, 0, 1, 0]]) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let min_x = evaluation.shapes[0]
        .mesh
        .positions
        .iter()
        .map(|position| position[0])
        .fold(f64::INFINITY, f64::min);
    assert!((min_x - 4.0).abs() < 1e-6, "min_x = {min_x}");
}

#[test]
fn multmatrix_accepts_a_matrix_assignment() {
    let source =
        "m = [[1, 0, 0, 5], [0, 1, 0, 0], [0, 0, 1, 0]];\nmultmatrix(m) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let min_x = evaluation.shapes[0]
        .mesh
        .positions
        .iter()
        .map(|position| position[0])
        .fold(f64::INFINITY, f64::min);
    assert!((min_x - 4.0).abs() < 1e-6, "min_x = {min_x}");
}

#[test]
fn multmatrix_forces_the_fourth_row() {
    let source =
        "multmatrix([[1, 0, 0, 5], [0, 1, 0, 0], [0, 0, 1, 0], [9, 9, 9, 9]]) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let min_x = evaluation.shapes[0]
        .mesh
        .positions
        .iter()
        .map(|position| position[0])
        .fold(f64::INFINITY, f64::min);
    assert!((min_x - 4.0).abs() < 1e-6, "min_x = {min_x}");
}

#[test]
fn mirror_reflects_geometry() {
    let source = "mirror([1, 0, 0]) translate([1, 0, 0]) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let min_x = evaluation.shapes[0]
        .mesh
        .positions
        .iter()
        .map(|position| position[0])
        .fold(f64::INFINITY, f64::min);
    assert!((min_x + 2.0).abs() < 1e-6, "min_x = {min_x}");
}

#[test]
fn rejects_singular_transforms() {
    let mut kernel = TruckKernel::new();
    let error = evaluate_to_meshes("scale([1, 0, 1]) cube([2, 2, 2]);", &mut kernel, 0.01)
        .expect_err("singular scale");

    assert!(matches!(error, EvalError::InvalidTransform(_)));
}

#[test]
fn rejects_bad_multmatrix_shapes() {
    let mut kernel = TruckKernel::new();
    let error = evaluate_to_meshes(
        "multmatrix([[1, 0], [0, 1]]) cube([2, 2, 2]);",
        &mut kernel,
        0.01,
    )
    .expect_err("bad matrix");

    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "multmatrix"));
}

#[test]
fn resize_matches_the_requested_bounding_box() {
    let source = "resize([4, 2, 0]) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");
    let shape = &evaluation.shapes[0];

    let (min, max) = bounds(&shape.mesh);
    assert!(
        (max[0] - min[0] - 4.0).abs() < 1e-6,
        "x = {}",
        max[0] - min[0]
    );
    assert!(
        (max[1] - min[1] - 2.0).abs() < 1e-6,
        "y = {}",
        max[1] - min[1]
    );
    assert!(
        (max[2] - min[2] - 2.0).abs() < 1e-6,
        "z = {}",
        max[2] - min[2]
    );

    let (part_min, part_max) = bounds(&shape.parts[0].mesh);
    assert!((part_max[0] - part_min[0] - 4.0).abs() < 1e-6);
    assert!((part_max[1] - part_min[1] - 2.0).abs() < 1e-6);
    assert!((part_max[2] - part_min[2] - 2.0).abs() < 1e-6);
}

#[test]
fn resize_is_applied_before_the_parent_transform() {
    let source = "translate([10, 0, 0]) resize([4, 2, 2]) cube([2, 2, 2]);\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!((min[0] - 8.0).abs() < 1e-6, "min_x = {}", min[0]);
    assert!((max[0] - 12.0).abs() < 1e-6, "max_x = {}", max[0]);
    assert!((min[1] + 1.0).abs() < 1e-6, "min_y = {}", min[1]);
    assert!((max[1] - 1.0).abs() < 1e-6, "max_y = {}", max[1]);
}

#[test]
fn resize_uses_the_combined_child_bounds() {
    let source = "resize([4, 0, 0]) difference() { cube([2, 2, 2]); translate([0.2, 0, 0]) cylinder(0.5, 4); }\n";
    let mut kernel = TruckKernel::new();
    let evaluation = evaluate(source, &mut kernel, 0.01).expect("evaluate");

    let (min, max) = bounds(&evaluation.shapes[0].mesh);
    assert!(
        (max[0] - min[0] - 4.0).abs() < 1e-6,
        "x = {}",
        max[0] - min[0]
    );
}

#[test]
fn rejects_negative_resize_dimensions() {
    let mut kernel = TruckKernel::new();
    let error = evaluate_to_meshes("resize([-1, 2, 2]) cube([2, 2, 2]);", &mut kernel, 0.01)
        .expect_err("negative resize");

    assert!(matches!(error, EvalError::BadArguments { module, .. } if module == "resize"));
}

fn bounds(mesh: &visual_openscad_kernel_api::Mesh) -> ([f64; 3], [f64; 3]) {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for position in &mesh.positions {
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
    }
    (min, max)
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
